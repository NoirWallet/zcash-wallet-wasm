//! The envelope-level helpers the mobile client calls instead of assembling bytes itself.
//!
//! The numbers below are a cross-implementation contract: the phone encodes what the extension decodes, so
//! a code table that drifts is a silent misread. They are pinned here on purpose.

use frost_vault_core::protocol::{
    decode_sign_reject, decode_sign_result, encode_sign_reject, encode_sign_result, read_signing_envelope, signing_envelope, Envelope, RejectCode,
    ResultStatus, SigningError, COORDINATOR_SLOT, HEADER_LEN,
};

const SESSION: [u8; 16] = [0x5a; 16];
const PHONE: u8 = 2;
const TXID: [u8; 32] = [0x9c; 32];

#[test]
fn a_reject_round_trips_and_keeps_the_agreed_code_bytes() {
    // The four codes are the phone's mapping; if either side renumbers, this fails.
    for (code, byte) in [
        (RejectCode::UserDeclined, 1u8),
        (RejectCode::VerificationFailed, 2),
        (RejectCode::Malformed, 3),
        (RejectCode::Expired, 4),
    ] {
        let bytes = encode_sign_reject(SESSION, PHONE, COORDINATOR_SLOT, 1, code, "").expect("encode");
        let (envelope, decoded, note) = decode_sign_reject(&bytes).expect("decode");
        assert_eq!(decoded, code);
        assert_eq!(byte, decoded.to_byte(), "the wire value of {code:?}");
        assert!(note.is_empty(), "the phone leaves the note empty");
        assert_eq!(envelope.sender, PHONE);
        assert_eq!(envelope.session_id, SESSION);
        // A reject is a round-3 message even though it carries no share.
        assert_eq!(envelope.round, 3);
    }
}

#[test]
fn a_result_round_trips_with_and_without_a_txid() {
    let bytes = encode_sign_result(SESSION, COORDINATOR_SLOT, PHONE, 2, ResultStatus::Broadcasted, Some(TXID), "").expect("encode");
    let (envelope, status, txid, _) = decode_sign_result(&bytes).expect("decode");
    assert_eq!(status, ResultStatus::Broadcasted);
    assert_eq!(txid, Some(TXID));
    assert_eq!(envelope.round, 3);

    let bytes = encode_sign_result(SESSION, COORDINATOR_SLOT, PHONE, 3, ResultStatus::Failed, None, "aggregation failed").expect("encode");
    let (_, status, txid, note) = decode_sign_result(&bytes).expect("decode");
    assert_eq!(status, ResultStatus::Failed);
    assert_eq!(txid, None);
    assert_eq!(note, "aggregation failed");
}

#[test]
fn a_result_whose_shape_disagrees_with_its_status_is_refused() {
    // A broadcast result without a transaction id is not "broadcast with no txid", it is unreadable.
    assert!(encode_sign_result(SESSION, COORDINATOR_SLOT, PHONE, 1, ResultStatus::Broadcasted, None, "").is_err());
    assert!(encode_sign_result(SESSION, COORDINATOR_SLOT, PHONE, 1, ResultStatus::Failed, Some(TXID), "").is_err());
}

#[test]
fn the_round_table_matches_the_spec() {
    // Four of these were wrong here until a counterparty read the table and compared it with the spec's
    // message list. Pinning every row means the next drift is a failing test rather than a peer refusing
    // a message in the middle of a ceremony.
    use frost_vault_core::protocol::MsgType::*;
    for (msg_type, round) in [
        (PairInvite, 0u8),
        (PairResponse, 0),
        (RosterLock, 0),
        (DkgR1, 1),
        (DkgR1Bundle, 1),
        (DkgR1Echo, 1),
        (DkgR1Cert, 1),
        (DkgR2Batch, 2),
        (DkgR2Inbox, 2),
        (KeysetSetup, 3),
        (DkgFinal, 3),
        (DkgActivate, 3),
        (SignInit, 1),
        (SignCommitments, 1),
        (SignPayload, 3),
        (SignShare, 3),
        (SignReject, 3),
        (SignResult, 3),
        (SessionAbort, 3),
    ] {
        assert_eq!(msg_type.round(), round, "{msg_type:?}");
    }
}

