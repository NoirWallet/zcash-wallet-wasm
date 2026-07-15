use std::str::FromStr;

use crate::bindgen::sign::pczt_sign_inner;
use crate::bindgen::{Pczt as WasmPczt, ProofGenerationKey, SeedFingerprint};
use crate::{Error, Network, PRUNING_DEPTH, Wallet};

use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use tonic_web_wasm_client::Client;
use wasm_bindgen::prelude::*;
use zcash_address::ZcashAddress;
use zcash_client_backend::fees::StandardFeeRule;
use zcash_client_backend::proto::service::ChainSpec;
use zcash_client_backend::{
    data_api::{InputSource, WalletRead, WalletWrite},
    proto::service::compact_tx_streamer_client::CompactTxStreamerClient,
};
use zcash_client_memory::MemoryWalletDb;
use zcash_primitives::transaction::TxId;

pub type MemoryWallet<T> = Wallet<MemoryWalletDb<Network>, T>;
pub type AccountId = <MemoryWalletDb<Network> as WalletRead>::AccountId;
pub type NoteRef = <MemoryWalletDb<Network> as InputSource>::NoteRef;

#[wasm_bindgen]
#[derive(Clone)]
pub struct WebWallet {
    inner: MemoryWallet<tonic_web_wasm_client::Client>,
}

impl WebWallet {
    pub fn client(&self) -> CompactTxStreamerClient<tonic_web_wasm_client::Client> {
        self.inner.client.clone()
    }

    pub fn inner_mut(&mut self) -> &mut MemoryWallet<tonic_web_wasm_client::Client> {
        &mut self.inner
    }
}

#[wasm_bindgen]
impl WebWallet {
    #[wasm_bindgen(constructor)]
    pub fn new(
        network: &str,
        lightwalletd_url: &str,
        db_bytes: Option<Box<[u8]>>,
    ) -> Result<WebWallet, Error> {
        let network = Network::from_str(network)?;
        let client = Client::new(lightwalletd_url.to_string());

        let db = match db_bytes {
            Some(bytes) => {
                tracing::info!(
                    "Serialized db was provided to constructor. Attempting to deserialize"
                );
                MemoryWalletDb::decode_new(bytes.as_ref(), network, PRUNING_DEPTH)?
            }
            None => MemoryWalletDb::new(network, PRUNING_DEPTH),
        };

        Ok(Self {
            inner: Wallet::new(db, client, network)?,
        })
    }

    pub async fn create_account(
        &self,
        account_name: &str,
        seed_phrase: &str,
        account_hd_index: u32,
        birthday_height: Option<u32>,
        passphrase: Option<String>,
    ) -> Result<u32, Error> {
        tracing::info!("Create account called");
        self.inner
            .create_account(
                account_name,
                seed_phrase,
                passphrase.as_deref().unwrap_or(""),
                account_hd_index,
                birthday_height,
                None,
            )
            .await
            .map(|id| *id)
    }

    pub async fn sync(&self) -> Result<(), Error> {
        // Record start time
        let start_time = js_sys::Date::now();

        self.inner.sync().await?;

        // Calculate elapsed time in seconds
        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;
        tracing::info!("⏱️ Sync completed in {:.2} seconds", elapsed_seconds);

        Ok(())
    }

    /// Runs a single batch of the sync process without looping.
    /// Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
    pub async fn sync_next_batch(&self, batch_size: Option<u32>) -> Result<bool, Error> {
        let start_time = js_sys::Date::now();

        let processed_batch = self.inner.sync_next_batch(batch_size).await?;

        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;
        tracing::info!(
            "⏱️ Sync next batch completed in {:.2} seconds, processed_batch: {}",
            elapsed_seconds,
            processed_batch
        );

        Ok(processed_batch)
    }

