//! Delegation REST contract and durable dispatch journal.
//!
//! Wire authority: valargroup/vote-sdk 990fb1c3fbdbda9f4f459a5259551580c157c5dd,
//! api/{handler,codec}.go and proto/svote/v1/tx.proto. The canonical protobuf is
//! prefixed with TagDelegateVote (0x02); CometBFT hashes these bytes with SHA256.
//! Vizor 3acdff6c0e65b2aa90ebc3f433168e56686bc358 independently uses these routes.
use base64::{engine::general_purpose::STANDARD, Engine};
use prost::Message;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    confirmation::TxEvent,
    delegate::DelegationSubmission,
    platform::*,
    storage::{queries, VotingDb},
    wire::DelegationSubmissionWire,
    VotingError,
};

fn invalid() -> VotingError {
    VotingError::InvalidInput {
        message: "invalid delegation recovery state".into(),
    }
}

// Exact MsgDelegateVote field mapping from the pinned proto (field 10 reserved).
#[derive(Clone, PartialEq, Message)]
struct MsgDelegateVote {
    #[prost(bytes = "vec", tag = "1")]
    rk: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    spend_auth_sig: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    signed_note_nullifier: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    cmx_new: Vec<u8>,
    #[prost(bytes = "vec", tag = "6")]
    van_cmx: Vec<u8>,
    #[prost(bytes = "vec", repeated, tag = "7")]
    gov_nullifiers: Vec<Vec<u8>>,
    #[prost(bytes = "vec", tag = "8")]
    proof: Vec<u8>,
    #[prost(bytes = "vec", tag = "9")]
    vote_round_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "11")]
    tx1_effects: Vec<u8>,
}

/// Sensitive canonical bytes remain inside Rust and the owner-bound voting DB.
#[derive(Clone)]
pub struct CanonicalDelegation {
    json: Vec<u8>,
    tx_hash: String,
    round_id: String,
}
impl CanonicalDelegation {
    pub fn from_submission(submission: &DelegationSubmission) -> Result<Self, VotingError> {
        Self::parse(submission.to_wire_json()?.into_bytes())
    }
    fn parse(json: Vec<u8>) -> Result<Self, VotingError> {
        let wire: DelegationSubmissionWire =
            serde_json::from_slice(&json).map_err(|_| invalid())?;
        if wire.to_json()?.as_bytes() != json {
            return Err(invalid());
        }
        let decode = |s: &str| STANDARD.decode(s).map_err(|_| invalid());
        let msg = MsgDelegateVote {
            rk: decode(&wire.rk)?,
            spend_auth_sig: decode(&wire.spend_auth_sig)?,
            signed_note_nullifier: decode(&wire.nf_signed)?,
            cmx_new: decode(&wire.cmx_new)?,
            van_cmx: decode(&wire.gov_comm)?,
            gov_nullifiers: wire
                .gov_nullifiers
                .iter()
                .map(|s| decode(s))
                .collect::<Result<_, _>>()?,
            proof: decode(&wire.proof)?,
            vote_round_id: decode(&wire.vote_round_id)?,
            tx1_effects: decode(&wire.tx1_effects)?,
        };
        if [
            &msg.rk,
            &msg.signed_note_nullifier,
            &msg.cmx_new,
            &msg.van_cmx,
            &msg.vote_round_id,
        ]
        .iter()
        .any(|v| v.len() != 32)
            || msg.spend_auth_sig.len() != 64
            || msg.gov_nullifiers.len() != 5
            || msg.gov_nullifiers.iter().any(|v| v.len() != 32)
            || msg.proof.is_empty()
        {
            return Err(invalid());
        }
        crate::tx1::validate_tx1_effects(&msg.tx1_effects)?;
        let mut bytes = vec![0x02];
        msg.encode(&mut bytes).map_err(|_| invalid())?;
        Ok(Self {
            json,
            tx_hash: hex::encode_upper(Sha256::digest(bytes)),
            round_id: hex::encode(msg.vote_round_id),
        })
    }
    pub fn json_bytes(&self) -> &[u8] {
        &self.json
    }
    pub fn tx_hash(&self) -> &str {
        &self.tx_hash
    }

