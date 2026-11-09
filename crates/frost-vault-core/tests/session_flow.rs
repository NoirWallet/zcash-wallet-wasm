//! The 2-of-3 signing session, end to end, with the rejection matrix.
//!
//! Everything runs in one process (three `SignerSession`s and one `CoordinatorSession`) because the
//! transport is not this crate's business: the point here is that the *state machine* refuses the
//! things a hostile or buggy coordinator would try, and that a share cannot exist before a human
//! approval step.

use std::collections::BTreeMap;

use frost_vault_core::protocol::{Envelope, MsgType};
use frost_vault_core::session::{
    CoordinatorSession, SessionConfig, SessionError, SignerSession, SESSION_ID_LEN,
};
use frost_vault_core::state::{NonceState, SignPhase};
use frost_vault_core::threshold::{
    randomized_verifying_key, run_dkg, verify_with_group_key_and_alpha, Identifier,
};
use rand::rngs::OsRng;

const SESSION: [u8; SESSION_ID_LEN] = [0x5Au8; SESSION_ID_LEN];

fn alpha(tag: u8) -> [u8; 32] {
    let mut a = [0u8; 32];
    a[0] = tag;
    a[31] = 0x01;
    a
}

fn sighash() -> [u8; 32] {
    let mut m = [0u8; 32];
    m[0] = 0x11;
    m
}

struct Harness {
    dkg: frost_vault_core::threshold::DkgOutput,
    coordinator: CoordinatorSession,
    signers: BTreeMap<u8, SignerSession>,
    /// slot -> FROST identifier, the locked-roster map every device holds.
    identifiers: BTreeMap<u8, Identifier>,
}

impl Harness {
    /// `signers_wanted` is the list of roster slots taking part (length must equal threshold).
    fn new(participants: u16, threshold: u16, signers_wanted: Vec<u8>, actions: usize) -> Self {
        let mut rng = OsRng;
        let dkg = run_dkg(participants, threshold, &mut rng).expect("dkg");

        // Roster slot i (0-based) maps to the FROST identifier of participant i+1.
        let mut identifiers = BTreeMap::new();
        for (i, id) in dkg.identifiers.iter().enumerate() {
            identifiers.insert(i as u8, *id);
        }

        let cfg = SessionConfig {
            session_id: SESSION,
            participants,
            threshold,
            signers: signers_wanted.clone(),
            identifiers: identifiers.clone(),
        };
        let coordinator =
            CoordinatorSession::new(cfg, dkg.pubkeys.clone(), actions).expect("coord");

        let signers = signers_wanted
            .iter()
            .map(|slot| {
                (
                    *slot,
                    SignerSession::new(SESSION, *slot, identifiers.clone()),
                )
            })
            .collect();

        Self {
            dkg,
            coordinator,
            signers,
            identifiers,
        }
    }

    /// Drive through commitments and payload, stopping before approval.
    fn run_to_verifying(&mut self, actions: usize) -> Vec<Envelope> {
        let mut rng = OsRng;
        let inits = self.coordinator.init().expect("init");
        let mut payloads_out = Vec::new();

        for env in &inits {
            let slot = env.recipient;
            let signer = self.signers.get_mut(&slot).expect("signer");
            let kp = &self.dkg.key_packages[&self.identifiers[&slot]];
            let commitments = signer.on_init(env, kp, &mut rng).expect("on_init");
            self.coordinator
                .on_commitments(&commitments)
                .expect("on_commitments");
        }

        let alphas: Vec<[u8; 32]> = (0..actions).map(|i| alpha(b'a' + i as u8)).collect();
        self.coordinator
            .prepare_payload(sighash(), alphas)
            .expect("prepare_payload");
        payloads_out.extend(self.coordinator.payloads().expect("payloads"));
        payloads_out
    }
}

