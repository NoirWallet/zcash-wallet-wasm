//! Stable browser DTOs and owned authenticated identity for later persistence.
#![allow(dead_code)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_error_and_phase_serialization() {
        assert_eq!(
            VotingErrorCode::WalletNotSynced.as_str(),
            "WALLET_NOT_SYNCED"
        );
        assert_eq!(
            serde_json::to_string(&VotingErrorCode::RoundUnauthenticated).unwrap(),
            "\"ROUND_UNAUTHENTICATED\""
        );
        assert_eq!(
            serde_json::to_string(&VotingRoundPhase::ConfigAuthenticated).unwrap(),
            "\"config_authenticated\""
        );
    }
}
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VotingDiscovery {
    pub accepted: Vec<RoundSummary>,
    pub rejected: Vec<RejectedRound>,
    pub config_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VotingEligibility {
    pub round_id: String,
    pub eligible: bool,
    pub selected_value_zats: String,
    pub voting_power: String,
    pub selected_note_count: u32,
    pub bundle_count: u32,
    pub trimmed_value_zats: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedVotingRound {
    pub round_id: String,
    pub bundle_count: u32,
    pub selected_note_count: u32,
    pub voting_power: String,
    pub state: VotingRoundPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VotingRoundStatus {
    pub round_id: String,
    pub phase: VotingRoundPhase,
    pub completed_bundles: u32,
    pub total_bundles: u32,
    pub confirmed_votes: u32,
    pub total_votes: u32,
    /// Confirmed helper-share records, each representing Valar's helper quorum.
    pub confirmed_helpers: u32,
    pub required_helpers: u32,
    pub next_action: String,
}

/// Task 9 persists this envelope through Valar's round creation API.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VotingSessionEnvelope {
    pub version: u32,
    pub authenticated_round: AuthenticatedRoundIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSummary {
    pub round_id: String,
    pub name: String,
    pub snapshot_height: u64,
    pub starts_at: String,
    pub ends_at: String,
    pub proposal_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedRound {
    pub round_id: String,
    pub code: VotingErrorCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub proposal_id: u32,
    pub title: String,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HelperEndpoint {
    pub id: String,
    pub url: String,
    pub public_key: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VotingRoundPhase {
    Uninitialized,
    ConfigAuthenticated,
    Eligible,
    RoundPrepared,
    DelegationPending,
    DelegationConfirmed,
    VotePending,
    VoteConfirmed,
    SharesConfirmed,
    NotEligible,
    RoundRejected,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VotingErrorCode {
    ConfigFetchFailed,
    ConfigUntrusted,
    RoundUnauthenticated,
    RoundExpired,
    NetworkMismatch,
    WalletNotSynced,
    NotEligible,
    PirSnapshotMismatch,
    ProofFailed,
    PersistenceFailed,
    SubmissionRejected,
    SubmissionUnknown,
    OperationInProgress,
    RecoveryConflict,
}

impl VotingErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigFetchFailed => "CONFIG_FETCH_FAILED",
            Self::ConfigUntrusted => "CONFIG_UNTRUSTED",
            Self::RoundUnauthenticated => "ROUND_UNAUTHENTICATED",
            Self::RoundExpired => "ROUND_EXPIRED",
            Self::NetworkMismatch => "NETWORK_MISMATCH",
            Self::WalletNotSynced => "WALLET_NOT_SYNCED",
            Self::NotEligible => "NOT_ELIGIBLE",
            Self::PirSnapshotMismatch => "PIR_SNAPSHOT_MISMATCH",
            Self::ProofFailed => "PROOF_FAILED",
            Self::PersistenceFailed => "PERSISTENCE_FAILED",
            Self::SubmissionRejected => "SUBMISSION_REJECTED",
            Self::SubmissionUnknown => "SUBMISSION_UNKNOWN",
            Self::OperationInProgress => "OPERATION_IN_PROGRESS",
            Self::RecoveryConflict => "RECOVERY_CONFLICT",
        }
    }
}

#[derive(Debug)]
pub(crate) struct VotingApiError {
    pub code: VotingErrorCode,
    pub message: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TrustedRound {
    pub service_metadata_fingerprint: String,
    pub config_summary: zcash_voting::config::ResolvedVotingConfigSummary,
    pub config_fingerprint: String,
    pub summary: RoundSummary,
    pub params: zcash_voting::VotingRoundParams,
    pub proposals: Vec<Proposal>,
    pub vote_server_urls: Vec<String>,
    pub pir_urls: Vec<String>,
    pub pir_layout: zcash_voting::wire::PirLayout,
    pub tree_url: String,
    pub helpers: Vec<HelperEndpoint>,
}

/// Durable comparison material, never sufficient by itself to authenticate a round.
/// v2 signs ID/key/layout; service metadata is pinned here for resume comparison.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthenticatedRoundIdentity {
    pub service_metadata_fingerprint: String,
    pub config_summary: zcash_voting::config::ResolvedVotingConfigSummary,
    pub config_fingerprint: String,
    pub summary: RoundSummary,
    pub params: zcash_voting::VotingRoundParams,
    pub proposals: Vec<Proposal>,
    pub vote_server_urls: Vec<String>,
    pub pir_urls: Vec<String>,
    pub pir_layout: zcash_voting::wire::PirLayout,
    pub tree_url: String,
    pub helpers: Vec<HelperEndpoint>,
}

impl TrustedRound {
    pub(crate) fn identity(&self) -> AuthenticatedRoundIdentity {
        AuthenticatedRoundIdentity {
            service_metadata_fingerprint: self.service_metadata_fingerprint.clone(),
            config_summary: self.config_summary.clone(),
            config_fingerprint: self.config_fingerprint.clone(),
            summary: self.summary.clone(),
            params: self.params.clone(),
            proposals: self.proposals.clone(),
            vote_server_urls: self.vote_server_urls.clone(),
            pir_urls: self.pir_urls.clone(),
            pir_layout: self.pir_layout,
            tree_url: self.tree_url.clone(),
            helpers: self.helpers.clone(),
        }
    }

    pub(crate) fn compare_identity(
        &self,
        saved: &AuthenticatedRoundIdentity,
    ) -> Result<(), VotingApiError> {
        if &self.identity() == saved {
            Ok(())
        } else {
            Err(VotingApiError::new(VotingErrorCode::RecoveryConflict))
        }
    }
}
