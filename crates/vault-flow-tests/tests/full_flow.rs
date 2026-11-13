//! One transaction, every tier: DKG → keyset → build → prove → strip → session → sign → apply.
//!
//! Each crate is tested on its own elsewhere. This is the only place the four of them run against a
//! single transaction, which is where the interesting failures live: a sighash computed one way by
//! the coordinator and another by the signer, an `alpha` that no longer matches the action, or a
//! signature that verifies in isolation but not against the PCZT that will be broadcast.

use std::collections::BTreeMap;

use frost_vault_core::protocol::COORDINATOR_SLOT;
use frost_vault_core::session::{CoordinatorSession, SessionConfig, SessionError, SignerSession, SESSION_ID_LEN};
use frost_vault_core::state::SignPhase;
use frost_vault_core::threshold::{run_dkg, Identifier};
use orchard::ValuePool;
use pczt::Pczt;
use pczt_signer_core::apply::apply_material;
use pczt_signer_core::gate::{check, PcztShape};
use pczt_signer_core::material::{bind_alphas, ironwood_action_rks};
use pczt_signer_core::sighash::verify_shielded_sighash;
use rand::{rand_core::UnwrapErr, rngs::SysRng};
use rand08::rngs::OsRng as FrostRng;
use vault_keyset::VaultKeyset;
use vault_pczt::{prepare, prove_ironwood, Prepared};
use zcash_protocol::consensus::Network;

const SESSION: [u8; SESSION_ID_LEN] = [0xC7u8; SESSION_ID_LEN];

fn qsk() -> [u8; 32] {
    let mut k = [0u8; 32];
    k[0] = 0x42;
    k[31] = 0x01;
    k
}

/// Build, prove and strip one vault transaction to its own address.
fn coordinator_side(participants: u16, threshold: u16) -> (DkgOut, VaultKeyset, Prepared) {
    let dkg = run_dkg(participants, threshold, &mut FrostRng).expect("dkg");
    let ak = dkg.group_verifying_key().expect("ak");
    let keyset = VaultKeyset::derive(&ak, &qsk(), Network::TestNetwork, 0).expect("keyset");

    let unproven = vault_pczt::fixture::ironwood_pczt(&keyset.fvk, ValuePool::Ironwood, 1_000_000, 990_000, UnwrapErr(SysRng)).expect("build");
    let proven = prove_ironwood(unproven, UnwrapErr(SysRng)).expect("prove");
    let prepared = prepare(proven).expect("prepare");

    (
        DkgOut {
            key_packages: dkg.key_packages,
            pubkeys: dkg.pubkeys,
            identifiers: dkg.identifiers,
        },
        keyset,
        prepared,
    )
}

struct DkgOut {
    key_packages: BTreeMap<Identifier, frost_vault_core::threshold::KeyPackage>,
    pubkeys: frost_vault_core::threshold::PublicKeyPackage,
    identifiers: Vec<Identifier>,
}

fn roster(identifiers: &[Identifier]) -> BTreeMap<u8, Identifier> {
    identifiers.iter().enumerate().map(|(i, id)| (i as u8, *id)).collect()
}