    /// Runs a single batch using the optimized scanning strategy.
    /// Returns `true` if a scan range was processed, `false` if no scan ranges were suggested.
    pub async fn sync_next_batch_optimized(&self, batch_size: Option<u32>) -> Result<bool, Error> {
        let start_time = js_sys::Date::now();

        let processed_batch = self.inner.sync_next_batch_optimized(batch_size).await?;

        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;
        tracing::info!(
            "⏱️ Optimized sync next batch completed in {:.2} seconds, processed_batch: {}",
            elapsed_seconds,
            processed_batch
        );

        Ok(processed_batch)
    }

    pub async fn get_wallet_summary(&self) -> Result<Option<WalletSummary>, Error> {
        self.inner
            .get_wallet_summary()
            .await?
            .map(WalletSummary::try_from)
            .transpose()
    }

    /// Get the birthday height for a specific account
    ///
    /// # Arguments
    ///
    /// * `account_id` - The account ID to get the birthday height for
    ///
    /// # Returns
    ///
    /// The birthday height (block height) for the specified account
    ///
    /// # Example
    ///
    /// ```javascript
    /// const accountId = 0;
    /// const birthdayHeight = await wallet.get_account_birthday(accountId);
    /// console.log(`Account ${accountId} birthday height: ${birthdayHeight}`);
    /// ```
    pub async fn get_account_birthday(&self, account_id: u32) -> Result<u32, Error> {
        self.inner
            .get_account_birthday(AccountId::from(account_id))
            .await
    }

    /// Get the Unix timestamp of a block at a given height
    ///
    /// # Arguments
    ///
    /// * `height` - The block height
    ///
    /// # Returns
    ///
    /// Unix timestamp (seconds since epoch) when the block was mined
    ///
    /// # Example
    ///
    /// ```javascript
    /// const height = 2855000;
    /// const timestamp = await wallet.get_block_time(height);
    /// const date = new Date(timestamp * 1000);
    /// console.log(`Block ${height} was mined at: ${date}`);
    /// ```
    pub async fn get_block_time(&self, height: u32) -> Result<u32, Error> {
        self.inner.get_block_time(height).await
    }

    /// Get the latest block height from the blockchain
    ///
    /// # Returns
    ///
    /// The current chain tip height
    ///
    /// # Example
    ///
    /// ```javascript
    /// const latestHeight = await wallet.get_latest_height();
    /// console.log(`Current blockchain height: ${latestHeight}`);
    /// ```
    pub async fn get_latest_height(&self) -> Result<u32, Error> {
        self.inner.get_latest_height().await
    }

    /// Find the block height closest to a given Unix timestamp
    ///
    /// Uses binary search to efficiently locate the block height corresponding
    /// to a specific time. Useful for determining wallet birthday heights based
    /// on dates.
    ///
    /// # Arguments
    ///
    /// * `time` - Unix timestamp (seconds since epoch)
    ///
    /// # Returns
    ///
    /// Block height closest to the given timestamp (within ~1000 blocks precision)
    ///
    /// # Example
    ///
    /// ```javascript
    /// // Find block height for January 1, 2024
    /// const timestamp = Math.floor(new Date('2024-01-01').getTime() / 1000);
    /// const height = await wallet.get_block_by_time(timestamp);
    /// console.log(`Block height at 2024-01-01: ${height}`);
    ///
    /// // Use as birthday height when importing a wallet
    /// const birthdayTimestamp = Math.floor(new Date('2023-06-15').getTime() / 1000);
    /// const birthdayHeight = await wallet.get_block_by_time(birthdayTimestamp);
    /// ```
    pub async fn get_block_by_time(&self, time: u32) -> Result<u32, Error> {
        self.inner.get_block_by_time(time).await
    }

    pub async fn get_transaction_history(&self, account_id: u32) -> Result<JsValue, Error> {
        let db = self.inner.db.read().await;
        let entries = db.get_transaction_history(AccountId::from(account_id));
        serde_wasm_bindgen::to_value(&entries).map_err(Error::from)
    }

