//! The BLE Noise channel: three-message XX handshake, then transport.
//!
//! These are the properties the extension and the phone rely on: both ends derive the same handshake
//! hash, a tampered or out-of-order message is refused, and a mismatched prologue fails rather than
//! silently producing a channel to the wrong context.

use frost_vault_core::noise::{generate_keypair, static_public_from, NoiseChannel, NoiseError, PROLOGUE};

fn handshake() -> (NoiseChannel, NoiseChannel, [u8; 32], [u8; 32]) {
    let (init_sk, init_pk) = generate_keypair().expect("initiator keys");
    let (resp_sk, resp_pk) = generate_keypair().expect("responder keys");
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).expect("initiator");
    let mut responder = NoiseChannel::responder(&resp_sk, PROLOGUE).expect("responder");

    let m1 = initiator.write_handshake(b"hello from the extension").expect("m1");
    let payload = responder.read_handshake(&m1).expect("read m1");
    assert_eq!(payload, b"hello from the extension");

    let m2 = responder.write_handshake(b"hello from the phone").expect("m2");
    let payload = initiator.read_handshake(&m2).expect("read m2");
    assert_eq!(payload, b"hello from the phone");

    let m3 = initiator.write_handshake(b"").expect("m3");
    responder.read_handshake(&m3).expect("read m3");

    (initiator, responder, init_pk, resp_pk)
}

#[test]
fn a_full_handshake_agrees_on_the_hash_and_the_peers_key() {
    let (initiator, responder, init_pk, resp_pk) = handshake();

    assert!(initiator.is_handshake_finished());
    assert!(responder.is_handshake_finished());
    assert_eq!(
        initiator.handshake_hash().expect("initiator hash"),
        responder.handshake_hash().expect("responder hash"),
        "both ends must derive the same handshake hash"
    );

    // Each side learns the other's static key from the handshake itself.
    assert_eq!(initiator.remote_static().expect("initiator sees peer"), resp_pk);
    assert_eq!(responder.remote_static().expect("responder sees peer"), init_pk);
}

#[test]
fn transport_messages_round_trip_in_both_directions() {
    let (mut initiator, mut responder, _, _) = handshake();

    let from_initiator = initiator.encrypt(b"commitments").expect("encrypt");
    assert_ne!(from_initiator, b"commitments".to_vec());
    assert_eq!(responder.decrypt(&from_initiator).expect("decrypt"), b"commitments");

    let from_responder = responder.encrypt(b"signature share").expect("encrypt");
    assert_eq!(initiator.decrypt(&from_responder).expect("decrypt"), b"signature share");

    // Nonces advance, so the same plaintext never produces the same ciphertext twice.
    let again = initiator.encrypt(b"commitments").expect("encrypt");
    assert_ne!(again, from_initiator);
    responder.decrypt(&again).expect("decrypt second");
}

#[test]
fn a_tampered_transport_message_kills_the_channel() {
    // Noise advances the nonce before verifying the tag, so one failed message leaves the two sides out
    // of step for good. The only correct behaviour is to refuse everything afterwards and handshake
    // again — a caller that kept sending would just produce messages the peer can never read.
    let (mut initiator, mut responder, _, _) = handshake();
    let mut ciphertext = initiator.encrypt(b"share").unwrap();
    let last = ciphertext.len() - 1;
    ciphertext[last] ^= 0x01;
    assert_eq!(responder.decrypt(&ciphertext), Err(NoiseError::Decrypt));
    assert!(responder.is_poisoned());

    // Everything after the failure is refused, including a well-formed message and a rekey.
    let good = initiator.encrypt(b"later").unwrap();
    assert_eq!(responder.decrypt(&good), Err(NoiseError::Poisoned));
    assert_eq!(responder.rekey(), Err(NoiseError::Poisoned));
    assert_eq!(responder.encrypt(b"x"), Err(NoiseError::Poisoned));

    // The other side is untouched, which is exactly why the host must tear the channel down rather than
    // keep one half alive.
    assert!(!initiator.is_poisoned());
}

#[test]
fn a_failed_handshake_kills_the_channel() {
    let (init_sk, _) = generate_keypair().unwrap();
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap();
    assert_eq!(initiator.read_handshake(&[0u8; 40]), Err(NoiseError::Handshake));
    assert!(initiator.is_poisoned());
    assert_eq!(initiator.write_handshake(b""), Err(NoiseError::Poisoned));
}

