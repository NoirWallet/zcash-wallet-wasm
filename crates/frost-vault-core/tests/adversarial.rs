//! Adversarial input: the decoders must refuse, never panic and never allocate on a claimed length.
//!
//! The frontend hands this wasm bytes from a camera. Most of those bytes are, at some point, wrong —
//! a partially scanned frame, another protocol's payload, a hostile phone. A panic in wasm is not a
//! crash the host can catch: it aborts the instance, and in the Extension case that can lose the
//! nonces that make the share safe to produce. So "returns Err" is a requirement, not a nicety.

use std::collections::BTreeMap;

use frost_vault_core::protocol::Envelope;
use frost_vault_core::session::{SessionError, SignerSession, MAX_ACTIONS};
use frost_vault_core::threshold::run_dkg;
use rand::rngs::OsRng;

/// Deterministic pseudo-random bytes: a failed case has to be reproducible.
fn stream(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn envelope_decode_never_panics_on_arbitrary_bytes() {
    for seed in 0..2_000u64 {
        let len = (seed % 128) as usize;
        let bytes = stream(seed, len);
        // Either parses or refuses; the only failure mode that matters is a panic.
        let _ = Envelope::decode(&bytes);
    }
    // And specifically on lengths that pass the header check.
    for seed in 0..500u64 {
        let mut bytes = stream(seed, 29 + (seed as usize % 64));
        bytes[0] = 1; // correct version
        bytes[1] = 0x20 + (seed as u8 % 6); // a known message type
        let _ = Envelope::decode(&bytes);
    }
}

#[test]
fn envelope_refuses_a_huge_declared_payload_without_allocating() {
    let mut bytes = vec![0u8; 29];
    bytes[0] = 1;
    bytes[1] = 0x21;
    bytes[3] = 0;
    bytes[4] = 0xFE;
    // Claim 1 GiB of payload in a 29-byte buffer.
    bytes[25..29].copy_from_slice(&(1u32 << 30).to_le_bytes());
    let err = Envelope::decode(&bytes).unwrap_err();
    assert!(
        matches!(
            err,
            frost_vault_core::protocol::EnvelopeError::PayloadTooLarge(_) | frost_vault_core::protocol::EnvelopeError::LengthMismatch { .. }
        ),
        "{err:?}"
    );
}

#[test]
fn the_signer_refuses_a_payload_claiming_a_thousand_actions() {
    // `MAX_ACTIONS` exists because the wire field is a u16: without a cap, a 3-byte payload could ask
    // the decoder to reserve for 65535 actions.
    assert_eq!(MAX_ACTIONS, 96);

    let mut rng = OsRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    let identifiers: BTreeMap<u8, _> = dkg.identifiers.iter().enumerate().map(|(i, id)| (i as u8, *id)).collect();
    let mut signer = SignerSession::new([1u8; 16], 0, identifiers);

    // Hand-build a SIGN_INIT claiming 1000 actions.
    let mut payload = Vec::new();
    payload.extend_from_slice(&1000u16.to_le_bytes());
    payload.extend_from_slice(&2u16.to_le_bytes()); // participants
    payload.extend_from_slice(&2u16.to_le_bytes()); // threshold
    payload.push(2); // two signers
    payload.extend_from_slice(&[0, 1]);

    let env = Envelope::new(
        frost_vault_core::protocol::MsgType::SignInit,
        1,
        frost_vault_core::protocol::COORDINATOR_SLOT,
        0,
        1,
        [1u8; 16],
        payload,
    )
    .unwrap();

    let err = signer.on_init(&env, &dkg.key_packages[&dkg.identifiers[0]], &mut rng).unwrap_err();
    assert!(matches!(err, SessionError::Counts { .. }), "expected a count refusal, got {err:?}");
}

#[test]
fn the_signer_refuses_a_commitment_grid_with_too_many_entries() {
    // A u8 could claim 255 commitments for a 2-of-3 group. Refuse before deserialising any of them.
    let mut rng = OsRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    let identifiers: BTreeMap<u8, _> = dkg.identifiers.iter().enumerate().map(|(i, id)| (i as u8, *id)).collect();
    let mut signer = SignerSession::new([1u8; 16], 0, identifiers);
    let key_package = dkg.key_packages[&dkg.identifiers[0]].clone();

    // A valid init first, so the session is in the right phase.
    let mut init_payload = Vec::new();
    init_payload.extend_from_slice(&1u16.to_le_bytes());
    init_payload.extend_from_slice(&2u16.to_le_bytes());
    init_payload.extend_from_slice(&2u16.to_le_bytes());
    init_payload.push(2);
    init_payload.extend_from_slice(&[0, 1]);
    let init = Envelope::new(
        frost_vault_core::protocol::MsgType::SignInit,
        1,
        frost_vault_core::protocol::COORDINATOR_SLOT,
        0,
        1,
        [1u8; 16],
        init_payload,
    )
    .unwrap();
    signer.on_init(&init, &key_package, &mut rng).expect("init");

    // SIGN_PAYLOAD with a grid claiming 200 entries.
    let mut payload = Vec::new();
    payload.extend_from_slice(&[9u8; 32]); // sighash
    payload.extend_from_slice(&1u16.to_le_bytes()); // one action
    payload.extend_from_slice(&[3u8; 32]); // alpha
    payload.push(200); // 200 commitments claimed
    let env = Envelope::new(
        frost_vault_core::protocol::MsgType::SignPayload,
        3,
        frost_vault_core::protocol::COORDINATOR_SLOT,
        0,
        2,
        [1u8; 16],
        payload,
    )
    .unwrap();

    let err = signer.on_payload(&env).unwrap_err();
    assert!(matches!(err, SessionError::UnknownSender(_)), "expected a slot refusal, got {err:?}");
}

#[test]
fn random_payloads_never_panic_the_signer_entry_points() {
    let mut rng = OsRng;
    let dkg = run_dkg(2, 2, &mut rng).expect("dkg");
    let identifiers: BTreeMap<u8, _> = dkg.identifiers.iter().enumerate().map(|(i, id)| (i as u8, *id)).collect();
    let key_package = dkg.key_packages[&dkg.identifiers[0]].clone();

    for seed in 0..500u64 {
        let mut signer = SignerSession::new([1u8; 16], 0, identifiers.clone());
        let payload = stream(seed, (seed as usize % 96) + 1);
        let env = Envelope::new(
            frost_vault_core::protocol::MsgType::SignInit,
            1,
            frost_vault_core::protocol::COORDINATOR_SLOT,
            0,
            1,
            [1u8; 16],
            payload,
        )
        .unwrap();
        // Must refuse (garbage is not a valid init), must not panic.
        assert!(signer.on_init(&env, &key_package, &mut rng).is_err());
    }
}