    /// Journal-aware sync: download + scan one batch, return journal data for persistence.
    /// Returns null if fully synced, or Uint8Array with journal entry data.
    /// When `refresh` is false, session setup is skipped (call `sync_session_setup`
    /// once at the start of the catch-up session first).
    pub async fn prepare_sync_batch(
        &self,
        batch_size: Option<u32>,
        refresh: bool,
    ) -> Result<Option<Box<[u8]>>, Error> {
        let start_time = js_sys::Date::now();

        let result = self.inner.prepare_sync_batch(batch_size, refresh).await?;

        let elapsed_seconds = (js_sys::Date::now() - start_time) / 1000.0;
        match &result {
            Some(data) => {
                tracing::info!(
                    "⏱️ prepare_sync_batch completed in {:.2}s, journal entry: {} bytes",
                    elapsed_seconds,
                    data.len()
                );
            }
            None => {
                tracing::info!(
                    "⏱️ prepare_sync_batch: fully synced ({:.2}s)",
                    elapsed_seconds
                );
            }
        }

        Ok(result.map(|v| v.into_boxed_slice()))
    }

    /// Benchmark helper: download a block range over gRPC-web without scanning.
    /// Returns the number of blocks fetched. Used to probe download throughput.
    pub async fn bench_download_range(&self, start_height: u32, count: u32) -> Result<u32, Error> {
        self.inner.bench_download_range(start_height, count).await
    }

    /// Run session-level setup (subtree roots, chain tip, transparent UTXOs)
    /// once before a multi-batch catch-up so subsequent batches can skip it.
    pub async fn sync_session_setup(&self) -> Result<(), Error> {
        self.inner.sync_session_setup().await
    }

    /// Benchmark helper: one baseline journal batch with per-stage timings (ms)
    /// as JSON `{"subtree","setup","download","scan"}`, or undefined when synced.
    /// When `refresh` is false, session setup is skipped (call
    /// `sync_session_setup` once first).
    pub async fn bench_prepare_sync_batch_timed(
        &self,
        batch_size: Option<u32>,
        refresh: bool,
    ) -> Result<Option<String>, Error> {
        self.inner
            .bench_prepare_sync_batch_timed(batch_size, refresh)
            .await
    }

    /// Optimized journal-aware sync: same as [`prepare_sync_batch`] but uses
    /// External-only IVK scanning for contiguous forward ranges.
    pub async fn prepare_sync_batch_optimized(
        &self,
        batch_size: Option<u32>,
    ) -> Result<Option<Box<[u8]>>, Error> {
        let start_time = js_sys::Date::now();

        let result = self.inner.prepare_sync_batch_optimized(batch_size).await?;

        let elapsed_seconds = (js_sys::Date::now() - start_time) / 1000.0;
        match &result {
            Some(data) => {
                tracing::info!(
                    "⏱️ prepare_sync_batch_optimized completed in {:.2}s, journal entry: {} bytes",
                    elapsed_seconds,
                    data.len()
                );
            }
            None => {
                tracing::info!(
                    "⏱️ prepare_sync_batch_optimized: fully synced ({:.2}s)",
                    elapsed_seconds
                );
            }
        }

        Ok(result.map(|v| v.into_boxed_slice()))
    }

    /// Scan a journal entry for recovery (replay from base snapshot + journal).
    /// Returns true if more scan ranges remain.
    pub async fn scan_journal_entry(&self, journal_data: &[u8]) -> Result<bool, Error> {
        let start_time = js_sys::Date::now();

        let has_more = self.inner.scan_journal_entry(journal_data).await?;

        let elapsed_seconds = (js_sys::Date::now() - start_time) / 1000.0;
        tracing::info!(
            "⏱️ scan_journal_entry completed in {:.2}s, has_more: {}",
            elapsed_seconds,
            has_more
        );

        Ok(has_more)
    }