    fn validate_bundle(&self, db: &VotingDb, round: &str, bundle: u32) -> Result<(), VotingError> {
        let wire: DelegationSubmissionWire =
            serde_json::from_slice(&self.json).map_err(|_| invalid())?;
        let signature = STANDARD
            .decode(wire.spend_auth_sig)
            .map_err(|_| invalid())?;
        let sighash = {
            let conn = db.conn();
            queries::load_pczt_sighash(&conn, round, &db.wallet_id(), bundle)?
        };
        let signer = crate::delegate::DelegationSigner::signature_from_bytes(&signature, &sighash)?;
        let expected = crate::delegate::submission(db, round, bundle, signer)?.to_wire_json()?;
        if expected.as_bytes() != self.json {
            return Err(invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelegationOutcome {
    Prepared,
    Dispatching,
    Accepted,
    Ambiguous,
    Rejected,
}
impl DelegationOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Dispatching => "dispatching",
            Self::Accepted => "accepted",
            Self::Ambiguous => "ambiguous",
            Self::Rejected => "rejected",
        }
    }
    fn parse(value: &str) -> Result<Self, VotingError> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "dispatching" => Ok(Self::Dispatching),
            "accepted" => Ok(Self::Accepted),
            "ambiguous" => Ok(Self::Ambiguous),
            "rejected" => Ok(Self::Rejected),
            _ => Err(invalid()),
        }
    }
}
pub struct DelegationAttempt {
    pub canonical: CanonicalDelegation,
    pub outcome: DelegationOutcome,
}

