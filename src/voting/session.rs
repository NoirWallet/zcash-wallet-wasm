//! One local owner for voting state and authenticated discovery.
#![allow(dead_code)]
use super::{config, persistence::Persistence, types::*};
use crate::bindgen::wallet::{AccountId, MemoryWallet};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
};
use tonic_web_wasm_client::Client;
use zcash_client_backend::data_api::{Account as _, WalletRead};
use zcash_protocol::consensus::{NetworkType, Parameters};
use zcash_voting::{
    phases::{DelegationPhase, SharePhase, VotePhase},
    session::{NextStep, resume_plan},
    storage::VotingDb,
};

pub(crate) struct VotingSessionInner {
    pub(crate) wallet: MemoryWallet<Client>,
    pub(crate) account_id: AccountId,
    pub(crate) wallet_id: String,
    pub(crate) db: VotingDb,
    #[cfg(target_arch = "wasm32")]
    pub(crate) transport: super::transport::BrowserFetchTransport,
    pub(crate) persistence: Persistence,
    pub(crate) trusted_rounds: RefCell<BTreeMap<String, TrustedRound>>,
    pub(crate) selected_round_id: RefCell<Option<String>>,
    pub(crate) mutation_active: Rc<Cell<bool>>,
    eligibility: RefCell<BTreeMap<String, VotingEligibility>>,
}

fn conflict(_: impl Sized) -> VotingApiError {
    VotingApiError::new(VotingErrorCode::RecoveryConflict)
}
pub(crate) fn now_secs() -> Result<u64, VotingApiError> {
    u64::try_from(time::OffsetDateTime::now_utc().unix_timestamp()).map_err(conflict)
}

impl VotingSessionInner {
    fn validate_delegation_round(
        &self,
        id: &str,
        round: &TrustedRound,
        now: u64,
    ) -> Result<(), VotingApiError> {
        if round.summary.round_id != id
            || !self.db.has_round(id).map_err(conflict)?
            || self
                .selected_round_id
                .borrow()
                .as_ref()
                .is_some_and(|selected| selected != id)
        {
            return Err(conflict(()));
        }
        if now >= round.summary.ends_at.parse::<u64>().map_err(conflict)? {
            return Err(VotingApiError::new(VotingErrorCode::RoundExpired));
        }
        self.validate_saved_identity(round)?;
        *self.selected_round_id.borrow_mut() = Some(id.into());
        Ok(())
    }
    fn delegation_progress(
        &self,
        id: &str,
        now: u64,
    ) -> Result<VotingActionResult, VotingApiError> {
        let status = self.round_status_at(id, now)?;
        Ok(VotingActionResult {
            round_id: status.round_id,
            phase: status.phase,
            completed_bundles: status.completed_bundles,
            total_bundles: status.total_bundles,
            next_action: status.next_action,
        })
    }