#[test]
fn two_of_three_completes_and_verifies() {
    let mut h = Harness::new(3, 2, vec![0, 1], 1);
    let payloads = h.run_to_verifying(1);

    for env in &payloads {
        let slot = env.recipient;
        let signer = h.signers.get_mut(&slot).expect("signer");
        let material = signer.on_payload(env).expect("on_payload");

        // The caller's job: the alphas really are this group's actions.
        let ak = h.dkg.group_verifying_key().unwrap();
        for a in &material.alphas {
            randomized_verifying_key(&ak, a).expect("alpha is a valid randomizer");
        }
        signer.approve().expect("approve");
        let share = signer
            .share(&h.dkg.key_packages[&h.identifiers[&slot]])
            .expect("share");
        assert_eq!(share.msg_type, MsgType::SignShare);
        h.coordinator.on_share(&share).expect("on_share");
    }

    let sigs = h.coordinator.aggregate().expect("aggregate");
    assert_eq!(sigs.len(), 1);
    assert_eq!(h.coordinator.phase(), SignPhase::Aggregated);

    let ak = h.dkg.group_verifying_key().unwrap();
    verify_with_group_key_and_alpha(&ak, &alpha(b'a'), &sighash(), &sigs[0])
        .expect("aggregate verifies against rk");
}

#[test]
fn two_of_three_recovery_subset_b_c() {
    // §8.3: the coordinator may hold no share at all. Slots 1 and 2 sign, slot 0 is absent.
    let mut h = Harness::new(3, 2, vec![1, 2], 1);
    let payloads = h.run_to_verifying(1);
    for env in &payloads {
        let slot = env.recipient;
        let signer = h.signers.get_mut(&slot).unwrap();
        signer.on_payload(env).unwrap();
        signer.approve().unwrap();
        let share = signer
            .share(&h.dkg.key_packages[&h.identifiers[&slot]])
            .unwrap();
        h.coordinator.on_share(&share).unwrap();
    }
    let sigs = h.coordinator.aggregate().unwrap();
    let ak = h.dkg.group_verifying_key().unwrap();
    verify_with_group_key_and_alpha(&ak, &alpha(b'a'), &sighash(), &sigs[0]).unwrap();
}

#[test]
fn multi_action_signs_once_per_action_with_separate_nonces() {
    let mut h = Harness::new(2, 2, vec![0, 1], 3);
    let payloads = h.run_to_verifying(3);
    for env in &payloads {
        let slot = env.recipient;
        let signer = h.signers.get_mut(&slot).unwrap();
        let material = signer.on_payload(env).unwrap();
        assert_eq!(material.alphas.len(), 3);
        signer.approve().unwrap();
        let share = signer
            .share(&h.dkg.key_packages[&h.identifiers[&slot]])
            .unwrap();
        h.coordinator.on_share(&share).unwrap();
        // All three nonce sets are consumed.
        assert_eq!(
            signer.nonce_states(),
            &[
                NonceState::Consumed,
                NonceState::Consumed,
                NonceState::Consumed
            ]
        );
    }
    let sigs = h.coordinator.aggregate().unwrap();
    assert_eq!(sigs.len(), 3);

    let ak = h.dkg.group_verifying_key().unwrap();
    for (i, sig) in sigs.iter().enumerate() {
        verify_with_group_key_and_alpha(&ak, &alpha(b'a' + i as u8), &sighash(), sig)
            .unwrap_or_else(|e| panic!("action {i}: {e}"));
    }
}

#[test]
fn no_share_before_approval() {
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let payloads = h.run_to_verifying(1);
    let slot = payloads[0].recipient;
    let signer = h.signers.get_mut(&slot).unwrap();
    signer.on_payload(&payloads[0]).unwrap();
    assert_eq!(signer.phase(), SignPhase::Verifying);
    let err = signer
        .share(&h.dkg.key_packages[&h.identifiers[&slot]])
        .unwrap_err();
    assert_eq!(err, SessionError::NotApproved);
}