    /// Replay multiple journal entries in sequence (for recovery).
    pub async fn replay_journal_entries(
        &self,
        entries: Vec<js_sys::Uint8Array>,
    ) -> Result<(), Error> {
        let entries: Vec<Vec<u8>> = entries.iter().map(|e| e.to_vec()).collect();

        let start_time = js_sys::Date::now();
        tracing::info!("Replaying {} journal entries...", entries.len());

        self.inner.replay_journal_entries(entries).await?;

        let elapsed_seconds = (js_sys::Date::now() - start_time) / 1000.0;
        tracing::info!("⏱️ Journal replay completed in {:.2}s", elapsed_seconds);

        Ok(())
    }

    pub async fn db_to_bytes(&self) -> Result<Box<[u8]>, Error> {
        let bytes = self.inner.db_to_bytes().await?;
        Ok(bytes.into_boxed_slice())
    }

    pub async fn db_to_bytes_chunked(&self, callback: &js_sys::Function) -> Result<(), Error> {
        let cb = callback.clone();
        self.inner
            .db_to_bytes_chunked(move |chunk: &[u8]| {
                let arr = js_sys::Uint8Array::from(chunk);
                let _ = cb.call1(&JsValue::NULL, &arr);
            })
            .await?;
        Ok(())
    }

