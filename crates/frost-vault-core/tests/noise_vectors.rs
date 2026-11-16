//! Fixed interop vectors for the BLE Noise channel.
//!
//! `snow` normally draws ephemeral keys from the system RNG, which makes a handshake impossible to pin.
//! With the `test-vectors` feature the ephemeral is supplied by the caller, so the whole handshake becomes
//! a pure function of its inputs and two implementations can be compared byte for byte — which is what
//! the mobile client asked for.
//!
//! Run with `cargo test --features test-vectors -- --ignored --nocapture print_vectors` to regenerate.

#![cfg(feature = "test-vectors")]

use frost_vault_core::noise::{static_public_from, NoiseChannel, PROLOGUE};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const INIT_STATIC: [u8; 32] = [0x11; 32];
const RESP_STATIC: [u8; 32] = [0x22; 32];
const INIT_EPHEMERAL: [u8; 32] = [0x33; 32];
const RESP_EPHEMERAL: [u8; 32] = [0x44; 32];

struct Transcript {
    msg1: Vec<u8>,
    msg2: Vec<u8>,
    msg3: Vec<u8>,
    hash: [u8; 32],
    c_init: Vec<u8>,
    c_resp: Vec<u8>,
}

fn run() -> Transcript {
    let mut initiator = NoiseChannel::initiator_with_ephemeral(&INIT_STATIC, PROLOGUE, &INIT_EPHEMERAL).unwrap();
    let mut responder = NoiseChannel::responder_with_ephemeral(&RESP_STATIC, PROLOGUE, &RESP_EPHEMERAL).unwrap();

    let msg1 = initiator.write_handshake(b"").unwrap();
    responder.read_handshake(&msg1).unwrap();
    let msg2 = responder.write_handshake(b"").unwrap();
    initiator.read_handshake(&msg2).unwrap();
    let msg3 = initiator.write_handshake(b"").unwrap();
    responder.read_handshake(&msg3).unwrap();

    let hash_i = initiator.handshake_hash().unwrap();
    let hash_r = responder.handshake_hash().unwrap();
    assert_eq!(hash_i, hash_r);

    let c_init = initiator.encrypt(b"commitments").unwrap();
    let c_resp = responder.encrypt(b"share").unwrap();

    // The peer's static key must come out of the handshake itself.
    assert_eq!(initiator.remote_static().unwrap(), static_public_from(&RESP_STATIC));
    assert_eq!(responder.remote_static().unwrap(), static_public_from(&INIT_STATIC));

    Transcript {
        msg1,
        msg2,
        msg3,
        hash: hash_i,
        c_init,
        c_resp,
    }
}

#[test]
fn the_handshake_is_a_pure_function_of_its_inputs() {
    // Two runs with the same keys must produce identical bytes. Without this the vectors would be a
    // recording rather than a test.
    let a = run();
    let b = run();
    assert_eq!(a.msg1, b.msg1);
    assert_eq!(a.msg2, b.msg2);
    assert_eq!(a.msg3, b.msg3);
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.c_init, b.c_init);
    assert_eq!(a.c_resp, b.c_resp);

    // Changing one ephemeral changes everything downstream, which is what makes the vectors sensitive to
    // a pattern or prologue mismatch.
    let mut initiator = NoiseChannel::initiator_with_ephemeral(&INIT_STATIC, PROLOGUE, &[0x55; 32]).unwrap();
    let other = initiator.write_handshake(b"").unwrap();
    assert_ne!(other, a.msg1);
}

#[test]
#[ignore]
fn print_vectors() {
    let t = run();
    println!("pattern = Noise_XX_25519_ChaChaPoly_BLAKE2s");
    println!("prologue_utf8 = noir/pair/v1");
    println!("prologue_hex = {}", hex(PROLOGUE));
    println!("initiator_static_private = {}", hex(&INIT_STATIC));
    println!("initiator_static_public = {}", hex(&static_public_from(&INIT_STATIC)));
    println!("responder_static_private = {}", hex(&RESP_STATIC));
    println!("responder_static_public = {}", hex(&static_public_from(&RESP_STATIC)));
    println!("initiator_ephemeral_private = {}", hex(&INIT_EPHEMERAL));
    println!("initiator_ephemeral_public = {}", hex(&static_public_from(&INIT_EPHEMERAL)));
    println!("responder_ephemeral_private = {}", hex(&RESP_EPHEMERAL));
    println!("responder_ephemeral_public = {}", hex(&static_public_from(&RESP_EPHEMERAL)));
    println!("msg1 = {}", hex(&t.msg1));
    println!("msg2 = {}", hex(&t.msg2));
    println!("msg3 = {}", hex(&t.msg3));
    println!("handshake_hash = {}", hex(&t.hash));
    println!("plaintext_from_initiator = {}", hex(b"commitments"));
    println!("ciphertext_from_initiator = {}", hex(&t.c_init));
    println!("plaintext_from_responder = {}", hex(b"share"));
    println!("ciphertext_from_responder = {}", hex(&t.c_resp));
}