#[test]
fn the_first_handshake_message_carries_its_payload_in_the_clear() {
    // Noise has no key before the first DH, so message 1's payload is plaintext and unauthenticated.
    // Nothing security-relevant may ride there: the initiator's static key is only revealed and proved
    // in message 3. This test makes that property explicit rather than tribal knowledge.
    let (init_sk, _) = generate_keypair().unwrap();

    let empty = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap().write_handshake(b"").unwrap();
    // A 32-byte ephemeral public key and nothing else: no AEAD tag, because no key is set yet.
    assert_eq!(empty.len(), 32);

    let with_payload = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap().write_handshake(b"abc").unwrap();
    assert_eq!(with_payload.len(), 35, "payload is appended verbatim");
}

#[test]
fn tampering_with_the_first_message_fails_the_handshake() {
    // Message 1 is not authenticated on arrival, but its bytes are hashed into the transcript, so a
    // forged payload makes the peer's next message fail to open. The channel never establishes.
    let (init_sk, _) = generate_keypair().unwrap();
    let (resp_sk, _) = generate_keypair().unwrap();
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap();
    let mut responder = NoiseChannel::responder(&resp_sk, PROLOGUE).unwrap();

    let mut m1 = initiator.write_handshake(b"x").unwrap();
    let last = m1.len() - 1;
    m1[last] ^= 0x01;
    if responder.read_handshake(&m1).is_ok() {
        let m2 = responder.write_handshake(b"").unwrap();
        assert_eq!(initiator.read_handshake(&m2), Err(NoiseError::Handshake));
    }
}

#[test]
fn tampering_with_the_second_message_is_refused_immediately() {
    // Message 2 is encrypted and authenticated, so it fails at the point of arrival.
    let (init_sk, _) = generate_keypair().unwrap();
    let (resp_sk, _) = generate_keypair().unwrap();
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap();
    let mut responder = NoiseChannel::responder(&resp_sk, PROLOGUE).unwrap();

    let m1 = initiator.write_handshake(b"").unwrap();
    responder.read_handshake(&m1).unwrap();
    let mut m2 = responder.write_handshake(b"y").unwrap();
    let last = m2.len() - 1;
    m2[last] ^= 0x01;
    assert_eq!(initiator.read_handshake(&m2), Err(NoiseError::Handshake));
}

#[test]
fn a_different_prologue_does_not_produce_a_channel() {
    // Same keys, different prologue: the handshake must not complete, because a channel established
    // under one context must not be usable in another.
    let (init_sk, _) = generate_keypair().unwrap();
    let (resp_sk, _) = generate_keypair().unwrap();
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap();
    let mut responder = NoiseChannel::responder(&resp_sk, b"noir/something-else/v1").unwrap();

    let m1 = initiator.write_handshake(b"").unwrap();
    // The responder either rejects the first message or fails later; either way the handshake does not
    // reach a transport state that both ends accept.
    let outcome = responder.read_handshake(&m1);
    if outcome.is_ok() {
        let m2 = responder.write_handshake(b"").unwrap();
        assert!(initiator.read_handshake(&m2).is_err(), "mismatched prologue must fail");
    }
}

#[test]
fn nothing_is_accepted_before_the_handshake_finishes() {
    let (init_sk, _) = generate_keypair().unwrap();
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap();

    assert_eq!(initiator.handshake_hash(), Err(NoiseError::NotEstablished));
    assert_eq!(initiator.encrypt(b"x"), Err(NoiseError::NotEstablished));
    assert_eq!(initiator.decrypt(&[0u8; 32]), Err(NoiseError::NotEstablished));
    assert_eq!(initiator.rekey(), Err(NoiseError::NotEstablished));
}

#[test]
fn rekey_keeps_the_channel_working() {
    let (mut initiator, mut responder, _, _) = handshake();
    let first = initiator.encrypt(b"before").unwrap();
    assert_eq!(responder.decrypt(&first).unwrap(), b"before");

    initiator.rekey().unwrap();
    responder.rekey().unwrap();

    let second = initiator.encrypt(b"after").unwrap();
    assert_eq!(responder.decrypt(&second).unwrap(), b"after");
}

#[test]
fn the_static_public_key_matches_what_the_handshake_reports() {
    let (init_sk, init_pk) = generate_keypair().unwrap();
    assert_eq!(static_public_from(&init_sk), init_pk);
}

#[test]
fn an_oversized_payload_is_refused_before_allocating() {
    let (init_sk, _) = generate_keypair().unwrap();
    let mut initiator = NoiseChannel::initiator(&init_sk, PROLOGUE).unwrap();
    assert_eq!(
        initiator.write_handshake(&vec![0u8; frost_vault_core::noise::MAX_PLAINTEXT + 1]),
        Err(NoiseError::TooLarge)
    );
}
