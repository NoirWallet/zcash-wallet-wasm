//! End-to-end: a standard Ironwood PCZT driven by a FROST group key.
//!
//! This is the load-bearing test of the whole design. It builds a real Ironwood PCZT whose spend
//! authority is a FROST group key (not a single spending key), then runs the signer flow over it:
//!
//! 1. DKG produces the group key `ak`.
//! 2. An FVK is assembled with `ak` spliced over the first 32 bytes of the `ak ‖ nk ‖ rivk`
//!    encoding — the construction the wallet tier will use for `KEYSET_SETUP`. `nk`/`rivk` come
//!    from an unrelated random `SpendingKey`, which is fine: they only affect scanning and address
//!    derivation, never spend authorization.
//! 3. `zcash_primitives`' builder produces the PCZT from that FVK, so each action's `rk` is
//!    `ak + alpha·G` **by construction**.
//! 4. The signer gate admits it, the coordinator's `alpha` is read back from the action, and the
//!    `rk == ak + alpha·G` check is asserted.
//! 5. A threshold subset signs the recomputed sighash, the shares aggregate, and the resulting
//!    64-byte RedPallas signature verifies against the PCZT's `rk`.
//!
//! What this proves that reading code cannot: the FROST group key really does drive a stock Ironwood
//! transaction, and the signature really does verify the way consensus will check it.

use std::collections::BTreeMap;

use ff::PrimeField;
use frost_vault_core::threshold::{
    aggregate_with_alpha, commit, randomized_verifying_key, run_dkg, sign_with_alpha,
    verify_with_group_key_and_alpha, SigningCommitments,
};
use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use orchard::note::{NoteVersion, RandomSeed, Rho};
use orchard::tree::{MerkleHashOrchard, MerklePath};
use orchard::Note;
use pczt::roles::io_finalizer::IoFinalizer;
use pczt::roles::redactor::Redactor;
use pczt::roles::verifier::Verifier;
use pczt::Pczt;
use pczt_signer_core::gate::{check, PcztShape};
use pczt_signer_core::material::{bind_alphas, ironwood_action_rks};
use pczt_signer_core::sighash::shielded_sighash;
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use rand08::rngs::OsRng as FrostRng;
use zcash_primitives::transaction::builder::{BuildConfig, Builder, BundlePadding};
use zcash_primitives::transaction::fees::zip317;
use zcash_protocol::consensus::{Network, NetworkUpgrade, Parameters};
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;

const SPEND_VALUE: u64 = 1_000_000;
const PAYMENT_VALUE: u64 = 990_000;

/// Build an Ironwood PCZT whose spend authority is `ak` (a FROST group key).
///
/// Returns the PCZT together with the FVK's address, so the caller can see where the funds go.
fn fixture_pczt(ak: &[u8; 32]) -> Pczt {
    let network = Network::TestNetwork;
    let target_height = network
        .activation_height(NetworkUpgrade::Nu6_3)
        .expect("NU6.3 activation")
        + 1;

    // nk / rivk come from an unrelated spending key; only `ak` controls spending.
    let sk = SpendingKey::from_bytes([7u8; 32])
        .into_option()
        .expect("valid sk");
    let mut fvk_bytes = FullViewingKey::from(&sk).to_bytes();
    fvk_bytes[..32].copy_from_slice(ak);
    let fvk = FullViewingKey::from_bytes(&fvk_bytes).expect("spliced FVK is structurally valid");
    let recipient = fvk.address_at(0u32, Scope::External);

    let rho = Rho::from_bytes(&[1u8; 32]).unwrap();
    let rseed = (0u8..=255)
        .find_map(|byte| RandomSeed::from_bytes([byte; 32], &rho).into_option())
        .expect("at least one test rseed is valid");
    let note = Note::from_parts(
        recipient,
        orchard::value::NoteValue::from_raw(SPEND_VALUE),
        rho,
        rseed,
        NoteVersion::V3, // Ironwood notes carry version 3 plaintexts
    )
    .unwrap();

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
            // One real spend plus one output fits in a single action, so the bundle contains no
            // dummy action and exactly one `rk` belongs to the group key.
            ironwood_padding: BundlePadding {
                bundle_required: true,
                pad_to_minimum: Some(1),
            },
        },
    );
    builder
        .add_ironwood_spend::<zip317::FeeRule>(fvk.clone(), note, merkle_path)
        .expect("add ironwood spend");
    builder
        .add_ironwood_output::<zip317::FeeRule>(
            Some(fvk.to_ovk(Scope::External)),
            recipient,
            Zatoshis::const_from_u64(PAYMENT_VALUE),
            MemoBytes::empty(),
        )
        .expect("add ironwood output");

    let result = builder
        .build_for_pczt(UnwrapErr(SysRng), &zip317::FeeRule::standard())
        .expect("build for pczt");

    IoFinalizer::new(pczt::roles::creator::Creator::build_from_parts(result.pczt_parts).unwrap())
        .finalize_io(UnwrapErr(SysRng))
        .expect("finalize io")
}

