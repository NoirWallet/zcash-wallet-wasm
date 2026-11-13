//! P4: what the coordinator sends, what the signer keeps, and what it costs on a QR.
//!
//! This is the first test that links a prover, so it is also the first place the proof-size argument
//! can be measured rather than derived.

use frost_vault_core::threshold::run_dkg;
use orchard::keys::{FullViewingKey, Scope};
use orchard::note::{NoteVersion, RandomSeed, Rho};
use orchard::tree::{MerkleHashOrchard, MerklePath};
use orchard::Note;
use pczt::roles::io_finalizer::IoFinalizer;
use pczt::Pczt;
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use rand08::rngs::OsRng as FrostRng;
use vault_keyset::VaultKeyset;
use vault_pczt::{frames, prepare, prove_ironwood, shielded_sighash};
use zcash_primitives::transaction::builder::{BuildConfig, Builder, BundlePadding};
use zcash_primitives::transaction::fees::zip317;
use zcash_protocol::consensus::{Network, NetworkUpgrade, Parameters};
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;

const SPEND: u64 = 1_000_000;
const PAY: u64 = 990_000;

fn fixture(ak: &[u8; 32], qsk: &[u8; 32]) -> (Pczt, VaultKeyset) {
    let network = Network::TestNetwork;
    let keyset = VaultKeyset::derive(ak, qsk, network, 0).expect("keyset");
    let fvk = FullViewingKey::from_bytes(&keyset.fvk).expect("fvk");
    let recipient = fvk.address_at(0u32, Scope::External);
    let target_height = network.activation_height(NetworkUpgrade::Nu6_3).expect("nu6.3") + 1;

    let rho = Rho::from_bytes(&[1u8; 32]).unwrap();
    let rseed = (0u8..=255)
        .find_map(|byte| RandomSeed::from_bytes([byte; 32], &rho).into_option())
        .expect("rseed");
    let note = Note::from_parts(recipient, orchard::value::NoteValue::from_raw(SPEND), rho, rseed, NoteVersion::V3).unwrap();

    let zero = MerkleHashOrchard::from_bytes(&[0; 32]).unwrap();
    let merkle_path = MerklePath::from_parts(0, [zero; 32]);
    let anchor = merkle_path.root(note.commitment().into());

    let mut builder = Builder::new(
        network,
        target_height,
        BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: None,
            ironwood_anchor: Some(anchor),
            orchard_padding: BundlePadding::DEFAULT,
            ironwood_padding: BundlePadding {
                bundle_required: true,
                pad_to_minimum: Some(1),
            },
        },
    );
    builder.add_ironwood_spend::<zip317::FeeRule>(fvk.clone(), note, merkle_path).expect("spend");
    builder
        .add_ironwood_output::<zip317::FeeRule>(Some(fvk.to_ovk(Scope::External)), recipient, Zatoshis::const_from_u64(PAY), MemoBytes::empty())
        .expect("output");

    let parts = builder.build_for_pczt(UnwrapErr(SysRng), &zip317::FeeRule::standard()).expect("build");
    let pczt = IoFinalizer::new(pczt::roles::creator::Creator::build_from_parts(parts.pczt_parts).expect("creator"))
        .finalize_io(UnwrapErr(SysRng))
        .expect("io finalizer");
    (pczt, keyset)
}

#[test]
fn proving_costs_five_kilobytes_and_stripping_recovers_it() {
    let dkg = run_dkg(2, 2, &mut FrostRng).expect("dkg");
    let ak = dkg.group_verifying_key().unwrap();
    let mut qsk = [0u8; 32];
    qsk[0] = 0x21;
    qsk[31] = 0x01;

    let (unproven, _keyset) = fixture(&ak, &qsk);
    let unproven_len = unproven.clone().serialize().expect("ser").len();
    let sighash_before = shielded_sighash(unproven.clone()).expect("sighash");

    let proven = prove_ironwood(unproven, UnwrapErr(SysRng)).expect("prove");
    let proven_len = proven.clone().serialize().expect("ser").len();

    // The proof is the whole point of this step: it should add roughly
    // `Proof::expected_proof_size(1) = 2720 + 2272 = 4992` bytes.
    let proof_bytes = proven_len - unproven_len;
    println!("unproven {unproven_len} B, proven {proven_len} B, proof ≈ {proof_bytes} B");
    assert!(
        (4_900..=5_100).contains(&proof_bytes),
        "expected one action's proof (~4992 B), got {proof_bytes}"
    );

    // Preparing must not move the sighash: the signer authorizes the proven transaction.
    let prepared = prepare(proven).expect("prepare");
    assert_eq!(prepared.sighash, sighash_before);
    assert_eq!(prepared.alphas.len(), 1);
    assert_eq!(prepared.proven_len, proven_len);
    assert_eq!(prepared.transport.len(), prepared.transport_len);

    let saved = prepared.proven_len - prepared.transport_len;
    println!(
        "transport {} B (saved {saved} B, {:.0}%): {} frames @200 B, {} frames @800 B",
        prepared.transport_len,
        100.0 * saved as f64 / prepared.proven_len as f64,
        frames(prepared.transport_len, 200),
        frames(prepared.transport_len, 800),
    );
    assert!(saved >= 4_900, "stripping must recover the proof's bytes");

    // The kept copy still carries the proof: it is the one that will be broadcast.
    assert!(Pczt::parse(&prepared.kept.clone().serialize().expect("ser")).is_ok());
    let kept_len = prepared.kept.clone().serialize().expect("ser").len();
    assert_eq!(kept_len, proven_len);
}

#[test]
fn the_transport_copy_still_verifies_as_the_same_transaction() {
    let dkg = run_dkg(2, 2, &mut FrostRng).expect("dkg");
    let ak = dkg.group_verifying_key().unwrap();
    let mut qsk = [0u8; 32];
    qsk[0] = 0x31;
    qsk[31] = 0x01;

    let (unproven, _) = fixture(&ak, &qsk);
    let proven = prove_ironwood(unproven, UnwrapErr(SysRng)).expect("prove");
    let prepared = prepare(proven).expect("prepare");

    let transport = Pczt::parse(&prepared.transport).expect("transport parses");
    // Same digest, from bytes that crossed a wire.
    assert_eq!(shielded_sighash(transport.clone()).expect("sighash"), prepared.sighash);
    // And the alphas the signer will be given match the action.
    let alphas = vault_pczt::extract_alphas(&transport).expect("alphas");
    assert_eq!(alphas, prepared.alphas);
}

#[test]
fn frame_budget_shows_why_frame_size_matters() {
    // Pure arithmetic, but it is the number the P9 metric decision rests on.
    assert_eq!(frames(0, 200), 0);
    assert_eq!(frames(200, 200), 1);
    assert_eq!(frames(201, 200), 2);
    assert_eq!(frames(2_362, 200), 12);
    assert_eq!(frames(2_362, 800), 3);
    assert_eq!(frames(7_354, 200), 37);
    assert_eq!(frames(7_354, 800), 10);
}