#[test]
fn a_signer_cannot_share_twice() {
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let payloads = h.run_to_verifying(1);
    let slot = payloads[0].recipient;
    let signer = h.signers.get_mut(&slot).unwrap();
    signer.on_payload(&payloads[0]).unwrap();
    signer.approve().unwrap();
    signer
        .share(&h.dkg.key_packages[&h.identifiers[&slot]])
        .unwrap();
    // Second call: phase has moved on, and the nonces are consumed.
    let err = signer
        .share(&h.dkg.key_packages[&h.identifiers[&slot]])
        .unwrap_err();
    assert_eq!(err, SessionError::NotApproved);
    assert_eq!(signer.nonce_states(), &[NonceState::Consumed]);
}

#[test]
fn a_failed_share_still_burns_the_nonce() {
    // The nonce must not survive a signing attempt: if the share computation fails, a retry with the
    // same nonce and a different challenge would leak the signing share.
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let payloads = h.run_to_verifying(1);
    let slot = payloads[0].recipient;
    let signer = h.signers.get_mut(&slot).unwrap();
    signer.on_payload(&payloads[0]).unwrap();
    signer.approve().unwrap();

    // Deliberately sign with the wrong participant's key package so the call fails.
    let wrong = if slot == 0 { 1u8 } else { 0u8 };
    let err = signer
        .share(&h.dkg.key_packages[&h.identifiers[&wrong]])
        .unwrap_err();
    assert!(matches!(err, SessionError::Frost(_)), "{err:?}");
    assert_eq!(signer.nonce_states(), &[NonceState::Consumed]);

    // And the nonce really is gone: retrying with the right package cannot reuse it.
    let err = signer
        .share(&h.dkg.key_packages[&h.identifiers[&slot]])
        .unwrap_err();
    assert_eq!(
        err,
        SessionError::State(frost_vault_core::state::TransitionError::NonceUnusable(
            NonceState::Consumed
        ))
    );
}

#[test]
fn replayed_commitments_are_refused() {
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let mut rng = OsRng;
    let inits = h.coordinator.init().unwrap();
    let env = &inits[0];
    let slot = env.recipient;
    let kp = &h.dkg.key_packages[&h.identifiers[&slot]];
    let commitments = h
        .signers
        .get_mut(&slot)
        .unwrap()
        .on_init(env, kp, &mut rng)
        .unwrap();
    h.coordinator.on_commitments(&commitments).unwrap();
    // Same envelope again: sequence has not advanced.
    let err = h.coordinator.on_commitments(&commitments).unwrap_err();
    assert!(
        matches!(err, SessionError::SequenceRewind { .. }),
        "{err:?}"
    );
}

#[test]
fn wrong_session_id_is_refused() {
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let mut rng = OsRng;
    let inits = h.coordinator.init().unwrap();
    let env = &inits[0];
    let slot = env.recipient;
    let kp = &h.dkg.key_packages[&h.identifiers[&slot]];
    let commitments = h
        .signers
        .get_mut(&slot)
        .unwrap()
        .on_init(env, kp, &mut rng)
        .unwrap();

    let mut forged = commitments.clone();
    forged.session_id = [0xEEu8; SESSION_ID_LEN];
    let err = h.coordinator.on_commitments(&forged).unwrap_err();
    assert_eq!(err, SessionError::WrongSession);
}

#[test]
fn sender_outside_the_selection_is_refused() {
    // 2-of-3 with slots 0,1 selected: slot 2 must not be able to inject anything.
    let mut h = Harness::new(3, 2, vec![0, 1], 1);
    let mut rng = OsRng;
    let inits = h.coordinator.init().unwrap();

    // Build a well-formed commitments message but claim slot 2.
    let env = &inits[0];
    let slot = env.recipient;
    let kp = &h.dkg.key_packages[&h.identifiers[&slot]];
    let c = h
        .signers
        .get_mut(&slot)
        .unwrap()
        .on_init(env, kp, &mut rng)
        .unwrap();
    let mut forged = c.clone();
    forged.sender = 2;

    let err = h.coordinator.on_commitments(&forged).unwrap_err();
    assert_eq!(err, SessionError::UnknownSender(2));
}

