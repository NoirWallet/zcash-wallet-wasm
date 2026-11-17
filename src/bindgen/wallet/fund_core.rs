//! Fund-critical wallet bindings: address exposure and external-key account creation.
//!
//! Split out of `src/bindgen/wallet.rs` so that this surface is one reviewable file;
//! pure move, code unchanged.

use super::*;

pub(super) fn is_user_facing_shielded_address(address: &zcash_keys::address::UnifiedAddress, default_address: &zcash_keys::address::UnifiedAddress) -> bool {
    let has_shielded_receiver = address.has_orchard() || address.has_sapling();
    has_shielded_receiver && (address == default_address || address.transparent().is_none())
}

#[wasm_bindgen]
impl WebWallet {
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
    /// // Display the selected address in the wallet UI; avoid logging account addresses.
    /// ```
    pub async fn get_unified_addresses(&self, account_id: u32) -> Result<JsValue, Error> {
        use zcash_client_backend::data_api::Account;
        use zcash_keys::keys::UnifiedAddressRequest;

        let db = self.inner.db.read().await;
        let account = db.get_account(account_id.into())?.ok_or(Error::AccountNotFound(account_id))?;

        // Get the unified incoming viewing key from the account
        let uivk = account.uivk();

        // Generate the first address that includes every receiver supported by the UIVK.
        let (full_address, diversifier_index) = uivk.default_address(UnifiedAddressRequest::AllAvailableKeys)?;
        let address_index = crate::bindgen::mnemonic::transparent_address_index(&full_address, diversifier_index);
        let result = crate::bindgen::mnemonic::unified_addresses_from_full(&full_address, &self.inner.network, address_index);

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
        let account = db.get_account(account_ref)?.ok_or(Error::AccountNotFound(account_id))?;
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
        let result = crate::bindgen::mnemonic::unified_addresses_from_full(&address, &self.inner.network, None);

        Ok(serde_wasm_bindgen::to_value(&result)?)
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
        let seed_fp: [u8; 32] = seed_fingerprint.try_into().map_err(|_| Error::SeedFingerprint)?;

        self.inner
            .create_account_ufvk(account_name, ufvk, seed_fp, account_hd_index, birthday_height, key_source.as_deref())
            .await
            .map(|id| *id)
    }
}
