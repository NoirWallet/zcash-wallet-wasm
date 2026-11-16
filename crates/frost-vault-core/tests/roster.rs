//! The roster hash, pinned to the vectors agreed with the mobile client.
//!
//! These values came from an independent Python implementation, so a match here is a real
//! cross-implementation check rather than a self-consistency test: if our encoding drifted by one byte
//! anywhere — a length prefix, the domain tag, the network field — every hash below would change.

use frost_vault_core::roster::{RosterError, RosterManifest};

const GROUP_ID: &str = "11111111-2222-4333-8444-555555555555";
const CIPHERSUITE: &str = "frost-redpallas-blake2b-512";

fn identity(slot: u8) -> Vec<u8> {
    let mut v = vec![0x01u8];
    v.extend(std::iter::repeat(0xA0 + slot).take(32));
    v.extend(std::iter::repeat(0xB0 + slot).take(32));
    v
}

fn transport(slot: u8) -> Vec<u8> {
    std::iter::repeat(0xC0 + slot).take(32).collect()
}

/// Under QR the transport key is a 32-byte zero placeholder, by agreement.
fn zero_transport() -> Vec<u8> {
    vec![0u8; 32]
}

fn build(slots: &[u8], threshold: u16, mainnet: bool, qr: bool) -> RosterManifest {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, mainnet, threshold, slots.len() as u16).expect("manifest");
    for slot in slots {
        let t = if qr { zero_transport() } else { transport(*slot) };
        m.add_participant(*slot, &identity(*slot), &t).expect("participant");
    }
    m
}

#[test]
fn the_agreed_vectors_match_byte_for_byte() {
    let cases: &[(&str, &[u8], u16, bool, bool, usize, &str)] = &[
        (
            "V1",
            &[0, 1],
            2,
            true,
            true,
            343,
            "6b326d514157bbf27699f60b224cdaa1041d475b8b18ed6f3e10b8de984bc4bb",
        ),
        (
            "V1'",
            &[0, 1],
            2,
            false,
            true,
            343,
            "0b317bda0d52df33aee8b0be9dd5134266b37e43ff251d0b247e6d05adea00bc",
        ),
        (
            "V2",
            &[0, 1, 2],
            2,
            true,
            false,
            453,
            "76567471919f13b7e39bb8f21a41dae67974f16a2b21beb582a7b01c8233a268",
        ),
        (
            "V2'",
            &[0, 1, 2],
            2,
            false,
            false,
            453,
            "8635f5c7479fc76860e542ef95faddda05ab89bcf393e5ace10e25065791506e",
        ),
        (
            "V3",
            &[0, 1, 2],
            3,
            true,
            false,
            453,
            "a6579d1ea9a2f855c8c76db441b96c0f2bb40da9b5c7cf50cfd6428c5b9621c9",
        ),
        (
            "V3'",
            &[0, 1, 2],
            3,
            false,
            false,
            453,
            "3604bdb4292232ab3ed814985694a24c4042e0229d36ff33f7cce6e608efd5f8",
        ),
    ];

    for (name, slots, threshold, mainnet, qr, expected_len, expected_hash) in cases {
        let m = build(slots, *threshold, *mainnet, *qr);
        let canonical = m.canonical_bytes().expect("canonical");
        assert_eq!(canonical.len(), *expected_len, "{name}: canonical length");
        assert_eq!(&m.hash_hex().expect("hex"), expected_hash, "{name}: hash");
        // The domain tag is concatenated raw, with no length prefix.
        assert_eq!(&canonical[..26], b"noir-frost-vault/roster/v1", "{name}");
    }
}

#[test]
fn insertion_order_does_not_change_the_hash() {
    // V2 built in the order 2, 0, 1 must equal V2.
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 2, 3).expect("manifest");
    for slot in [2u8, 0, 1] {
        m.add_participant(slot, &identity(slot), &transport(slot)).expect("participant");
    }
    assert_eq!(m.hash_hex().unwrap(), "76567471919f13b7e39bb8f21a41dae67974f16a2b21beb582a7b01c8233a268");
}

#[test]
fn a_single_flipped_byte_changes_the_hash() {
    // V2 with the last byte of slot 1's identity key flipped.
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 2, 3).expect("manifest");
    for slot in [0u8, 1, 2] {
        let mut id = identity(slot);
        if slot == 1 {
            let last = id.len() - 1;
            id[last] ^= 0x01;
        }
        m.add_participant(slot, &id, &transport(slot)).expect("participant");
    }
    assert_eq!(m.hash_hex().unwrap(), "eb99746dec9ef7edfafb576958bde91b2e15e0ae99ad58d14ea4c564147e953f");
}

#[test]
fn the_hash_is_not_taken_before_the_roster_is_complete() {
    // A hash over a partial roster would silently become the value a device compares against, so it is
    // refused rather than computed.
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 2, 3).expect("manifest");
    m.add_participant(0, &identity(0), &transport(0)).unwrap();
    assert_eq!(m.hash(), Err(RosterError::Incomplete { declared: 3, added: 1 }));
    assert!(m.canonical_bytes().is_err());
}

#[test]
fn malformed_rosters_are_refused() {
    let mut m = RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 2, 2).expect("manifest");
    m.add_participant(0, &identity(0), &transport(0)).unwrap();
    // Duplicate slot.
    assert_eq!(m.add_participant(0, &identity(0), &transport(0)), Err(RosterError::DuplicateSlot(0)));
    // Slot outside the roster.
    assert_eq!(m.add_participant(2, &identity(2), &transport(2)), Err(RosterError::BadSlot(2)));
    // Declared count already reached.
    m.add_participant(1, &identity(1), &transport(1)).unwrap();
    assert_eq!(m.add_participant(1, &identity(1), &transport(1)), Err(RosterError::DuplicateSlot(1)));
    // Threshold bounds.
    assert!(RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 0, 2).is_err());
    assert!(RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 3, 2).is_err());
    assert!(RosterManifest::new(GROUP_ID, 1, CIPHERSUITE, true, 2, 0).is_err());
}