    pub(crate) async fn submit_delegation_with<A, B, C>(
        &self,
        id: &str,
        usk: zcash_keys::keys::UnifiedSpendingKey,
        fingerprint: zip32::fingerprint::SeedFingerprint,
        transport: &impl zcash_voting::platform::HttpTransport,
        authenticate: impl Fn() -> A,
        mut build: impl FnMut(TrustedRound, u32) -> B,
        checkpoint: impl Fn() -> C,
        now: impl Fn() -> Result<u64, VotingApiError>,
    ) -> Result<VotingActionResult, VotingApiError>
    where
        A: Future<Output = Result<TrustedRound, VotingApiError>>,
        B: Future<
            Output = Result<
                zcash_voting::delegation_submission::CanonicalDelegation,
                VotingApiError,
            >,
        >,
        C: Future<Output = Result<(), VotingApiError>>,
    {
        use zcash_voting::delegation_submission::*;
        let _guard = self.mutation_guard()?;
        self.validate_delegation_key(&usk, fingerprint)?;
        drop(usk);
        let id = config::normalize_round_id(id)?;
        let round = authenticate().await?;
        self.validate_delegation_round(&id, &round, now()?)?;
        let roster = round
            .proposals
            .iter()
            .map(|p| p.proposal_id)
            .collect::<Vec<_>>();
        loop {
            let steps = zcash_voting::session::delegation_resume_steps(&self.db, &id, &roster)
                .map_err(conflict)?;
            let Some(step) = steps.first() else {
                checkpoint()
                    .await
                    .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                return self.delegation_progress(&id, now()?);
            };
            let bundle = match step {
                NextStep::Delegate { bundle_index } | NextStep::PollDelegation { bundle_index } => {
                    *bundle_index
                }
                _ => unreachable!(),
            };
            let mut attempt = self
                .db
                .delegation_submission(&id, bundle)
                .map_err(conflict)?;
            if attempt.is_none() {
                if matches!(step, NextStep::PollDelegation { .. }) {
                    return Err(conflict(()));
                }
                let round = authenticate().await?;
                self.validate_delegation_round(&id, &round, now()?)?;
                let canonical = build(round, bundle).await?;
                self.db
                    .prepare_delegation_submission(&id, bundle, &canonical)
                    .map_err(conflict)?;
                attempt = self
                    .db
                    .delegation_submission(&id, bundle)
                    .map_err(conflict)?;
            }
            let attempt = attempt.ok_or_else(|| conflict(()))?;
            let canonical = attempt.canonical;
            if attempt.outcome == DelegationOutcome::Rejected {
                checkpoint()
                    .await
                    .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                return Err(VotingApiError::new(VotingErrorCode::SubmissionRejected));
            }
            if attempt.outcome == DelegationOutcome::Prepared {
                checkpoint().await?;
                let round = authenticate().await?;
                self.validate_delegation_round(&id, &round, now()?)?;
                let base = round.vote_server_urls.first().ok_or_else(|| conflict(()))?;
                self.db
                    .mark_delegation_dispatching(&id, bundle)
                    .map_err(conflict)?;
                // From this checkpoint onward cancellation can only recover by lookup.
                checkpoint()
                    .await
                    .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                self.validate_delegation_round(&id, &round, now()?)?;
                // The request and its future are deliberately created after checkpoint resolution.
                let outcome = transport
                    .execute(delegation_request(base, &canonical))
                    .await
                    .and_then(|r| parse_delegation_result(r, &canonical));
                let recorded = match outcome {
                    Ok(outcome) => outcome,
                    Err(zcash_voting::platform::TransportError::Definite) => {
                        DelegationOutcome::Rejected
                    }
                    Err(_) => DelegationOutcome::Ambiguous,
                };
                self.db
                    .record_delegation_outcome(&id, bundle, recorded)
                    .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                checkpoint()
                    .await
                    .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                match recorded {
                    DelegationOutcome::Ambiguous => {
                        return Err(VotingApiError::new(VotingErrorCode::SubmissionUnknown));
                    }
                    DelegationOutcome::Rejected => {
                        return Err(VotingApiError::new(VotingErrorCode::SubmissionRejected));
                    }
                    _ => continue,
                }
            }
            // Flush the authoritative in-memory state after a failed result checkpoint.
            checkpoint()
                .await
                .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
            let round = authenticate().await?;
            self.validate_delegation_round(&id, &round, now()?)?;
            let mut confirmed = false;
            for base in &round.vote_server_urls {
                self.validate_delegation_round(&id, &round, now()?)?;
                let response = match transport
                    .execute(delegation_status_request(base, &canonical))
                    .await
                {
                    Ok(response) => response,
                    Err(zcash_voting::platform::TransportError::Definite) => continue,
                    Err(_) => return Err(conflict(())),
                };
                match parse_delegation_status(response).map_err(conflict)? {
                    DelegationStatus::Pending => continue,
                    DelegationStatus::Rejected => {
                        self.db
                            .record_delegation_outcome(&id, bundle, DelegationOutcome::Rejected)
                            .map_err(conflict)?;
                        checkpoint()
                            .await
                            .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                        return Err(VotingApiError::new(VotingErrorCode::SubmissionRejected));
                    }
                    DelegationStatus::Confirmed(events) => {
                        self.validate_delegation_round(&id, &round, now()?)?;
                        self.db
                            .confirm_journaled_delegation(&id, bundle, canonical.tx_hash(), &events)
                            .map_err(conflict)?;
                        checkpoint()
                            .await
                            .map_err(|_| VotingApiError::new(VotingErrorCode::SubmissionUnknown))?;
                        confirmed = true;
                        break;
                    }
                }
            }
            if !confirmed {
                return self.delegation_progress(&id, now()?);
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn submit_delegation(
        &self,
        id: &str,
        usk: zcash_keys::keys::UnifiedSpendingKey,
        fingerprint: zip32::fingerprint::SeedFingerprint,
    ) -> Result<VotingActionResult, VotingApiError> {
        self.submit_delegation_with(
            id,
            usk.clone(),
            fingerprint,
            &self.transport,
            || self.ensure_trusted_round(id),
            |round, bundle| self.build_delegation(round, bundle, usk.clone(), fingerprint),
            || self.persistence.checkpoint(&self.db),
            now_secs,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn build_delegation(
        &self,
        round: TrustedRound,
        bundle: u32,
        usk: zcash_keys::keys::UnifiedSpendingKey,
        fingerprint: zip32::fingerprint::SeedFingerprint,
    ) -> Result<zcash_voting::delegation_submission::CanonicalDelegation, VotingApiError> {
        use zcash_voting::{delegate, proof_executor::ProofExecutor};
        let id = &round.summary.round_id;
        let account = self.validate_delegation_key(&usk, fingerprint)?;
        let input = super::wallet_adapter::gather_eligibility_inputs(
            &self.wallet,
            self.account_id,
            zcash_voting::Network::Testnet,
            round.params.snapshot_height,
        )
        .await?;
        let trusted = self.ensure_trusted_round(id).await?;
        self.validate_delegation_round(id, &trusted, now_secs()?)?;
        let hotkey = self
            .db
            .load_voting_hotkey(id)
            .map_err(conflict)?
            .ok_or_else(|| conflict(()))?;
        let keys = delegate::DelegationKeys::with_voting_hotkey(
            account.orchard_fvk_bytes.to_vec(),
            &hotkey,
            account.seed_fingerprint,
            account.account_index,
            round.summary.name.clone(),
        )
        .map_err(conflict)?;
        let policy = self
            .db
            .effective_bundle_policy(id, zcash_voting::BundlePolicy::default())
            .map_err(conflict)?;
        self.db
            .ensure_bundles_with_policy(id, &input.notes, policy)
            .map_err(conflict)?;
        let notes = zcash_voting::round::note_bundles_with_policy(&input.notes, policy)
            .map_err(conflict)?
            .into_iter()
            .nth(bundle as usize)
            .ok_or_else(|| conflict(()))?;
        if !self
            .db
            .has_complete_witnesses(id, bundle, &notes)
            .map_err(conflict)?
        {
            return Err(conflict(()));
        }
        let setup = self
            .db
            .build_governance_pczt(
                id,
                bundle,
                &notes,
                &keys,
                delegate::branch_id_for_height(
                    zcash_voting::Network::Testnet,
                    round.params.snapshot_height,
                )
                .map_err(conflict)?,
            )
            .map_err(conflict)?;
        let signed = crate::bindgen::sign::sign_delegation_owned(
            pczt::Pczt::parse(&setup.pczt_bytes).map_err(conflict)?,
            usk,
            account,
            fingerprint,
            setup.action_index,
        )?;
        let signature =
            delegate::spend_auth_signature(&signed, setup.action_index).map_err(conflict)?;
        let sighash = delegate::pczt_sighash(&signed).map_err(conflict)?;
        drop(signed);
        let job = self
            .db
            .delegation_proof_job(id, bundle, notes, keys)
            .map_err(conflict)?;
        self.ensure_trusted_round(id).await?;
        #[cfg(feature = "wasm-parallel")]
        let executor = super::proof_executor::RayonProofExecutor;
        #[cfg(not(feature = "wasm-parallel"))]
        let executor = zcash_voting::proof_executor::SequentialProofExecutor;
        let result = executor
            .prove(job, |job| job(&zcash_voting::types::NoopProgressReporter))
            .await
            .map_err(|_| VotingApiError::new(VotingErrorCode::ProofFailed))?;
        let trusted = self.ensure_trusted_round(id).await?;
        self.validate_delegation_round(id, &trusted, now_secs()?)?;
        self.db
            .store_delegation_proof(id, bundle, &result)
            .map_err(conflict)?;
        let submission = delegate::submission(
            &self.db,
            id,
            bundle,
            delegate::DelegationSigner::signature(signature, sighash),
        )
        .map_err(conflict)?;
        zcash_voting::delegation_submission::CanonicalDelegation::from_submission(&submission)
            .map_err(conflict)
    }

    pub(crate) fn validate_delegation_key(
        &self,
        usk: &zcash_keys::keys::UnifiedSpendingKey,
        fingerprint: zip32::fingerprint::SeedFingerprint,
    ) -> Result<zcash_voting::delegate::DelegationAccountKeys, VotingApiError> {
        let db = self
            .wallet
            .db
            .try_read()
            .map_err(|_| VotingApiError::new(VotingErrorCode::OperationInProgress))?;
        let account = db
            .get_account(self.account_id)
            .map_err(conflict)?
            .ok_or_else(|| conflict(()))?;
        let derivation = account
            .source()
            .key_derivation()
            .ok_or_else(|| conflict(()))?;
        let ufvk = account.ufvk().ok_or_else(|| conflict(()))?;
        if self.wallet.network.network_type() != NetworkType::Test
            || db.params().network_type() != NetworkType::Test
            || *derivation.seed_fingerprint() != fingerprint
            || ufvk.encode(&self.wallet.network)
                != usk
                    .to_unified_full_viewing_key()
                    .encode(&self.wallet.network)
        {
            return Err(conflict(()));
        }
        let mut digest = Sha256::new();
        digest.update(b"zcash-wallet-wasm/voting-wallet-id/v1");
        digest.update(b"testnet");
        let encoding = SecretString::new(ufvk.encode(&self.wallet.network));
        digest.update(encoding.expose_secret().as_bytes());
        if hex::encode(digest.finalize()) != self.wallet_id {
            return Err(conflict(()));
        }
        Ok(zcash_voting::delegate::DelegationAccountKeys {
            account_index: derivation.account_index().into(),
            seed_fingerprint: fingerprint.to_bytes(),
            orchard_fvk_bytes: ufvk.orchard().ok_or_else(|| conflict(()))?.to_bytes(),
        })
    }
    pub(crate) fn open(
        wallet: MemoryWallet<Client>,
        account_id: AccountId,
        bytes: Option<&[u8]>,
        persistence: Persistence,
    ) -> Result<Self, VotingApiError> {
        let wallet_id = {
            let db = wallet
                .db
                .try_read()
                .map_err(|_| VotingApiError::new(VotingErrorCode::OperationInProgress))?;
            if wallet.network.network_type() != NetworkType::Test
                || db.params().network_type() != NetworkType::Test
            {
                return Err(VotingApiError::new(VotingErrorCode::NetworkMismatch));
            }
            let account = db
                .get_account(account_id)
                .map_err(|_| VotingApiError::new(VotingErrorCode::NotEligible))?
                .ok_or_else(|| VotingApiError::new(VotingErrorCode::NotEligible))?;
            if account.source().key_derivation().is_none() {
                return Err(VotingApiError::new(VotingErrorCode::NotEligible));
            }
            let ufvk = account
                .ufvk()
                .filter(|key| key.orchard().is_some())
                .ok_or_else(|| VotingApiError::new(VotingErrorCode::NotEligible))?;
            let encoding = SecretString::new(ufvk.encode(&wallet.network));
            let mut digest = Sha256::new();
            digest.update(b"zcash-wallet-wasm/voting-wallet-id/v1");
            digest.update(b"testnet");
            digest.update(encoding.expose_secret().as_bytes());
            hex::encode(digest.finalize())
        };
        let db = VotingDb::open_browser(&wallet_id, zcash_voting::Network::Testnet, bytes)
            .map_err(conflict)?;
        let session = Self {
            wallet,
            account_id,
            wallet_id,
            db,
            persistence,
            #[cfg(target_arch = "wasm32")]
            transport: super::transport::BrowserFetchTransport,
            trusted_rounds: RefCell::new(BTreeMap::new()),
            selected_round_id: RefCell::new(None),
            mutation_active: Rc::new(Cell::new(false)),
            eligibility: RefCell::new(BTreeMap::new()),
        };
        let mut pending = None;
        for round in session.db.list_rounds().map_err(conflict)? {
            let id = config::normalize_round_id(&round.round_id).map_err(conflict)?;
            if id != round.round_id {
                return Err(conflict(()));
            }
            // Without a complete caller roster we cannot prove terminality.
            let terminal = if let Some(saved) = session.saved_identity(&id)? {
                let roster = saved
                    .proposals
                    .iter()
                    .map(|p| p.proposal_id)
                    .collect::<Vec<_>>();
                let plan = resume_plan(&session.db, &id, &roster).map_err(conflict)?;
                !roster.is_empty() && plan.all_decided && !plan.pending_recovery
            } else {
                false
            };
            if !terminal && pending.replace(id).is_some() {
                return Err(conflict(()));
            }
        }
        *session.selected_round_id.borrow_mut() = pending;
        Ok(session)
    }
    pub(crate) fn storage_key(&self) -> String {
        format!("zcash-voting:testnet:{}", self.wallet_id)
    }
    pub(crate) fn export_state(&self) -> Result<Vec<u8>, VotingApiError> {
        self.db
            .serialize_bytes()
            .map_err(|_| VotingApiError::new(VotingErrorCode::PersistenceFailed))
    }
    pub(crate) fn mutation_guard(&self) -> Result<MutationGuard, VotingApiError> {
        MutationGuard::acquire(&self.mutation_active)
    }
    pub(crate) fn replace_discovery(
        &self,
        discovery: VotingDiscovery,
        rounds: BTreeMap<String, TrustedRound>,
    ) -> VotingDiscovery {
        self.eligibility.borrow_mut().clear();
        *self.trusted_rounds.borrow_mut() = rounds;
        discovery
    }
    pub(crate) async fn discover_with<F>(&self, fetch: F) -> Result<VotingDiscovery, VotingApiError>
    where
        F: Future<
            Output = Result<(VotingDiscovery, BTreeMap<String, TrustedRound>), VotingApiError>,
        >,
    {
        let _guard = self.mutation_guard()?;
        let (discovery, rounds) = fetch.await?;
        Ok(self.replace_discovery(discovery, rounds))
    }
    pub(crate) fn cached_round_at(
        &self,
        id: &str,
        now: u64,
    ) -> Result<TrustedRound, VotingApiError> {
        let id = config::normalize_round_id(id)?;
        let round = self
            .trusted_rounds
            .borrow()
            .get(&id)
            .cloned()
            .ok_or_else(|| VotingApiError::new(VotingErrorCode::RoundUnauthenticated))?;
        if now >= round.summary.ends_at.parse::<u64>().map_err(conflict)? {
            return Err(VotingApiError::new(VotingErrorCode::RoundExpired));
        }
        Ok(round)
    }
    fn saved_identity(
        &self,
        id: &str,
    ) -> Result<Option<AuthenticatedRoundIdentity>, VotingApiError> {
        if !self.db.has_round(id).map_err(conflict)? {
            return Ok(None);
        }
        self.db
            .round_session_json(id)
            .map_err(conflict)?
            .map(|json| {
                let envelope: VotingSessionEnvelope =
                    serde_json::from_str(&json).map_err(conflict)?;
                if envelope.version != 1
                    || envelope.authenticated_round.summary.round_id != id
                    || envelope.authenticated_round.params.vote_round_id != id
                {
                    return Err(conflict(()));
                }
                Ok(envelope.authenticated_round)
            })
            .transpose()
    }
    pub(crate) fn validate_saved_identity(
        &self,
        round: &TrustedRound,
    ) -> Result<(), VotingApiError> {
        let id = &round.summary.round_id;
        if !self.db.has_round(id).map_err(conflict)? {
            return Ok(());
        }
        let saved = self.saved_identity(id)?.ok_or_else(|| conflict(()))?;
        let (params, network) = self.db.round_params_with_network(id).map_err(conflict)?;
        if network != zcash_voting::Network::Testnet || params != saved.params {
            return Err(conflict(()));
        }
        round.compare_identity(&saved)?;
        Ok(())
    }
    /// Call under the mutation guard before every future network/proof action.
    /// Restored caller data never establishes authentication on its own.
    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn ensure_trusted_round(
        &self,
        id: &str,
    ) -> Result<TrustedRound, VotingApiError> {
        self.ensure_trusted_round_with(id, config::discover_rounds(&self.transport), now_secs)
            .await
    }
    pub(crate) async fn ensure_trusted_round_with<F>(
        &self,
        id: &str,
        fetch: F,
        now: impl Fn() -> Result<u64, VotingApiError>,
    ) -> Result<TrustedRound, VotingApiError>
    where
        F: Future<
            Output = Result<(VotingDiscovery, BTreeMap<String, TrustedRound>), VotingApiError>,
        >,
    {
        let id = config::normalize_round_id(id)?;
        if !self.trusted_rounds.borrow().contains_key(&id) {
            let (discovery, rounds) = fetch.await?;
            self.replace_discovery(discovery, rounds);
        }
        let round = self.cached_round_at(&id, now()?)?;
        self.validate_saved_identity(&round)?;
        Ok(round)
    }
    pub(crate) async fn check_eligibility_at(
        &self,
        id: &str,
        now: impl Fn() -> Result<u64, VotingApiError>,
    ) -> Result<VotingEligibility, VotingApiError> {
        let _guard = self.mutation_guard()?;
        let round = self.cached_round_at(id, now()?)?;
        self.validate_saved_identity(&round)?;
        let eligibility = config::check_eligibility(&self.wallet, self.account_id, &round).await?;
        self.cached_round_at(id, now()?)?;
        self.eligibility
            .borrow_mut()
            .insert(round.summary.round_id, eligibility.clone());
        Ok(eligibility)
    }
    pub(crate) fn round_status_at(
        &self,
        id: &str,
        now: u64,
    ) -> Result<VotingRoundStatus, VotingApiError> {
        let id = config::normalize_round_id(id)?;
        let mut status = VotingRoundStatus {
            round_id: id.clone(),
            phase: VotingRoundPhase::Uninitialized,
            completed_bundles: 0,
            total_bundles: 0,
            confirmed_votes: 0,
            total_votes: 0,
            confirmed_helpers: 0,
            required_helpers: 0,
            next_action: "discover_rounds".into(),
        };
        let cached = self.trusted_rounds.borrow().get(&id).cloned();
        if let Some(round) = cached.as_ref() {
            self.validate_saved_identity(round)?;
        }
        let saved = self.saved_identity(&id)?;
        let identity = cached.as_ref().map(TrustedRound::identity).or(saved);
        if self.db.has_round(&id).map_err(conflict)? {
            status.phase = VotingRoundPhase::RoundPrepared;
            status.total_bundles = self.db.get_bundle_count(&id).map_err(conflict)?;
            let Some(identity) = identity.as_ref() else {
                if status.total_bundles == 0 {
                    status.phase = VotingRoundPhase::Uninitialized;
                }
                return Ok(status);
            };
            let roster = identity
                .proposals
                .iter()
                .map(|p| p.proposal_id)
                .collect::<Vec<_>>();
            let plan = resume_plan(&self.db, &id, &roster).map_err(conflict)?;
            status.completed_bundles = plan
                .delegation_statuses
                .iter()
                .filter(|d| d.phase == DelegationPhase::Confirmed)
                .count() as u32;
            let votes = self.db.vote_phases(&id).map_err(conflict)?;
            status.confirmed_votes = votes
                .iter()
                .filter(|(_, p, phase)| roster.contains(p) && *phase == VotePhase::Confirmed)
                .count() as u32;
            let intents = self.db.ballot_intents(&id).map_err(conflict)?;
            status.total_votes = status
                .total_bundles
                .checked_mul(
                    intents
                        .iter()
                        .filter(|(p, d)| {
                            roster.contains(p)
                                && matches!(d, zcash_voting::session::Decision::Choice(_))
                        })
                        .count() as u32,
                )
                .ok_or_else(|| conflict(()))?;
            let shares = self.db.share_phases(&id).map_err(conflict)?;
            status.confirmed_helpers = shares
                .iter()
                .filter(|(_, p, _, phase)| roster.contains(p) && *phase == SharePhase::Confirmed)
                .count() as u32;
            status.required_helpers = shares
                .iter()
                .filter(|(_, p, _, _)| roster.contains(p))
                .count() as u32
                + plan
                    .next_steps
                    .iter()
                    .filter(|step| matches!(step, NextStep::SubmitShares { .. }))
                    .count() as u32;
            status.next_action = plan
                .next_steps
                .first()
                .map(|s| s.kind())
                .unwrap_or(if plan.needs_draft_setup {
                    "submit_ballot"
                } else {
                    plan.primary_action.as_str()
                })
                .into();
            status.phase = if !roster.is_empty() && plan.all_decided && !plan.pending_recovery {
                VotingRoundPhase::SharesConfirmed
            } else if status.total_bundles == 0 {
                status.next_action = if cached.is_some() {
                    "prepare_round"
                } else {
                    "discover_rounds"
                }
                .into();
                if cached.is_some() {
                    VotingRoundPhase::ConfigAuthenticated
                } else {
                    VotingRoundPhase::Uninitialized
                }
            } else if status.confirmed_votes > 0
                && !votes.iter().any(|(_, _, p)| *p != VotePhase::Confirmed)
            {
                VotingRoundPhase::VoteConfirmed
            } else if !votes.is_empty() {
                VotingRoundPhase::VotePending
            } else if status.total_bundles > 0 && status.completed_bundles == status.total_bundles {
                VotingRoundPhase::DelegationConfirmed
            } else if plan
                .delegation_statuses
                .iter()
                .any(|d| d.phase != DelegationPhase::Prepared)
            {
                VotingRoundPhase::DelegationPending
            } else {
                VotingRoundPhase::RoundPrepared
            };
            if self.db.load_voting_hotkey(&id).map_err(conflict)?.is_some()
                && self.db.get_round_state(&id).map_err(conflict)?.phase
                    == zcash_voting::storage::RoundPhase::Initialized
            {
                status.phase = VotingRoundPhase::ConfigAuthenticated;
                status.next_action = "prepare_round".into();
            }
        } else if let Some(round) = cached.as_ref() {
            status.phase = VotingRoundPhase::ConfigAuthenticated;
            status.next_action = "check_eligibility".into();
            if let Some(eligibility) = self.eligibility.borrow().get(&id) {
                status.total_bundles = eligibility.bundle_count;
                status.phase = if eligibility.eligible {
                    VotingRoundPhase::Eligible
                } else {
                    VotingRoundPhase::NotEligible
                };
                status.next_action = if eligibility.eligible {
                    "prepare_round"
                } else {
                    "none"
                }
                .into();
            }
            if now >= round.summary.ends_at.parse::<u64>().map_err(conflict)? {
                status.phase = VotingRoundPhase::Expired;
                status.next_action = "none".into();
            }
        } else {
            return Err(VotingApiError::new(VotingErrorCode::RoundUnauthenticated));
        }
        if status.phase != VotingRoundPhase::SharesConfirmed
            && identity.as_ref().is_some_and(|identity| {
                identity
                    .summary
                    .ends_at
                    .parse::<u64>()
                    .is_ok_and(|end| now >= end)
            })
        {
            status.phase = VotingRoundPhase::Expired;
            status.next_action = "none".into();
        }
        Ok(status)
    }
    pub(crate) async fn clear_round_with<F>(
        &self,
        id: &str,
        checkpoint: impl FnOnce() -> F,
    ) -> Result<(), VotingApiError>
    where
        F: Future<Output = Result<(), VotingApiError>>,
    {
        let id = config::normalize_round_id(id)?;
        let _guard = self.mutation_guard()?;
        self.db.clear_round(&id).map_err(conflict)?;
        // Cleanup runs on success, callback failure, and future cancellation.
        let _cleanup = ClearedRound { session: self, id };
        checkpoint().await
    }

    pub(crate) async fn prepare_round_with<P, C>(
        &self,
        id: &str,
        prepare: impl FnOnce(TrustedRound) -> P,
        checkpoint: impl FnOnce() -> C,
        now: impl Fn() -> Result<u64, VotingApiError>,
    ) -> Result<PreparedVotingRound, VotingApiError>
    where
        P: Future<Output = Result<PreparedVotingRound, VotingApiError>>,
        C: Future<Output = Result<(), VotingApiError>>,
    {
        let _mutation = self.mutation_guard()?;
        let round = self.cached_round_at(id, now()?)?;
        self.validate_saved_identity(&round)?;
        let id = round.summary.round_id.clone();
        if self
            .selected_round_id
            .borrow()
            .as_ref()
            .is_some_and(|selected| selected != &id)
        {
            return Err(conflict(()));
        }
        *self.selected_round_id.borrow_mut() = Some(id.clone());
        let _selection = PreparingRound {
            session: self,
            id: id.clone(),
        };
        let prepared = prepare(round.clone()).await?;
        self.cached_round_at(&id, now()?)?;
        self.validate_saved_identity(&round)?;
        checkpoint().await?;
        Ok(prepared)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn prepare_round(
        &self,
        id: &str,
    ) -> Result<PreparedVotingRound, VotingApiError> {
        use prost::Message;
        use zcash_voting::{Network, proof_executor::ProofExecutor};
        let mismatch = |_| VotingApiError::new(VotingErrorCode::PirSnapshotMismatch);
        self.prepare_round_with(
            id,
            |round| async move {
                // This read is public, and releases its client before any wallet access.
                let tree = self
                    .wallet
                    .client
                    .clone()
                    .get_tree_state(zcash_client_backend::proto::service::BlockId {
                        height: round.params.snapshot_height,
                        hash: vec![],
                    })
                    .await
                    .map_err(|_| VotingApiError::new(VotingErrorCode::PirSnapshotMismatch))?
                    .into_inner()
                    .encode_to_vec();
                zcash_voting::precompute::validate_snapshot_tree_state(
                    &round.params,
                    Network::Testnet,
                    &tree,
                )
                .map_err(mismatch)?;
                let mut client = zcash_voting::pir::OrderedSnapshotPir::connect(
                    round.pir_layout,
                    round.pir_urls.clone(),
                    std::sync::Arc::new(self.transport),
                    Network::Testnet,
                    &round.params.nullifier_imt_root,
                    round.params.snapshot_height,
                )
                .await
                .map_err(mismatch)?;
                self.cached_round_at(&round.summary.round_id, now_secs()?)?;
                let eligibility =
                    config::check_eligibility(&self.wallet, self.account_id, &round).await?;
                self.cached_round_at(&round.summary.round_id, now_secs()?)?;
                if !eligibility.eligible {
                    return Err(VotingApiError::new(VotingErrorCode::NotEligible));
                }
                if !self
                    .db
                    .has_round(&round.summary.round_id)
                    .map_err(conflict)?
                {
                    let envelope = serde_json::to_string(&VotingSessionEnvelope {
                        version: 1,
                        authenticated_round: round.identity(),
                    })
                    .map_err(conflict)?;
                    self.db
                        .init_round(Network::Testnet, &round.params, Some(&envelope))
                        .map_err(conflict)?;
                }
                let hotkey = self
                    .db
                    .ensure_voting_hotkey(&round.summary.round_id)
                    .map_err(conflict)?;
                let input = super::wallet_adapter::gather_wallet_inputs(
                    &self.wallet,
                    self.account_id,
                    super::wallet_adapter::WalletInputRequest {
                        network: Network::Testnet,
                        snapshot_height: round.params.snapshot_height,
                        anchor_tree_state_bytes: tree,
                        resolved_round_name: round.summary.name.clone(),
                        hotkey: &hotkey,
                    },
                )
                .await?;
                self.cached_round_at(&round.summary.round_id, now_secs()?)?;
                // Executors receive owned Rust data only. Reads precede execution; writes follow it.
                #[cfg(feature = "wasm-parallel")]
                let executor = super::proof_executor::RayonProofExecutor;
                #[cfg(not(feature = "wasm-parallel"))]
                let executor = zcash_voting::proof_executor::SequentialProofExecutor;
                let input = executor
                    .prove(input, |input| {
                        for witness in &input.witnesses {
                            zcash_voting::precompute::verify_witness(witness)?;
                        }
                        Ok(input)
                    })
                    .await
                    .map_err(|_| VotingApiError::new(VotingErrorCode::ProofFailed))?;
                self.cached_round_at(&round.summary.round_id, now_secs()?)?;
                let notes = &input.delegation.round_note_infos;
                let policy = self
                    .db
                    .effective_bundle_policy(
                        &round.summary.round_id,
                        zcash_voting::BundlePolicy::default(),
                    )
                    .map_err(conflict)?;
                let bundles = zcash_voting::round::note_bundles_with_policy(notes, policy)
                    .map_err(conflict)?;
                let layout = self
                    .db
                    .ensure_bundles_with_policy(&round.summary.round_id, notes, policy)
                    .map_err(conflict)?;
                if bundles.is_empty() {
                    return Err(VotingApiError::new(VotingErrorCode::NotEligible));
                }
                for (index, notes) in bundles.iter().enumerate() {
                    self.db
                        .ensure_padded_secrets(&round.summary.round_id, index as u32, notes)
                        .map_err(conflict)?;
                    let witnesses = input
                        .witnesses
                        .iter()
                        .filter(|w| notes.iter().any(|n| n.position == w.position))
                        .cloned()
                        .collect::<Vec<_>>();
                    zcash_voting::store_tree_state_and_witnesses(
                        &self.db,
                        &round.summary.round_id,
                        index as u32,
                        &input.delegation.anchor_tree_state_bytes,
                        notes,
                        &witnesses,
                    )
                    .map_err(conflict)?;
                }
                for (index, notes) in bundles.iter().enumerate() {
                    self.cached_round_at(&round.summary.round_id, now_secs()?)?;
                    let client = &mut client;
                    self.db
                        .precompute_delegation_pir_async(
                            &round.summary.round_id,
                            index as u32,
                            notes,
                            Network::Testnet,
                            |nullifiers| async move { client.fetch_proofs(&nullifiers).await },
                        )
                        .await
                        .map_err(mismatch)?;
                }
                self.db
                    .advance_round_phase(
                        &round.summary.round_id,
                        zcash_voting::storage::RoundPhase::HotkeyGenerated,
                    )
                    .map_err(conflict)?;
                Ok(PreparedVotingRound {
                    round_id: round.summary.round_id,
                    bundle_count: layout.bundle_count,
                    selected_note_count: bundles.iter().map(Vec::len).sum::<usize>() as u32,
                    voting_power: layout.eligible_weight.to_string(),
                    state: VotingRoundPhase::RoundPrepared,
                })
            },
            || self.persistence.checkpoint(&self.db),
            now_secs,
        )
        .await
    }
}

struct PreparingRound<'a> {
    session: &'a VotingSessionInner,
    id: String,
}
impl Drop for PreparingRound<'_> {
    fn drop(&mut self) {
        // A partial durable round is explicit recovery state, including on cancellation.
        if matches!(self.session.db.has_round(&self.id), Ok(false)) {
            *self.session.selected_round_id.borrow_mut() = None;
        }
    }
}

struct ClearedRound<'a> {
    session: &'a VotingSessionInner,
    id: String,
}
impl Drop for ClearedRound<'_> {
    fn drop(&mut self) {
        self.session.trusted_rounds.borrow_mut().remove(&self.id);
        self.session.eligibility.borrow_mut().remove(&self.id);
        if self.session.selected_round_id.borrow().as_ref() == Some(&self.id) {
            *self.session.selected_round_id.borrow_mut() = None;
        }
    }
}

pub(crate) struct MutationGuard(Rc<Cell<bool>>);
impl MutationGuard {
    pub(crate) fn acquire(active: &Rc<Cell<bool>>) -> Result<Self, VotingApiError> {
        if active.replace(true) {
            return Err(VotingApiError::new(VotingErrorCode::OperationInProgress));
        }
        Ok(Self(active.clone()))
    }
}
impl Drop for MutationGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[cfg(test)]
#[path = "../../zcash_voting/zcash_voting/tests/support/delegation_fixture.rs"]
mod delegation_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{future::pending, poll};
    use zcash_client_backend::data_api::{
        AccountBirthday, AccountPurpose, WalletWrite, Zip32Derivation,
    };
    use zcash_protocol::consensus::BlockHeight;
    const ID: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    #[tokio::test]
    async fn delegation_signing_rejects_wrong_fingerprint_and_account_before_work() {
        let (session, _) = session().await;
        let fingerprint = zip32::fingerprint::SeedFingerprint::from_seed(&[17; 32]).unwrap();
        let usk = |index| {
            zcash_keys::keys::UnifiedSpendingKey::from_seed(
                &crate::Network::TestNetwork,
                &[17; 32],
                zip32::AccountId::try_from(index).unwrap(),
            )
            .unwrap()
        };
        let wrong = zip32::fingerprint::SeedFingerprint::from_seed(&[18; 32]).unwrap();
        assert_eq!(
            session
                .validate_delegation_key(&usk(0), wrong)
                .unwrap_err()
                .code,
            VotingErrorCode::RecoveryConflict
        );
        assert_eq!(
            session
                .validate_delegation_key(&usk(1), fingerprint)
                .unwrap_err()
                .code,
            VotingErrorCode::RecoveryConflict
        );
        assert_eq!(
            session
                .validate_delegation_key(&usk(0), fingerprint)
                .unwrap()
                .account_index,
            0
        );
        assert!(session.wallet.db.try_write().is_ok());
    }

    #[tokio::test]
    async fn delegation_checkpoint_failure_has_no_post_and_ambiguity_resumes_by_polling() {
        use std::sync::Mutex;
        use zcash_voting::platform::{
            HttpTransport, OwnedHttpRequest, OwnedHttpResponse, TransportError, TransportFuture,
        };
        struct Transport(Mutex<Vec<OwnedHttpRequest>>);
        impl HttpTransport for Transport {
            fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
                let post = request.method == "POST";
                self.0.lock().unwrap().push(request);
                Box::pin(async move {
                    if post {
                        Err(TransportError::Ambiguous)
                    } else {
                        Ok(OwnedHttpResponse {
                            status: 404,
                            body: vec![],
                        })
                    }
                })
            }
        }
        let (session, mut round) = session().await;
        round.params.snapshot_height = delegation_fixture::height();
        round.summary.snapshot_height = round.params.snapshot_height;
        session
            .trusted_rounds
            .borrow_mut()
            .insert(ID.into(), round.clone());
        let envelope = serde_json::to_string(&VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        })
        .unwrap();
        session
            .db
            .init_round(
                zcash_voting::Network::Testnet,
                &round.params,
                Some(&envelope),
            )
            .unwrap();
        let canonical = delegation_fixture::prepare(&session.db, &round.params, 17);
        let builds = Cell::new(0);
        let transport = Transport(Mutex::new(vec![]));
        let usk = zcash_keys::keys::UnifiedSpendingKey::from_seed(
            &crate::Network::TestNetwork,
            &[17; 32],
            zip32::AccountId::ZERO,
        )
        .unwrap();
        let fingerprint = zip32::fingerprint::SeedFingerprint::from_seed(&[17; 32]).unwrap();
        let authenticate = || std::future::ready(Ok(round.clone()));
        let build = |_, _| {
            builds.set(builds.get() + 1);
            std::future::ready(Ok(canonical.clone()))
        };
        let error = session
            .submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &transport,
                authenticate,
                build,
                || std::future::ready(Err(VotingApiError::new(VotingErrorCode::PersistenceFailed))),
                || Ok(1500),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, VotingErrorCode::PersistenceFailed);
        assert!(transport.0.lock().unwrap().is_empty());
        let prepared_bytes = session.export_state().unwrap();
        struct RejectedTransport;
        impl HttpTransport for RejectedTransport {
            fn execute(&self, _: OwnedHttpRequest) -> TransportFuture {
                Box::pin(std::future::ready(Err(TransportError::Definite)))
            }
        }
        let rejected = open(
            session.wallet.clone(),
            session.account_id,
            Some(&prepared_bytes),
        )
        .unwrap();
        let checkpoints = Cell::new(0);
        let fail_result = || {
            checkpoints.set(checkpoints.get() + 1);
            std::future::ready(if checkpoints.get() == 3 {
                Err(VotingApiError::new(VotingErrorCode::PersistenceFailed))
            } else {
                Ok(())
            })
        };
        assert_eq!(
            rejected
                .submit_delegation_with(
                    ID,
                    usk.clone(),
                    fingerprint,
                    &RejectedTransport,
                    authenticate,
                    build,
                    fail_result,
                    || Ok(1500)
                )
                .await
                .unwrap_err()
                .code,
            VotingErrorCode::SubmissionUnknown
        );
        assert_eq!(
            rejected
                .submit_delegation_with(
                    ID,
                    usk.clone(),
                    fingerprint,
                    &RejectedTransport,
                    authenticate,
                    build,
                    fail_result,
                    || Ok(1500)
                )
                .await
                .unwrap_err()
                .code,
            VotingErrorCode::SubmissionRejected
        );
        assert_eq!(checkpoints.get(), 4);
        struct PendingPost(Mutex<Vec<String>>);
        impl HttpTransport for PendingPost {
            fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
                let post = request.method == "POST";
                self.0.lock().unwrap().push(request.method);
                Box::pin(async move {
                    if post {
                        pending::<()>().await;
                    }
                    Ok(OwnedHttpResponse {
                        status: 404,
                        body: vec![],
                    })
                })
            }
        }
        let pending_post = PendingPost(Mutex::new(vec![]));
        let cancelled = open(
            session.wallet.clone(),
            session.account_id,
            Some(&prepared_bytes),
        )
        .unwrap();
        let mut operation = Box::pin(cancelled.submit_delegation_with(
            ID,
            usk.clone(),
            fingerprint,
            &pending_post,
            authenticate,
            build,
            || std::future::ready(Ok(())),
            || Ok(1500),
        ));
        assert!(poll!(operation.as_mut()).is_pending());
        assert!(cancelled.mutation_guard().is_err());
        drop(operation);
        assert!(cancelled.mutation_guard().is_ok());
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&cancelled.export_state().unwrap()),
        )
        .unwrap();
        restored
            .submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &pending_post,
                authenticate,
                build,
                || std::future::ready(Ok(())),
                || Ok(1500),
            )
            .await
            .unwrap();
        assert_eq!(*pending_post.0.lock().unwrap(), ["POST", "GET"]);
        for cancel_at in [1, 2, 3] {
            let cancelled = open(
                session.wallet.clone(),
                session.account_id,
                Some(&prepared_bytes),
            )
            .unwrap();
            let checkpoints = Cell::new(0);
            let checkpoint = || async {
                checkpoints.set(checkpoints.get() + 1);
                if checkpoints.get() == cancel_at {
                    pending::<()>().await;
                }
                Ok(())
            };
            let mut operation = Box::pin(cancelled.submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &transport,
                authenticate,
                build,
                checkpoint,
                || Ok(1500),
            ));
            assert!(poll!(operation.as_mut()).is_pending());
            assert!(cancelled.mutation_guard().is_err());
            drop(operation);
            assert!(cancelled.mutation_guard().is_ok());
            assert_eq!(
                transport.0.lock().unwrap().len(),
                usize::from(cancel_at == 3)
            );
            if cancel_at >= 2 {
                cancelled
                    .submit_delegation_with(
                        ID,
                        usk.clone(),
                        fingerprint,
                        &transport,
                        authenticate,
                        build,
                        || std::future::ready(Ok(())),
                        || Ok(1500),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    transport.0.lock().unwrap().len(),
                    1 + usize::from(cancel_at == 3)
                );
                assert_eq!(transport.0.lock().unwrap().last().unwrap().method, "GET");
                transport.0.lock().unwrap().clear();
            }
        }
        let failed_result = open(
            session.wallet.clone(),
            session.account_id,
            Some(&prepared_bytes),
        )
        .unwrap();
        let checkpoints = Cell::new(0);
        let error = failed_result
            .submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &transport,
                authenticate,
                build,
                || {
                    checkpoints.set(checkpoints.get() + 1);
                    std::future::ready(if checkpoints.get() == 3 {
                        Err(VotingApiError::new(VotingErrorCode::PersistenceFailed))
                    } else {
                        Ok(())
                    })
                },
                || Ok(1500),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, VotingErrorCode::SubmissionUnknown);
        failed_result
            .submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &transport,
                authenticate,
                build,
                || std::future::ready(Ok(())),
                || Ok(1500),
            )
            .await
            .unwrap();
        assert_eq!(
            transport
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|r| r.method.as_str())
                .collect::<Vec<_>>(),
            ["POST", "GET"]
        );
        transport.0.lock().unwrap().clear();
        let error = session
            .submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &transport,
                authenticate,
                build,
                || std::future::ready(Ok(())),
                || Ok(1500),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, VotingErrorCode::SubmissionUnknown);
        assert_eq!(builds.get(), 1);
        let reopened = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        let progress = reopened
            .submit_delegation_with(
                ID,
                usk.clone(),
                fingerprint,
                &transport,
                authenticate,
                build,
                || std::future::ready(Ok(())),
                || Ok(1500),
            )
            .await
            .unwrap();
        assert_eq!(progress.completed_bundles, 0);
        assert_eq!(builds.get(), 1);
        let calls = transport.0.lock().unwrap();
        assert_eq!(
            calls.iter().map(|r| r.method.as_str()).collect::<Vec<_>>(),
            ["POST", "GET"]
        );
        assert_eq!(calls[0].body, canonical.json_bytes());
        assert!(calls.iter().all(|r| {
            r.url
                .starts_with("https://vote.example.com/shielded-vote/v1/")
        }));
        assert!(calls[1].url.ends_with(canonical.tx_hash()));
        assert!(!session.mutation_active.get());
        drop(calls);
        struct ConfirmedTransport(Mutex<Vec<String>>, String);
        impl HttpTransport for ConfirmedTransport {
            fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
                let post = request.method == "POST";
                self.0.lock().unwrap().push(request.method);
                let body = if post {
                    serde_json::json!({"code":0,"tx_hash":self.1})
                } else {
                    serde_json::json!({"height":"42","code":0,"events":[{"type":"delegate_vote","attributes":[{"key":"vote_round_id","value":ID},{"key":"leaf_index","value":"42"}]}]})
                };
                Box::pin(async move {
                    Ok(OwnedHttpResponse {
                        status: 200,
                        body: serde_json::to_vec(&body).unwrap(),
                    })
                })
            }
        }
        let confirmed = open(
            session.wallet.clone(),
            session.account_id,
            Some(&prepared_bytes),
        )
        .unwrap();
        let chain = ConfirmedTransport(Mutex::new(vec![]), canonical.tx_hash().into());
        for _ in 0..2 {
            let result = confirmed
                .submit_delegation_with(
                    ID,
                    usk.clone(),
                    fingerprint,
                    &chain,
                    authenticate,
                    build,
                    || std::future::ready(Ok(())),
                    || Ok(1500),
                )
                .await
                .unwrap();
            assert_eq!(result.completed_bundles, 1);
            assert_eq!(result.phase, VotingRoundPhase::DelegationConfirmed);
        }
        assert_eq!(*chain.0.lock().unwrap(), ["POST", "GET"]);
        assert_eq!(builds.get(), 1);
    }

    #[tokio::test]
    async fn prepare_round_rejects_before_work_and_clears_predurable_selection() {
        let (session, _) = session().await;
        let calls = Cell::new(0);
        let work = |_| {
            calls.set(calls.get() + 1);
            std::future::ready(Err::<super::super::types::PreparedVotingRound, _>(
                conflict(()),
            ))
        };
        let checkpoint = || std::future::ready(Ok(()));
        let err = session
            .prepare_round_with("02".repeat(32).as_str(), work, checkpoint, || Ok(1500))
            .await
            .unwrap_err();
        assert_eq!(err.code, VotingErrorCode::RoundUnauthenticated);
        assert_eq!(calls.get(), 0);
        *session.selected_round_id.borrow_mut() = Some("02".repeat(32));
        assert_eq!(
            session
                .prepare_round_with(ID, work, checkpoint, || Ok(1500))
                .await
                .unwrap_err()
                .code,
            VotingErrorCode::RecoveryConflict
        );
        assert_eq!(calls.get(), 0);
        *session.selected_round_id.borrow_mut() = None;
        assert!(
            session
                .prepare_round_with(ID, work, checkpoint, || Ok(1500))
                .await
                .is_err()
        );
        assert_eq!(calls.get(), 1);
        assert!(session.selected_round_id.borrow().is_none());
        assert!(!session.mutation_active.get());
    }

    #[tokio::test]
    async fn prepare_round_cancellation_releases_gate_and_preserves_only_durable_selection() {
        let (session, round) = session().await;
        let mut preparing = Box::pin(session.prepare_round_with(
            ID,
            |_| pending(),
            || std::future::ready(Ok(())),
            || Ok(1500),
        ));
        assert!(poll!(preparing.as_mut()).is_pending());
        assert_eq!(
            session.mutation_guard().err().unwrap().code,
            VotingErrorCode::OperationInProgress
        );
        drop(preparing);
        assert!(session.selected_round_id.borrow().is_none());
        let envelope = serde_json::to_string(&VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        })
        .unwrap();
        let mut preparing = Box::pin(session.prepare_round_with(
            ID,
            |_| {
                session
                    .db
                    .init_round(
                        zcash_voting::Network::Testnet,
                        &round.params,
                        Some(&envelope),
                    )
                    .unwrap();
                session.db.ensure_voting_hotkey(ID).unwrap();
                pending()
            },
            || std::future::ready(Ok(())),
            || Ok(1500),
        ));
        assert!(poll!(preparing.as_mut()).is_pending());
        drop(preparing);
        assert_eq!(*session.selected_round_id.borrow(), Some(ID.into()));
        assert!(session.mutation_guard().is_ok());
        assert_eq!(
            session.round_status_at(ID, 1500).unwrap().next_action,
            "prepare_round"
        );
    }

    #[tokio::test]
    async fn prepare_round_checkpoint_rejection_retains_authoritative_hotkey_and_identity() {
        let (session, round) = session().await;
        let prepare = |round: TrustedRound| {
            let envelope = serde_json::to_string(&VotingSessionEnvelope {
                version: 1,
                authenticated_round: round.identity(),
            })
            .unwrap();
            if !session.db.has_round(ID).unwrap() {
                session
                    .db
                    .init_round(
                        zcash_voting::Network::Testnet,
                        &round.params,
                        Some(&envelope),
                    )
                    .unwrap();
            }
            session.db.ensure_voting_hotkey(ID).unwrap();
            std::future::ready(Ok(super::super::types::PreparedVotingRound {
                round_id: ID.into(),
                bundle_count: 1,
                selected_note_count: 1,
                voting_power: "1".into(),
                state: VotingRoundPhase::RoundPrepared,
            }))
        };
        let failed = session
            .prepare_round_with(
                ID,
                prepare,
                || std::future::ready(Err(VotingApiError::new(VotingErrorCode::PersistenceFailed))),
                || Ok(1500),
            )
            .await
            .unwrap_err();
        assert_eq!(failed.code, VotingErrorCode::PersistenceFailed);
        let hotkey = session.db.ensure_voting_hotkey(ID).unwrap();
        assert_eq!(*session.selected_round_id.borrow(), Some(ID.into()));
        session
            .prepare_round_with(ID, prepare, || std::future::ready(Ok(())), || Ok(1500))
            .await
            .unwrap();
        assert_eq!(
            hotkey.stored_secret(),
            session.db.ensure_voting_hotkey(ID).unwrap().stored_secret()
        );
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        restored.validate_saved_identity(&round).unwrap();
        assert_eq!(
            hotkey.stored_secret(),
            restored
                .db
                .ensure_voting_hotkey(ID)
                .unwrap()
                .stored_secret()
        );
    }

    fn wallet(
        network: crate::Network,
        seed: u8,
        derived: bool,
    ) -> (MemoryWallet<Client>, AccountId, String) {
        let mut db = zcash_client_memory::MemoryWalletDb::new(network, 100);
        let birthday = AccountBirthday::from_parts(
            zcash_client_backend::data_api::chain::ChainState::empty(
                BlockHeight::from_u32(3_000_000),
                zcash_primitives::block::BlockHash([0; 32]),
            ),
            None,
        );
        let ufvk = zcash_keys::keys::UnifiedSpendingKey::from_seed(
            &network,
            &[seed; 32],
            zip32::AccountId::ZERO,
        )
        .unwrap()
        .to_unified_full_viewing_key();
        let encoding = ufvk.encode(&network);
        let derivation = derived.then(|| {
            Zip32Derivation::new(
                zip32::fingerprint::SeedFingerprint::from_seed(&[seed; 32]).unwrap(),
                zip32::AccountId::ZERO,
            )
        });
        let id = db
            .import_account_ufvk(
                "fixture",
                &ufvk,
                &birthday,
                AccountPurpose::Spending { derivation },
                None,
            )
            .unwrap()
            .id();
        (
            crate::Wallet::new(db, Client::new("https://example.invalid".into()), network).unwrap(),
            id,
            encoding,
        )
    }
    fn open(
        wallet: MemoryWallet<Client>,
        account: AccountId,
        bytes: Option<&[u8]>,
    ) -> Result<VotingSessionInner, VotingApiError> {
        VotingSessionInner::open(wallet, account, bytes, Persistence::default())
    }
    async fn session() -> (VotingSessionInner, TrustedRound) {
        let (wallet, account, _) = wallet(crate::Network::TestNetwork, 17, true);
        let session = open(wallet, account, None).unwrap();
        let (discovery, rounds) = super::super::config::tests::discover(
            vec![super::super::config::tests::active()],
            1500,
        )
        .await;
        let round = rounds[ID].clone();
        session.replace_discovery(discovery, rounds);
        (session, round)
    }
    #[tokio::test]
    async fn constructor_validates_identity_and_fails_without_blocking_wallet_writer() {
        let (wallet, account, encoding) = wallet(crate::Network::TestNetwork, 17, true);
        let guard = wallet.db.write().await;
        assert_eq!(
            open(wallet.clone(), account, None).err().unwrap().code,
            VotingErrorCode::OperationInProgress
        );
        drop(guard);
        assert_eq!(
            open(wallet.clone(), 999.into(), None).err().unwrap().code,
            VotingErrorCode::NotEligible
        );
        let session = open(wallet.clone(), account, None).unwrap();
        assert_eq!(
            session.wallet_id,
            hex::encode(Sha256::digest(
                [
                    b"zcash-wallet-wasm/voting-wallet-id/v1".as_slice(),
                    b"testnet",
                    encoding.as_bytes()
                ]
                .concat()
            ))
        );
        assert_eq!(
            session.storage_key(),
            format!("zcash-voting:testnet:{}", session.wallet_id)
        );
        assert!(!session.storage_key().contains(&encoding));
        let bytes = session.export_state().unwrap();
        assert_eq!(
            open(wallet, account, Some(&bytes)).unwrap().wallet_id,
            session.wallet_id
        );
        for (network, seed, derived, code) in [
            (
                crate::Network::MainNetwork,
                17,
                true,
                VotingErrorCode::NetworkMismatch,
            ),
            (
                crate::Network::TestNetwork,
                18,
                true,
                VotingErrorCode::RecoveryConflict,
            ),
            (
                crate::Network::TestNetwork,
                17,
                false,
                VotingErrorCode::NotEligible,
            ),
        ] {
            let (other, account, _) = self::wallet(network, seed, derived);
            assert_eq!(open(other, account, Some(&bytes)).err().unwrap().code, code);
        }
    }
    #[tokio::test]
    async fn restored_selection_requires_exactly_one_nonterminal_round() {
        let (session, round) = session().await;
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &round.params, None)
            .unwrap();
        let restore = || {
            open(
                session.wallet.clone(),
                session.account_id,
                Some(&session.export_state().unwrap()),
            )
        };
        assert_eq!(
            *restore().unwrap().selected_round_id.borrow(),
            Some(ID.into())
        );
        let mut other = round.params.clone();
        other.vote_round_id = "01".repeat(32);
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &other, None)
            .unwrap();
        assert_eq!(
            restore().err().unwrap().code,
            VotingErrorCode::RecoveryConflict
        );
    }
    #[tokio::test]
    async fn cached_status_is_available_during_mutation_and_ids_fail_closed() {
        let (session, _) = session().await;
        let _guard = session.mutation_guard().unwrap();
        assert_eq!(
            session.round_status_at(ID, 1500).unwrap().phase,
            VotingRoundPhase::ConfigAuthenticated
        );
        assert_eq!(
            session.round_status_at(ID, 2000).unwrap().phase,
            VotingRoundPhase::Expired
        );
        assert_eq!(
            session.cached_round_at("ff", 1500).unwrap_err().code,
            VotingErrorCode::RoundUnauthenticated
        );
        assert_eq!(
            session
                .cached_round_at(&"01".repeat(32), 1500)
                .unwrap_err()
                .code,
            VotingErrorCode::RoundUnauthenticated
        );
        assert_eq!(
            session.cached_round_at(ID, 2000).unwrap_err().code,
            VotingErrorCode::RoundExpired
        );
    }
    #[tokio::test]
    async fn discovery_replaces_only_on_success_and_unknown_ids_never_wait_for_wallet() {
        let (session, _) = session().await;
        let guard = session.wallet.db.write().await;
        let unknown_id = "01".repeat(32);
        let mut eligibility = Box::pin(session.check_eligibility_at(&unknown_id, || Ok(1500)));
        assert!(matches!(
            poll!(&mut eligibility),
            std::task::Poll::Ready(Err(VotingApiError {
                code: VotingErrorCode::RoundUnauthenticated,
                ..
            }))
        ));
        drop(guard);
        assert!(
            session
                .discover_with(async {
                    Err(VotingApiError::new(VotingErrorCode::ConfigFetchFailed))
                })
                .await
                .is_err()
        );
        assert!(session.cached_round_at(ID, 1500).is_ok());
        session
            .discover_with(async {
                Ok((
                    VotingDiscovery {
                        accepted: vec![],
                        rejected: vec![],
                        config_fingerprint: "new".into(),
                    },
                    BTreeMap::new(),
                ))
            })
            .await
            .unwrap();
        assert!(session.cached_round_at(ID, 1500).is_err());
    }
    #[tokio::test]
    async fn ensure_uses_current_cache_but_restored_state_must_rediscover() {
        let (session, round) = session().await;
        session
            .ensure_trusted_round_with(ID, async { panic!("cached round fetched") }, || Ok(1500))
            .await
            .unwrap();
        let envelope = VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        };
        session
            .db
            .init_round(
                zcash_voting::Network::Testnet,
                &round.params,
                Some(&serde_json::to_string(&envelope).unwrap()),
            )
            .unwrap();
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        let fetched = Cell::new(false);
        restored
            .ensure_trusted_round_with(
                ID,
                async {
                    fetched.set(true);
                    Ok(super::super::config::tests::discover(
                        vec![super::super::config::tests::active()],
                        1500,
                    )
                    .await)
                },
                || Ok(1500),
            )
            .await
            .unwrap();
        assert!(fetched.get());
        assert_eq!(
            restored
                .ensure_trusted_round_with(ID, async { panic!("cached round fetched") }, || Ok(
                    2000
                ))
                .await
                .unwrap_err()
                .code,
            VotingErrorCode::RoundExpired
        );
    }
    #[tokio::test]
    async fn authorization_requires_identity_for_existing_cached_round() {
        let (session, round) = session().await;
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &round.params, None)
            .unwrap();
        let err = session
            .ensure_trusted_round_with(ID, async { panic!("cached round fetched") }, || Ok(1500))
            .await
            .unwrap_err();
        assert_eq!(err.code, VotingErrorCode::RecoveryConflict);
        let _writer = session.wallet.db.write().await;
        let mut eligibility = Box::pin(session.check_eligibility_at(ID, || Ok(1500)));
        assert!(matches!(
            poll!(&mut eligibility),
            std::task::Poll::Ready(Err(VotingApiError {
                code: VotingErrorCode::RecoveryConflict,
                ..
            }))
        ));
    }
    #[tokio::test]
    async fn authorization_requires_identity_after_rediscovery_of_restored_round() {
        let (session, round) = session().await;
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &round.params, None)
            .unwrap();
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        let fetched = Cell::new(false);
        let err = restored
            .ensure_trusted_round_with(
                ID,
                async {
                    fetched.set(true);
                    Ok(super::super::config::tests::discover(
                        vec![super::super::config::tests::active()],
                        1500,
                    )
                    .await)
                },
                || Ok(1500),
            )
            .await
            .unwrap_err();
        assert!(fetched.get());
        assert_eq!(err.code, VotingErrorCode::RecoveryConflict);
        assert!(!restored.export_state().unwrap().is_empty());
        restored
            .clear_round_with(ID, || async { Ok(()) })
            .await
            .unwrap();
        assert!(!restored.db.has_round(ID).unwrap());
    }
    #[tokio::test]
    async fn authorization_allows_new_round_after_authenticated_discovery() {
        let (session, _) = session().await;
        assert!(!session.db.has_round(ID).unwrap());
        session
            .ensure_trusted_round_with(ID, async { panic!("cached round fetched") }, || Ok(1500))
            .await
            .unwrap();
        session.trusted_rounds.borrow_mut().clear();
        session
            .ensure_trusted_round_with(
                ID,
                async {
                    Ok(super::super::config::tests::discover(
                        vec![super::super::config::tests::active()],
                        1500,
                    )
                    .await)
                },
                || Ok(1500),
            )
            .await
            .unwrap();
        assert!(!session.db.has_round(ID).unwrap());
    }
    async fn reject_stored_protocol_mismatch(field: &str) {
        let (mut session, round) = session().await;
        let mut params = round.params.clone();
        let mut network = zcash_voting::Network::Testnet;
        match field {
            "snapshot" => params.snapshot_height += 1,
            "authority" => params.ea_pk[0] ^= 1,
            "commitment_root" => params.nc_root[0] ^= 1,
            "nullifier_root" => params.nullifier_imt_root[0] ^= 1,
            "network" => network = zcash_voting::Network::Mainnet,
            _ => panic!("unknown fixture field"),
        }
        // A native DB allows testing the authorization boundary's network check
        // independently of open_browser's earlier owner/network rejection.
        session.db = VotingDb::open(":memory:").unwrap();
        session.db.set_wallet_id(&session.wallet_id);
        let envelope = VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        };
        session
            .db
            .init_round(
                network,
                &params,
                Some(&serde_json::to_string(&envelope).unwrap()),
            )
            .unwrap();
        for cached in [true, false] {
            if !cached {
                session.trusted_rounds.borrow_mut().clear();
            }
            let err = session
                .ensure_trusted_round_with(
                    ID,
                    async {
                        assert!(!cached, "current authenticated cache should be reused");
                        Ok(super::super::config::tests::discover(
                            vec![super::super::config::tests::active()],
                            1500,
                        )
                        .await)
                    },
                    || Ok(1500),
                )
                .await
                .unwrap_err();
            assert_eq!(err.code, VotingErrorCode::RecoveryConflict, "{field}");
            assert_eq!(
                err.message,
                VotingApiError::new(VotingErrorCode::RecoveryConflict).message
            );
        }
    }
    #[tokio::test]
    async fn authorization_rejects_stored_snapshot_mismatch() {
        reject_stored_protocol_mismatch("snapshot").await;
    }
    #[tokio::test]
    async fn authorization_rejects_stored_authority_mismatch() {
        reject_stored_protocol_mismatch("authority").await;
    }
    #[tokio::test]
    async fn authorization_rejects_stored_commitment_root_mismatch() {
        reject_stored_protocol_mismatch("commitment_root").await;
    }
    #[tokio::test]
    async fn authorization_rejects_stored_nullifier_root_mismatch() {
        reject_stored_protocol_mismatch("nullifier_root").await;
    }
    #[tokio::test]
    async fn authorization_rejects_stored_network_mismatch() {
        reject_stored_protocol_mismatch("network").await;
    }
    #[tokio::test]
    async fn restored_empty_roster_is_never_complete_even_after_expiry() {
        let (session, round) = session().await;
        let mut identity = round.identity();
        identity.proposals.clear();
        let envelope = VotingSessionEnvelope {
            version: 1,
            authenticated_round: identity,
        };
        session
            .db
            .init_round(
                zcash_voting::Network::Testnet,
                &round.params,
                Some(&serde_json::to_string(&envelope).unwrap()),
            )
            .unwrap();
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        assert!(restored.trusted_rounds.borrow().is_empty());
        assert_eq!(*restored.selected_round_id.borrow(), Some(ID.into()));
        for now in [1500, 2000, 2001] {
            let status = restored.round_status_at(ID, now).unwrap();
            assert_ne!(status.phase, VotingRoundPhase::SharesConfirmed);
            assert_eq!(
                status.phase,
                if now < 2000 {
                    VotingRoundPhase::Uninitialized
                } else {
                    VotingRoundPhase::Expired
                }
            );
        }
    }
    #[tokio::test]
    async fn durable_status_uses_proposal_roster_and_terminal_rounds_do_not_conflict() {
        let (session, round) = session().await;
        let envelope = VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        };
        session
            .db
            .init_round(
                zcash_voting::Network::Testnet,
                &round.params,
                Some(&serde_json::to_string(&envelope).unwrap()),
            )
            .unwrap();
        let status = session.round_status_at(ID, 1500).unwrap();
        assert_eq!(status.phase, VotingRoundPhase::ConfigAuthenticated);
        assert_eq!(status.next_action, "prepare_round");
        assert_eq!(
            session.round_status_at(ID, 2000).unwrap().phase,
            VotingRoundPhase::Expired
        );
        session
            .db
            .set_ballot_intent(ID, 1, zcash_voting::session::Decision::Skipped, 2)
            .unwrap();
        assert_eq!(
            session.round_status_at(ID, 1500).unwrap().phase,
            VotingRoundPhase::SharesConfirmed
        );
        let mut other = round.params.clone();
        other.vote_round_id = "01".repeat(32);
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &other, None)
            .unwrap();
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        assert_eq!(
            *restored.selected_round_id.borrow(),
            Some(other.vote_round_id)
        );
        assert_eq!(
            restored.round_status_at(ID, 1500).unwrap().phase,
            VotingRoundPhase::SharesConfirmed
        );
    }
    #[tokio::test]
    async fn malformed_versioned_envelopes_fail_closed_without_echoing_json() {
        let (session, round) = session().await;
        for json in [
            "private invalid JSON".to_owned(),
            "{}".into(),
            serde_json::to_string(&VotingSessionEnvelope {
                version: 2,
                authenticated_round: round.identity(),
            })
            .unwrap(),
            {
                let mut value = serde_json::to_value(VotingSessionEnvelope {
                    version: 1,
                    authenticated_round: round.identity(),
                })
                .unwrap();
                value["unknown"] = true.into();
                value.to_string()
            },
        ] {
            session
                .db
                .init_round(zcash_voting::Network::Testnet, &round.params, Some(&json))
                .unwrap();
            let err = open(
                session.wallet.clone(),
                session.account_id,
                Some(&session.export_state().unwrap()),
            )
            .err()
            .unwrap();
            assert_eq!(err.code, VotingErrorCode::RecoveryConflict);
            assert!(!err.message.contains("private"));
            session.db.clear_round(ID).unwrap();
        }
    }
    #[tokio::test]
    async fn restored_identity_requires_discovery_and_exact_envelope_match() {
        let (session, round) = session().await;
        let envelope = VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        };
        session
            .db
            .init_round(
                zcash_voting::Network::Testnet,
                &round.params,
                Some(&serde_json::to_string(&envelope).unwrap()),
            )
            .unwrap();
        let restored = open(
            session.wallet.clone(),
            session.account_id,
            Some(&session.export_state().unwrap()),
        )
        .unwrap();
        assert!(restored.cached_round_at(ID, 1500).is_err());
        assert!(restored.validate_saved_identity(&round).is_ok());
        let mut changed = round.clone();
        changed.proposals[0].title.push('!');
        assert_eq!(
            restored.validate_saved_identity(&changed).unwrap_err().code,
            VotingErrorCode::RecoveryConflict
        );
        assert_eq!(
            session.db.round_session_json(ID).unwrap(),
            Some(serde_json::to_string(&envelope).unwrap())
        );
        assert!(session.db.round_session_json(&"01".repeat(32)).is_err());
    }
    #[tokio::test]
    async fn clear_failure_keeps_deleted_db_authoritative_and_allows_checkpoint_retry() {
        let (session, round) = session().await;
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &round.params, None)
            .unwrap();
        session
            .db
            .set_ballot_intent(ID, 1, zcash_voting::session::Decision::Skipped, 2)
            .unwrap();
        *session.selected_round_id.borrow_mut() = Some(ID.into());
        let result = session
            .clear_round_with(ID, || async {
                Err(VotingApiError::new(VotingErrorCode::PersistenceFailed))
            })
            .await;
        assert_eq!(result.unwrap_err().code, VotingErrorCode::PersistenceFailed);
        assert!(!session.db.has_round(ID).unwrap());
        assert!(session.db.ballot_intents(ID).unwrap().is_empty());
        assert!(session.trusted_rounds.borrow().is_empty());
        assert!(session.selected_round_id.borrow().is_none());
        session
            .clear_round_with(ID, || async { Ok(()) })
            .await
            .unwrap();
        assert!(
            open(
                session.wallet.clone(),
                session.account_id,
                Some(&session.export_state().unwrap())
            )
            .unwrap()
            .db
            .list_rounds()
            .unwrap()
            .is_empty()
        );
    }
    #[tokio::test]
    async fn cancelled_clear_releases_gate_and_does_not_reuse_checkpoint_revision() {
        let (session, round) = session().await;
        session
            .db
            .init_round(zcash_voting::Network::Testnet, &round.params, None)
            .unwrap();
        let revisions = RefCell::new(Vec::new());
        let mut clearing = Box::pin(session.clear_round_with(ID, || {
            session.persistence.revision.checkpoint(
                || session.export_state(),
                |_, revision| {
                    revisions.borrow_mut().push(revision);
                    pending::<Result<(), VotingApiError>>()
                },
            )
        }));
        assert!(poll!(&mut clearing).is_pending());
        assert!(!session.db.has_round(ID).unwrap());
        assert_eq!(
            session.mutation_guard().err().unwrap().code,
            VotingErrorCode::OperationInProgress
        );
        // A read can use the last authenticated cache during the pending callback.
        assert_eq!(
            session.round_status_at(ID, 1500).unwrap().phase,
            VotingRoundPhase::ConfigAuthenticated
        );
        drop(clearing);
        assert!(session.trusted_rounds.borrow().is_empty());
        session
            .clear_round_with(ID, || {
                session.persistence.revision.checkpoint(
                    || session.export_state(),
                    |_, revision| {
                        revisions.borrow_mut().push(revision);
                        std::future::ready(Ok(()))
                    },
                )
            })
            .await
            .unwrap();
        assert_eq!(*revisions.borrow(), [1, 2]);
    }
    #[tokio::test]
    #[ignore = "generates public synthetic inputs for the Chrome session harness"]
    async fn write_browser_session_fixtures() {
        let path = std::path::Path::new("/tmp/task-8-browser-fixtures");
        std::fs::create_dir_all(path).unwrap();
        let (session, round) = session().await;
        std::fs::write(
            path.join("wallet.bin"),
            session.wallet.db_to_bytes().await.unwrap(),
        )
        .unwrap();
        let (other, _, _) = wallet(crate::Network::TestNetwork, 18, true);
        std::fs::write(
            path.join("other-wallet.bin"),
            other.db_to_bytes().await.unwrap(),
        )
        .unwrap();
        let envelope = VotingSessionEnvelope {
            version: 1,
            authenticated_round: round.identity(),
        };
        session
            .db
            .init_round(
                zcash_voting::Network::Testnet,
                &round.params,
                Some(&serde_json::to_string(&envelope).unwrap()),
            )
            .unwrap();
        std::fs::write(path.join("voting.bin"), session.export_state().unwrap()).unwrap();
        std::fs::write(path.join("metadata.json"),serde_json::to_vec(&serde_json::json!({"accountId":*session.account_id,"roundId":ID,"storageKey":session.storage_key()})).unwrap()).unwrap();
    }
    #[tokio::test]
    async fn mutation_gate_rejects_overlap_and_releases_on_cancellation() {
        let active = Rc::new(Cell::new(false));
        let mut operation = Box::pin(async {
            let _guard = MutationGuard::acquire(&active).unwrap();
            pending::<()>().await;
        });
        assert!(poll!(&mut operation).is_pending());
        assert!(matches!(
            MutationGuard::acquire(&active),
            Err(VotingApiError {
                code: VotingErrorCode::OperationInProgress,
                ..
            })
        ));
        drop(operation);
        assert!(MutationGuard::acquire(&active).is_ok());
    }
}
