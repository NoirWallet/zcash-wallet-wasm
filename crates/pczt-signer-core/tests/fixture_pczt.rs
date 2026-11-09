//! Regression pin on real PCZT bytes.
//!
//! Committed fixture: one Ironwood action, one output, no proof (the Prover role is not part of the
//! signer tier). Its purpose is to exercise the parse → gate → material → sighash path on bytes that
//! came from the real builder, so a change in the wallet stack or in `pczt` shows up as a test
//! failure rather than as a surprise on a device.
//!
//! Regenerate with:
//!   cargo test --target x86_64-unknown-linux-gnu --test ironwood_e2e -- --ignored regen_fixture

use pczt::Pczt;
use pczt_signer_core::apply::{apply_all, ApplyError};
use pczt_signer_core::gate::{check, PcztShape, BRANCH_NU6_3, TX_VERSION_V6};
use pczt_signer_core::material::ironwood_action_rks;
use pczt_signer_core::sighash::shielded_sighash;

fn fixture_bytes() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ironwood_1action.hex");
    let hexed = std::fs::read_to_string(&path).expect("fixture present");
    hex::decode(hexed.trim()).expect("fixture is hex")
}

fn fixture() -> Pczt {
    Pczt::parse(&fixture_bytes()).expect("fixture parses")
}

#[test]
fn fixture_parses_and_passes_the_gate() {
    let pczt = fixture();
    let shape = PcztShape::of(&pczt);
    assert_eq!(shape.tx_version, TX_VERSION_V6);
    assert_eq!(shape.consensus_branch_id, BRANCH_NU6_3);
    assert_eq!(shape.ironwood_actions, 1);
    assert_eq!(shape.orchard_actions, 0);
    assert_eq!(shape.transparent_inputs, 0);
    assert_eq!(shape.transparent_outputs, 0);
    assert_eq!(shape.sapling_spends + shape.sapling_outputs, 0);
    check(&shape).expect("gate admits the fixture");
}

#[test]
fn fixture_yields_exactly_one_action_rk() {
    let pczt = fixture();
    let rks = ironwood_action_rks(&pczt).expect("rks");
    assert_eq!(rks.len(), 1);
    assert_eq!(rks[0].action_index, 0);
    assert_ne!(rks[0].rk, [0u8; 32], "rk must not be the placeholder");
}

#[test]
fn fixture_sighash_is_stable() {
    // A digest that changes without a format change means the code moved; a digest that stayed the
    // same while the bytes changed means the format is not being covered by the sighash.
    let sighash = shielded_sighash(fixture()).expect("sighash");
    assert_eq!(
        hex::encode(sighash),
        FIXTURE_SIGHASH,
        "sighash drifted: regenerate the fixture and update this constant deliberately"
    );
}

/// Pinned value; update only together with the fixture.
const FIXTURE_SIGHASH: &str = "4b61072777d3c452120cceaa472a2e82d28a0387b4dd49e17f06dd606f6d1f9c";

#[test]
fn a_signature_that_did_not_come_from_the_group_is_rejected() {
    // The Signer role verifies each contribution against the action's rk before storing it, so a
    // random 64-byte string cannot be smuggled into the transaction here.
    let err = apply_all(fixture(), &[(0, [0xABu8; 64])]).unwrap_err();
    assert_eq!(err, ApplyError::Rejected { action_index: 0 });
}

#[test]
fn an_out_of_range_action_is_rejected() {
    let err = apply_all(fixture(), &[(7, [0x01u8; 64])]).unwrap_err();
    assert_eq!(err, ApplyError::Rejected { action_index: 7 });
}

#[test]
fn duplicate_action_indices_are_refused_before_anything_is_applied() {
    let err = apply_all(fixture(), &[(0, [0x01u8; 64]), (0, [0x02u8; 64])]).unwrap_err();
    assert_eq!(err, ApplyError::Duplicate { action_index: 0 });
}
