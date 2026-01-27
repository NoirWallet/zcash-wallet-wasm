use std::{num::NonZeroU32, str::FromStr};

use crate::bindgen::sign::pczt_sign_inner;
use crate::bindgen::{Pczt, ProofGenerationKey, SeedFingerprint};
use crate::{Error, Network, PRUNING_DEPTH, Wallet, usk_from_seed_str};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use nonempty::NonEmpty;
use serde::{Deserialize, Serialize};
use tonic_web_wasm_client::Client;
use wasm_bindgen::prelude::*;
use zcash_address::ZcashAddress;
use zcash_client_backend::fees::StandardFeeRule;
use zcash_client_backend::proto::service::ChainSpec;
use zcash_client_backend::{
    data_api::{AccountPurpose, InputSource, WalletRead, Zip32Derivation},
    proto::service::compact_tx_streamer_client::CompactTxStreamerClient,
};
use zcash_client_memory::MemoryWalletDb;
use zcash_keys::keys::UnifiedFullViewingKey;
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
        min_confirmations: u32,
        db_bytes: Option<Box<[u8]>>,
    ) -> Result<WebWallet, Error> {
        let network = Network::from_str(network)?;
        let min_confirmations = NonZeroU32::try_from(min_confirmations)
            .map_err(|_| Error::InvalidMinConformations(min_confirmations))?;
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
            inner: Wallet::new(db, client, network, min_confirmations)?,
        })
    }

    pub async fn create_account(
        &self,
        account_name: &str,
        seed_phrase: &str,
        account_hd_index: u32,
        birthday_height: Option<u32>,
    ) -> Result<u32, Error> {
        tracing::info!("Create account called");
        self.inner
            .create_account(
                account_name,
                seed_phrase,
                account_hd_index,
                birthday_height,
                None,
            )
            .await
            .map(|id| *id)
    }

    pub async fn create_account_ufvk(
        &self,
        account_name: &str,
        encoded_ufvk: &str,
        seed_fingerprint: SeedFingerprint,
        account_hd_index: u32,
        birthday_height: Option<u32>,
    ) -> Result<u32, Error> {
        let ufvk = UnifiedFullViewingKey::decode(&self.inner.network, encoded_ufvk)
            .map_err(Error::KeyParse)?;
        let derivation = Some(Zip32Derivation::new(
            seed_fingerprint.into(),
            zip32::AccountId::try_from(account_hd_index)?,
        ));
        self.inner
            .import_ufvk(
                account_name,
                &ufvk,
                AccountPurpose::Spending { derivation },
                birthday_height,
                None,
            )
            .await
            .map(|id| *id)
    }

    pub async fn create_account_view_ufvk(
        &self,
        account_name: &str,
        encoded_ufvk: &str,
        birthday_height: Option<u32>,
    ) -> Result<u32, Error> {
        let ufvk = UnifiedFullViewingKey::decode(&self.inner.network, encoded_ufvk)
            .map_err(Error::KeyParse)?;

        self.inner
            .import_ufvk(
                account_name,
                &ufvk,
                AccountPurpose::ViewOnly,
                birthday_height,
                None,
            )
            .await
            .map(|id| *id)
    }

    // /// Import an account from a Sapling Extended Spending Key
    // ///
    // /// Creates a spending account with only Sapling key material.
    // /// Transparent and Orchard keys will not be available.
    // ///
    // /// # Arguments
    // /// * `account_name` - Name for the new account
    // /// * `sapling_key` - Bech32-encoded Sapling Extended Spending Key (e.g., "secret-extended-key-main...")
    // /// * `birthday_height` - Optional wallet birthday height (defaults to current chain tip)
    // ///
    // /// # Returns
    // /// Account ID of the newly imported account
    // pub async fn import_account_sapling_key(
    //     &self,
    //     account_name: &str,
    //     sapling_key: &str,
    //     birthday_height: Option<u32>,
    // ) -> Result<u32, Error> {
    //     self.inner
    //         .import_sapling_spending_key(account_name, sapling_key, birthday_height, None)
    //         .await
    //         .map(|id| *id)
    // }

    // /// Import an account from a Sapling Extended Full Viewing Key
    // ///
    // /// Creates a view-only account with only Sapling key material.
    // /// The account can view transactions but cannot spend funds.
    // ///
    // /// # Arguments
    // /// * `account_name` - Name for the new account
    // /// * `sapling_fvk` - Bech32-encoded Sapling Extended Full Viewing Key (e.g., "zxviews..." or "zxviewtestsapling...")
    // /// * `birthday_height` - Optional wallet birthday height (defaults to current chain tip)
    // ///
    // /// # Returns
    // /// Account ID of the newly imported account
    // pub async fn import_account_sapling_fvk(
    //     &self,
    //     account_name: &str,
    //     sapling_fvk: &str,
    //     birthday_height: Option<u32>,
    // ) -> Result<u32, Error> {
    //     self.inner
    //         .import_sapling_viewing_key(account_name, sapling_fvk, birthday_height, None)
    //         .await
    //         .map(|id| *id)
    // }

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

    pub async fn get_wallet_summary(&self) -> Result<Option<WalletSummary>, Error> {
        Ok(self.inner.get_wallet_summary().await?.map(Into::into))
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

    pub async fn propose_transfer(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
    ) -> Result<Proposal, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        let proposal = self
            .inner
            .propose_transfer(AccountId::from(account_id), to_address, value)
            .await?;
        Ok(proposal.into())
    }

    pub async fn create_proposed_transactions(
        &self,
        proposal: Proposal,
        seed_phrase: &str,
        account_hd_index: u32,
    ) -> Result<Vec<u8>, Error> {
        let (usk, _) = usk_from_seed_str(seed_phrase, account_hd_index, &self.inner.network)?;

        // Record start time
        let start_time = js_sys::Date::now();

        let txids = self
            .inner
            .create_proposed_transactions(proposal.into(), &usk)
            .await?;

        // Calculate elapsed time in seconds
        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;
        tracing::info!(
            "⏱️ create_proposed_transaction completed in {:.2} seconds",
            elapsed_seconds
        );

        let flattened_txid_bytes = txids.iter().flat_map(|&x| *x.as_ref()).collect();
        Ok(flattened_txid_bytes)
    }

    pub async fn db_to_bytes(&self) -> Result<Box<[u8]>, Error> {
        let bytes = self.inner.db_to_bytes().await?;
        Ok(bytes.into_boxed_slice())
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

    pub async fn get_current_address(&self, account_id: u32) -> Result<String, Error> {
        use zcash_client_backend::data_api::Account;
        use zcash_keys::keys::UnifiedAddressRequest;

        let db = self.inner.db.read().await;
        let account = db
            .get_account(account_id.into())?
            .ok_or(Error::AccountNotFound(account_id))?;

        // Get the unified incoming viewing key from the account
        let uivk = account.uivk();

        // Generate the default address (diversifier index 0) with all receivers
        let (address, _) = uivk.default_address(UnifiedAddressRequest::ALLOW_ALL)?;

        Ok(address.encode(&self.inner.network))
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

    /// Get unified addresses for an account
    ///
    /// # Arguments
    ///
    /// * `account_id` - The account ID to get addresses for
    ///
    /// # Returns
    ///
    /// Returns a UnifiedAddresses object containing:
    /// - unified: The full unified address (u1 format with all receivers)
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
        use zcash_address::unified::{self, Container};
        use zcash_client_backend::data_api::Account;
        use zcash_client_backend::encoding::AddressCodec;
        use zcash_keys::keys::UnifiedAddressRequest as UAR;
        use zcash_primitives::consensus::Parameters;
        use zcash_primitives::legacy::TransparentAddress;

        let db = self.inner.db.read().await;
        let account = db
            .get_account(account_id.into())?
            .ok_or(Error::AccountNotFound(account_id))?;

        // Get the unified incoming viewing key from the account
        let uivk = account.uivk();

        // Generate the full unified address with all available receivers
        let (full_address, _) = uivk.default_address(UAR::ALLOW_ALL)?;
        let unified_str = full_address.encode(&self.inner.network);

        // Parse the unified address to extract individual receivers
        use zcash_address::unified::Encoding;
        let (_, ua) = unified::Address::decode(&unified_str).map_err(|_| Error::InvalidAddress)?;

        let mut orchard_addr: Option<String> = None;
        let mut sapling_addr: Option<String> = None;
        let mut sapling_zs_addr: Option<String> = None;
        let mut transparent_addr: Option<String> = None;

        // Extract each receiver type
        for receiver in ua.items() {
            match receiver {
                unified::Receiver::Orchard(data) => {
                    // Create Orchard-only unified address
                    let single_ua =
                        unified::Address::try_from_items(vec![unified::Receiver::Orchard(data)])
                            .map_err(|_| Error::InvalidAddress)?;
                    orchard_addr = Some(single_ua.encode(&self.inner.network.network_type()));
                }
                unified::Receiver::Sapling(data) => {
                    // Create Sapling-only unified address (u1 format)
                    let single_ua =
                        unified::Address::try_from_items(vec![unified::Receiver::Sapling(data)])
                            .map_err(|_| Error::InvalidAddress)?;
                    sapling_addr = Some(single_ua.encode(&self.inner.network.network_type()));

                    // Also generate traditional zs format address
                    use sapling::PaymentAddress;
                    if let Some(payment_addr) = PaymentAddress::from_bytes(&data) {
                        sapling_zs_addr = Some(payment_addr.encode(&self.inner.network));
                    }
                }
                unified::Receiver::P2pkh(data) => {
                    let t_addr = TransparentAddress::PublicKeyHash(data);
                    transparent_addr = Some(t_addr.encode(&self.inner.network));
                }
                _ => {}
            }
        }

        // Create the result structure
        let result = crate::bindgen::mnemonic::UnifiedAddresses {
            unified: unified_str,
            orchard: orchard_addr,
            sapling: sapling_addr,
            sapling_zs: sapling_zs_addr,
            transparent: transparent_addr,
        };

        Ok(serde_wasm_bindgen::to_value(&result)?)
    }

    pub async fn pczt_shield(&self, account_id: u32) -> Result<Pczt, Error> {
        self.inner
            .pczt_shield(account_id.into())
            .await
            .map(Into::into)
    }

    /// Shield transparent funds by moving them to the Orchard shielded pool
    /// This is a direct method that doesn't use PCZT (works around metadata limitations)
    ///
    /// # Arguments
    ///
    /// * `account_id` - The ID of the account to shield funds from
    /// * `mnemonic` - The mnemonic phrase to derive spending keys
    ///
    /// # Returns
    ///
    /// Returns transaction IDs of the shielding transactions
    pub async fn shield_funds(
        &self,
        account_id: u32,
        usk: crate::bindgen::UnifiedSpendingKey,
    ) -> Result<JsValue, Error> {
        let txids = self
            .inner
            .shield_funds(account_id.into(), usk.as_inner())
            .await?;

        // Convert NonEmpty<TxId> to Vec<String>
        let txid_strings: Vec<String> = txids.iter().map(|txid| txid.to_string()).collect();

        serde_wasm_bindgen::to_value(&txid_strings).map_err(Error::from)
    }

    pub async fn oneclick_pczt_transfer(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
        usk: super::UnifiedSpendingKey,
        seed_fp: SeedFingerprint,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
        use_prove_server: Option<bool>,
    ) -> Result<String, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        let proof_gen_key = sapling_proof_gen_key.map(Into::into);
        let pczt = self
            .inner
            .pczt_create(AccountId::from(account_id), to_address, value)
            .await?;
        let signed_pczt =
            pczt_sign_inner(self.inner.network, pczt, usk.into(), seed_fp.into()).await?;
        let proved_pczt = if use_prove_server.unwrap_or(false) {
            let signed_pczt_string = BASE64_STANDARD.encode(signed_pczt.serialize());
            match send_pczt_to_prove_server(signed_pczt_string).await {
                Ok(ProveResponse { proved_pczt }) => {
                    let proved_pczt_bytes = BASE64_STANDARD.decode(&proved_pczt).unwrap();
                    pczt::Pczt::parse(&proved_pczt_bytes).unwrap()
                }
                Err(e) => {
                    tracing::error!("send_pczt_to_prove_server failed: {e}, try inner.pczt_prove.");
                    self.inner.pczt_prove(signed_pczt, proof_gen_key).await?
                }
            }
        } else {
            self.inner.pczt_prove(signed_pczt, proof_gen_key).await?
        };
        let txid = self.inner.pczt_send(proved_pczt).await?;
        Ok(txid)
    }

    pub async fn pczt_create(
        &self,
        account_id: u32,
        to_address: String,
        value: u64,
    ) -> Result<Pczt, Error> {
        let to_address = ZcashAddress::try_from_encoded(&to_address)?;
        self.inner
            .pczt_create(AccountId::from(account_id), to_address, value)
            .await
            .map(Into::into)
    }

    pub async fn pczt_prove(
        &self,
        pczt: Pczt,
        sapling_proof_gen_key: Option<ProofGenerationKey>,
    ) -> Result<Pczt, Error> {
        let pczt_inner = pczt.into();
        let proof_gen_key = sapling_proof_gen_key.map(Into::into);

        // Record start time
        let start_time = js_sys::Date::now();

        let result_pczt = self.inner.pczt_prove(pczt_inner, proof_gen_key).await?;

        // Calculate elapsed time in seconds
        let end_time = js_sys::Date::now();
        let elapsed_seconds = (end_time - start_time) / 1000.0;

        tracing::info!("⏱️ pczt_prove completed in {:.2} seconds", elapsed_seconds);

        Ok(result_pczt.into())
    }

    pub async fn pczt_send(&self, pczt: Pczt) -> Result<String, Error> {
        self.inner.pczt_send(pczt.into()).await
    }

    pub fn pczt_combine(&self, pczts: Vec<Pczt>) -> Result<Pczt, Error> {
        self.inner
            .pczt_combine(pczts.into_iter().map(Into::into).collect())
            .map(Into::into)
    }

    pub async fn get_current_address_transparent(&self, account_id: u32) -> Result<String, Error> {
        use zcash_client_backend::data_api::Account;
        use zcash_client_backend::encoding::AddressCodec;
        use zcash_keys::keys::UnifiedAddressRequest;

        let db = self.inner.db.read().await;
        let account = db
            .get_account(account_id.into())?
            .ok_or(Error::AccountNotFound(account_id))?;

        // Get the unified incoming viewing key from the account
        let uivk = account.uivk();

        // Generate the default address (diversifier index 0) with all receivers
        let (address, _) = uivk.default_address(UnifiedAddressRequest::ALLOW_ALL)?;

        // Extract the transparent receiver from the unified address
        let transparent_addr = address.transparent().ok_or_else(|| {
            Error::Generic("No transparent receiver in unified address".to_string())
        })?;

        Ok(transparent_addr.encode(&self.inner.network))
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
    pub fn account_balances(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.account_balances).unwrap()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountBalance {
    pub sapling_balance: u64,
    pub orchard_balance: u64,
    pub unshielded_balance: u64,
}

impl From<zcash_client_backend::data_api::AccountBalance> for AccountBalance {
    fn from(balance: zcash_client_backend::data_api::AccountBalance) -> Self {
        AccountBalance {
            sapling_balance: balance.sapling_balance().spendable_value().into(),
            orchard_balance: balance.orchard_balance().spendable_value().into(),
            unshielded_balance: balance.unshielded_balance().spendable_value().into(),
        }
    }
}

impl<T> From<zcash_client_backend::data_api::WalletSummary<T>> for WalletSummary
where
    T: std::cmp::Eq + std::hash::Hash + std::ops::Deref<Target = u32> + Clone,
{
    fn from(summary: zcash_client_backend::data_api::WalletSummary<T>) -> Self {
        let mut account_balances: Vec<_> = summary
            .account_balances()
            .iter()
            .map(|(k, v)| (*(*k).clone().deref(), (*v).into()))
            .collect();

        account_balances.sort_by(|a, b| a.0.cmp(&b.0));

        WalletSummary {
            account_balances,
            chain_tip_height: summary.chain_tip_height().into(),
            fully_scanned_height: summary.fully_scanned_height().into(),
            next_sapling_subtree_index: summary.next_sapling_subtree_index(),
            next_orchard_subtree_index: summary.next_orchard_subtree_index(),
        }
    }
}

#[derive(Serialize)]
pub struct ProveRequest {
    pub pczt: String,
}

#[derive(Deserialize)]
pub struct ProveResponse {
    pub proved_pczt: String,
}

pub async fn send_pczt_to_prove_server(pczt: String) -> Result<ProveResponse, gloo_net::Error> {
    use gloo_net::http::Request;
    let resp = Request::post("")
        .header("Content-Type", "application/json")
        .json(&ProveRequest { pczt })?
        .send()
        .await?;
    resp.json::<ProveResponse>().await
}
