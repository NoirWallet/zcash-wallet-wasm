//! The public browser surface keeps SQLite and JavaScript on its owning thread.
use super::wallet::WebWallet;
use crate::voting::{
    config,
    persistence::Persistence,
    session::{VotingSessionInner, now_secs},
    types::{VotingApiError, VotingErrorCode},
};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WebVotingSession {
    pub(crate) inner: Rc<VotingSessionInner>,
}

fn value(value: &impl serde::Serialize) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value)
        .map_err(|_| VotingApiError::new(VotingErrorCode::RecoveryConflict).into())
}

#[wasm_bindgen]
impl WebVotingSession {
    #[wasm_bindgen(constructor)]
    pub fn new(
        wallet: &WebWallet,
        account_id: u32,
        voting_db_bytes: Option<Box<[u8]>>,
        persist_checkpoint: js_sys::Function,
    ) -> Result<WebVotingSession, JsValue> {
        Ok(Self {
            inner: Rc::new(VotingSessionInner::open(
                wallet.shared_inner(),
                account_id.into(),
                voting_db_bytes.as_deref(),
                Persistence::new(persist_checkpoint),
            )?),
        })
    }

    #[wasm_bindgen(getter, js_name = storageKey)]
    pub fn storage_key(&self) -> String {
        self.inner.storage_key()
    }

    #[wasm_bindgen(js_name = exportState)]
    pub fn export_state(&self) -> Result<Box<[u8]>, JsValue> {
        self.inner
            .export_state()
            .map(Vec::into_boxed_slice)
            .map_err(Into::into)
    }

    #[wasm_bindgen(js_name = discoverRounds)]
    pub async fn discover_rounds(&self) -> Result<JsValue, JsValue> {
        value(
            &self
                .inner
                .discover_with(config::discover_rounds(&self.inner.transport))
                .await?,
        )
    }

    #[wasm_bindgen(js_name = checkEligibility)]
    pub async fn check_eligibility(&self, round_id: String) -> Result<JsValue, JsValue> {
        value(&self.inner.check_eligibility_at(&round_id, now_secs).await?)
    }

    #[wasm_bindgen(js_name = roundStatus)]
    pub async fn round_status(&self, round_id: String) -> Result<JsValue, JsValue> {
        value(&self.inner.round_status_at(&round_id, now_secs()?)?)
    }

    #[wasm_bindgen(js_name = prepareRound)]
    pub async fn prepare_round(&self, round_id: String) -> Result<JsValue, JsValue> {
        value(&self.inner.prepare_round(&round_id).await?)
    }

    #[wasm_bindgen(js_name = submitDelegation)]
    pub async fn submit_delegation(
        &self,
        round_id: String,
        usk: super::keys::UnifiedSpendingKey,
        seed_fingerprint: super::keys::SeedFingerprint,
    ) -> Result<JsValue, JsValue> {
        let (usk, network, index, key_fingerprint) = usk.into_voting_parts();
        let fingerprint: zip32::fingerprint::SeedFingerprint = seed_fingerprint.into();
        let account = self.inner.validate_delegation_key(&usk, fingerprint)?;
        if !matches!(network, crate::Network::TestNetwork)
            || u32::from(index) != account.account_index
            || key_fingerprint != Some(fingerprint)
        {
            return Err(VotingApiError::new(VotingErrorCode::RecoveryConflict).into());
        }
        value(
            &self
                .inner
                .submit_delegation(&round_id, usk, fingerprint)
                .await?,
        )
    }

    /// The host obtains explicit user confirmation naming this round before calling.
    #[wasm_bindgen(js_name = clearRound)]
    pub async fn clear_round(&self, round_id: String) -> Result<(), JsValue> {
        self.inner
            .clear_round_with(&round_id, || {
                self.inner.persistence.checkpoint(&self.inner.db)
            })
            .await
            .map_err(Into::into)
    }
}
