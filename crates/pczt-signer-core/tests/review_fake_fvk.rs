//! Can a restored device be shown a viewing key that hides where the money went?
//!
//! The scenario: the phone is restored from its 24 words and learns its full viewing key from the
//! extension. The extension is compromised. It keeps the real `ak` (so every `ak`-based check passes) and
//! changes `nk` or `rivk`, which is what `dk` — and therefore "which addresses are ours" — is derived from.
//!
//! What these tests establish:
//!
//! * the addresses a *fake* key calls ours are entirely the attacker's choice (test 1);
//! * but the spent note is a real note of the real vault, and its address is fixed by the chain data. Since
//!   `dk` is derived from `nk` and `rivk` together, any change to either one puts that note *outside* the
//!   fake key's scope, so the review refuses the transaction rather than mislabelling its outputs (tests 2
//!   and 3);
//! * an `ak`-only check and a self-computed fingerprint both pass, so neither is a defence (test 4).
//!
//! The conclusion is conditional: the review fails closed **because it checks that each spend belongs to
//! this viewing key**. Without that check the outputs would be mislabelled in the dangerous direction.

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use orchard::note::{NoteVersion, RandomSeed, Rho};
use orchard::tree::{MerkleHashOrchard, MerklePath};
use orchard::Note;
use pczt::roles::creator::Creator;
use pczt::roles::io_finalizer::IoFinalizer;
use pczt_signer_core::review::{review, ReviewError};
use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;
use zcash_primitives::transaction::builder::{BuildConfig, Builder, BundlePadding};
use zcash_primitives::transaction::fees::zip317;
use zcash_protocol::consensus::{Network, NetworkUpgrade, Parameters};
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;

const SPEND_VALUE: u64 = 1_000_000;
/// What the extension quietly pays itself.
const STOLEN: u64 = 100_000;
const CHANGE: u64 = 890_000;
const FEE: u64 = 10_000;

/// `ak` spliced over the `nk`/`rivk` implied by `sk`. The real construction, and the fake one — the only
/// difference between them is which `sk` supplies `nk` and `rivk`.
fn fvk_for(ak: &[u8; 32], sk: u8) -> FullViewingKey {
    let key = SpendingKey::from_bytes([sk; 32]).into_option().expect("valid sk");
    let mut bytes = FullViewingKey::from(&key).to_bytes();
    bytes[..32].copy_from_slice(ak);
    FullViewingKey::from_bytes(&bytes).expect("structurally valid")
}

fn build_pczt(ak: &[u8; 32], honest: &FullViewingKey, thief_address: &orchard::Address) -> pczt::Pczt {
    build_pczt_with_memo(ak, honest, thief_address, b"")
}

fn build_pczt_with_memo(ak: &[u8; 32], honest: &FullViewingKey, thief_address: &orchard::Address, memo: &[u8]) -> pczt::Pczt {
    let _ = ak;
    let network = Network::TestNetwork;
    let target_height = network.activation_height(NetworkUpgrade::Nu6_3).expect("NU6_3") + 1;

    let recipient = honest.address_at(0u32, Scope::External);
    let rho = Rho::from_bytes(&[1u8; 32]).unwrap();
    let rseed = (0u8..=255)
        .find_map(|byte| RandomSeed::from_bytes([byte; 32], &rho).into_option())
        .expect("a valid rseed");
    let note = Note::from_parts(recipient, orchard::value::NoteValue::from_raw(SPEND_VALUE), rho, rseed, NoteVersion::V3).unwrap();

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
                pad_to_minimum: Some(2),
            },
        },
    );
    builder.add_ironwood_spend::<zip317::FeeRule>(honest.clone(), note, merkle_path).expect("spend");
    // The output the user never meant to make, paid to an address the *fake* key considers its own.
    builder
        .add_ironwood_output::<zip317::FeeRule>(
            None,
            *thief_address,
            Zatoshis::const_from_u64(STOLEN),
            MemoBytes::from_bytes(memo).expect("memo fits"),
        )
        .expect("thief output");
    builder
        .add_ironwood_output::<zip317::FeeRule>(
            Some(honest.to_ovk(Scope::External)),
            recipient,
            Zatoshis::const_from_u64(CHANGE),
            MemoBytes::empty(),
        )
        .expect("change");

    let result = builder.build_for_pczt(UnwrapErr(SysRng), &zip317::FeeRule::standard()).expect("build");
    IoFinalizer::new(Creator::build_from_parts(result.pczt_parts).unwrap())
        .finalize_io(UnwrapErr(SysRng))
        .expect("finalize")
        .clone()
        .into()
}

#[test]
fn one_swap_moves_the_whole_address_space_to_the_attacker() {
    let ak = [0x11u8; 32];
    let honest = fvk_for(&ak, 7);
    let fake = fvk_for(&ak, 0x22);
    assert_eq!(honest.to_bytes()[..32], fake.to_bytes()[..32], "ak is identical");

    let thief = fake.address_at(0u32, Scope::External);
    assert!(honest.scope_for_address(&thief).is_none(), "the thief's address is not ours under the real key");
    assert_eq!(
        fake.scope_for_address(&thief),
        Some(Scope::External),
        "and it is ours under the fake one — this is the whole attack"
    );

    // The real receiving address is the mirror image: ours under the real key, foreign to the fake.
    let real = honest.address_at(0u32, Scope::External);
    assert!(honest.scope_for_address(&real).is_some());
    assert!(fake.scope_for_address(&real).is_none());
}

