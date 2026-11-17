//! Fund-critical transfer path (address/key derivation, payment construction and fees,
//! PCZT signing-and-broadcast).
//!
//! Split out of `src/wallet.rs` so that the fund-critical surface is one reviewable file;
//! this is a pure move, the code is unchanged. Callers reach these items through
//! `use fund_core::*;` re-exported by the parent module.

use super::*;

// ---------------------------------------------------------------- fee / policy helpers

pub(super) fn dynamic_zip317_fee_rule(marginal_fee_zats: u64) -> Result<prim_zip317::FeeRule, Error> {
    if marginal_fee_zats == 0 {
        return Err(Error::Generic("marginal_fee_zats must be greater than 0".to_string()));
    }

    prim_zip317::FeeRule::non_standard(
        Zatoshis::from_u64(marginal_fee_zats)?,
        prim_zip317::GRACE_ACTIONS,
        prim_zip317::P2PKH_STANDARD_INPUT_SIZE,
        prim_zip317::P2PKH_STANDARD_OUTPUT_SIZE,
    )
    .ok_or_else(|| Error::Generic("Invalid ZIP-317 fee parameters".to_string()))
}

pub(super) fn validate_expiry_delta_blocks(expiry_delta_blocks: u32) -> Result<u32, Error> {
    if expiry_delta_blocks == 0 {
        Err(Error::Generic("expiry_delta_blocks must be greater than 0".to_string()))
    } else {
        Ok(expiry_delta_blocks)
    }
}

pub(super) fn transfer_expiry_height(min_target_height: TargetHeight, expiry_delta_blocks: Option<u32>) -> Result<Option<BlockHeight>, Error> {
    expiry_delta_blocks
        .map(validate_expiry_delta_blocks)
        .transpose()
        .map(|delta| delta.map(|delta| BlockHeight::from(min_target_height) + delta))
}

pub(super) fn transparent_transfer_policies() -> (SpendPolicy, TransparentChangePolicy) {
    (
        SpendPolicy::shielded_pools(std::iter::empty::<ShieldedPool>()).with_transparent(TransparentSpendPolicy::any_account_addr()),
        TransparentChangePolicy::TransparentChangeAllowed,
    )
}

pub(super) fn hardware_orchard_ovk_policy(ufvk: &UnifiedFullViewingKey) -> Result<OvkPolicy, Error> {
    let orchard_fvk = ufvk
        .orchard()
        .ok_or_else(|| Error::Generic("Hardware transaction review requires an Orchard viewing key".to_owned()))?;

    Ok(OvkPolicy::Custom {
        external_ovk: orchard_fvk.to_ovk(orchard::keys::Scope::External).into(),
        internal_ovk: Some(orchard_fvk.to_ovk(orchard::keys::Scope::Internal).into()),
    })
}

pub(super) fn transfer_policies(source: TransferFundingSource) -> (SpendPolicy, TransparentChangePolicy) {
    match source {
        TransferFundingSource::Shielded => (SpendPolicy::default(), TransparentChangePolicy::ShieldChange),
        TransferFundingSource::Transparent => transparent_transfer_policies(),
    }
}

pub(super) fn allow_exact_send_max_fallback(source: TransferFundingSource, has_dynamic_fee: bool) -> bool {
    source == TransferFundingSource::Shielded && has_dynamic_fee
}

pub(super) fn proposal_fee_required_zats<FeeRuleT, NoteRef>(proposal: &zcash_client_backend::proposal::Proposal<FeeRuleT, NoteRef>) -> Result<u64, Error> {
    let fee = proposal.steps().iter().try_fold(Zatoshis::ZERO, |total, step| {
        (total + step.balance().fee_required()).ok_or(zcash_protocol::value::BalanceError::Overflow)
    })?;

    Ok(fee.into())
}

pub(super) fn proposal_payment_amount_zats<FeeRuleT, NoteRef>(proposal: &zcash_client_backend::proposal::Proposal<FeeRuleT, NoteRef>) -> Result<u64, Error> {
    let amount = proposal
        .steps()
        .iter()
        .try_fold(Zatoshis::ZERO, |step_total, step| -> Result<Zatoshis, Error> {
            step.transaction_request().payments().values().try_fold(step_total, |total, payment| {
                let payment_amount = payment
                    .amount()
                    .ok_or_else(|| Error::Generic("Max-transfer proposal omitted a payment amount".to_string()))?;
                (total + payment_amount)
                    .ok_or(zcash_protocol::value::BalanceError::Overflow)
                    .map_err(Error::from)
            })
        })?;

    Ok(amount.into())
}

pub(super) fn transfer_estimate_from_proposal<FeeRuleT, NoteRef>(
    proposal: &zcash_client_backend::proposal::Proposal<FeeRuleT, NoteRef>,
    marginal_fee_zats: u64,
) -> Result<TransferEstimate, Error> {
    TransferEstimate::from_amount_and_step_fees(
        Zatoshis::from_u64(proposal_payment_amount_zats(proposal)?)?,
        proposal.steps().iter().map(|step| step.balance().fee_required()),
        marginal_fee_zats,
    )
}

pub(super) fn create_and_store_transfer<W, FeeRuleT>(
    db: &mut W,
    network: &Network,
    usk: UnifiedSpendingKey,
    proposal: &zcash_client_backend::proposal::Proposal<FeeRuleT, <W as InputSource>::NoteRef>,
    target_expiry_height: Option<BlockHeight>,
) -> Result<NonEmpty<TxId>, Error>
where
    W: WalletWrite + WalletCommitmentTrees + InputSource,
    FeeRuleT: FeeRule,
    FeeRuleT::Error: Debug,
    <W as WalletRead>::Error: Debug,
    <W as WalletCommitmentTrees>::Error: Debug,
    <W as InputSource>::NoteRef: Debug,
{
    let prover = get_prover();
    let spending_keys = SpendingKeys::from_unified_spending_key(usk);
    let result = create_proposed_transactions::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <FeeRuleT as FeeRule>::Error, _>(
        db,
        network,
        &WalletClock,
        &mut UnwrapErr(SysRng),
        prover,
        prover,
        &spending_keys,
        OvkPolicy::Sender,
        proposal,
        target_expiry_height,
    );

    result.map_err(|error| Error::Generic(format!("Failed to create transfer transactions: {error:?}")))
}