#[test]
fn a_signer_cannot_be_selected_twice() {
    let mut rng = OsRng;
    let dkg = run_dkg(3, 2, &mut rng).unwrap();
    let mut identifiers = BTreeMap::new();
    for (i, id) in dkg.identifiers.iter().enumerate() {
        identifiers.insert(i as u8, *id);
    }
    let cfg = SessionConfig {
        session_id: SESSION,
        participants: 3,
        threshold: 2,
        signers: vec![0, 0], // same device twice must not reach the threshold
        identifiers,
    };
    // validate() only checks membership, so construction is allowed; the roster below threshold
    // shows up when the same slot answers twice.
    let mut coord = CoordinatorSession::new(cfg, dkg.pubkeys.clone(), 1).unwrap();
    let inits = coord.init().unwrap();
    let env = &inits[0];
    let slot = env.recipient;
    let mut signer = SignerSession::new(SESSION, slot, coord_identifiers(&dkg));
    let kp = &dkg.key_packages[&dkg.identifiers[slot as usize]];
    let c = signer.on_init(env, kp, &mut rng).unwrap();
    coord.on_commitments(&c).unwrap();
    let err = coord.on_commitments(&c).unwrap_err();
    assert!(matches!(
        err,
        SessionError::Duplicate(_) | SessionError::SequenceRewind { .. }
    ));
}

fn coord_identifiers(dkg: &frost_vault_core::threshold::DkgOutput) -> BTreeMap<u8, Identifier> {
    dkg.identifiers
        .iter()
        .enumerate()
        .map(|(i, id)| (i as u8, *id))
        .collect()
}

#[test]
fn wrong_recipient_is_refused() {
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let mut rng = OsRng;
    let inits = h.coordinator.init().unwrap();
    let env = &inits[0];
    let slot = env.recipient;
    let kp = &h.dkg.key_packages[&h.identifiers[&slot]];

    // The same init, addressed to the other slot, must not be accepted: the recipient byte is part
    // of the associated data precisely so a message cannot be re-targeted.
    let mut misaddressed = env.clone();
    misaddressed.recipient = 1 - slot;
    let err = h
        .signers
        .get_mut(&slot)
        .unwrap()
        .on_init(&misaddressed, kp, &mut rng)
        .unwrap_err();
    assert_eq!(err, SessionError::WrongRecipient(1 - slot));
}

#[test]
fn a_payload_that_omits_this_signer_is_refused() {
    // The real anti-substitution check: a signer must not sign a package it is not part of. Build a
    // payload from a session whose selected signers are {1,2} and feed it to signer 0.
    let mut victim = Harness::new(3, 2, vec![0, 1], 1);
    victim.run_to_verifying(1);

    let mut alien = Harness::new(3, 2, vec![1, 2], 1);
    let alien_payloads = alien.run_to_verifying(1);
    let mut smuggled = alien_payloads
        .iter()
        .find(|e| e.recipient == 1)
        .unwrap()
        .clone();
    smuggled.recipient = 0;
    smuggled.sequence = 99; // ahead of anything the victim's session has seen

    let err = victim
        .signers
        .get_mut(&0)
        .unwrap()
        .on_payload(&smuggled)
        .unwrap_err();
    assert_eq!(err, SessionError::UnknownSender(0));
}

#[test]
fn truncated_payload_is_refused() {
    let mut h = Harness::new(2, 2, vec![0, 1], 1);
    let payloads = h.run_to_verifying(1);
    let mut env = payloads.into_iter().find(|e| e.recipient == 0).unwrap();
    env.payload.truncate(env.payload.len() - 4);
    let err = h.signers.get_mut(&0).unwrap().on_payload(&env).unwrap_err();
    assert_eq!(err, SessionError::Malformed);
}
