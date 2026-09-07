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
        if let Some(saved) = self.saved_identity(&round.summary.round_id)? {
            round.compare_identity(&saved)?;
        }
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
            status.phase = if plan.all_decided && !plan.pending_recovery {
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
mod tests {
    use super::*;
    use futures_util::{future::pending, poll};
    use zcash_client_backend::data_api::{
        AccountBirthday, AccountPurpose, WalletWrite, Zip32Derivation,
    };
    use zcash_protocol::consensus::BlockHeight;
    const ID: &str = "0000000000000000000000000000000000000000000000000000000000000001";

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