    pub async fn send_authorized_transactions(&self, txids: Vec<u8>) -> Result<(), Error> {
        let txids = txids
            .chunks(32)
            .map(|txid| {
                let txid_arr: [u8; 32] = txid.try_into().map_err(|_| Error::TxIdParse)?;
                Ok(TxId::from_bytes(txid_arr))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let txids = NonEmpty::from_vec(txids).ok_or(Error::TxIdParse)?;
        self.inner.send_authorized_transactions(&txids).await
    }

    /// Get account ID by account name
    ///
    /// # Arguments
    ///
    /// * `account_name` - The name of the account to find
    ///
    /// # Returns
    ///
    /// Returns the account ID as u32 if found
    ///
    /// # Example
    ///
    /// ```javascript
    /// const accountId = await wallet.get_account_id_by_name("MyAccount");
    /// console.log("Account ID:", accountId);
    /// ```
    pub async fn get_account_id_by_name(&self, account_name: &str) -> Result<u32, Error> {
        let account_id = self.inner.get_account_id_by_name(account_name).await?;
        Ok(*account_id)
    }

    /// Get the HD account index for an account.
    ///
    /// This returns the ZIP-32 account index that was used to derive this account's keys.
    /// Returns None if the account was imported and doesn't have derivation info.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The account ID
    ///
    /// # Returns
    ///
    /// The HD account index (u32), or None if not available
    pub async fn get_account_hd_index(&self, account_id: u32) -> Result<Option<u32>, Error> {
        use zcash_client_backend::data_api::{Account, AccountPurpose, AccountSource};

        let db = self.inner.db.read().await;
        let account = db
            .get_account(account_id.into())?
            .ok_or(Error::AccountNotFound(account_id))?;

        // Get the derivation info from the account
        match account.source() {
            AccountSource::Derived { derivation, .. } => {
                Ok(Some(u32::from(derivation.account_index())))
            }
            AccountSource::Imported { purpose, .. } => {
                // Imported accounts can also have derivation info in their purpose
                match purpose {
                    AccountPurpose::Spending { derivation } => {
                        Ok(derivation.as_ref().map(|d| u32::from(d.account_index())))
                    }
                    AccountPurpose::ViewOnly => Ok(None),
                }
            }
        }
    }

    /// Get addresses for an account
    ///
    /// # Arguments
    ///
    /// * `account_id` - The account ID to get addresses for
    ///
    /// # Returns
    ///
    /// Returns a UnifiedAddresses object containing:
    /// - unified: The shielded unified address (u1 format with Orchard/Sapling receivers only)
    /// - orchard: Orchard-only unified address (if available)
    /// - sapling: Sapling-only unified address (u1 format, if available)
    /// - sapling_zs: Sapling traditional address (zs format, if available)
    /// - transparent: Transparent P2PKH address (if available)
    ///
    /// # Example
    ///
    /// ```javascript
    /// const addresses = await wallet.get_unified_addresses(accountId);
    /// console.log("Unified Address:", addresses.unified);
    /// console.log("Sapling (zs):", addresses.sapling_zs);
    /// console.log("Transparent:", addresses.transparent);
    /// ```
    pub async fn get_unified_addresses(&self, account_id: u32) -> Result<JsValue, Error> {
        use zcash_client_backend::data_api::Account;
        use zcash_keys::keys::UnifiedAddressRequest;

        let db = self.inner.db.read().await;
        let account = db
            .get_account(account_id.into())?
            .ok_or(Error::AccountNotFound(account_id))?;

        // Get the unified incoming viewing key from the account
        let uivk = account.uivk();

        // Generate the first address that includes every receiver supported by the UIVK.
        let (full_address, diversifier_index) =
            uivk.default_address(UnifiedAddressRequest::AllAvailableKeys)?;
        let address_index =
            crate::bindgen::mnemonic::transparent_address_index(&full_address, diversifier_index);
        let result = crate::bindgen::mnemonic::unified_addresses_from_full(
            &full_address,
            &self.inner.network,
            address_index,
        );

        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    /// Generate and persist the next shielded receiving address for an account.
    ///
    /// This returns a unified address containing the account's available shielded receivers
    /// (Orchard/Sapling) and no transparent receiver. It advances the wallet's stored address
    /// position, so callers should persist the wallet DB after using this method.
    ///
    /// # Arguments
    ///
    /// * `account_id` - The account ID to generate the next shielded address for
    ///
    /// # Returns
    ///
    /// Returns a UnifiedAddresses object. `transparent` and `address_index` will be null.
    pub async fn get_next_shielded_address(&self, account_id: u32) -> Result<JsValue, Error> {
        use zcash_client_backend::data_api::Account;
        use zcash_keys::keys::{ReceiverRequirement, UnifiedAddressRequest};

        let account_ref = AccountId::from(account_id);
        let mut db = self.inner.db.write().await;
        let account = db
            .get_account(account_ref)?
            .ok_or(Error::AccountNotFound(account_id))?;
        let uivk = account.uivk();

        let request = UnifiedAddressRequest::custom(
            if uivk.has_orchard() {
                ReceiverRequirement::Require
            } else {
                ReceiverRequirement::Omit
            },
            if uivk.has_sapling() {
                ReceiverRequirement::Require
            } else {
                ReceiverRequirement::Omit
            },
            ReceiverRequirement::Omit,
        )
        .map_err(|_| Error::Generic("Account has no shielded receivers".into()))?;

        let (address, _) = db
            .get_next_available_address(account_ref, request)?
            .ok_or_else(|| Error::Generic("Account cannot generate new addresses".into()))?;
        let result = crate::bindgen::mnemonic::unified_addresses_from_full(
            &address,
            &self.inner.network,
            None,
        );

        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn oneclick_pczt_transfer(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
        usk: super::UnifiedSpendingKey,
        seed_fp: SeedFingerprint,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
        memo: Option<Vec<u8>>,
    ) -> Result<String, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        let pczt = self
            .inner
            .pczt_create(AccountId::from(account_id), to_address, value, memo)
            .await?;

        self.sign_prove_and_send_pczt(pczt, usk, seed_fp, sapling_proof_gen_key)
            .await
    }

    pub async fn oneclick_pczt_shield(
        &self,
        account_id: u32,
        usk: super::UnifiedSpendingKey,
        seed_fp: SeedFingerprint,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
    ) -> Result<String, Error> {
        let pczt = self.inner.pczt_shield(account_id.into()).await?;

        self.sign_prove_and_send_pczt(pczt, usk, seed_fp, sapling_proof_gen_key)
            .await
    }

    /// Import a watch-only spending account from an encoded UFVK (hardware wallet flow).
    ///
    /// # Arguments
    /// * `account_name` - Display name for the account
    /// * `ufvk` - The encoded Unified Full Viewing Key exported by the device
    /// * `seed_fingerprint` - 32-byte ZIP-32 seed fingerprint exported by the device
    /// * `account_hd_index` - ZIP-32 account index on the device
    /// * `birthday_height` - Block height to start scanning from
    /// * `key_source` - Optional tag describing the key origin (e.g. "keystone")
    ///
    /// # Returns
    /// The new account id
    pub async fn create_account_ufvk(
        &self,
        account_name: &str,
        ufvk: &str,
        seed_fingerprint: &[u8],
        account_hd_index: u32,
        birthday_height: Option<u32>,
        key_source: Option<String>,
    ) -> Result<u32, Error> {
        let seed_fp: [u8; 32] = seed_fingerprint
            .try_into()
            .map_err(|_| Error::SeedFingerprint)?;

        self.inner
            .create_account_ufvk(
                account_name,
                ufvk,
                seed_fp,
                account_hd_index,
                birthday_height,
                key_source.as_deref(),
            )
            .await
            .map(|id| *id)
    }

    /// Create an unsigned, unproven PCZT for a transfer (hardware wallet flow).
    ///
    /// The returned PCZT carries ZIP-32 derivation metadata so an external signer
    /// (e.g. Keystone) can locate its keys. Serialize it with `Pczt.serialize()`
    /// before transporting via QR. `marginal_fee_zats` selects the ZIP-317 fee
    /// tier so hardware sends honor the same dynamic-fee tiers as software sends.
    pub async fn pczt_create(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<WasmPczt, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        self.inner
            .pczt_create_dynamic_fee(
                AccountId::from(account_id),
                to_address,
                value,
                memo,
                marginal_fee_zats,
            )
            .await
            .map(Into::into)
    }

    /// Create an unsigned, unproven shielding PCZT (transparent -> Orchard) for
    /// external signing (hardware wallet flow). `marginal_fee_zats` selects the
    /// ZIP-317 fee tier so hardware shields honor the dynamic-fee tiers too.
    pub async fn pczt_shield_create(
        &self,
        account_id: u32,
        marginal_fee_zats: u64,
    ) -> Result<WasmPczt, Error> {
        self.inner
            .pczt_shield_dynamic_fee(account_id.into(), marginal_fee_zats)
            .await
            .map(Into::into)
    }

    /// Prove and broadcast an externally signed PCZT (hardware wallet flow).
    ///
    /// Hardware-wallet accounts never hold Sapling notes (Keystone supports
    /// transparent + Orchard only), so no Sapling proof generation key is needed.
    ///
    /// # Returns
    /// Transaction ID string on success
    pub async fn pczt_prove_and_send(&self, pczt: WasmPczt) -> Result<String, Error> {
        self.inner.pczt_prove_and_send(pczt.into(), None).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn oneclick_pczt_transfer_dynamic_fee(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
        usk: super::UnifiedSpendingKey,
        seed_fp: SeedFingerprint,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
        expiry_delta_blocks: Option<u32>,
    ) -> Result<String, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        let pczt = match expiry_delta_blocks {
            Some(expiry_delta_blocks) => {
                self.inner
                    .pczt_create_dynamic_fee_with_expiry_delta(
                        AccountId::from(account_id),
                        to_address,
                        value,
                        memo,
                        marginal_fee_zats,
                        expiry_delta_blocks,
                    )
                    .await?
            }
            None => {
                self.inner
                    .pczt_create_dynamic_fee(
                        AccountId::from(account_id),
                        to_address,
                        value,
                        memo,
                        marginal_fee_zats,
                    )
                    .await?
            }
        };

        self.sign_prove_and_send_pczt(pczt, usk, seed_fp, sapling_proof_gen_key)
            .await
    }

    pub async fn oneclick_pczt_shield_dynamic_fee(
        &self,
        account_id: u32,
        usk: super::UnifiedSpendingKey,
        seed_fp: SeedFingerprint,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
        marginal_fee_zats: u64,
        expiry_delta_blocks: Option<u32>,
    ) -> Result<String, Error> {
        let pczt = match expiry_delta_blocks {
            Some(expiry_delta_blocks) => {
                self.inner
                    .pczt_shield_dynamic_fee_with_expiry_delta(
                        account_id.into(),
                        marginal_fee_zats,
                        expiry_delta_blocks,
                    )
                    .await?
            }
            None => {
                self.inner
                    .pczt_shield_dynamic_fee(account_id.into(), marginal_fee_zats)
                    .await?
            }
        };

        self.sign_prove_and_send_pczt(pczt, usk, seed_fp, sapling_proof_gen_key)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn estimate_pczt_transfer_fee(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
        memo: Option<Vec<u8>>,
        marginal_fee_zats: u64,
    ) -> Result<u64, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        self.inner
            .estimate_pczt_transfer_fee(
                AccountId::from(account_id),
                to_address,
                value,
                memo,
                marginal_fee_zats,
            )
            .await
    }

    pub async fn estimate_pczt_shield_fee(
        &self,
        account_id: u32,
        marginal_fee_zats: u64,
    ) -> Result<u64, Error> {
        self.inner
            .estimate_pczt_shield_fee(account_id.into(), marginal_fee_zats)
            .await
    }

    /// Sign, prove locally, and send a PCZT transaction.
    ///
    /// This helper method encapsulates the common workflow of:
    /// 1. Signing a PCZT with the provided spending key
    /// 2. Proving the PCZT locally
    /// 3. Sending the proven PCZT to the network
    ///
    /// # Arguments
    /// * `pczt` - The PCZT to process
    /// * `usk` - Unified spending key for signing
    /// * `seed_fp` - Seed fingerprint for signing
    /// * `sapling_proof_gen_key` - Optional Sapling proof generation key
    ///
    /// # Returns
    /// Transaction ID string on success
    async fn sign_prove_and_send_pczt(
        &self,
        pczt: pczt::Pczt,
        usk: super::UnifiedSpendingKey,
        seed_fp: SeedFingerprint,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
    ) -> Result<String, Error> {
        let proof_gen_key = sapling_proof_gen_key.map(Into::into);

        // Sign the PCZT
        let signed_pczt =
            pczt_sign_inner(self.inner.network, pczt, usk.into(), seed_fp.into()).await?;

        // Prove the PCZT locally.
        let proved_pczt = self.inner.pczt_prove(signed_pczt, proof_gen_key).await?;

        // Send the proven PCZT
        let txid = self.inner.pczt_send(proved_pczt).await?;
        Ok(txid)
    }

    pub async fn get_latest_block(&self) -> Result<u64, Error> {
        self.client()
            .get_latest_block(ChainSpec {})
            .await
            .map(|response| response.into_inner().height)
            .map_err(Error::from)
    }
}

#[wasm_bindgen]
pub struct Proposal {
    inner: zcash_client_backend::proposal::Proposal<StandardFeeRule, NoteRef>,
}

impl From<zcash_client_backend::proposal::Proposal<StandardFeeRule, NoteRef>> for Proposal {
    fn from(inner: zcash_client_backend::proposal::Proposal<StandardFeeRule, NoteRef>) -> Self {
        Self { inner }
    }
}

impl From<Proposal> for zcash_client_backend::proposal::Proposal<StandardFeeRule, NoteRef> {
    fn from(proposal: Proposal) -> Self {
        proposal.inner
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct WalletSummary {
    account_balances: Vec<(u32, AccountBalance)>,
    pub chain_tip_height: u32,
    pub fully_scanned_height: u32,
    // scan_progress: Option<Ratio<u64>>,
    pub next_sapling_subtree_index: u64,
    pub next_orchard_subtree_index: u64,
}

#[wasm_bindgen]
impl WalletSummary {
    #[wasm_bindgen(getter)]
    pub fn account_balances(&self) -> Result<JsValue, Error> {
        serde_wasm_bindgen::to_value(&self.account_balances).map_err(Error::from)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountBalance {
    pub sapling_balance: u64,
    pub orchard_balance: u64,
    pub unshielded_balance: u64,
    pub sapling_pending_balance: u64,
    pub orchard_pending_balance: u64,
    pub unshielded_pending_balance: u64,
    pub sapling_total_balance: u64,
    pub orchard_total_balance: u64,
    pub unshielded_total_balance: u64,
    pub spendable_balance: u64,
    pub pending_balance: u64,
    pub total_balance: u64,
}

impl TryFrom<zcash_client_backend::data_api::AccountBalance> for AccountBalance {
    type Error = Error;

    fn try_from(balance: zcash_client_backend::data_api::AccountBalance) -> Result<Self, Error> {
        use zcash_protocol::value::Zatoshis;

        fn pending_value(
            balance: &zcash_client_backend::data_api::Balance,
        ) -> Result<Zatoshis, Error> {
            (balance.change_pending_confirmation() + balance.value_pending_spendability())
                .ok_or(zcash_protocol::value::BalanceError::Overflow)
                .map_err(Error::from)
        }

        let sapling_pending_balance = pending_value(balance.sapling_balance())?;
        let orchard_pending_balance = pending_value(balance.orchard_balance())?;
        let unshielded_pending_balance = pending_value(balance.unshielded_balance())?;
        let pending_balance = (balance.change_pending_confirmation()
            + balance.value_pending_spendability())
        .ok_or(zcash_protocol::value::BalanceError::Overflow)
        .map_err(Error::from)?;

        Ok(AccountBalance {
            sapling_balance: balance.sapling_balance().spendable_value().into(),
            orchard_balance: balance.orchard_balance().spendable_value().into(),
            unshielded_balance: balance.unshielded_balance().spendable_value().into(),
            sapling_pending_balance: sapling_pending_balance.into(),
            orchard_pending_balance: orchard_pending_balance.into(),
            unshielded_pending_balance: unshielded_pending_balance.into(),
            sapling_total_balance: balance.sapling_balance().total().into(),
            orchard_total_balance: balance.orchard_balance().total().into(),
            unshielded_total_balance: balance.unshielded_balance().total().into(),
            spendable_balance: balance.spendable_value().into(),
            pending_balance: pending_balance.into(),
            total_balance: balance.total().into(),
        })
    }
}

impl<T> TryFrom<zcash_client_backend::data_api::WalletSummary<T>> for WalletSummary
where
    T: std::cmp::Eq + std::hash::Hash + std::ops::Deref<Target = u32> + Clone,
{
    type Error = Error;

    fn try_from(summary: zcash_client_backend::data_api::WalletSummary<T>) -> Result<Self, Error> {
        let mut account_balances: Vec<_> = summary
            .account_balances()
            .iter()
            .map(|(k, v)| {
                AccountBalance::try_from(*v).map(|balance| (*(*k).clone().deref(), balance))
            })
            .collect::<Result<_, _>>()?;

        account_balances.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(WalletSummary {
            account_balances,
            chain_tip_height: summary.chain_tip_height().into(),
            fully_scanned_height: summary.fully_scanned_height().into(),
            next_sapling_subtree_index: summary.next_sapling_subtree_index(),
            next_orchard_subtree_index: summary.next_orchard_subtree_index(),
        })
    }
}