impl TransferEstimate {
    pub(super) fn from_amount_and_step_fees(amount: Zatoshis, step_fees: impl IntoIterator<Item = Zatoshis>, marginal_fee_zats: u64) -> Result<Self, Error> {
        let mut fee = Zatoshis::ZERO;
        let mut step_fees_zats = vec![];

        for step_fee in step_fees {
            fee = (fee + step_fee).ok_or(zcash_protocol::value::BalanceError::Overflow)?;
            step_fees_zats.push(u64::from(step_fee).to_string());
        }

        Ok(Self {
            amount_zats: u64::from(amount).to_string(),
            fee_zats: u64::from(fee).to_string(),
            marginal_fee_zats: marginal_fee_zats.to_string(),
            transaction_count: step_fees_zats.len(),
            step_fees_zats,
        })
    }
}

impl<W, T, AccountId, NoteRef> Wallet<W, T>
where
    W: WalletRead<AccountId = AccountId>
        + WalletWrite
        + InputSource<AccountId = <W as WalletRead>::AccountId, Error = <W as WalletRead>::Error, NoteRef = NoteRef>
        + WalletCommitmentTrees,
    AccountId: Copy + Debug + Eq + Hash + Default + Send + Sync + ConditionallySelectable + Serialize + DeserializeOwned + 'static,
    NoteRef: Copy + Eq + Ord + Debug,
    Error: From<<W as WalletRead>::Error>,
    <W as WalletRead>::Error: std::error::Error + Send + Sync + 'static,
    <W as WalletCommitmentTrees>::Error: std::error::Error + Send + Sync + 'static,
    // GRPC connection Trait Bounds
    T: GrpcService<tonic::body::Body> + Clone,
    T::Error: Into<StdError>,
    T::ResponseBody: Body<Data = Bytes> + std::marker::Send + 'static,
    <T::ResponseBody as Body>::Error: Into<StdError> + std::marker::Send,
{
    // ---------------------------------------------------------------- accounts and keys

    pub async fn create_account(
        &self,
        account_name: &str,
        seed_phrase: &str,
        passphrase: &str,
        account_hd_index: u32,
        birthday_height: Option<u32>,
        key_source: Option<&str>,
    ) -> Result<AccountId, Error> {
        // decode the mnemonic and derive the first account
        let (usk, seed_fp) = usk_from_seed_str(seed_phrase, passphrase, account_hd_index, &self.network)?;
        let ufvk = usk.to_unified_full_viewing_key();

        let derivation = Zip32Derivation::new(seed_fp, zip32::AccountId::try_from(account_hd_index)?);

        tracing::info!("Key successfully decoded. Importing into wallet");

        self.import_account_ufvk(
            account_name,
            &ufvk,
            birthday_height,
            AccountPurpose::Spending { derivation: Some(derivation) },
            key_source,
        )
        .await
    }

    /// Import a watch-only spending account from an encoded UFVK (hardware wallet flow).
    ///
    /// The ZIP-32 seed fingerprint and HD index exported by the device (e.g. Keystone
    /// `zcash-accounts` QR) are recorded as the account derivation so that PCZTs created
    /// from this account carry the metadata the device needs to locate its signing keys.
    ///
    /// This is a thin wrapper around `import_account_ufvk`; it adds no new
    /// import logic.
    pub async fn create_account_ufvk(
        &self,
        account_name: &str,
        ufvk_encoded: &str,
        seed_fingerprint: [u8; 32],
        account_hd_index: u32,
        birthday_height: Option<u32>,
        key_source: Option<&str>,
    ) -> Result<AccountId, Error> {
        let ufvk = UnifiedFullViewingKey::decode(&self.network, ufvk_encoded).map_err(|error| Error::KeyDecoding(error.to_string()))?;

        let derivation = Zip32Derivation::new(SeedFingerprint::from_bytes(seed_fingerprint), zip32::AccountId::try_from(account_hd_index)?);

        tracing::info!("Importing UFVK account (hd_index: {})", account_hd_index);

        self.import_account_ufvk(
            account_name,
            &ufvk,
            birthday_height,
            AccountPurpose::Spending { derivation: Some(derivation) },
            key_source,
        )
        .await
    }

    pub(super) async fn import_account_ufvk(
        &self,
        account_name: &str,
        ufvk: &UnifiedFullViewingKey,
        birthday_height: Option<u32>,
        purpose: AccountPurpose,
        key_source: Option<&str>,
    ) -> Result<AccountId, Error> {
        // Check if an account with this name already exists
        match self.get_account_id_by_name(account_name).await {
            Ok(_) => {
                // Account exists
                return Err(Error::AccountExists(account_name.to_string()));
            }
            Err(Error::AccountNameNotFound) => {
                // Account doesn't exist, continue
            }
            Err(e) => {
                // Some other error occurred
                return Err(e);
            }
        }

        let mut client = self.client.clone();
        let birthday = match birthday_height {
            Some(height) => height,
            None => {
                let chain_tip: u32 = client
                    .get_latest_block(service::ChainSpec::default())
                    .await?
                    .into_inner()
                    .height
                    .try_into()
                    .map_err(|_| Error::Birthday)?;
                chain_tip.checked_sub(100).ok_or(Error::Birthday)?
            }
        };
        // Construct an `AccountBirthday` for the account's birthday.
        let birthday = {
            // Fetch the tree state corresponding to the last block prior to the wallet's
            // birthday height. NOTE: THIS APPROACH LEAKS THE BIRTHDAY TO THE SERVER!
            let request = service::BlockId {
                height: birthday.checked_sub(1).ok_or(Error::Birthday)?.into(),
                ..Default::default()
            };
            let treestate = client.get_tree_state(request).await?.into_inner();
            AccountBirthday::from_treestate(treestate, None).map_err(|_| Error::Birthday)?
        };

        // Import the account first
        let account_id = {
            let mut db = self.db.write().await;
            db.import_account_ufvk(account_name, ufvk, &birthday, purpose, key_source)?.id()
        };

        // IMPORTANT: Ensure that the transparent address at diversifier index 0 is stored.
        //
        // Account::new() uses AllAvailableKeys which requires ALL key types to have valid
        // receivers. When Sapling has no valid diversifier at index 0 (which is common), it
        // skips to a higher index (e.g., 7). This means the transparent address at index 0
        // is never stored in account.addresses().
        //
        // External tools (exchanges, other wallets) typically derive transparent addresses
        // starting from index 0 (BIP44 path m/44'/133'/account'/0/0). If funds were sent
        // to the index-0 transparent address, get_transparent_receivers() won't return it,
        // refresh_utxos() won't query lightwalletd for it, and the balance will show as 0.
        //
        // Fix: Explicitly store the address at diversifier 0 using ALLOW_ALL (which does
        // not require Sapling), then also generate the next available address for metadata.
        use zcash_keys::keys::UnifiedAddressRequest;
        use zip32::DiversifierIndex;

        {
            let mut db = self.db.write().await;
            // Store address at diversifier 0 to ensure low-index transparent addresses
            // are discoverable by get_transparent_receivers() during sync.
            let _ = db.get_address_for_index(account_id, DiversifierIndex::new(), UnifiedAddressRequest::ALLOW_ALL)?;
            // Also generate the next available address for transparent address metadata
            // needed when spending transparent funds.
            let _ = db.get_next_available_address(account_id, UnifiedAddressRequest::ALLOW_ALL)?;
        }

        Ok(account_id)
    }

    // ---------------------------------------------------------------- transfer construction and fees

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn propose_unified_transfer<FeeRuleT>(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        fee_rule: FeeRuleT,
        funding_source: TransferFundingSource,
        allow_send_max_fallback: bool,
        expiry_delta_blocks: Option<u32>,
    ) -> Result<zcash_client_backend::proposal::Proposal<FeeRuleT, <W as InputSource>::NoteRef>, Error>
    where
        FeeRuleT: FeeRule + Clone + zcash_client_backend::fees::zip317::Zip317FeeRule,
        FeeRuleT::Error: Debug + From<zcash_protocol::value::BalanceError>,
    {
        let (spend_policy, transparent_change_policy) = transfer_policies(funding_source);
        let change_strategy = zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
            fee_rule.clone(),
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        )
        .with_transparent_change_policy(transparent_change_policy);

        let input_selector = GreedyInputSelector::new();
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment =
            Payment::new(to_address.clone(), Some(Zatoshis::from_u64(value)?), memo.clone(), None, None, vec![]).map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
        let mut db = self.db.write().await;

        let confirmations_policy = self.transfer_confirmations_policy(&*db, expiry_delta_blocks)?;
        let propose = if expiry_delta_blocks.is_some() {
            ordinary::propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>
        } else {
            propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>
        };
        match propose(
            &mut *db,
            &self.network,
            account_id,
            &input_selector,
            &change_strategy,
            request,
            confirmations_policy,
            &spend_policy,
            None,
            None,
        ) {
            Ok(proposal) => Ok(proposal),
            Err(transfer_error) => {
                if allow_send_max_fallback {
                    let max_proposal = propose_send_max_transfer::<_, _, _, <W as WalletCommitmentTrees>::Error>(
                        &mut *db,
                        &self.network,
                        account_id,
                        &[ShieldedPool::Sapling, ShieldedPool::Orchard, ShieldedPool::Ironwood],
                        &fee_rule,
                        to_address,
                        memo,
                        MaxSpendMode::MaxSpendable,
                        confirmations_policy,
                        &Default::default(),
                        None,
                    )
                    .ok();

                    if let Some(proposal) = max_proposal
                        && proposal_payment_amount_zats(&proposal)? == value
                    {
                        return Ok(proposal);
                    }
                }

                let message = format!("Failed to propose transfer. Possibly insufficient permitted funds: {transfer_error:?}");
                Err(if matches!(transfer_error, WalletError::InsufficientFunds { .. }) {
                    Error::InsufficientTransferFunds(message)
                } else {
                    Error::Generic(message)
                })
            }
        }
    }

    pub(super) async fn resolve_transfer_fee(&self, supplied_fee: Option<u64>) -> Result<Option<u64>, Error> {
        if let Some(fee) = supplied_fee {
            dynamic_zip317_fee_rule(fee)?;
            return Ok(Some(fee));
        }

        let mut client = self.client.clone();
        let recommended = tokio_with_wasm::alias::time::timeout(STANDARD_FEE_QUERY_TIMEOUT, client.get_standard_fee(service::Empty {})).await;
        match recommended {
            Ok(Ok(response)) => {
                let fee = response.into_inner().fee_zat;
                if dynamic_zip317_fee_rule(fee).is_ok() {
                    Ok(Some(fee))
                } else {
                    tracing::warn!(fee, "Invalid recommended marginal fee; using standard ZIP-317 fee");
                    Ok(None)
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "Standard fee RPC unavailable; using standard ZIP-317 fee");
                Ok(None)
            }
            Err(_) => {
                tracing::warn!("Standard fee RPC timed out; using standard ZIP-317 fee");
                Ok(None)
            }
        }
    }

    pub(super) async fn execute_unified_transfer<FeeRuleT>(
        &self,
        usk: UnifiedSpendingKey,
        proposal: zcash_client_backend::proposal::Proposal<FeeRuleT, <W as InputSource>::NoteRef>,
        expiry_delta_blocks: Option<u32>,
    ) -> Result<TransferResult, Error>
    where
        FeeRuleT: FeeRule,
        FeeRuleT::Error: Debug,
    {
        let expiry = transfer_expiry_height(proposal.min_target_height(), expiry_delta_blocks)?;
        let txids = {
            let mut db = self.db.write().await;
            create_and_store_transfer(&mut *db, &self.network, usk, &proposal, expiry)?
        };
        self.submit_stored_transfer_transactions(&txids).await
    }

    pub async fn pczt_create(&self, account_id: AccountId, to_address: ZcashAddress, value: u64, memo: Option<Vec<u8>>) -> Result<Pczt, Error> {
        self.pczt_create_with_policy(
            account_id,
            to_address,
            value,
            memo,
            SpendPolicy::default(),
            TransparentChangePolicy::ShieldChange,
        )
        .await
    }

    pub async fn pczt_create_transparent(&self, account_id: AccountId, to_address: ZcashAddress, value: u64, memo: Option<Vec<u8>>) -> Result<Pczt, Error> {
        let (spend_policy, transparent_change_policy) = transparent_transfer_policies();
        self.pczt_create_with_policy(account_id, to_address, value, memo, spend_policy, transparent_change_policy)
            .await
    }

    pub(super) async fn pczt_create_with_policy(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        spend_policy: SpendPolicy,
        transparent_change_policy: TransparentChangePolicy,
    ) -> Result<Pczt, Error> {
        let change_strategy = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        )
        .with_transparent_change_policy(transparent_change_policy);

        let input_selector = GreedyInputSelector::new();
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment = Payment::new(to_address, Some(Zatoshis::from_u64(value)?), memo, None, None, vec![]).map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
        let mut db = self.db.write().await;
        let proposal = propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &input_selector,
            &change_strategy,
            request,
            self.selection_confirmations_policy(),
            &spend_policy,
            None,
            None,
        )
        .map_err(|error| {
            let message = format!("Failed to propose transfer. Possibly insufficient permitted funds: {error:?}");
            if matches!(error, WalletError::InsufficientFunds { .. }) {
                Error::InsufficientTransferFunds(message)
            } else {
                Error::Generic(message)
            }
        })?;
        let pczt = create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <StandardFeeRule as FeeRule>::Error, _>(
            &mut *db,
            &self.network,
            &WalletClock,
            &mut UnwrapErr(SysRng),
            account_id,
            OvkPolicy::Sender,
            &proposal,
            None,
            zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
        )
        .map_err(|e| Error::Generic(format!("Failed to create PCZT for transfer: {e:?}")))?;
        Ok(pczt)
    }

    pub async fn pczt_create_dynamic_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<Pczt, Error> {
        self.pczt_create_dynamic_fee_impl(
            account_id,
            to_address,
            value,
            memo,
            marginal_fee_zats,
            None,
            SpendPolicy::default(),
            TransparentChangePolicy::ShieldChange,
            true,
            false,
        )
        .await
    }

    pub async fn pczt_create_dynamic_fee_with_expiry_delta(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
        expiry_delta_blocks: u32,
    ) -> Result<Pczt, Error> {
        let expiry_delta_blocks = validate_expiry_delta_blocks(expiry_delta_blocks)?;
        self.pczt_create_dynamic_fee_impl(
            account_id,
            to_address,
            value,
            memo,
            marginal_fee_zats,
            Some(expiry_delta_blocks),
            SpendPolicy::default(),
            TransparentChangePolicy::ShieldChange,
            true,
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn pczt_create_dynamic_fee_impl(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
        expiry_delta_blocks: Option<u32>,
        spend_policy: SpendPolicy,
        transparent_change_policy: TransparentChangePolicy,
        allow_send_max_fallback: bool,
        hardware_orchard_ovk: bool,
    ) -> Result<Pczt, Error> {
        let change_strategy = zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
            dynamic_zip317_fee_rule(marginal_fee_zats)?,
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        )
        .with_transparent_change_policy(transparent_change_policy);

        let input_selector = GreedyInputSelector::new();
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment =
            Payment::new(to_address.clone(), Some(Zatoshis::from_u64(value)?), memo.clone(), None, None, vec![]).map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
        let mut db = self.db.write().await;
        let confirmations_policy = self.transfer_confirmations_policy(&*db, expiry_delta_blocks)?;
        let propose = if expiry_delta_blocks.is_some() {
            ordinary::propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>
        } else {
            propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>
        };
        let proposal = match propose(
            &mut *db,
            &self.network,
            account_id,
            &input_selector,
            &change_strategy,
            request,
            confirmations_policy,
            &spend_policy,
            None,
            None,
        ) {
            Ok(proposal) => proposal,
            Err(transfer_error) => {
                let max_proposal = if allow_send_max_fallback {
                    propose_send_max_transfer::<_, _, _, <W as WalletCommitmentTrees>::Error>(
                        &mut *db,
                        &self.network,
                        account_id,
                        &[ShieldedPool::Sapling, ShieldedPool::Orchard, ShieldedPool::Ironwood],
                        &dynamic_zip317_fee_rule(marginal_fee_zats)?,
                        to_address,
                        memo,
                        MaxSpendMode::MaxSpendable,
                        confirmations_policy,
                        &Default::default(),
                        None,
                    )
                    .ok()
                } else {
                    None
                };

                match max_proposal {
                    Some(proposal) if proposal_payment_amount_zats(&proposal)? == value => proposal,
                    _ => {
                        let message = format!("Failed to propose transfer. Possibly insufficient permitted funds: {transfer_error:?}");
                        return Err(if matches!(transfer_error, WalletError::InsufficientFunds { .. }) {
                            Error::InsufficientTransferFunds(message)
                        } else {
                            Error::Generic(message)
                        });
                    }
                }
            }
        };
        let ovk_policy = if hardware_orchard_ovk {
            let account = db
                .get_account(account_id)?
                .ok_or_else(|| Error::Generic("Hardware transaction account not found".to_owned()))?;
            hardware_orchard_ovk_policy(
                account
                    .ufvk()
                    .ok_or_else(|| Error::Generic("Hardware transaction account has no viewing key".to_owned()))?,
            )?
        } else {
            OvkPolicy::Sender
        };
        let pczt = match expiry_delta_blocks {
            Some(expiry_delta_blocks) => {
                create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <prim_zip317::FeeRule as FeeRule>::Error, _>(
                    &mut *db,
                    &self.network,
                    &WalletClock,
                    &mut UnwrapErr(SysRng),
                    account_id,
                    ovk_policy.clone(),
                    &proposal,
                    Some(BlockHeight::from(proposal.min_target_height()) + expiry_delta_blocks),
                    zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
                )
            }
            None => create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <prim_zip317::FeeRule as FeeRule>::Error, _>(
                &mut *db,
                &self.network,
                &WalletClock,
                &mut UnwrapErr(SysRng),
                account_id,
                ovk_policy,
                &proposal,
                None,
                zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
            ),
        }
        .map_err(|e| Error::Generic(format!("Failed to create PCZT for transfer: {e:?}")))?;
        Ok(pczt)
    }

    pub async fn pczt_create_transparent_dynamic_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<Pczt, Error> {
        let (spend_policy, transparent_change_policy) = transparent_transfer_policies();
        self.pczt_create_dynamic_fee_impl(
            account_id,
            to_address,
            value,
            memo,
            marginal_fee_zats,
            None,
            spend_policy,
            transparent_change_policy,
            false,
            false,
        )
        .await
    }

    /// Creates a transparent-funded PCZT whose shielded outputs can be reviewed by an
    /// external hardware signer. Ledger derives Orchard review keys from the ZIP-32
    /// account path, so external Ironwood outputs must use that account's Orchard OVK
    /// even though ZIP-316 would ordinarily select the transparent OVK for this input set.
    pub async fn pczt_create_transparent_hardware_dynamic_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<Pczt, Error> {
        let (spend_policy, transparent_change_policy) = transparent_transfer_policies();
        self.pczt_create_dynamic_fee_impl(
            account_id,
            to_address,
            value,
            memo,
            marginal_fee_zats,
            None,
            spend_policy,
            transparent_change_policy,
            false,
            true,
        )
        .await
    }

    pub async fn pczt_create_transparent_dynamic_fee_with_expiry_delta(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
        expiry_delta_blocks: u32,
    ) -> Result<Pczt, Error> {
        let expiry_delta_blocks = validate_expiry_delta_blocks(expiry_delta_blocks)?;
        let (spend_policy, transparent_change_policy) = transparent_transfer_policies();
        self.pczt_create_dynamic_fee_impl(
            account_id,
            to_address,
            value,
            memo,
            marginal_fee_zats,
            Some(expiry_delta_blocks),
            spend_policy,
            transparent_change_policy,
            false,
            false,
        )
        .await
    }

    /// Sweeps all currently spendable Orchard notes into the Ironwood pool.
    ///
    /// The destination must belong to the same wallet account. The proposal is restricted to
    /// Orchard inputs and transaction V6; after NU6.3, its Orchard receiver is carried by the
    /// Ironwood bundle.
    pub async fn pczt_create_orchard_migration(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
        expiry_delta_blocks: Option<u32>,
    ) -> Result<Pczt, Error> {
        let expiry_delta_blocks = expiry_delta_blocks.map(validate_expiry_delta_blocks).transpose()?;
        let wallet_address = zcash_client_backend::address::Address::try_from_zcash_address(&self.network, to_address.clone())
            .map_err(|e| Error::Generic(format!("Invalid migration address: {e:?}")))?;
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let fee_rule = dynamic_zip317_fee_rule(marginal_fee_zats)?;
        let mut db = self.db.write().await;

        let destination_account = db
            .find_account_for_address(&self.network, &wallet_address)
            .map_err(|e| Error::Generic(format!("Failed to validate migration address: {e:?}")))?;
        if destination_account != Some(account_id) {
            return Err(Error::Generic("Orchard migration destination must belong to the source account".into()));
        }

        let target_height = db
            .chain_height()?
            .map(|height| height + 1)
            .ok_or_else(|| Error::Generic("Wallet must be synced before migration".into()))?;
        let activation_height = self
            .network
            .activation_height(zcash_protocol::consensus::NetworkUpgrade::Nu6_3)
            .ok_or_else(|| Error::Generic("NU6.3 is not configured for this network".into()))?;
        if target_height < activation_height {
            return Err(Error::Generic(format!("Ironwood is not active until height {}", u32::from(activation_height))));
        }

        let confirmations_policy = self.transfer_confirmations_policy(&*db, expiry_delta_blocks)?;
        let proposal = propose_send_max_transfer::<_, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &[ShieldedPool::Orchard],
            &fee_rule,
            to_address,
            memo,
            MaxSpendMode::MaxSpendable,
            confirmations_policy,
            &Default::default(),
            None,
        )
        .map_err(|e| Error::Generic(format!("Failed to propose Orchard migration: {e:?}")))?
        .with_proposed_version(Some(TxVersion::V6));

        create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <prim_zip317::FeeRule as FeeRule>::Error, _>(
            &mut *db,
            &self.network,
            &WalletClock,
            &mut UnwrapErr(SysRng),
            account_id,
            OvkPolicy::Sender,
            &proposal,
            expiry_delta_blocks.map(|delta| BlockHeight::from(proposal.min_target_height()) + delta),
            zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
        )
        .map_err(|e| Error::Generic(format!("Failed to create migration PCZT: {e:?}")))
    }

    pub async fn pczt_shield(&self, account_id: AccountId) -> Result<Pczt, Error> {
        let change_strategy = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        );

        let input_selector = GreedyInputSelector::new();
        let mut db = self.db.write().await;

        // Get all transparent receivers for this account to use as shielding sources
        let max_height = match db.chain_height()? {
            Some(max_height) => max_height,
            None => {
                return Err(Error::Generic("Havent scanned yet, cant shield".to_string()));
            }
        };

        let transparent_balances = db.get_transparent_balances(account_id, (max_height + 1).into(), self.selection_confirmations_policy())?;
        let from_addrs = transparent_balances.into_keys().collect::<Vec<_>>();

        if from_addrs.is_empty() {
            return Err(Error::Generic(
                "No transparent addresses found for this account. Make sure the account has received funds to a transparent address.".to_string(),
            ));
        }

        tracing::info!("Shielding from {} transparent addresses", from_addrs.len());

        let proposal = propose_shielding::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            &input_selector,
            &change_strategy,
            SHIELDING_THRESHOLD,
            &from_addrs,
            account_id,
            self.selection_confirmations_policy(),
            CoinbaseFilter::AllTransparentOutputs,
            None,
        )
        .map_err(|e| Error::Generic(format!("Error when shielding: {e:?}")))?;

        let pczt = create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <StandardFeeRule as FeeRule>::Error, _>(
            &mut *db,
            &self.network,
            &WalletClock,
            &mut UnwrapErr(SysRng),
            account_id,
            OvkPolicy::Sender,
            &proposal,
            None,
            zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
        )
        .map_err(|e| Error::Generic(format!("Failed to create PCZT for shielding: {e:?}")))?;

        Ok(pczt)
    }

    pub async fn pczt_shield_dynamic_fee(&self, account_id: AccountId, marginal_fee_zats: u64) -> Result<Pczt, Error> {
        self.pczt_shield_dynamic_fee_impl(account_id, marginal_fee_zats, None).await
    }

    pub async fn pczt_shield_dynamic_fee_with_expiry_delta(
        &self,
        account_id: AccountId,
        marginal_fee_zats: u64,
        expiry_delta_blocks: u32,
    ) -> Result<Pczt, Error> {
        let expiry_delta_blocks = validate_expiry_delta_blocks(expiry_delta_blocks)?;
        self.pczt_shield_dynamic_fee_impl(account_id, marginal_fee_zats, Some(expiry_delta_blocks))
            .await
    }

    pub(super) async fn pczt_shield_dynamic_fee_impl(
        &self,
        account_id: AccountId,
        marginal_fee_zats: u64,
        expiry_delta_blocks: Option<u32>,
    ) -> Result<Pczt, Error> {
        let change_strategy = zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
            dynamic_zip317_fee_rule(marginal_fee_zats)?,
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        );

        let input_selector = GreedyInputSelector::new();
        let mut db = self.db.write().await;

        // Get all transparent receivers for this account to use as shielding sources
        let max_height = match db.chain_height()? {
            Some(max_height) => max_height,
            None => {
                return Err(Error::Generic("Havent scanned yet, cant shield".to_string()));
            }
        };

        let transparent_balances = db.get_transparent_balances(account_id, (max_height + 1).into(), self.selection_confirmations_policy())?;
        let from_addrs = transparent_balances.into_keys().collect::<Vec<_>>();

        if from_addrs.is_empty() {
            return Err(Error::Generic(
                "No transparent addresses found for this account. Make sure the account has received funds to a transparent address.".to_string(),
            ));
        }

        tracing::info!(
            "Shielding from {} transparent addresses with ZIP-317 marginal fee {} zats",
            from_addrs.len(),
            marginal_fee_zats,
        );

        let proposal = propose_shielding::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            &input_selector,
            &change_strategy,
            SHIELDING_THRESHOLD,
            &from_addrs,
            account_id,
            self.selection_confirmations_policy(),
            CoinbaseFilter::AllTransparentOutputs,
            None,
        )
        .map_err(|e| Error::Generic(format!("Error when shielding: {e:?}")))?;

        let pczt = match expiry_delta_blocks {
            Some(expiry_delta_blocks) => {
                create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <prim_zip317::FeeRule as FeeRule>::Error, _>(
                    &mut *db,
                    &self.network,
                    &WalletClock,
                    &mut UnwrapErr(SysRng),
                    account_id,
                    OvkPolicy::Sender,
                    &proposal,
                    Some(BlockHeight::from(proposal.min_target_height()) + expiry_delta_blocks),
                    zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
                )
            }
            None => create_pczt_from_proposal::<_, _, <MemoryWalletDb<Network> as InputSource>::Error, _, <prim_zip317::FeeRule as FeeRule>::Error, _>(
                &mut *db,
                &self.network,
                &WalletClock,
                &mut UnwrapErr(SysRng),
                account_id,
                OvkPolicy::Sender,
                &proposal,
                None,
                zcash_primitives::transaction::builder::BundlePadding::DEFAULT,
            ),
        }
        .map_err(|e| Error::Generic(format!("Failed to create PCZT for shielding: {e:?}")))?;

        Ok(pczt)
    }

    pub async fn estimate_pczt_shield_fee(&self, account_id: AccountId, marginal_fee_zats: u64) -> Result<u64, Error> {
        let change_strategy = zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
            dynamic_zip317_fee_rule(marginal_fee_zats)?,
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        );

        let input_selector = GreedyInputSelector::new();
        let mut db = self.db.write().await;

        // Get all transparent receivers for this account to use as shielding sources.
        let max_height = match db.chain_height()? {
            Some(max_height) => max_height,
            None => {
                return Err(Error::Generic("Havent scanned yet, cant shield".to_string()));
            }
        };

        let transparent_balances = db.get_transparent_balances(account_id, (max_height + 1).into(), self.selection_confirmations_policy())?;
        let from_addrs = transparent_balances.into_keys().collect::<Vec<_>>();

        if from_addrs.is_empty() {
            return Err(Error::Generic(
                "No transparent addresses found for this account. Make sure the account has received funds to a transparent address.".to_string(),
            ));
        }

        let proposal = propose_shielding::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            &input_selector,
            &change_strategy,
            SHIELDING_THRESHOLD,
            &from_addrs,
            account_id,
            self.selection_confirmations_policy(),
            CoinbaseFilter::AllTransparentOutputs,
            None,
        )
        .map_err(|e| Error::Generic(format!("Error when estimating shielding fee: {e:?}")))?;

        proposal_fee_required_zats(&proposal)
    }

    pub async fn estimate_pczt_transfer_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<u64, Error> {
        self.estimate_pczt_transfer_fee_with_policy(
            account_id,
            to_address,
            value,
            memo,
            marginal_fee_zats,
            SpendPolicy::default(),
            TransparentChangePolicy::ShieldChange,
        )
        .await
    }

    pub async fn estimate_pczt_transparent_transfer_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<u64, Error> {
        let (spend_policy, transparent_change_policy) = transparent_transfer_policies();
        self.estimate_pczt_transfer_fee_with_policy(account_id, to_address, value, memo, marginal_fee_zats, spend_policy, transparent_change_policy)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn estimate_pczt_transfer_fee_with_policy(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
        spend_policy: SpendPolicy,
        transparent_change_policy: TransparentChangePolicy,
    ) -> Result<u64, Error> {
        let change_strategy = zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
            dynamic_zip317_fee_rule(marginal_fee_zats)?,
            None,
            ShieldedPool::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count).ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        )
        .with_transparent_change_policy(transparent_change_policy);

        let input_selector = GreedyInputSelector::new();
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment = Payment::new(to_address, Some(Zatoshis::from_u64(value)?), memo, None, None, vec![]).map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
        let mut db = self.db.write().await;
        let proposal = propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &input_selector,
            &change_strategy,
            request,
            self.selection_confirmations_policy(),
            &spend_policy,
            None,
            None,
        )
        .map_err(|error| {
            let message = format!("Error when estimating transfer fee: {error:?}");
            if matches!(error, WalletError::InsufficientFunds { .. }) {
                Error::InsufficientTransferFunds(message)
            } else {
                Error::Generic(message)
            }
        })?;

        proposal_fee_required_zats(&proposal)
    }

    pub async fn estimate_pczt_max_transfer(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<(u64, u64), Error> {
        let fee_rule = dynamic_zip317_fee_rule(marginal_fee_zats)?;
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let mut db = self.db.write().await;
        let proposal = propose_send_max_transfer::<_, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &[ShieldedPool::Sapling, ShieldedPool::Orchard, ShieldedPool::Ironwood],
            &fee_rule,
            to_address,
            memo,
            MaxSpendMode::MaxSpendable,
            self.query_confirmations_policy(),
            &Default::default(),
            None,
        )
        .map_err(|error| {
            let message = format!("Error when estimating max transfer: {error:?}");
            if matches!(error, WalletError::InsufficientFunds { .. }) {
                Error::InsufficientTransferFunds(message)
            } else {
                Error::Generic(message)
            }
        })?;

        Ok((proposal_payment_amount_zats(&proposal)?, proposal_fee_required_zats(&proposal)?))
    }

    pub async fn estimate_pczt_max_transparent_transfer(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<(u64, u64), Error> {
        let fee_rule = dynamic_zip317_fee_rule(marginal_fee_zats)?;
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let mut db = self.db.write().await;
        let proposal = propose_send_max_transparent_transfer::<_, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &fee_rule,
            to_address,
            memo,
            self.query_confirmations_policy(),
        )
        .map_err(|e| Error::Generic(format!("Error when estimating max transparent transfer: {e:?}")))?;

        Ok((proposal_payment_amount_zats(&proposal)?, proposal_fee_required_zats(&proposal)?))
    }

    pub async fn estimate_orchard_migration_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<u64, Error> {
        let wallet_address = zcash_client_backend::address::Address::try_from_zcash_address(&self.network, to_address.clone())
            .map_err(|e| Error::Generic(format!("Invalid migration address: {e:?}")))?;
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let fee_rule = dynamic_zip317_fee_rule(marginal_fee_zats)?;
        let mut db = self.db.write().await;

        let destination_account = db
            .find_account_for_address(&self.network, &wallet_address)
            .map_err(|e| Error::Generic(format!("Failed to validate migration address: {e:?}")))?;
        if destination_account != Some(account_id) {
            return Err(Error::Generic("Orchard migration destination must belong to the source account".into()));
        }

        let target_height = db
            .chain_height()?
            .map(|height| height + 1)
            .ok_or_else(|| Error::Generic("Wallet must be synced before migration".into()))?;
        let activation_height = self
            .network
            .activation_height(zcash_protocol::consensus::NetworkUpgrade::Nu6_3)
            .ok_or_else(|| Error::Generic("NU6.3 is not configured for this network".into()))?;
        if target_height < activation_height {
            return Err(Error::Generic(format!("Ironwood is not active until height {}", u32::from(activation_height))));
        }

        let proposal = propose_send_max_transfer::<_, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &[ShieldedPool::Orchard],
            &fee_rule,
            to_address,
            memo,
            MaxSpendMode::MaxSpendable,
            self.selection_confirmations_policy(),
            &Default::default(),
            None,
        )
        .map_err(|e| Error::Generic(format!("Failed to estimate Orchard migration: {e:?}")))?
        .with_proposed_version(Some(TxVersion::V6));

        proposal_fee_required_zats(&proposal)
    }

    // ---------------------------------------------------------------- PCZT prove / broadcast

    pub async fn pczt_prove(&self, pczt: Pczt, sapling_proof_gen_key: Option<ProofGenerationKey>) -> Result<Pczt, Error> {
        // Add Sapling proof generation key.
        // TODO: Check to see if there is any sapling in here in the first place
        let pczt = if let Some(sapling_proof_gen_key) = sapling_proof_gen_key {
            Updater::new(pczt)
                .update_sapling_with(|mut updater| {
                    let non_dummy_spends = updater
                        .bundle()
                        .spends()
                        .iter()
                        .enumerate()
                        // Dummy spends will already have a proof generation key.
                        .filter(|(_, spend)| spend.proof_generation_key().is_none())
                        .map(|(index, spend)| {
                            (
                                index,
                                spend.zip32_derivation().as_ref().map(|d| (*d.seed_fingerprint(), d.derivation_path().clone())),
                            )
                        })
                        .collect::<Vec<_>>();

                    // Assume all non-dummy spent notes are from the same account.
                    for (index, _) in non_dummy_spends {
                        updater.update_spend_with(index, |mut spend_updater| spend_updater.set_proof_generation_key(sapling_proof_gen_key.clone()))?;
                    }

                    Ok(())
                })
                .map_err(|e| Error::PcztProve(format!("Failed to add Sapling proof generation key: {e:?}")))?
                .finish()
        } else {
            pczt
        };

        let prover = get_prover();
        let orchard_circuit_version = orchard_circuit_version_for_pczt(&pczt)?;

        let total_start = js_sys::Date::now();
        let mut prover_instance = Prover::new(pczt);

        // Time Orchard proof generation
        let mut orchard_elapsed = 0.0;
        if prover_instance.requires_orchard_proof() {
            let orchard_proving_key = get_orchard_proving_key(orchard_circuit_version);

            tracing::info!("🔧 Starting Orchard proof generation...");
            let orchard_start = js_sys::Date::now();
            prover_instance = prover_instance
                .create_orchard_proof(UnwrapErr(SysRng), orchard_proving_key)
                .map_err(|e| Error::PcztProve(format!("Failed to create Orchard proof: {e:?}")))?;
            orchard_elapsed = (js_sys::Date::now() - orchard_start) / 1000.0;
            tracing::info!("✅ Orchard proof completed in {:.2}s", orchard_elapsed);
        }

        // Time Ironwood proof generation
        let mut ironwood_elapsed = 0.0;
        if prover_instance.requires_ironwood_proof() {
            let orchard_proving_key = get_orchard_proving_key(orchard_circuit_version);

            tracing::info!("🔧 Starting Ironwood proof generation...");
            let ironwood_start = js_sys::Date::now();
            prover_instance = prover_instance
                .create_ironwood_proof(UnwrapErr(SysRng), orchard_proving_key)
                .map_err(|e| Error::PcztProve(format!("Failed to create Ironwood proof: {e:?}")))?;
            ironwood_elapsed = (js_sys::Date::now() - ironwood_start) / 1000.0;
            tracing::info!("✅ Ironwood proof completed in {:.2}s", ironwood_elapsed);
        }

        // Time Sapling proof generation
        tracing::info!("🔧 Starting Sapling proofs generation...");
        let sapling_start = js_sys::Date::now();
        prover_instance = prover_instance
            .create_sapling_proofs(UnwrapErr(SysRng), prover, prover)
            .map_err(|e| Error::PcztProve(format!("Failed to create Sapling proofs: {e:?}")))?;
        let sapling_elapsed = (js_sys::Date::now() - sapling_start) / 1000.0;
        tracing::info!("✅ Sapling proofs completed in {:.2}s", sapling_elapsed);

        let pczt = prover_instance.finish();
        let total_elapsed = (js_sys::Date::now() - total_start) / 1000.0;
        tracing::info!(
            "⏱️ Total proof generation: {:.2}s (Orchard: {:.2}s, Ironwood: {:.2}s, Sapling: {:.2}s)",
            total_elapsed,
            orchard_elapsed,
            ironwood_elapsed,
            sapling_elapsed
        );
        Ok(pczt)
    }

    pub async fn pczt_send(&self, pczt: Pczt) -> Result<String, Error> {
        let prover = get_prover();
        let (spend_vk, output_vk) = prover.verifying_keys();
        let orchard_circuit_version = orchard_circuit_version_for_pczt(&pczt)?;
        let mut db = self.db.write().await;

        let orchard_verifying_key = get_orchard_verifying_key(orchard_circuit_version);
        tracing::info!("📤 Extracting and storing transaction from PCZT...");
        let start_time = js_sys::Date::now();
        let txid = extract_and_store_transaction_from_pczt::<_, ()>(
            &mut *db,
            &WalletClock,
            &mut UnwrapErr(SysRng),
            pczt,
            Some((&spend_vk, &output_vk)),
            Some(orchard_verifying_key),
        )
        .map_err(|e| Error::PcztSend(format!("Failed to extract and store transaction from PCZT: {e:?}")))?;
        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;
        tracing::info!("⏱️ extract_and_store_transaction_from_pczt completed in {:.2} seconds", elapsed_seconds);
        tracing::info!("✅ Transaction stored with txid: {:?}", txid);
        drop(db);

        tracing::info!("📡 Broadcasting transaction to network...");
        let submission = self.send_authorized_transactions(&NonEmpty::new(txid)).await?;
        let transaction = submission
            .transactions
            .into_iter()
            .next()
            .expect("a single PCZT transaction produces one submission result");
        match transaction.status {
            TransferSubmissionStatus::Submitted => {}
            TransferSubmissionStatus::Rejected => {
                return Err(Error::SendFailed {
                    code: transaction.error_code.unwrap_or_default(),
                    reason: transaction.error_message.unwrap_or_default(),
                });
            }
            TransferSubmissionStatus::Unknown => {
                return Err(Error::TransactionSubmissionUnknown {
                    txid: transaction.tx_id,
                    reason: transaction.error_message.unwrap_or_default(),
                });
            }
            TransferSubmissionStatus::NotAttempted => {
                return Err(Error::Generic("PCZT transaction submission was not attempted".to_string()));
            }
        }

        tracing::info!("🎉 Transaction broadcast successfully!");
        Ok(txid.to_string())
    }

    /// Prove and broadcast an externally signed PCZT (hardware wallet flow).
    ///
    /// The PCZT must already contain all required signatures (e.g. added by a Keystone
    /// device). Proof generation runs locally; `sapling_proof_gen_key` is only needed
    /// when the PCZT spends Sapling notes, which never happens for hardware-wallet
    /// accounts (Keystone supports transparent + Orchard only).
    ///
    /// This composes `pczt_prove` and `pczt_send` without modification.
    pub async fn pczt_prove_and_send(&self, pczt: Pczt, sapling_proof_gen_key: Option<ProofGenerationKey>) -> Result<String, Error> {
        let proved = self.pczt_prove(pczt, sapling_proof_gen_key).await?;
        self.pczt_send(proved).await
    }
}

// ---------------------------------------------------------------- seed material

pub(crate) fn usk_from_seed_str(seed: &str, passphrase: &str, account_id: u32, network: &Network) -> Result<(UnifiedSpendingKey, SeedFingerprint), Error> {
    let mnemonic = <Mnemonic<English>>::from_phrase(seed).map_err(|_| Error::InvalidSeedPhrase)?;
    let seed = {
        let mut seed = mnemonic.to_seed(passphrase);
        let secret = seed.to_vec();
        seed.zeroize();
        SecretVec::new(secret)
    };
    let seed_fingerprint = SeedFingerprint::from_seed(seed.expose_secret()).ok_or(Error::SeedFingerprint)?;
    let usk = UnifiedSpendingKey::from_seed(network, seed.expose_secret(), account_id.try_into()?)?;
    Ok((usk, seed_fingerprint))
}
