use crate::*;

use bip0039::{English, Mnemonic};
use nonempty::NonEmpty;
use pczt::Pczt;
use pczt::roles::prover::Prover;
use pczt::roles::updater::Updater;
use sapling::ProofGenerationKey;
use secrecy::{ExposeSecret, SecretVec, Zeroize};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::OnceLock;
use std::{num::NonZeroU32, sync::Arc};
use subtle::ConditionallySelectable;
use tokio::sync::RwLock;
use tonic::{
    client::GrpcService,
    codegen::{Body, Bytes, StdError},
};
use zcash_address::ZcashAddress;
use zcash_client_backend::data_api::wallet::input_selection::GreedyInputSelector;
use zcash_client_backend::data_api::wallet::{
    ConfirmationsPolicy, SpendingKeys, create_pczt_from_proposal, create_proposed_transactions,
    extract_and_store_transaction_from_pczt, propose_shielding, propose_transfer,
};
use zcash_client_backend::data_api::{
    Account, AccountBirthday, AccountPurpose, WalletSummary, Zip32Derivation,
};
use zcash_client_backend::fees::standard::MultiOutputChangeStrategy;
use zcash_client_backend::fees::{DustOutputPolicy, SplitPolicy, StandardFeeRule};
use zcash_client_backend::sync::{run, run_next_batch};
use zcash_client_backend::wallet::OvkPolicy;
use zcash_client_backend::zip321::{Payment, TransactionRequest};
use zcash_client_backend::{
    data_api::{InputSource, WalletCommitmentTrees, WalletRead, WalletWrite},
    proto::service::{self, compact_tx_streamer_client::CompactTxStreamerClient},
};
use zcash_primitives::transaction::TxId;
use zcash_primitives::transaction::fees::FeeRule;
use zcash_proofs::prover::LocalTxProver;

use zcash_keys::keys::{UnifiedFullViewingKey, UnifiedSpendingKey};
use zcash_protocol::ShieldedProtocol;
use zcash_protocol::value::Zatoshis;
use zip32::fingerprint::SeedFingerprint;

use zcash_client_memory::{MemBlockCache, MemoryWalletDb};
use zcash_protocol::consensus::Parameters;

const BATCH_SIZE: u32 = 10000;

/// constant that signals what's the minimum transparent balance for proposing a
/// shielding transaction
const SHIELDING_THRESHOLD: Zatoshis = Zatoshis::const_from_u64(100000);

/// Global prover instance, initialized once on first use
/// This avoids repeatedly loading the expensive Sapling parameters
static PROVER: OnceLock<LocalTxProver> = OnceLock::new();

/// Get or initialize the global prover instance
fn get_prover() -> &'static LocalTxProver {
    PROVER.get_or_init(|| {
        tracing::info!("🔧 Initializing LocalTxProver (loading Sapling parameters)...");
        LocalTxProver::bundled()
    })
}

/// Global Orchard proving key instance, initialized once on first use
/// This avoids repeatedly building the expensive Orchard proving key
static ORCHARD_PROVING_KEY: OnceLock<orchard::circuit::ProvingKey> = OnceLock::new();

/// Get or initialize the global Orchard proving key
fn get_orchard_proving_key() -> &'static orchard::circuit::ProvingKey {
    ORCHARD_PROVING_KEY.get_or_init(|| {
        tracing::info!("🔧 Initializing Orchard ProvingKey...");
        orchard::circuit::ProvingKey::build()
    })
}

/// Global Orchard verifying key instance, initialized once on first use
/// This avoids repeatedly building the expensive Orchard verifying key
static ORCHARD_VERIFYING_KEY: OnceLock<orchard::circuit::VerifyingKey> = OnceLock::new();

/// Get or initialize the global Orchard verifying key
fn get_orchard_verifying_key() -> &'static orchard::circuit::VerifyingKey {
    ORCHARD_VERIFYING_KEY.get_or_init(|| {
        tracing::info!("🔧 Initializing Orchard VerifyingKey...");
        orchard::circuit::VerifyingKey::build()
    })
}