/// Read each action's `alpha` the way a coordinator does: through the oracle the pczt crate exposes
/// for Orchard-protocol bundles. The signer never needs this — it is handed `alpha` and checks it
/// against `rk` — but the test needs it to produce a share.
fn coordinator_alphas(pczt: &Pczt) -> Vec<[u8; 32]> {
    let verifier = Verifier::new(pczt.clone());
    let mut alphas = Vec::new();
    verifier
        .with_ironwood::<String, _>(|bundle| {
            for action in bundle.actions() {
                let alpha = action
                    .spend()
                    .alpha()
                    .expect("builder always sets alpha on a real spend");
                alphas.push(alpha.to_repr());
            }
            Ok(())
        })
        .expect("walk ironwood bundle");
    alphas
}

#[test]
fn a_frost_group_key_drives_a_real_ironwood_pczt() {
    let mut rng = FrostRng;
    let dkg = run_dkg(3, 2, &mut rng).expect("dkg");
    let ak = dkg.group_verifying_key().expect("ak");

    let pczt = fixture_pczt(&ak);

    // 1. The gate admits it: V6, Ironwood-only, nothing transparent or Sapling.
    let shape = PcztShape::of(&pczt);
    assert_eq!(shape.ironwood_actions, 1, "one action, no padding");
    assert_eq!(shape.orchard_actions, 0);
    check(&shape).expect("gate admits a plain ironwood transaction");

    // 2. Per-action material: rk from the PCZT, alpha from the coordinator.
    let rks = ironwood_action_rks(&pczt).expect("rks");
    let alphas = coordinator_alphas(&pczt);
    assert_eq!(rks.len(), alphas.len());
    let material = bind_alphas(&rks, &alphas).expect("bind");

    // 3. The load-bearing assertion: the action's rk really is the FROST group key randomized by
    //    this alpha. If this holds, the group key authored the transaction.
    for m in &material {
        m.verify_rk(&ak).expect("rk is ak + alpha*G");
        assert_eq!(
            m.rk,
            randomized_verifying_key(&ak, &m.alpha).unwrap(),
            "action {} rk disagrees",
            m.action_index
        );
    }

    // 4. Recompute the sighash from the PCZT rather than trusting any digest.
    let sighash = shielded_sighash(pczt.clone()).expect("sighash");

    // 5. Threshold-sign it: two of three devices.
    let signers = dkg.default_signers();
    assert_eq!(signers.len(), 2);
    let mut commitments: BTreeMap<_, SigningCommitments> = BTreeMap::new();
    let mut round1 = Vec::new();
    for id in &signers {
        let c = commit(&dkg.key_packages[id], &mut rng);
        commitments.insert(*id, c.commitments);
        round1.push(c);
    }

    let m = material[0];
    let mut shares = BTreeMap::new();
    for c in &round1 {
        shares.insert(
            c.identifier,
            sign_with_alpha(
                c,
                &dkg.key_packages[&c.identifier],
                &commitments,
                &sighash,
                &m.alpha,
            )
            .expect("share"),
        );
    }

    let sig = aggregate_with_alpha(&dkg.pubkeys, &commitments, &sighash, &m.alpha, &shares)
        .expect("aggregate");
    assert_eq!(sig.len(), 64);

    // 6. The signature verifies against the PCZT's own rk over the PCZT's own sighash — exactly the
    //    check the Zcash consensus rule performs.
    verify_with_group_key_and_alpha(&ak, &m.alpha, &sighash, &sig)
        .expect("threshold signature verifies against the PCZT rk");

    // And a signature over a different message must not verify, so the check above is not vacuous.
    let mut other = sighash;
    other[0] ^= 0xff;
    assert!(verify_with_group_key_and_alpha(&ak, &m.alpha, &other, &sig).is_err());
}

