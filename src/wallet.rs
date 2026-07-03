use crate::*;

use bip0039::{English, Mnemonic};
use futures_util::TryStreamExt;
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
use std::sync::Arc;
use std::sync::OnceLock;
use subtle::ConditionallySelectable;
use tokio::sync::RwLock;
use tonic::{
    client::GrpcService,
    codegen::{Body, Bytes, StdError},
};
use zcash_address::ZcashAddress;
use zcash_client_backend::data_api::wallet::input_selection::GreedyInputSelector;
use zcash_client_backend::data_api::wallet::{
    ConfirmationsPolicy, create_pczt_from_proposal, extract_and_store_transaction_from_pczt,
    propose_shielding, propose_transfer,
};
use zcash_client_backend::data_api::{
    Account, AccountBirthday, AccountPurpose, TransparentOutputFilter, WalletSummary,
    Zip32Derivation,
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
use zcash_primitives::transaction::fees::{FeeRule, zip317 as prim_zip317};
use zcash_proofs::prover::LocalTxProver;

use zcash_keys::keys::{UnifiedFullViewingKey, UnifiedSpendingKey};
use zcash_protocol::ShieldedProtocol;
use zcash_protocol::consensus::Parameters;
use zcash_protocol::memo::MemoBytes;
use zcash_protocol::value::Zatoshis;
use zip32::fingerprint::SeedFingerprint;

use zcash_client_backend::data_api::chain::BlockCache;
use zcash_client_memory::{MemBlockCache, MemoryWalletDb};

const BATCH_SIZE: u32 = 10000;

/// constant that signals what's the minimum transparent balance for proposing a
/// shielding transaction
const SHIELDING_THRESHOLD: Zatoshis = Zatoshis::const_from_u64(100000);

fn dynamic_zip317_fee_rule(marginal_fee_zats: u64) -> Result<prim_zip317::FeeRule, Error> {
    if marginal_fee_zats == 0 {
        return Err(Error::Generic(
            "marginal_fee_zats must be greater than 0".to_string(),
        ));
    }

    prim_zip317::FeeRule::non_standard(
        Zatoshis::from_u64(marginal_fee_zats)?,
        prim_zip317::GRACE_ACTIONS,
        prim_zip317::P2PKH_STANDARD_INPUT_SIZE,
        prim_zip317::P2PKH_STANDARD_OUTPUT_SIZE,
    )
    .ok_or_else(|| Error::Generic("Invalid ZIP-317 fee parameters".to_string()))
}

fn proposal_fee_required_zats<FeeRuleT, NoteRef>(
    proposal: &zcash_client_backend::proposal::Proposal<FeeRuleT, NoteRef>,
) -> Result<u64, Error> {
    let fee = proposal
        .steps()
        .iter()
        .try_fold(Zatoshis::ZERO, |total, step| {
            (total + step.balance().fee_required())
                .ok_or(zcash_protocol::value::BalanceError::Overflow)
        })?;

    Ok(fee.into())
}

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
        let db = self.db.read().await;
        let encoded_len = db.encoded_len()?;
        memory_wallet_bytes
            .try_reserve_exact(encoded_len)
            .map_err(|e| {
                Error::Generic(format!(
                    "Failed to allocate {encoded_len} bytes for wallet database serialization: {e}"
                ))
            })?;
        db.encode(&mut memory_wallet_bytes)?;
        Ok(memory_wallet_bytes)
    }

    pub async fn db_to_bytes_chunked<F: FnMut(&[u8])>(&self, flush_fn: F) -> Result<(), Error> {
        let db = self.db.read().await;
        db.encode_chunked(flush_fn)?;
        Ok(())
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
        + Sync
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
    /// Creates the ZIP-315 default confirmations policy.
    ///
    /// This preserves ZIP-315's distinction between trusted and untrusted
    /// outputs while allowing zero-confirmation shielding of transparent UTXOs.
    fn confirmations_policy(&self) -> ConfirmationsPolicy {
        ConfirmationsPolicy::default()
    }

    pub fn new(db: W, client: T, network: Network) -> Result<Self, Error> {
        get_prover();
        get_orchard_proving_key();
        get_orchard_verifying_key();
        Ok(Wallet {
            db: Arc::new(RwLock::new(db)),
            client: CompactTxStreamerClient::new(client),
            network,
            target_note_count: 4,
            min_split_output_value: 10000000,
        })
    }

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
        let (usk, seed_fp) =
            usk_from_seed_str(seed_phrase, passphrase, account_hd_index, &self.network)?;
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
    /// Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
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

    /// Execute a full journal-aware sync cycle:
    /// 1) Prepare (download from network, return journal data)
    /// 2) JS persists the journal data
    /// 3) scan_journal_entry applies the scan
    ///
    /// This method handles step 1: network operations + data capture.
    /// Returns None if fully synced, Some(journal_bytes) with captured data.
    fn scan_with_reorg_handling<CaT: zcash_client_backend::data_api::chain::BlockCache>(
        network: &crate::Network,
        db_cache: &CaT,
        db: &mut W,
        start_height: zcash_protocol::consensus::BlockHeight,
        chain_state: &zcash_client_backend::data_api::chain::ChainState,
        limit: usize,
    ) -> Result<bool, Error>
    where
        CaT::Error: std::fmt::Display + Send,
    {
        use zcash_client_backend::data_api::chain::error::Error as ChainError;

        match zcash_client_backend::data_api::chain::scan_cached_blocks(
            network,
            db_cache,
            db,
            start_height,
            chain_state,
            limit,
        ) {
            Ok(_) => Ok(false),
            Err(ChainError::Scan(ref scan_err)) if scan_err.is_continuity_error() => {
                let rewind_height = scan_err.at_height().saturating_sub(10);
                tracing::warn!(
                    "Chain reorg detected at {}, rewinding to {}",
                    scan_err.at_height(),
                    rewind_height,
                );
                db.truncate_to_height(rewind_height)
                    .map_err(|e| Error::Generic(format!("Truncate failed: {e}")))?;
                Ok(true)
            }
            Err(e) => Err(Error::Generic(format!("Scan error: {e}"))),
        }
    }

    pub async fn prepare_sync_batch(
        &self,
        batch_size: Option<u32>,
    ) -> Result<Option<Vec<u8>>, Error> {
        use prost::Message;
        use zcash_client_backend::data_api::scanning::{ScanPriority, ScanRange};
        use zcash_client_backend::proto::compact_formats::CompactBlock;
        use zcash_client_backend::proto::service::{self, BlockId};

        let batch_size = batch_size.unwrap_or(BATCH_SIZE);
        let mut client = self.client.clone();

        // We need a write lock for update_chain_tip, suggest_scan_ranges, etc.
        let mut db = self.db.write().await;

        // 1. Update subtree roots
        Self::do_update_subtree_roots(&mut client, &mut *db).await?;

        // 2. Update chain tip
        let tip_height: zcash_protocol::consensus::BlockHeight = client
            .get_latest_block(service::ChainSpec::default())
            .await
            .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?
            .get_ref()
            .height
            .try_into()
            .map_err(|_| Error::Generic("Invalid tip height".into()))?;
        db.update_chain_tip(tip_height)
            .map_err(|e| Error::Generic(format!("DB error: {e}")))?;

        // 2.5. Refresh transparent UTXOs for all accounts
        self.refresh_transparent_utxos_for_all_accounts(&mut client, &mut *db)
            .await?;

        // 3. Handle verify-priority ranges inline (not journaled, they're small)
        {
            let db_cache = MemBlockCache::new();
            loop {
                let scan_ranges = db
                    .suggest_scan_ranges()
                    .map_err(|e| Error::Generic(format!("{e}")))?;
                match scan_ranges.first() {
                    Some(scan_range) if scan_range.priority() == ScanPriority::Verify => {
                        Self::download_and_insert_blocks(&mut client, &db_cache, scan_range)
                            .await?;
                        let chain_state = Self::fetch_chain_state(
                            &mut client,
                            scan_range.block_range().start - 1,
                        )
                        .await?;
                        let rewound = Self::scan_with_reorg_handling(
                            &self.network,
                            &db_cache,
                            &mut *db,
                            scan_range.block_range().start,
                            &chain_state,
                            scan_range.len(),
                        )?;
                        db_cache
                            .delete(scan_range.clone())
                            .await
                            .map_err(|e| Error::Generic(format!("{e}")))?;
                        if rewound {
                            continue;
                        }
                    }
                    _ => break,
                }
            }
        }

        // 4. Get next scan range
        let scan_ranges = db
            .suggest_scan_ranges()
            .map_err(|e| Error::Generic(format!("{e}")))?;
        let first_batch = scan_ranges
            .into_iter()
            .flat_map(|r| {
                (0..).scan(r, |acc, _| {
                    if acc.is_empty() {
                        None
                    } else if let Some((cur, next)) =
                        acc.split_at(acc.block_range().start + batch_size)
                    {
                        *acc = next;
                        Some(cur)
                    } else {
                        let cur = acc.clone();
                        let end_val = acc.block_range().end;
                        *acc = ScanRange::from_parts(end_val..end_val, acc.priority());
                        Some(cur)
                    }
                })
            })
            .next();

        let scan_range = match first_batch {
            Some(r) => r,
            None => return Ok(None),
        };

        // 5. Download compact blocks for the range
        let mut start_id = service::BlockId::default();
        start_id.height = scan_range.block_range().start.into();
        let mut end_id = service::BlockId::default();
        end_id.height = (scan_range.block_range().end - 1).into();
        let compact_blocks: Vec<CompactBlock> = client
            .get_block_range(service::BlockRange {
                start: Some(start_id),
                end: Some(end_id),
                pool_types: vec![],
            })
            .await
            .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?
            .into_inner()
            .try_collect()
            .await
            .map_err(|e| Error::Generic(format!("Stream error: {e}")))?;

        // 6. Download tree state for range.start - 1
        let tree_state_proto = client
            .get_tree_state(BlockId {
                height: (scan_range.block_range().start - 1).into(),
                hash: vec![],
            })
            .await
            .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?
            .into_inner();

        // 7. Serialize into journal entry format:
        //    [range_start:u32 | range_end:u32 | tree_state_len:u32 | tree_state_bytes | (block_len:u32 + block_bytes)*]
        let range_start: u32 = scan_range.block_range().start.into();
        let range_end: u32 = scan_range.block_range().end.into();
        let tree_state_bytes = tree_state_proto.encode_to_vec();
        let ts_len = tree_state_bytes.len() as u32;

        let mut blocks_bytes = Vec::new();
        for cb in &compact_blocks {
            let encoded = cb.encode_to_vec();
            blocks_bytes.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
            blocks_bytes.extend_from_slice(&encoded);
        }

        let mut journal_data = Vec::with_capacity(12 + tree_state_bytes.len() + blocks_bytes.len());
        journal_data.extend_from_slice(&range_start.to_le_bytes());
        journal_data.extend_from_slice(&range_end.to_le_bytes());
        journal_data.extend_from_slice(&ts_len.to_le_bytes());
        journal_data.extend_from_slice(&tree_state_bytes);
        journal_data.extend_from_slice(&blocks_bytes);

        // 8. Now scan the blocks we just downloaded (with reorg handling)
        let db_cache = MemBlockCache::new();
        db_cache
            .insert(compact_blocks)
            .await
            .map_err(|e| Error::Generic(format!("{e}")))?;
        let chain_state = tree_state_proto
            .to_chain_state()
            .map_err(|e| Error::Generic(format!("{e}")))?;
        let rewound = Self::scan_with_reorg_handling(
            &self.network,
            &db_cache,
            &mut *db,
            scan_range.block_range().start,
            &chain_state,
            scan_range.len(),
        )?;
        if rewound {
            return Ok(None);
        }

        Ok(Some(journal_data))
    }

    /// Scan a journal entry (for recovery from base snapshot + journal).
    /// This consumes the raw journal entry bytes and applies the scan to the wallet DB.
    pub async fn scan_journal_entry(&self, journal_data: &[u8]) -> Result<bool, Error> {
        use prost::Message;
        use zcash_client_backend::proto::compact_formats::CompactBlock;
        use zcash_client_backend::proto::service;

        if journal_data.len() < 12 {
            return Err(Error::Generic("Journal data too short".into()));
        }

        let range_start = u32::from_le_bytes(journal_data[0..4].try_into().unwrap());
        let range_end = u32::from_le_bytes(journal_data[4..8].try_into().unwrap());
        let ts_len = u32::from_le_bytes(journal_data[8..12].try_into().unwrap()) as usize;

        if journal_data.len() < 12 + ts_len {
            return Err(Error::Generic("Journal data truncated (tree state)".into()));
        }

        let tree_state_bytes = &journal_data[12..12 + ts_len];
        let blocks_data = &journal_data[12 + ts_len..];

        let tree_state_proto = service::TreeState::decode(tree_state_bytes)
            .map_err(|e| Error::Generic(format!("Failed to decode TreeState: {e}")))?;
        let chain_state = tree_state_proto
            .to_chain_state()
            .map_err(|e| Error::Generic(format!("Failed to parse ChainState: {e}")))?;

        // Deserialize compact blocks
        let mut compact_blocks = Vec::new();
        let mut offset = 0;
        while offset + 4 <= blocks_data.len() {
            let block_len =
                u32::from_le_bytes(blocks_data[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            if offset + block_len > blocks_data.len() {
                return Err(Error::Generic("Journal data truncated (block)".into()));
            }
            let cb = CompactBlock::decode(&blocks_data[offset..offset + block_len])
                .map_err(|e| Error::Generic(format!("Failed to decode CompactBlock: {e}")))?;
            compact_blocks.push(cb);
            offset += block_len;
        }

        let db_cache = MemBlockCache::new();
        db_cache
            .insert(compact_blocks)
            .await
            .map_err(|e| Error::Generic(format!("{e}")))?;

        let from_height = zcash_protocol::consensus::BlockHeight::from_u32(range_start);
        let block_count = (range_end - range_start) as usize;

        let mut db = self.db.write().await;
        let rewound = Self::scan_with_reorg_handling(
            &self.network,
            &db_cache,
            &mut *db,
            from_height,
            &chain_state,
            block_count,
        )?;
        if rewound {
            return Ok(true);
        }

        // Check if more work remains
        let remaining = db
            .suggest_scan_ranges()
            .map_err(|e| Error::Generic(format!("{e}")))?;
        let has_more = remaining.iter().any(|r| !r.is_empty());

        Ok(has_more)
    }

    /// Replay multiple journal entries in sequence (for recovery from base snapshot + journal).
    pub async fn replay_journal_entries(&self, entries: Vec<Vec<u8>>) -> Result<(), Error> {
        for (i, entry) in entries.iter().enumerate() {
            tracing::info!("Replaying journal entry {}/{}", i + 1, entries.len());
            self.scan_journal_entry(entry).await?;
        }
        Ok(())
    }

    // Internal helper: download blocks and insert into cache
    async fn download_and_insert_blocks(
        client: &mut CompactTxStreamerClient<T>,
        db_cache: &MemBlockCache,
        scan_range: &zcash_client_backend::data_api::scanning::ScanRange,
    ) -> Result<(), Error> {
        use zcash_client_backend::proto::service;

        let mut start = service::BlockId::default();
        start.height = scan_range.block_range().start.into();
        let mut end = service::BlockId::default();
        end.height = (scan_range.block_range().end - 1).into();
        let range = service::BlockRange {
            start: Some(start),
            end: Some(end),
            pool_types: vec![],
        };
        let compact_blocks: Vec<zcash_client_backend::proto::compact_formats::CompactBlock> =
            client
                .get_block_range(range)
                .await
                .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?
                .into_inner()
                .try_collect()
                .await
                .map_err(|e| Error::Generic(format!("Stream error: {e}")))?;
        db_cache
            .insert(compact_blocks)
            .await
            .map_err(|e| Error::Generic(format!("{e}")))?;
        Ok(())
    }

    // Internal helper: fetch chain state at a given height
    async fn fetch_chain_state(
        client: &mut CompactTxStreamerClient<T>,
        height: zcash_protocol::consensus::BlockHeight,
    ) -> Result<zcash_client_backend::data_api::chain::ChainState, Error> {
        use zcash_client_backend::proto::service::BlockId;
        let tree_state = client
            .get_tree_state(BlockId {
                height: height.into(),
                hash: vec![],
            })
            .await
            .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?;
        tree_state
            .into_inner()
            .to_chain_state()
            .map_err(|e| Error::Generic(format!("Failed to parse ChainState: {e}")))
    }

    // Internal helper: update subtree roots from lightwalletd
    async fn do_update_subtree_roots(
        client: &mut CompactTxStreamerClient<T>,
        db: &mut W,
    ) -> Result<(), Error> {
        use zcash_client_backend::data_api::chain::CommitmentTreeRoot;
        use zcash_client_backend::proto::service;
        use zcash_primitives::merkle_tree::HashSer;

        let mut request = service::GetSubtreeRootsArg::default();
        request.set_shielded_protocol(service::ShieldedProtocol::Sapling);
        let sapling_roots: Vec<CommitmentTreeRoot<sapling::Node>> = client
            .get_subtree_roots(request)
            .await
            .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?
            .into_inner()
            .and_then(|root| async move {
                let root_hash = sapling::Node::read(&root.root_hash[..])?;
                Ok(CommitmentTreeRoot::from_parts(
                    zcash_protocol::consensus::BlockHeight::from_u32(
                        root.completing_block_height as u32,
                    ),
                    root_hash,
                ))
            })
            .try_collect()
            .await
            .map_err(|e| Error::Generic(format!("Stream error: {e}")))?;
        db.put_sapling_subtree_roots(0, &sapling_roots)
            .map_err(|e| Error::Generic(format!("DB error: {e}")))?;

        let mut request = service::GetSubtreeRootsArg::default();
        request.set_shielded_protocol(service::ShieldedProtocol::Orchard);
        let orchard_roots: Vec<CommitmentTreeRoot<orchard::tree::MerkleHashOrchard>> = client
            .get_subtree_roots(request)
            .await
            .map_err(|e| Error::Generic(format!("gRPC error: {e}")))?
            .into_inner()
            .and_then(|root| async move {
                let root_hash = orchard::tree::MerkleHashOrchard::read(&root.root_hash[..])?;
                Ok(CommitmentTreeRoot::from_parts(
                    zcash_protocol::consensus::BlockHeight::from_u32(
                        root.completing_block_height as u32,
                    ),
                    root_hash,
                ))
            })
            .try_collect()
            .await
            .map_err(|e| Error::Generic(format!("Stream error: {e}")))?;
        db.put_orchard_subtree_roots(0, &orchard_roots)
            .map_err(|e| Error::Generic(format!("DB error: {e}")))?;

        Ok(())
    }

    async fn refresh_transparent_utxos_for_all_accounts(
        &self,
        client: &mut CompactTxStreamerClient<T>,
        db: &mut W,
    ) -> Result<(), Error> {
        let account_ids = db
            .get_account_ids()
            .map_err(|e| Error::Generic(format!("Failed to get account IDs: {e}")))?;

        for account_id in account_ids {
            let start_height = db
                .utxo_query_height(account_id)
                .map_err(|e| Error::Generic(format!("Failed to get UTXO query height: {e}")))?;

            tracing::info!(
                "Refreshing UTXOs for {:?} from height {}",
                account_id,
                start_height,
            );

            zcash_client_backend::sync::refresh_utxos(
                &self.network,
                client,
                db,
                account_id,
                start_height,
            )
            .await
            .map_err(|e: zcash_client_backend::sync::Error<(), _, ()>| {
                Error::Generic(format!("Failed to refresh UTXOs: {e:?}"))
            })?;
        }

        Ok(())
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
        use zcash_protocol::consensus::{NetworkUpgrade, Parameters};

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
            TransparentOutputFilter::All,
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

    pub async fn pczt_create(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
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
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment = Payment::new(
            to_address,
            Some(Zatoshis::from_u64(value)?),
            memo,
            None,
            None,
            vec![],
        )
        .map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
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

    pub async fn pczt_shield_dynamic_fee(
        &self,
        account_id: AccountId,
        marginal_fee_zats: u64,
    ) -> Result<Pczt, Error> {
        let change_strategy =
            zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
                dynamic_zip317_fee_rule(marginal_fee_zats)?,
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
            self.confirmations_policy(),
            TransparentOutputFilter::All,
        )
        .map_err(|e| Error::Generic(format!("Error when shielding: {:?}", e)))?;

        let pczt = create_pczt_from_proposal::<
            _,
            _,
            <MemoryWalletDb<Network> as InputSource>::Error,
            _,
            <prim_zip317::FeeRule as FeeRule>::Error,
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

    pub async fn pczt_create_dynamic_fee(
        &self,
        account_id: AccountId,
        to_address: ZcashAddress,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<Pczt, Error> {
        let change_strategy =
            zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
                dynamic_zip317_fee_rule(marginal_fee_zats)?,
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
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment = Payment::new(
            to_address,
            Some(Zatoshis::from_u64(value)?),
            memo,
            None,
            None,
            vec![],
        )
        .map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
        let mut db = self.db.write().await;
        let proposal = propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
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
            <prim_zip317::FeeRule as FeeRule>::Error,
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

    pub async fn estimate_pczt_shield_fee(
        &self,
        account_id: AccountId,
        marginal_fee_zats: u64,
    ) -> Result<u64, Error> {
        let change_strategy =
            zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
                dynamic_zip317_fee_rule(marginal_fee_zats)?,
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

        // Get all transparent receivers for this account to use as shielding sources.
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

        let proposal = propose_shielding::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            &input_selector,
            &change_strategy,
            SHIELDING_THRESHOLD,
            &from_addrs,
            account_id,
            self.confirmations_policy(),
            TransparentOutputFilter::All,
        )
        .map_err(|e| Error::Generic(format!("Error when estimating shielding fee: {:?}", e)))?;

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
        let change_strategy =
            zcash_client_backend::fees::zip317::MultiOutputChangeStrategy::<_, W>::new(
                dynamic_zip317_fee_rule(marginal_fee_zats)?,
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
        let memo = memo.as_deref().map(MemoBytes::from_bytes).transpose()?;
        let payment = Payment::new(
            to_address,
            Some(Zatoshis::from_u64(value)?),
            memo,
            None,
            None,
            vec![],
        )
        .map_err(|e| Error::Zip321(e.with_index(0)))?;
        let request = TransactionRequest::new(vec![payment])?;
        let mut db = self.db.write().await;
        let proposal = propose_transfer::<_, _, _, _, <W as WalletCommitmentTrees>::Error>(
            &mut *db,
            &self.network,
            account_id,
            &input_selector,
            &change_strategy,
            request,
            self.confirmations_policy(),
        )
        .map_err(|e| Error::Generic(format!("Error when estimating transfer fee: {:?}", e)))?;

        proposal_fee_required_zats(&proposal)
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
}

pub(crate) fn usk_from_seed_str(
    seed: &str,
    passphrase: &str,
    account_id: u32,
    network: &Network,
) -> Result<(UnifiedSpendingKey, SeedFingerprint), Error> {
    let mnemonic = <Mnemonic<English>>::from_phrase(seed).map_err(|_| Error::InvalidSeedPhrase)?;
    let seed = {
        let mut seed = mnemonic.to_seed(passphrase);
        let secret = seed.to_vec();
        seed.zeroize();
        SecretVec::new(secret)
    };
    let seed_fingerprint =
        SeedFingerprint::from_seed(seed.expose_secret()).ok_or(Error::SeedFingerprint)?;
    let usk = UnifiedSpendingKey::from_seed(network, seed.expose_secret(), account_id.try_into()?)?;
    Ok((usk, seed_fingerprint))
}

#[cfg(test)]
mod tests {
    use super::usk_from_seed_str;
    use crate::Network;

    #[test]
    fn usk_from_seed_str_uses_bip39_passphrase() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

        let (_, empty_passphrase_fingerprint) =
            usk_from_seed_str(phrase, "", 0, &Network::MainNetwork).unwrap();
        let (_, protected_fingerprint) = usk_from_seed_str(
            phrase,
            "correct horse battery staple",
            0,
            &Network::MainNetwork,
        )
        .unwrap();

        assert_ne!(
            empty_passphrase_fingerprint.to_bytes(),
            protected_fingerprint.to_bytes()
        );
    }
}