#[test]
fn the_round_comes_from_the_message_type_and_cannot_be_passed_in_wrong() {
    // There is no `round` argument anywhere in the signing constructors, so the table cannot be violated
    // by a caller. This test documents the resulting values.
    for (msg_type, round) in [
        (frost_vault_core::protocol::MsgType::SignInit, 1u8),
        (frost_vault_core::protocol::MsgType::SignCommitments, 1),
        (frost_vault_core::protocol::MsgType::SignPayload, 3),
        (frost_vault_core::protocol::MsgType::SignShare, 3),
        (frost_vault_core::protocol::MsgType::SignReject, 3),
        (frost_vault_core::protocol::MsgType::SignResult, 3),
    ] {
        let bytes = signing_envelope(msg_type, SESSION, PHONE, COORDINATOR_SLOT, 1, Vec::new()).expect("encode");
        let envelope = read_signing_envelope(&bytes).expect("read");
        assert_eq!(envelope.round, round, "{msg_type:?}");
    }
}

#[test]
fn an_envelope_with_a_hand_set_round_is_refused_on_read() {
    // A peer that fabricates the header is caught here, before any session state sees it.
    let bytes = Envelope::new(
        frost_vault_core::protocol::MsgType::SignShare,
        4, // the phone's old convention
        PHONE,
        COORDINATOR_SLOT,
        1,
        SESSION,
        b"share".to_vec(),
    )
    .expect("envelope")
    .encode();
    assert!(matches!(
        read_signing_envelope(&bytes),
        Err(SigningError::WrongRound { expected: 3, got: 4, .. })
    ));
}

#[test]
fn types_outside_the_signing_set_are_refused() {
    use frost_vault_core::protocol::MsgType;
    assert!(matches!(
        signing_envelope(MsgType::DkgR1, SESSION, PHONE, COORDINATOR_SLOT, 1, Vec::new()),
        Err(SigningError::NotSigning(MsgType::DkgR1))
    ));
    // And a DKG envelope read through the signing path is refused too.
    let bytes = signing_envelope_dkg_bytes();
    assert!(matches!(read_signing_envelope(&bytes), Err(SigningError::NotSigning(_))));
}

fn signing_envelope_dkg_bytes() -> Vec<u8> {
    use frost_vault_core::protocol::MsgType;
    Envelope::new(MsgType::DkgR1, 1, PHONE, COORDINATOR_SLOT, 1, SESSION, b"commitments".to_vec())
        .expect("envelope")
        .encode()
}

#[test]
fn a_wrong_type_is_not_accepted_by_the_wrong_reader() {
    // `decode_sign_result` must not accept a reject just because both are round-3 signing messages.
    let bytes = encode_sign_reject(SESSION, PHONE, COORDINATOR_SLOT, 1, RejectCode::UserDeclined, "").expect("encode");
    assert!(decode_sign_result(&bytes).is_err());

    let bytes = encode_sign_result(SESSION, COORDINATOR_SLOT, PHONE, 1, ResultStatus::Broadcasted, Some(TXID), "").expect("encode");
    assert!(decode_sign_reject(&bytes).is_err());
}

#[test]
fn trailing_bytes_after_the_payload_are_refused() {
    // The header declares a length, so a padded payload would otherwise be a place to hide data.
    let mut bytes = encode_sign_reject(SESSION, PHONE, COORDINATOR_SLOT, 1, RejectCode::UserDeclined, "").expect("encode");
    assert_eq!(bytes[HEADER_LEN..], [1u8, 0, 0][..]);
    bytes.push(0xAA);
    // The declared length no longer matches the bytes present.
    assert!(read_signing_envelope(&bytes).is_err());
}