#[test]
fn the_spent_note_is_not_ours_under_the_fake_key_so_the_review_refuses() {
    // The dangerous direction would be "output to the thief classified as change". It cannot be reached,
    // because the note being spent is a real note of the real vault and its address is fixed.
    let ak = [0x11u8; 32];
    let honest = fvk_for(&ak, 7);
    let fake = fvk_for(&ak, 0x22);
    let pczt = build_pczt(&ak, &honest, &fake.address_at(0u32, Scope::External));

    // The honest key reviews it and reports the theft the way it should: as a payment.
    let view = review(&pczt, &honest.to_bytes()).expect("honest review");
    let stolen = view.outputs.iter().find(|o| o.value == STOLEN).expect("stolen output");
    assert!(!stolen.is_change, "honest key calls it a payment");
    assert_eq!(view.sent_total, STOLEN);
    assert_eq!(view.change_total, CHANGE);
    assert_eq!(view.fee, FEE);

    // The fake key cannot review it at all. The index is not pinned: the builder shuffles outputs.
    assert!(
        matches!(review(&pczt, &fake.to_bytes()), Err(ReviewError::SpendNotOurs { .. })),
        "{:?}",
        review(&pczt, &fake.to_bytes())
    );
}

#[test]
fn changing_only_nk_also_changes_dk_so_the_same_check_catches_it() {
    // `dk` is a PRF of `(rivk, ak, nk)`, so there is no way to keep the real addresses labelled ours while
    // presenting a different `nk`. That is why the spend check catches both variants, and why the nullifier
    // recomputation is a second net rather than the only one.
    let ak = [0x11u8; 32];
    let honest = fvk_for(&ak, 7);
    let swapped_nk = fvk_for(&ak, 0x33);
    assert_ne!(honest.to_bytes()[32..64], swapped_nk.to_bytes()[32..64], "nk differs");
    assert!(
        swapped_nk.scope_for_address(&honest.address_at(0u32, Scope::External)).is_none(),
        "the real address stops being ours as soon as nk changes"
    );

    let pczt = build_pczt(&ak, &honest, &swapped_nk.address_at(0u32, Scope::External));
    // The action index is not pinned: the builder shuffles outputs, so the real spend is not always in the
    // same slot.
    assert!(
        matches!(review(&pczt, &swapped_nk.to_bytes()), Err(ReviewError::SpendNotOurs { .. })),
        "{:?}",
        review(&pczt, &swapped_nk.to_bytes())
    );
}

#[test]
fn the_ak_check_and_a_self_computed_fingerprint_are_not_a_defence() {
    let ak = [0x11u8; 32];
    let honest = fvk_for(&ak, 7);
    let fake = fvk_for(&ak, 0x22);

    // The check the recovery screen performs today: only the first 32 bytes are compared.
    assert_eq!(honest.to_bytes()[..32], fake.to_bytes()[..32]);

    // The fingerprint is computed from whatever bytes it is given, so a fake key has a self-consistent
    // fingerprint. Only a fingerprint recorded *before* the attack would differ.
    let network = Network::TestNetwork;
    let honest_fp = vault_keyset::fingerprint_from_fvk(&honest.to_bytes(), network);
    let fake_fp = vault_keyset::fingerprint_from_fvk(&fake.to_bytes(), network);
    assert_ne!(honest_fp, fake_fp, "the fingerprints differ — which is why a stored one helps");
    assert_eq!(
        vault_keyset::fingerprint_from_fvk(&fake.to_bytes(), network),
        fake_fp,
        "but recomputing it from the fake key always agrees with itself"
    );
}

#[test]
fn a_text_memo_is_recovered_verbatim() {
    // The memo is encrypted to the recipient, but a signer that rebuilds the output note can also decrypt
    // it: it re-derives the ephemeral secret instead of needing either viewing key. This is the case the
    // extension needs, because "unverified" must not be shown as "no memo".
    let ak = [0x11u8; 32];
    let honest = fvk_for(&ak, 7);
    let fake = fvk_for(&ak, 0x22);
    let pczt = build_pczt_with_memo(&ak, &honest, &fake.address_at(0u32, Scope::External), b"invoice 7");

    let view = review(&pczt, &honest.to_bytes()).expect("review");
    let paid = view.outputs.iter().find(|o| o.value == STOLEN).expect("payment output");
    match &paid.memo {
        pczt_signer_core::review::MemoReview::Read(bytes) => {
            // Verbatim bytes as the sender wrote them: this review does not re-encode or strip ZIP-302
            // markers, so the caller sees exactly what the recipient will see.
            assert_eq!(bytes.len(), 512);
            assert_eq!(&bytes[..b"invoice 7".len()], b"invoice 7");
            assert!(bytes[b"invoice 7".len()..].iter().all(|b| *b == 0));
        }
        other => panic!("expected a recovered memo, got {other:?}"),
    }
}