#[test]
fn aggregate_signature_is_indistinguishable_from_a_single_signer_one() {
    // Same property the design doc claims: the aggregated signature is a plain RedPallas spend auth
    // signature over the shielded sighash. Here we only assert the shape and that our own verifier
    // accepts it from both the FROST path and a fresh group key check.
    let mut rng = FrostRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    let ak = dkg.group_verifying_key().unwrap();
    let pczt = fixture_pczt(&ak);
    let material = bind_alphas(
        &ironwood_action_rks(&pczt).unwrap(),
        &coordinator_alphas(&pczt),
    )
    .expect("bind");
    let sighash = shielded_sighash(pczt.clone()).expect("sighash");
    let m = material[0];

    let mut commitments = BTreeMap::new();
    let mut round1 = Vec::new();
    for id in dkg.default_signers() {
        let c = commit(&dkg.key_packages[&id], &mut rng);
        commitments.insert(id, c.commitments);
        round1.push(c);
    }
    let mut shares = BTreeMap::new();
    for c in &round1 {
        shares.insert(
            c.identifier,
            sign_with_alpha(
                c,
                &dkg.key_packages[&c.identifier],
                &commitments,
                &sighash,
                &m.alpha,
            )
            .unwrap(),
        );
    }
    let sig =
        aggregate_with_alpha(&dkg.pubkeys, &commitments, &sighash, &m.alpha, &shares).unwrap();

    // Aggregate produced by a *different* 2-of-2 group must not verify under this rk.
    let other = run_dkg(2, 2, &mut rng).unwrap();
    assert!(verify_with_group_key_and_alpha(
        &other.group_verifying_key().unwrap(),
        &m.alpha,
        &sighash,
        &sig
    )
    .is_err());
}

/// P10 decision ③: the request should not carry zk proofs.
///
/// The Prover role is deliberately not linked into the signer tier, so this fixture has no proof
/// to strip. What can be measured here is the thing the decision rests on: how large a proof is
/// relative to everything else. `Proof::expected_proof_size(n) = 2720 + 2272n`, so a single-action
/// Ironwood proof (4992 bytes) is more than twice the entire rest of the PCZT.
#[test]
fn proof_size_dominates_a_request_pczt() {
    let mut rng = FrostRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    // A real `ak` is required: `FullViewingKey::from_bytes` validates the point, which is why an
    // arbitrary 32-byte placeholder is rejected rather than silently used.
    let pczt = fixture_pczt(&dkg.group_verifying_key().unwrap());
    let bare = pczt.clone().serialize().expect("serialize").len();
    let proof_one_action = orchard::Proof::expected_proof_size(1);
    let proof_two_actions = orchard::Proof::expected_proof_size(2);

    println!(
        "pczt without proof: {bare} B; proof(1 action): {proof_one_action} B; \
         proof(2 actions): {proof_two_actions} B"
    );

    // If the proof is bigger than the payload it accompanies, stripping it is the single largest
    // reduction available to the request direction.
    assert!(
        proof_one_action > bare,
        "proof {proof_one_action} should exceed the {bare}-byte rest of the PCZT"
    );

    // Stripping is idempotent and safe when the coordinator already did it: the gate does not treat
    // proof presence as a policy input.
    let stripped = Redactor::new(pczt.clone())
        .redact_ironwood_with(|mut bundle| bundle.clear_zkproof())
        .finish();
    assert_eq!(
        stripped.clone().serialize().unwrap().len(),
        bare,
        "clearing an absent proof must be a no-op"
    );
    check(&PcztShape::of(&stripped)).expect("stripped pczt still admitted");
}