#[test]
fn a_full_vault_transaction_round_trips_through_every_tier() {
    let (dkg, keyset, prepared) = coordinator_side(3, 2);
    let ak = &keyset.group_verifying_key;
    let by_slot = roster(&dkg.identifiers);

    // --- What the signer receives: bytes that crossed a wire. ---
    let transport = Pczt::parse(&prepared.transport).expect("transport parses");

    // 1. Gate: V6, Ironwood only.
    let shape = PcztShape::of(&transport);
    check(&shape).expect("gate admits the transport copy");
    assert_eq!(shape.ironwood_actions, 1);
    assert_eq!(shape.orchard_actions, 0);

    // 2. The signer independently recomputes the sighash and compares it to the coordinator's.
    let sighash = verify_shielded_sighash(transport.clone(), &prepared.sighash).expect("signer recomputes the same sighash");

    // 3. And checks every action's rk against the group key and the supplied alpha.
    let material = bind_alphas(&ironwood_action_rks(&transport).expect("rks"), &prepared.alphas).expect("bind");
    assert_eq!(material.len(), 1);
    material[0].verify_rk(ak).expect("rk is the group key randomized by alpha");

    // --- Session: 2-of-3, slots 0 and 1. ---
    let config = SessionConfig {
        session_id: SESSION,
        participants: 3,
        threshold: 2,
        signers: vec![0, 1],
        identifiers: by_slot.clone(),
    };
    let mut coordinator = CoordinatorSession::new(config, dkg.pubkeys.clone(), 1).expect("coordinator");
    let mut signers: BTreeMap<u8, SignerSession> = [0u8, 1]
        .iter()
        .map(|slot| (*slot, SignerSession::new(SESSION, *slot, by_slot.clone())))
        .collect();

    let mut rng = FrostRng;
    for env in coordinator.init().expect("init") {
        assert_eq!(env.sender, COORDINATOR_SLOT);
        let slot = env.recipient;
        let signer = signers.get_mut(&slot).expect("signer");
        let commitments = signer.on_init(&env, &dkg.key_packages[&by_slot[&slot]], &mut rng).expect("commitments");
        coordinator.on_commitments(&commitments).expect("collect");
    }

    coordinator.prepare_payload(sighash, prepared.alphas.clone()).expect("prepare payload");
    let payloads = coordinator.payloads().expect("payloads");

    for env in &payloads {
        let slot = env.recipient;
        let signer = signers.get_mut(&slot).expect("signer");
        let material = signer.on_payload(env).expect("payload");
        // The signer's view must match the coordinator's, or it is signing something else.
        assert_eq!(material.sighash, sighash);
        assert_eq!(material.alphas, prepared.alphas);

        signer.approve().expect("approve");
        let share = signer.share(&dkg.key_packages[&by_slot[&slot]]).expect("share");
        coordinator.on_share(&share).expect("collect share");
    }

    assert_eq!(coordinator.phase(), SignPhase::SharesCollected);
    let signatures = coordinator.aggregate().expect("aggregate");
    assert_eq!(signatures.len(), 1);

    // --- Back on the coordinator: land the aggregate on the proven copy. ---
    let signed = apply_material(prepared.kept, &material, &signatures).expect("apply");
    assert_eq!(
        signed.ironwood().actions()[material[0].action_index as usize].spend().spend_auth_sig(),
        &Some(signatures[0])
    );
    // The signed transaction is the proven one: stripping was transport-only.
    assert!(signed.ironwood().zkproof().is_some(), "proof must survive");
    // Applying a signature adds exactly one 64-byte spend auth signature per action; nothing else
    // about the transaction changes.
    assert_eq!(signed.clone().serialize().expect("ser").len(), prepared.proven_len + 64);
}

#[test]
fn a_doctored_alpha_is_caught_before_any_share_exists() {
    let (_dkg, keyset, prepared) = coordinator_side(2, 2);
    let ak = &keyset.group_verifying_key;
    let transport = Pczt::parse(&prepared.transport).expect("parse");

    // The coordinator swaps in an alpha the transaction was not built with.
    let mut alphas = prepared.alphas.clone();
    alphas[0][0] ^= 0xff;
    let material = bind_alphas(&ironwood_action_rks(&transport).unwrap(), &alphas).unwrap();
    assert_eq!(
        material[0].verify_rk(ak),
        Err(pczt_signer_core::material::MaterialError::RkMismatch { action_index: 0 })
    );
}

#[test]
fn a_payload_bound_to_another_transaction_is_caught_by_the_sighash_check() {
    let (_a, keyset, prepared_a) = coordinator_side(2, 2);
    let (_b, _keyset_b, prepared_b) = coordinator_side(2, 2);

    let transport = Pczt::parse(&prepared_a.transport).expect("parse");
    // The coordinator claims the digest of a different transaction.
    assert!(verify_shielded_sighash(transport, &prepared_b.sighash).is_err());

    // And the alphas of the other transaction must not match either.
    let material = bind_alphas(&ironwood_action_rks(&Pczt::parse(&prepared_a.transport).unwrap()).unwrap(), &prepared_b.alphas).unwrap();
    // Two independent transactions have unrelated randomizers, so rk cannot match.
    assert!(material[0].verify_rk(&keyset.group_verifying_key).is_err());
}

#[test]
fn a_signer_outside_the_selection_cannot_join_the_session() {
    let (dkg, _keyset, prepared) = coordinator_side(3, 2);
    let by_slot = roster(&dkg.identifiers);
    let config = SessionConfig {
        session_id: SESSION,
        participants: 3,
        threshold: 2,
        signers: vec![0, 1],
        identifiers: by_slot,
    };
    let mut coordinator = CoordinatorSession::new(config, dkg.pubkeys.clone(), 1).unwrap();
    let inits = coordinator.init().unwrap();

    // Slot 2 was not selected. The signer itself refuses first, because the init payload lists the
    // selection and slot 2 is not in it — so an unselected device cannot even produce commitments,
    // which is the stronger guarantee. The coordinator's own check is defence in depth.
    let mut rng = FrostRng;
    let mut signer2 = SignerSession::new(SESSION, 2, roster(&dkg.identifiers));
    let mut forged = inits[0].clone();
    forged.recipient = 2;
    forged.sequence = 50;
    let err = signer2.on_init(&forged, &dkg.key_packages[&dkg.identifiers[2]], &mut rng).unwrap_err();
    assert_eq!(err, SessionError::UnknownSender(2));
    let _ = prepared;
}

