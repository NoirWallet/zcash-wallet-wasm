//! The roster hash, pinned to the vectors the counterparty supplied.
//!
//! Those values were produced by an independent Python implementation and cross-checked on the phone;
//! a match here is a cross-implementation check rather than a self-consistency test. If our encoding
//! drifted by one byte anywhere — a length prefix, the domain tag, how a slot is written — every hash
//! below would change.

use frost_vault_core::roster::{RosterError, RosterManifest};

const GROUP_ID: &str = "11111111-2222-4333-8444-555555555555";
const CIPHERSUITE: &str = "frost-redpallas-blake2b-512";

/// `0x01 ‖ (0xA0+s)×32 ‖ (0xB0+s)×32`
fn identity(slot: u8) -> Vec<u8> {
    let mut v = vec![0x01u8];
    v.extend(std::iter::repeat(0xA0 + slot).take(32));
    v.extend(std::iter::repeat(0xB0 + slot).take(32));
    v
}

/// `(0xC0+s)×32`
fn transport(slot: u8) -> Vec<u8> {
    std::iter::repeat(0xC0 + slot).take(32).collect()
}

/// Under QR the transport key is 32 zero bytes: "no transport key", not a real point.
fn zero_transport() -> Vec<u8> {
    vec![0u8; 32]
}

fn build(slots: &[u8], threshold: u16, qr: bool) -> RosterManifest {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, threshold, slots.len() as u16).expect("manifest");
    for slot in slots {
        let t = if qr { zero_transport() } else { transport(*slot) };
        m.add_participant(*slot, &identity(*slot), &t).expect("participant");
    }
    m
}

#[test]
fn vector_a_three_devices_over_bluetooth() {
    let m = build(&[0, 1, 2], 2, false);
    let canonical = m.canonical_bytes().expect("canonical");
    assert_eq!(canonical.len(), 430, "canonical length");
    assert_eq!(m.hash_hex().expect("hex"), "d99931702edb7a7500ab2980522919f455459118f3c893f4836768ed0fe2d227");
    // The domain tag is concatenated raw, with no length prefix.
    assert_eq!(&canonical[..14], b"noir/roster/v1");
    // The slot is one raw byte inside its field, not the decimal text "0".
    let slot_field = [0u8, 0, 0, 1, 0u8];
    assert!(canonical.windows(5).any(|w| w == slot_field), "slot must be encoded as a single raw byte");
}

#[test]
fn vector_b_two_devices_over_qr() {
    // The transport keys here are 32 zero bytes: a low-order X25519 point, deliberately not validated.
    let m = build(&[0, 1], 2, true);
    let canonical = m.canonical_bytes().expect("canonical");
    assert_eq!(canonical.len(), 320, "canonical length");
    assert_eq!(m.hash_hex().expect("hex"), "6fda07956a4bec07c197095857ce4f83a97aa42cdbd37f922b51dc282aa0ac0a");
}

#[test]
fn insertion_order_does_not_change_the_hash() {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 3).expect("manifest");
    for slot in [2u8, 0, 1] {
        m.add_participant(slot, &identity(slot), &transport(slot)).expect("participant");
    }
    assert_eq!(m.hash_hex().unwrap(), "d99931702edb7a7500ab2980522919f455459118f3c893f4836768ed0fe2d227");
}

#[test]
fn a_single_flipped_byte_changes_the_hash() {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 3).expect("manifest");
    for slot in [0u8, 1, 2] {
        let mut id = identity(slot);
        if slot == 1 {
            let last = id.len() - 1;
            id[last] ^= 0x01;
        }
        m.add_participant(slot, &id, &transport(slot)).expect("participant");
    }
    assert_ne!(m.hash_hex().unwrap(), "d99931702edb7a7500ab2980522919f455459118f3c893f4836768ed0fe2d227");
}

#[test]
fn the_hash_is_not_taken_before_the_roster_is_complete() {
    // A hash over a partial roster would become the value a device compares against, so it is refused
    // rather than computed.
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 3).expect("manifest");
    m.add_participant(0, &identity(0), &transport(0)).unwrap();
    assert_eq!(m.hash(), Err(RosterError::Incomplete { declared: 3, added: 1 }));
}

#[test]
fn a_gapped_slot_set_is_refused() {
    // Slots must be exactly 0..participants. A gap is not reported as such: it is a count mismatch, and
    // `add_participant` already rejects slots at or above the declared count, so the two together make a
    // gapped roster unrepresentable rather than merely rejected.
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 3).expect("manifest");
    m.add_participant(0, &identity(0), &transport(0)).unwrap();
    m.add_participant(2, &identity(2), &transport(2)).unwrap();
    assert_eq!(m.hash(), Err(RosterError::Incomplete { declared: 3, added: 2 }));
}

#[test]
fn keys_are_checked_for_shape_not_for_curve_membership() {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 2).expect("manifest");
    // Identity key must be 65 bytes starting with 0x01.
    assert_eq!(m.add_participant(0, &[0x02u8; 65], &transport(0)), Err(RosterError::BadIdentityKey { len: 65 }));
    assert_eq!(m.add_participant(0, &[0x01u8; 64], &transport(0)), Err(RosterError::BadIdentityKey { len: 64 }));
    // Transport key must be 32 bytes; its contents are never validated, so zeros are fine.
    assert_eq!(m.add_participant(0, &identity(0), &[0u8; 31]), Err(RosterError::BadTransportKey { len: 31 }));
    m.add_participant(0, &identity(0), &[0u8; 32]).expect("zero transport key is legal");
}

#[test]
fn malformed_rosters_are_refused() {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 2).expect("manifest");
    m.add_participant(0, &identity(0), &transport(0)).unwrap();
    assert_eq!(m.add_participant(0, &identity(0), &transport(0)), Err(RosterError::DuplicateSlot(0)));
    assert_eq!(m.add_participant(2, &identity(2), &transport(2)), Err(RosterError::BadSlot(2)));
    assert!(RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 0, 2).is_err());
    assert!(RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 3, 2).is_err());
    assert!(RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, 2, 0).is_err());
}