impl VotingDb {
    pub fn delegation_submission(
        &self,
        round: &str,
        bundle: u32,
    ) -> Result<Option<DelegationAttempt>, VotingError> {
        let row: Option<(Vec<u8>,String,String)> = self.conn().query_row(
            "SELECT canonical_json, tx_hash, outcome FROM delegation_attempts WHERE round_id=?1 AND wallet_id=?2 AND bundle_index=?3",
            params![round,self.wallet_id(),bundle], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional().map_err(|_| invalid())?;
        row.map(|(json, hash, outcome)| {
            let canonical = CanonicalDelegation::parse(json)?;
            canonical.validate_bundle(self, round, bundle)?;
            let outcome = DelegationOutcome::parse(&outcome)?;
            if canonical.round_id != round || canonical.tx_hash != hash {
                return Err(invalid());
            }
            let stored = self.get_delegation_tx_hash(round, bundle)?;
            if (outcome == DelegationOutcome::Prepared && stored.is_some())
                || (outcome != DelegationOutcome::Prepared && stored.as_deref() != Some(&hash))
            {
                return Err(invalid());
            }
            Ok(DelegationAttempt { canonical, outcome })
        })
        .transpose()
    }
    pub fn prepare_delegation_submission(
        &self,
        round: &str,
        bundle: u32,
        canonical: &CanonicalDelegation,
    ) -> Result<(), VotingError> {
        canonical.validate_bundle(self, round, bundle)?;
        if canonical.round_id != round || self.get_delegation_tx_hash(round, bundle)?.is_some() {
            return Err(invalid());
        }
        if let Some(existing) = self.delegation_submission(round, bundle)? {
            return if existing.canonical.json == canonical.json {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        self.conn().execute("INSERT INTO delegation_attempts (round_id,wallet_id,bundle_index,canonical_json,tx_hash,outcome) VALUES (?1,?2,?3,?4,?5,'prepared')",
            params![round,self.wallet_id(),bundle,canonical.json,canonical.tx_hash]).map_err(|_| invalid())?;
        Ok(())
    }
    pub fn mark_delegation_dispatching(&self, round: &str, bundle: u32) -> Result<(), VotingError> {
        let attempt = self
            .delegation_submission(round, bundle)?
            .ok_or_else(invalid)?;
        if attempt.outcome != DelegationOutcome::Prepared {
            return Err(invalid());
        }
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(|_| invalid())?;
        queries::store_delegation_tx_hash(
            &tx,
            round,
            &self.wallet_id(),
            bundle,
            &attempt.canonical.tx_hash,
        )?;
        let changed=tx.execute("UPDATE delegation_attempts SET outcome='dispatching' WHERE round_id=?1 AND wallet_id=?2 AND bundle_index=?3 AND outcome='prepared'",
            params![round,self.wallet_id(),bundle]).map_err(|_| invalid())?;
        if changed != 1 {
            return Err(invalid());
        }
        tx.commit().map_err(|_| invalid())
    }
    pub fn record_delegation_outcome(
        &self,
        round: &str,
        bundle: u32,
        outcome: DelegationOutcome,
    ) -> Result<(), VotingError> {
        if matches!(
            outcome,
            DelegationOutcome::Prepared | DelegationOutcome::Dispatching
        ) {
            return Err(invalid());
        }
        let existing = self
            .delegation_submission(round, bundle)?
            .ok_or_else(invalid)?;
        if existing.outcome == DelegationOutcome::Prepared {
            return Err(invalid());
        }
        if existing.outcome == DelegationOutcome::Rejected && outcome != DelegationOutcome::Rejected
        {
            return Err(invalid());
        }
        self.conn().execute("UPDATE delegation_attempts SET outcome=?4 WHERE round_id=?1 AND wallet_id=?2 AND bundle_index=?3",
            params![round,self.wallet_id(),bundle,outcome.as_str()]).map_err(|_| invalid())?;
        Ok(())
    }
    pub fn confirm_journaled_delegation(
        &self,
        round: &str,
        bundle: u32,
        hash: &str,
        events: &[TxEvent],
    ) -> Result<(), VotingError> {
        let attempt = self
            .delegation_submission(round, bundle)?
            .ok_or_else(invalid)?;
        if attempt.canonical.tx_hash != hash
            || matches!(
                attempt.outcome,
                DelegationOutcome::Prepared | DelegationOutcome::Rejected
            )
        {
            return Err(invalid());
        }
        if events
            .iter()
            .filter(|event| {
                event.event_type == "delegate_vote"
                    || STANDARD
                        .decode(&event.event_type)
                        .is_ok_and(|name| name == b"delegate_vote")
            })
            .count()
            != 1
        {
            return Err(invalid());
        }
        for event in events
            .iter()
            .filter(|event| event.event_type == "delegate_vote")
        {
            let mut keys = std::collections::BTreeSet::new();
            let mut round_attributes = 0;
            for attribute in &event.attributes {
                if !keys.insert(attribute.key.as_str()) {
                    return Err(invalid());
                }
                if matches!(attribute.key.as_str(), "vote_round_id" | "round_id") {
                    round_attributes += 1;
                }
            }
            if round_attributes != 1 {
                return Err(invalid());
            }
        }
        crate::confirmation::confirm_delegation_submission(self, round, bundle, hash, events)?;
        Ok(())
    }
}

pub fn delegation_request(base: &str, canonical: &CanonicalDelegation) -> OwnedHttpRequest {
    OwnedHttpRequest {
        method: "POST".into(),
        url: format!(
            "{}/shielded-vote/v1/delegate-vote",
            base.trim_end_matches('/')
        ),
        headers: vec![("content-type".into(), "application/json".into())],
        body: canonical.json.clone(),
        timeout_ms: 30_000,
        max_response_bytes: API_LIMIT,
        kind: RequestKind::NonIdempotent,
    }
}
pub fn delegation_status_request(base: &str, canonical: &CanonicalDelegation) -> OwnedHttpRequest {
    OwnedHttpRequest {
        method: "GET".into(),
        url: format!(
            "{}/shielded-vote/v1/tx/{}",
            base.trim_end_matches('/'),
            canonical.tx_hash
        ),
        headers: vec![],
        body: vec![],
        timeout_ms: 30_000,
        max_response_bytes: API_LIMIT,
        kind: RequestKind::Idempotent,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BroadcastResult {
    tx_hash: String,
    code: u32,
    #[serde(rename = "log")]
    _log: Option<String>,
}
pub fn parse_delegation_result(
    response: OwnedHttpResponse,
    expected: &CanonicalDelegation,
) -> Result<DelegationOutcome, TransportError> {
    if response.body.len() > API_LIMIT {
        return Err(TransportError::Ambiguous);
    }
    if response.status != 200 && response.status != 422 {
        return Err(TransportError::Ambiguous);
    }
    let result: BroadcastResult =
        serde_json::from_slice(&response.body).map_err(|_| TransportError::Ambiguous)?;
    if result.tx_hash != expected.tx_hash {
        return Err(TransportError::Ambiguous);
    }
    match (response.status, result.code) {
        (200, 0) => Ok(DelegationOutcome::Accepted),
        (422, 1..) => Ok(DelegationOutcome::Rejected),
        _ => Err(TransportError::Ambiguous),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TxStatus {
    height: String,
    code: u32,
    events: Option<Vec<TxEvent>>,
    #[serde(rename = "log")]
    _log: Option<String>,
}
pub enum DelegationStatus {
    Pending,
    Rejected,
    Confirmed(Vec<TxEvent>),
}
pub fn parse_delegation_status(
    response: OwnedHttpResponse,
) -> Result<DelegationStatus, TransportError> {
    if response.body.len() > API_LIMIT {
        return Err(TransportError::ResponseTooLarge { limit: API_LIMIT });
    }
    if response.status == 404 {
        return Ok(DelegationStatus::Pending);
    }
    if response.status != 200 && response.status != 422 {
        return Err(TransportError::Definite);
    }
    let result: TxStatus =
        serde_json::from_slice(&response.body).map_err(|_| TransportError::Definite)?;
    if result
        .height
        .parse::<u64>()
        .ok()
        .filter(|h| *h > 0)
        .is_none()
    {
        return Err(TransportError::Definite);
    }
    match (response.status, result.code) {
        (200, 0) => Ok(DelegationStatus::Confirmed(
            result.events.ok_or(TransportError::Definite)?,
        )),
        (422, 1..) => Ok(DelegationStatus::Rejected),
        _ => Err(TransportError::Definite),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delegation_status_requires_exact_api_shape() {
        let response = |status, body: &[u8]| OwnedHttpResponse {
            status,
            body: body.to_vec(),
        };
        // vote-sdk api/handler.go emits height, code, log, events, but no hash.
        assert!(matches!(
            parse_delegation_status(response(
                200,
                br#"{"height":"42","code":0,"log":"","events":[]}"#
            )),
            Ok(DelegationStatus::Confirmed(_))
        ));
        assert!(matches!(
            parse_delegation_status(response(404, br#"{"error":"tx not found"}"#)),
            Ok(DelegationStatus::Pending)
        ));
        assert!(matches!(
            parse_delegation_status(response(
                422,
                br#"{"height":"42","code":1,"log":"rejected","events":null}"#
            )),
            Ok(DelegationStatus::Rejected)
        ));
        for body in [
            br#"{"height":"42","code":0,"events":[],"tx_hash":"wrong"}"#.as_slice(),
            br#"{"height":"-1","code":0,"events":[]}"#,
            br#"{"height":"0","code":0,"events":[]}"#,
            br#"{"height":"42","code":0}"#,
            br#"{"height":"42","code":1,"events":[]}"#,
        ] {
            assert!(parse_delegation_status(response(200, body)).is_err());
        }
    }
    #[test]
    fn delegation_hash_matches_independent_protoc_server_golden() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../test-vectors/delegation_submission_golden.json"
        ))
        .unwrap();
        let canonical = CanonicalDelegation::parse(
            fixture["canonical_json"]
                .as_str()
                .unwrap()
                .as_bytes()
                .to_vec(),
        )
        .unwrap();
        assert_eq!(
            canonical.tx_hash(),
            fixture["transaction_hash"].as_str().unwrap()
        );
        assert_eq!(
            delegation_request("https://vote.example/mount/", &canonical).url,
            "https://vote.example/mount/shielded-vote/v1/delegate-vote"
        );
        assert_eq!(
            delegation_request("https://vote.example", &canonical).body,
            canonical.json_bytes()
        );
        for response in [
            OwnedHttpResponse {
                status: 200,
                body: br#"{"code":0,"tx_hash":"wrong"}"#.to_vec(),
            },
            OwnedHttpResponse {
                status: 200,
                body: br#"{"code":0}"#.to_vec(),
            },
            OwnedHttpResponse {
                status: 502,
                body: vec![],
            },
            OwnedHttpResponse {
                status: 200,
                body: vec![0; API_LIMIT + 1],
            },
        ] {
            assert!(matches!(
                parse_delegation_result(response, &canonical),
                Err(TransportError::Ambiguous)
            ));
        }
    }
}