pub struct Wallet<W, T> {
    /// Internal database used to maintain wallet data (e.g. accounts, transactions, cached blocks)
    pub(crate) db: Arc<RwLock<W>>,
    // gRPC client used to connect to a lightwalletd instance for network data
    pub(crate) client: CompactTxStreamerClient<T>,
    pub(crate) network: Network,
    pub(crate) min_confirmations: NonZeroU32,
    /// Note management: the number of notes to maintain in the wallet
    pub(crate) target_note_count: usize,
    /// Note management: the minimum allowed value for split change amounts
    pub(crate) min_split_output_value: u64,
}

impl<W, T: Clone> Clone for Wallet<W, T> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            client: self.client.clone(),
            network: self.network,
            min_confirmations: self.min_confirmations,
            target_note_count: self.target_note_count,
            min_split_output_value: self.min_split_output_value,
        }
    }
}

impl<P, T> Wallet<MemoryWalletDb<P>, T>
where
    P: Parameters,
{
    // Encodes the MemoryWallet into protobuf bytes
    pub async fn db_to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut memory_wallet_bytes = Vec::new();
        self.db.read().await.encode(&mut memory_wallet_bytes)?;
        Ok(memory_wallet_bytes)
    }
}

impl<W, T, AccountId, NoteRef> Wallet<W, T>
where
    W: WalletRead<AccountId = AccountId>
        + WalletWrite
        + InputSource<
            AccountId = <W as WalletRead>::AccountId,
            Error = <W as WalletRead>::Error,
            NoteRef = NoteRef,
        > + WalletCommitmentTrees,
    AccountId: Copy
        + Debug
        + Eq
        + Hash
        + Default
        + Send
        + ConditionallySelectable
        + Serialize
        + DeserializeOwned
        + 'static,
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
    /// Creates a confirmations policy using the wallet's min_confirmations setting.
    /// This applies the same confirmation requirement to both trusted and untrusted transactions,
    /// and allows zero-confirmation shielding of transparent UTXOs (matching ZIP 315 defaults).
    fn confirmations_policy(&self) -> ConfirmationsPolicy {
        ConfirmationsPolicy::new_symmetrical(
            self.min_confirmations,
            true, // allow zero-conf shielding (matches default behavior)
        )
    }

    pub fn new(
        db: W,
        client: T,
        network: Network,
        min_confirmations: NonZeroU32,
    ) -> Result<Self, Error> {
        get_prover();
        get_orchard_proving_key();
        get_orchard_verifying_key();
        Ok(Wallet {
            db: Arc::new(RwLock::new(db)),
            client: CompactTxStreamerClient::new(client),
            network,
            min_confirmations,
            target_note_count: 4,
            min_split_output_value: 10000000,
        })
    }

    pub async fn create_account(
        &self,
        account_name: &str,
        seed_phrase: &str,
        account_hd_index: u32,
        birthday_height: Option<u32>,
        key_source: Option<&str>,
    ) -> Result<AccountId, Error> {
        // decode the mnemonic and derive the first account
        let (usk, seed_fp) = usk_from_seed_str(seed_phrase, account_hd_index, &self.network)?;
        let ufvk = usk.to_unified_full_viewing_key();

        let derivation =
            Zip32Derivation::new(seed_fp, zip32::AccountId::try_from(account_hd_index)?);

        tracing::info!("Key successfully decoded. Importing into wallet");

        self.import_account_ufvk(
            account_name,
            &ufvk,
            birthday_height,
            AccountPurpose::Spending {
                derivation: Some(derivation),
            },
            key_source,
        )
        .await
    }

    // pub async fn import_ufvk(
    //     &self,
    //     account_name: &str,
    //     ufvk: &UnifiedFullViewingKey,
    //     purpose: AccountPurpose,
    //     birthday_height: Option<u32>,
    //     key_source: Option<&str>,
    // ) -> Result<AccountId, Error> {
    //     self.import_account_ufvk(account_name, ufvk, birthday_height, purpose, key_source)
    //         .await
    // }

    // /// Import an account from a Sapling Extended Spending Key
    // ///
    // /// This creates a spending account with only Sapling key material.
    // /// Transparent and Orchard keys will not be available.
    // pub async fn import_sapling_spending_key(
    //     &self,
    //     account_name: &str,
    //     sapling_key_encoded: &str,
    //     birthday_height: Option<u32>,
    //     key_source: Option<&str>,
    // ) -> Result<AccountId, Error> {
    //     use zcash_keys::encoding::decode_extended_spending_key;

    //     let hrp = match &self.network {
    //         Network::MainNetwork => "secret-extended-key-main",
    //         Network::TestNetwork => "secret-extended-key-test",
    //     };

    //     // Decode the Sapling Extended Spending Key
    //     let extsk = decode_extended_spending_key(hrp, sapling_key_encoded)
    //         .map_err(|e| Error::KeyParse(format!("Failed to decode Sapling spending key: {:?}", e)))?;

    //     // Convert to Extended Full Viewing Key
    //     #[allow(deprecated)]
    //     let extfvk = extsk.to_extended_full_viewing_key();

    //     // Create UFVK from Sapling FVK
    //     let ufvk = zcash_keys::keys::UnifiedFullViewingKey::from_sapling_extended_full_viewing_key(extfvk)
    //         .map_err(Error::Derivation)?;

    //     // Import as a spending account (no derivation info since we don't have the seed)
    //     self.import_account_ufvk(
    //         account_name,
    //         &ufvk,
    //         birthday_height,
    //         AccountPurpose::Spending { derivation: None },
    //         key_source,
    //     )
    //     .await
    // }

    // /// Import an account from a Sapling Extended Full Viewing Key
    // ///
    // /// This creates a view-only account with only Sapling key material.
    // /// The account can view transactions but cannot spend funds.
    // pub async fn import_sapling_viewing_key(
    //     &self,
    //     account_name: &str,
    //     sapling_fvk_encoded: &str,
    //     birthday_height: Option<u32>,
    //     key_source: Option<&str>,
    // ) -> Result<AccountId, Error> {
    //     use zcash_keys::encoding::decode_extended_full_viewing_key;

    //     let hrp = match &self.network {
    //         Network::MainNetwork => "zxviews",
    //         Network::TestNetwork => "zxviewtestsapling",
    //     };

    //     // Decode the Sapling Extended Full Viewing Key
    //     let extfvk = decode_extended_full_viewing_key(hrp, sapling_fvk_encoded)
    //         .map_err(|e| Error::KeyParse(format!("Failed to decode Sapling viewing key: {:?}", e)))?;

    //     // Create UFVK from Sapling FVK
    //     let ufvk = zcash_keys::keys::UnifiedFullViewingKey::from_sapling_extended_full_viewing_key(extfvk)
    //         .map_err(Error::Derivation)?;

    //     // Import as a view-only account
    //     self.import_account_ufvk(
    //         account_name,
    //         &ufvk,
    //         birthday_height,
    //         AccountPurpose::ViewOnly,
    //         key_source,
    //     )
    //     .await
    // }

    async fn import_account_ufvk(
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
                    .expect("block heights must fit into u32");
                chain_tip - 100
            }
        };
        // Construct an `AccountBirthday` for the account's birthday.
        let birthday = {
            // Fetch the tree state corresponding to the last block prior to the wallet's
            // birthday height. NOTE: THIS APPROACH LEAKS THE BIRTHDAY TO THE SERVER!
            let request = service::BlockId {
                height: (birthday - 1).into(),
                ..Default::default()
            };
            let treestate = client.get_tree_state(request).await?.into_inner();
            AccountBirthday::from_treestate(treestate, None).map_err(|_| Error::Birthday)?
        };

        // Import the account first
        let account_id = {
            let mut db = self.db.write().await;
            db.import_account_ufvk(account_name, ufvk, &birthday, purpose, key_source)?
                .id()
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
            let _ = db.get_address_for_index(
                account_id,
                DiversifierIndex::new(),
                UnifiedAddressRequest::ALLOW_ALL,
            )?;
            // Also generate the next available address for transparent address metadata
            // needed when spending transparent funds.
            let _ = db.get_next_available_address(account_id, UnifiedAddressRequest::ALLOW_ALL)?;
        }

        Ok(account_id)
    }

    // pub async fn suggest_scan_ranges(&self) -> Result<Vec<BlockRange>, Error> {
    //     Ok(self.db.read().await.suggest_scan_ranges().map(|ranges| {
    //         ranges
    //             .iter()
    //             .map(|scan_range| {
    //                 BlockRange(
    //                     scan_range.block_range().start.into(),
    //                     scan_range.block_range().end.into(),
    //                 )
    //             })
    //             .collect()
    //     })?)
    // }

    pub async fn sync(&self) -> Result<(), Error> {
        let mut client = self.client.clone();
        // TODO: This should be held in the Wallet struct so we can download in parallel
        let db_cache = MemBlockCache::new();

        let mut db = self.db.write().await;
        run(
            &mut client,
            &self.network.clone(),
            &db_cache,
            &mut *db,
            BATCH_SIZE,
        )
        .await
        .map_err(Into::into)
    }

    /// Runs a single batch of the sync process without looping.
    /// Returns `true` if there is more work to do, `false` if fully synced.
    pub async fn sync_next_batch(&self, batch_size: Option<u32>) -> Result<bool, Error> {
        let mut client = self.client.clone();
        let db_cache = MemBlockCache::new();

        let mut db = self.db.write().await;
        run_next_batch(
            &mut client,
            &self.network.clone(),
            &db_cache,
            &mut *db,
            batch_size.unwrap_or(BATCH_SIZE),
        )
        .await
        .map_err(Into::into)
    }

    pub async fn get_wallet_summary(&self) -> Result<Option<WalletSummary<AccountId>>, Error> {
        Ok(self
            .db
            .read()
            .await
            .get_wallet_summary(self.confirmations_policy())?)
    }

    /// Get account ID by account name
    ///
    /// # Arguments
    ///
    /// * `account_name` - The name of the account to find
    ///
    /// # Returns
    ///
    /// Returns the AccountId if an account with the given name exists, or an error if not found
    pub async fn get_account_id_by_name(&self, account_name: &str) -> Result<AccountId, Error> {
        let db = self.db.read().await;

        // Get all account IDs
        let account_ids = db.get_account_ids()?;

        // Find the account with matching name
        for account_id in account_ids {
            if let Some(account) = db.get_account(account_id)? {
                if let Some(name) = account.name() {
                    if name == account_name {
                        return Ok(account_id);
                    }
                }
            }
        }

        Err(Error::AccountNameNotFound)
    }

    pub async fn get_account_birthday(&self, account_id: AccountId) -> Result<u32, Error> {
        let db = self.db.read().await;
        let birthday_height = db.get_account_birthday(account_id)?;
        Ok(birthday_height.into())
    }

    /// Get the timestamp (Unix time) of a block at a given height
    pub async fn get_block_time(&self, height: u32) -> Result<u32, Error> {
        use tonic::Request;
        use zcash_client_backend::proto::service::BlockId;

        let mut client = self.client.clone();
        let block = client
            .get_block(Request::new(BlockId {
                height: height as u64,
                hash: vec![],
            }))
            .await?
            .into_inner();
        Ok(block.time)
    }

    /// Get the latest block height from the chain
    pub async fn get_latest_height(&self) -> Result<u32, Error> {
        use tonic::Request;
        use zcash_client_backend::proto::service::ChainSpec;

        let mut client = self.client.clone();
        let chainspec = ChainSpec {};
        let rep = client.get_latest_block(Request::new(chainspec)).await?;
        let block_id = rep.into_inner();
        Ok(block_id.height as u32)
    }

    /// Find the block height closest to a given Unix timestamp using binary search
    ///
    /// # Arguments
    ///
    /// * `time` - Unix timestamp (seconds since epoch)
    ///
    /// # Returns
    ///
    /// The block height closest to the given timestamp
    ///
    /// # Algorithm
    ///
    /// Uses binary search to efficiently find the block height:
    /// - If time is before Sapling activation, returns 0
    /// - If time is after the latest block, returns latest height
    /// - Otherwise, performs binary search with ~1000 block precision
    pub async fn get_block_by_time(&self, time: u32) -> Result<u32, Error> {
        use zcash_primitives::consensus::{NetworkUpgrade, Parameters};

        // Get Sapling activation height as the starting point
        let start_height = self
            .network
            .activation_height(NetworkUpgrade::Sapling)
            .ok_or(Error::Generic(
                "Sapling activation height not found".to_string(),
            ))?;
        let mut start = u32::from(start_height);
        let mut end = self.get_latest_height().await?;

        // Check boundary conditions
        let start_time = self.get_block_time(start).await?;
        if time <= start_time {
            return Ok(0);
        }

        let end_time = self.get_block_time(end).await?;
        if time >= end_time {
            return Ok(end);
        }

        // Binary search with ~1000 block precision
        while end - start >= 1000 {
            let block_mid = (start + end) / 2;
            let mid_time = self.get_block_time(block_mid).await?;

            match time.cmp(&mid_time) {
                std::cmp::Ordering::Less => {
                    end = block_mid - 1;
                }
                std::cmp::Ordering::Equal => {
                    return Ok(block_mid);
                }
                std::cmp::Ordering::Greater => {
                    start = block_mid + 1;
                }
            }
        }

        Ok(start)
    }

    // pub async fn propose_transfer(
    //     &self,
    //     account_id: AccountId,
    //     to_address: ZcashAddress,
    //     value: u64,
    // ) -> Result<Proposal<StandardFeeRule, NoteRef>, Error> {
    //     let input_selector = GreedyInputSelector::new();

    //     let change_strategy = MultiOutputChangeStrategy::new(
    //         StandardFeeRule::Zip317,
    //         None,
    //         ShieldedProtocol::Orchard,
    //         DustOutputPolicy::default(),
    //         SplitPolicy::with_min_output_value(
    //             NonZeroUsize::new(self.target_note_count)
    //                 .ok_or(Error::FailedToCreateTransaction)?,
    //             Zatoshis::from_u64(self.min_split_output_value)?,
    //         ),
    //     );
    //     let request = TransactionRequest::new(vec![Payment::without_memo(
    //         to_address,
    //         Zatoshis::from_u64(value)?,
    //     )])?;

    //     tracing::info!("Chain height: {:?}", self.db.read().await.chain_height()?);
    //     tracing::info!(
    //         "target and anchor heights: {:?}",
    //         self.db
    //             .read()
    //             .await
    //             .get_target_and_anchor_heights(self.min_confirmations)?
    //     );
    //     let mut db = self.db.write().await;
    //     let proposal = propose_transfer::<_, _, _,_, <W as WalletCommitmentTrees>::Error>(
    //         &mut *db,
    //         &self.network,
    //         account_id,
    //         &input_selector,
    //         &change_strategy,
    //         request,
    //         self.confirmations_policy(),
    //     )
    //     .map_err(|_e| Error::Generic("something bad happened when calling propose transfer. Possibly insufficient balance..".to_string()))?;
    //     Ok(proposal)
    // }

    // pub async fn create_proposed_transactions(
    //     &self,
    //     proposal: Proposal<StandardFeeRule, NoteRef>,
    //     usk: &UnifiedSpendingKey,
    // ) -> Result<NonEmpty<TxId>, Error> {
    //     use js_sys::Date;

    //     let start = Date::now();
    //     tracing::info!("🔧 Starting create_proposed_transactions");

    //     let prover = get_prover();

    //     let spending_keys = SpendingKeys::from_unified_spending_key(usk.clone());

    //     let db_lock_start = Date::now();
    //     let mut db = self.db.write().await;
    //     tracing::info!(
    //         "🔒 DB write lock acquired in {:.2}s",
    //         (Date::now() - db_lock_start) / 1000.0
    //     );

    //     let create_start = Date::now();
    //     tracing::info!("📝 Calling create_proposed_transactions from librustzcash...");
    //     let transactions = create_proposed_transactions::<
    //         _,
    //         _,
    //         <MemoryWalletDb<Network> as InputSource>::Error,
    //         _,
    //         <StandardFeeRule as FeeRule>::Error,
    //         _,
    //     >(
    //         &mut *db,
    //         &self.network,
    //         prover,
    //         prover,
    //         &spending_keys,
    //         OvkPolicy::Sender,
    //         &proposal,
    //     )
    //     .map_err(|_| Error::FailedToCreateTransaction)?;

    //     tracing::info!(
    //         "✅ create_proposed_transactions completed in {:.2}s",
    //         (Date::now() - create_start) / 1000.0
    //     );
    //     tracing::info!(
    //         "⏱️ Total create_proposed_transactions took {:.2}s",
    //         (Date::now() - start) / 1000.0
    //     );

    //     Ok(transactions)
    // }

    pub async fn send_authorized_transactions(&self, txids: &NonEmpty<TxId>) -> Result<(), Error> {
        let mut client = self.client.clone();
        for txid in txids.iter() {
            let (txid, raw_tx) = self
                .db
                .read()
                .await
                .get_transaction(*txid)?
                .map(|tx| {
                    let mut raw_tx = service::RawTransaction::default();
                    tx.write(&mut raw_tx.data).unwrap(); // safe to unwrap here as we know the tx is valid
                    (tx.txid(), raw_tx)
                })
                .ok_or(Error::TransactionNotFound(*txid))?;

            let response = client.send_transaction(raw_tx).await?.into_inner();

            if response.error_code != 0 {
                return Err(Error::SendFailed {
                    code: response.error_code,
                    reason: response.error_message,
                });
            } else {
                tracing::info!("Transaction {} send successfully :)", txid);
            }
        }
        Ok(())
    }

    // pub async fn transfer(
    //     &self,
    //     seed_phrase: &str,
    //     account_hd_index: u32,
    //     from_account_id: AccountId,
    //     to_address: ZcashAddress,
    //     value: u64,
    // ) -> Result<(), Error> {
    //     let (usk, _) = usk_from_seed_str(seed_phrase, account_hd_index, &self.network)?;
    //     let proposal = self
    //         .propose_transfer(from_account_id, to_address, value)
    //         .await?;
    //     // TODO: Add callback for approving the transaction here
    //     let txids = self.create_proposed_transactions(proposal, &usk).await?;

    //     // send the transactions to the network!!
    //     tracing::info!("Sending transactions");
    //     self.send_authorized_transactions(&txids).await
    // }

    pub async fn pczt_shield(&self, account_id: AccountId) -> Result<Pczt, Error> {
        let change_strategy = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedProtocol::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count)
                    .ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        );

        let input_selector = GreedyInputSelector::new();
        let mut db = self.db.write().await;

        // Get all transparent receivers for this account to use as shielding sources
        let max_height = match db.chain_height()? {
            Some(max_height) => max_height,
            None => {
                return Err(Error::Generic(
                    "Havent scanned yet, cant shield".to_string(),
                ));
            }
        };

        let transparent_balances = db.get_transparent_balances(
            account_id,
            (max_height + 1).into(),
            self.confirmations_policy(),
        )?;
        let from_addrs = transparent_balances.into_keys().collect::<Vec<_>>();

        if from_addrs.is_empty() {
            return Err(Error::Generic(
                "No transparent addresses found for this account. Make sure the account has received funds to a transparent address.".to_string()
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
            self.confirmations_policy(),
        )
        .map_err(|e| Error::Generic(format!("Error when shielding: {:?}", e)))?;

        let pczt = create_pczt_from_proposal::<
            _,
            _,
            <MemoryWalletDb<Network> as InputSource>::Error,
            _,
            <StandardFeeRule as FeeRule>::Error,
            _,
        >(
            &mut *db,
            &self.network,
            account_id,
            OvkPolicy::Sender,
            &proposal,
        )
        .map_err(|e| Error::Generic(format!("Failed to create PCZT for shielding: {:?}", e)))?;

        Ok(pczt)
    }

    /// Shield transparent funds by moving them to the Orchard shielded pool
    /// This is a simplified version that doesn't use PCZT
    pub async fn shield_funds(
        &self,
        account_id: AccountId,
        usk: &UnifiedSpendingKey,
    ) -> Result<NonEmpty<TxId>, Error> {
        let change_strategy = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedProtocol::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count)
                    .ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        );

        let input_selector = GreedyInputSelector::new();
        let mut db = self.db.write().await;

        // Shield all funds immediately
        let max_height = match db.chain_height()? {
            Some(max_height) => max_height,
            None => {
                return Err(Error::Generic(
                    "Haven't scanned yet, can't shield".to_string(),
                ));
            }
        };

        let transparent_balances = db.get_transparent_balances(
            account_id,
            (max_height + 1).into(),
            self.confirmations_policy(),
        )?;
        let from_addrs = transparent_balances.into_keys().collect::<Vec<_>>();

        if from_addrs.is_empty() {
            return Err(Error::Generic(
                "No transparent addresses found for this account. Make sure the account has received funds to a transparent address.".to_string()
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
            ConfirmationsPolicy::new_symmetrical(NonZeroU32::new(1).unwrap(), true),
        )
        .map_err(|e| Error::Generic(format!("Error when proposing shielding: {:?}", e)))?;

        // Create and send the transaction directly
        let prover = get_prover();
        let spending_keys = SpendingKeys::from_unified_spending_key(usk.clone());

        let transactions = create_proposed_transactions::<
            _,
            _,
            <MemoryWalletDb<Network> as InputSource>::Error,
            _,
            <StandardFeeRule as FeeRule>::Error,
            _,
        >(
            &mut *db,
            &self.network,
            prover,
            prover,
            &spending_keys,
            OvkPolicy::Sender,
            &proposal,
        )
        .map_err(|e| Error::Generic(format!("Failed to create shielding transaction: {:?}", e)))?;

        // Transactions are automatically stored in the database by create_proposed_transactions
        // Now we need to broadcast them
        drop(db); // Release the lock before sending

        self.send_authorized_transactions(&transactions).await?;

        Ok(transactions)
    }

    pub async fn pczt_create(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
    ) -> Result<Pczt, Error> {
        // Create the PCZT.
        let change_strategy = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedProtocol::Orchard,
            DustOutputPolicy::default(),
            SplitPolicy::with_min_output_value(
                NonZeroUsize::new(self.target_note_count)
                    .ok_or(Error::FailedToCreateTransaction)?,
                Zatoshis::from_u64(self.min_split_output_value)?,
            ),
        );

        let input_selector = GreedyInputSelector::new();
        let request = TransactionRequest::new(vec![Payment::without_memo(
            to_address,
            Zatoshis::from_u64(value)?,
        )])?;
        let mut db = self.db.write().await;
        let proposal = propose_transfer::<_, _, _,_, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &input_selector,
            &change_strategy,
            request,
            self.confirmations_policy(),
        )
            .map_err(|e| Error::Generic(format!("something bad happened when calling propose transfer. Possibly insufficient balance... {:?}", e)))?;
        let pczt = create_pczt_from_proposal::<
            _,
            _,
            <MemoryWalletDb<Network> as InputSource>::Error,
            _,
            <StandardFeeRule as FeeRule>::Error,
            _,
        >(
            &mut *db,
            &self.network,
            account_id,
            OvkPolicy::Sender,
            &proposal,
        )
        .map_err(|e| Error::Generic(format!("Failed to create PCZT for transfer: {:?}", e)))?;
        Ok(pczt)
    }

    pub async fn pczt_prove(
        &self,
        pczt: Pczt,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
    ) -> Result<Pczt, Error> {
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
                                spend
                                    .zip32_derivation()
                                    .as_ref()
                                    .map(|d| (*d.seed_fingerprint(), d.derivation_path().clone())),
                            )
                        })
                        .collect::<Vec<_>>();

                    // Assume all non-dummy spent notes are from the same account.
                    for (index, _) in non_dummy_spends {
                        updater.update_spend_with(index, |mut spend_updater| {
                            spend_updater.set_proof_generation_key(sapling_proof_gen_key.clone())
                        })?;
                    }

                    Ok(())
                })
                .map_err(|e| {
                    Error::PcztProve(format!(
                        "Failed to add Sapling proof generation key: {:?}",
                        e
                    ))
                })?
                .finish()
        } else {
            pczt
        };

        let prover = get_prover();
        let orchard_proving_key = get_orchard_proving_key();

        let total_start = js_sys::Date::now();
        let mut prover_instance = Prover::new(pczt);

        // Time Orchard proof generation
        tracing::info!("🔧 Starting Orchard proof generation...");
        let orchard_start = js_sys::Date::now();
        prover_instance = prover_instance
            .create_orchard_proof(orchard_proving_key)
            .map_err(|e| Error::PcztProve(format!("Failed to create Orchard proof: {:?}", e)))?;
        let orchard_elapsed = (js_sys::Date::now() - orchard_start) / 1000.0;
        tracing::info!("✅ Orchard proof completed in {:.2}s", orchard_elapsed);

        // Time Sapling proof generation
        tracing::info!("🔧 Starting Sapling proofs generation...");
        let sapling_start = js_sys::Date::now();
        prover_instance = prover_instance
            .create_sapling_proofs(prover, prover)
            .map_err(|e| Error::PcztProve(format!("Failed to create Sapling proofs: {:?}", e)))?;
        let sapling_elapsed = (js_sys::Date::now() - sapling_start) / 1000.0;
        tracing::info!("✅ Sapling proofs completed in {:.2}s", sapling_elapsed);

        let pczt = prover_instance.finish();
        let total_elapsed = (js_sys::Date::now() - total_start) / 1000.0;
        tracing::info!(
            "⏱️ Total proof generation: {:.2}s (Orchard: {:.2}s, Sapling: {:.2}s)",
            total_elapsed,
            orchard_elapsed,
            sapling_elapsed
        );
        Ok(pczt)
    }

    pub async fn pczt_send(&self, pczt: Pczt) -> Result<String, Error> {
        let prover = get_prover();
        let (spend_vk, output_vk) = prover.verifying_keys();
        let mut db = self.db.write().await;

        let orchard_verifying_key = get_orchard_verifying_key();
        tracing::info!("📤 Extracting and storing transaction from PCZT...");
        let start_time = js_sys::Date::now();
        let txid = extract_and_store_transaction_from_pczt::<_, ()>(
            &mut *db,
            pczt,
            Some((&spend_vk, &output_vk)),
            Some(orchard_verifying_key),
        )
        .map_err(|e| {
            Error::PcztSend(format!(
                "Failed to extract and store transaction from PCZT: {:?}",
                e
            ))
        })?;
        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;
        tracing::info!(
            "⏱️ extract_and_store_transaction_from_pczt completed in {:.2} seconds",
            elapsed_seconds
        );
        tracing::info!("✅ Transaction stored with txid: {:?}", txid);
        drop(db);

        tracing::info!("📡 Broadcasting transaction to network...");
        self.send_authorized_transactions(&NonEmpty::new(txid))
            .await?;

        tracing::info!("🎉 Transaction broadcast successfully!");
        Ok(txid.to_string())
    }

    // pub fn pczt_combine(&self, pczts: Vec<Pczt>) -> Result<Pczt, Error> {
    //     Combiner::new(pczts)
    //         .combine()
    //         .map_err(|e| Error::PcztCombine(format!("Failed to combine PCZT: {:?}", e)))
    // }
}

pub(crate) fn usk_from_seed_str(
    seed: &str,
    account_id: u32,
    network: &Network,
) -> Result<(UnifiedSpendingKey, SeedFingerprint), Error> {
    let mnemonic = <Mnemonic<English>>::from_phrase(seed).map_err(|_| Error::InvalidSeedPhrase)?;
    let seed = {
        let mut seed = mnemonic.to_seed("");
        let secret = seed.to_vec();
        seed.zeroize();
        SecretVec::new(secret)
    };
    let seed_fingerprint =
        SeedFingerprint::from_seed(seed.expose_secret()).expect("seed fingerprint");
    let usk = UnifiedSpendingKey::from_seed(network, seed.expose_secret(), account_id.try_into()?)?;
    Ok((usk, seed_fingerprint))
}