// ---------------------------------------------------------------------------
// The consensus check: finalize, extract, verify.
// ---------------------------------------------------------------------------

/// The strongest test in the workspace: the transaction that comes out the other end is handed to
/// `pczt`'s Transaction Extractor, which recomputes the binding signature and runs batch
/// verification of the Halo2 proof *and* every spend authorization signature.
///
/// Everything else asserts that intermediate values agree with each other. This asserts that a
/// Zcash node would accept the result.
#[test]
fn the_finalized_transaction_verifies_as_a_real_transaction() {
    let (dkg, keyset, prepared) = coordinator_side_with_ak(2, 2);
    let ak = &keyset.group_verifying_key;
    let by_slot = roster(&dkg.identifiers);

    let transport = Pczt::parse(&prepared.transport).expect("transport parses");
    let material = bind_alphas(&ironwood_action_rks(&transport).expect("rks"), &prepared.alphas).expect("bind");
    material[0].verify_rk(ak).expect("rk");

    // Sign with both participants, as the flow test does.
    let config = SessionConfig {
        session_id: SESSION,
        participants: 2,
        threshold: 2,
        signers: vec![0, 1],
        identifiers: by_slot.clone(),
    };
    let mut coordinator = CoordinatorSession::new(config, dkg.pubkeys.clone(), 1).expect("coordinator");
    let mut signers: BTreeMap<u8, SignerSession> = [0u8, 1]
        .iter()
        .map(|slot| (*slot, SignerSession::new(SESSION, *slot, by_slot.clone())))
        .collect();

    let mut rng = FrostRng;
    for env in coordinator.init().expect("init") {
        let slot = env.recipient;
        let commitments = signers
            .get_mut(&slot)
            .unwrap()
            .on_init(&env, &dkg.key_packages[&by_slot[&slot]], &mut rng)
            .expect("commitments");
        coordinator.on_commitments(&commitments).expect("collect");
    }
    coordinator.prepare_payload(prepared.sighash, prepared.alphas.clone()).expect("prepare");
    for env in coordinator.payloads().expect("payloads") {
        let slot = env.recipient;
        let signer = signers.get_mut(&slot).unwrap();
        signer.on_payload(&env).expect("payload");
        signer.approve().expect("approve");
        let share = signer.share(&dkg.key_packages[&by_slot[&slot]]).expect("share");
        coordinator.on_share(&share).expect("collect");
    }
    let signatures = coordinator.aggregate().expect("aggregate");

    let signed = apply_material(prepared.kept, &material, &signatures).expect("apply");

    // SpendFinalizer recomputes the binding signature over the now-authorized bundles.
    let finalized = pczt::roles::spend_finalizer::SpendFinalizer::new(signed)
        .finalize_spends()
        .expect("finalize spends");

    // Extracting verifies the proof and every spend auth signature; a bad share would fail here.
    let tx = pczt::roles::tx_extractor::TransactionExtractor::new(finalized)
        .extract(UnwrapErr(SysRng))
        .expect("extract and verify");

    let bundle_ref = tx.ironwood_bundle();
    let bundle = bundle_ref.as_ref().expect("ironwood bundle");
    assert_eq!(bundle.actions().len(), 1);
    let rk_bytes: [u8; 32] = bundle.actions()[material[0].action_index as usize].rk().into();
    assert_eq!(rk_bytes, material[0].rk, "the extracted transaction carries the rk we authorized");

    // The transaction round-trips through its own consensus encoding.
    let mut bytes = Vec::new();
    tx.write(&mut bytes).expect("serialize");
    assert!(bytes.len() > 1_000, "transaction is {} bytes", bytes.len());
    let parsed = zcash_primitives::transaction::Transaction::read(bytes.as_slice(), zcash_protocol::consensus::BranchId::Nu6_3)
        .expect("the extracted transaction parses back");
    let mut again = Vec::new();
    parsed.write(&mut again).expect("reserialize");
    assert_eq!(again, bytes);
}

fn coordinator_side_with_ak(participants: u16, threshold: u16) -> (DkgOut, VaultKeyset, Prepared) {
    coordinator_side(participants, threshold)
}
