//! Key-material derivation for the fund-critical transfer path.
//!
//! Split out of `src/bindgen/keys.rs` so that this surface is one reviewable file;
//! pure move, code unchanged. Items are re-exported by the parent module.

use super::*;

#[wasm_bindgen]
impl UnifiedSpendingKey {
    /// Construct a new UnifiedSpendingKey
    ///
    /// # Arguments
    ///
    /// * `network` - Must be either "main" or "test"
    /// * `seed` - At least 32 bytes of entry. Care should be taken as to how this is derived
    /// * `hd_index` - [ZIP32](https://zips.z.cash/zip-0032) hierarchical deterministic index of the account
    ///
    #[wasm_bindgen(constructor)]
    pub fn new(network: &str, mut seed: Box<[u8]>, hd_index: u32) -> Result<UnifiedSpendingKey, Error> {
        let network = Network::from_str(network)?;
        let account_id = AccountId::try_from(hd_index)?;
        #[cfg(feature = "voting")]
        let voting_derivation = (network, account_id, zip32::fingerprint::SeedFingerprint::from_seed(&seed));
        let result = zcash_keys::keys::UnifiedSpendingKey::from_seed(&network, &seed, account_id)
            .map(|inner| Self {
                inner,
                #[cfg(feature = "voting")]
                voting_derivation,
            })
            .map_err(Error::from);
        seed.zeroize();
        result
    }

    /// Obtain the UFVK corresponding to this spending key
    pub fn to_unified_full_viewing_key(&self) -> UnifiedFullViewingKey {
        UnifiedFullViewingKey {
            inner: self.inner.to_unified_full_viewing_key(),
        }
    }

    pub fn to_sapling_proof_generation_key(&self) -> ProofGenerationKey {
        ProofGenerationKey {
            inner: self.inner.sapling().expsk().proof_generation_key(),
        }
    }

    // /// Encode the Sapling Extended Spending Key to Bech32 format
    // ///
    // /// # Arguments
    // /// * `network` - Must be either "main" or "test"
    // ///
    // /// # Returns
    // /// String in format "secret-extended-key-main..." or "secret-extended-key-test..."
    // pub fn encode_sapling_extended_spending_key(&self, network: &str) -> Result<String, Error> {
    //     let network = Network::from_str(network)?;
    //     let hrp = match network {
    //         Network::MainNetwork => "secret-extended-key-main",
    //         Network::TestNetwork => "secret-extended-key-test",
    //     };
    //     Ok(zcash_keys::encoding::encode_extended_spending_key(
    //         hrp,
    //         self.inner.sapling(),
    //     ))
    // }

    // /// Encode the Sapling Extended Full Viewing Key to Bech32 format
    // ///
    // /// # Arguments
    // /// * `network` - Must be either "main" or "test"
    // ///
    // /// # Returns
    // /// String in format "zxviews..." or "zxviewtestsapling..."
    // #[allow(deprecated)]
    // pub fn encode_sapling_extended_full_viewing_key(&self, network: &str) -> Result<String, Error> {
    //     let network = Network::from_str(network)?;

    //     // Get ExtendedFullViewingKey from ExtendedSpendingKey
    //     let extfvk = self.inner.sapling().to_extended_full_viewing_key();

    //     let hrp = match network {
    //         Network::MainNetwork => "zxviews",
    //         Network::TestNetwork => "zxviewtestsapling",
    //     };

    //     Ok(zcash_keys::encoding::encode_extended_full_viewing_key(hrp, &extfvk))
    // }

    // /// Encode the transparent secret key to WIF (Wallet Import Format)
    // ///
    // /// # Arguments
    // /// * `network` - Must be either "main" or "test"
    // ///
    // /// # Returns
    // /// String in WIF format
    // pub fn encode_transparent_secret_key(&self, network: &str) -> Result<String, Error> {
    //     use transparent::keys::NonHardenedChildIndex;

    //     let network = Network::from_str(network)?;
    //     let account_priv_key = self.inner.transparent();

    //     // Get the extended private key at the external address path (m/44'/coin_type'/account'/0/0)
    //     let address_index = NonHardenedChildIndex::from_index(0)
    //         .ok_or(Error::KeyDerivation)?;
    //     let external_priv_key = account_priv_key.derive_external_secret_key(address_index)
    //         .map_err(|_| Error::KeyDerivation)?;

    //     // Encode to WIF format
    //     let secret_key_bytes = external_priv_key.secret_bytes();
    //     let prefix = match network {
    //         Network::MainNetwork => [0x80u8],
    //         Network::TestNetwork => [0xEF],
    //     };

    //     let mut data = Vec::new();
    //     data.extend_from_slice(&prefix);
    //     data.extend_from_slice(&secret_key_bytes);
    //     data.push(0x01); // Compressed public key flag

    //     // Double SHA256 for checksum
    //     use sha2::{Sha256, Digest};
    //     let hash1 = Sha256::digest(&data);
    //     let hash2 = Sha256::digest(&hash1);
    //     data.extend_from_slice(&hash2[..4]);

    //     Ok(bs58::encode(data).into_string())
    // }
}

#[cfg(feature = "voting")]
impl UnifiedSpendingKey {
    pub(crate) fn into_voting_parts(
        self,
    ) -> (
        zcash_keys::keys::UnifiedSpendingKey,
        Network,
        AccountId,
        Option<zip32::fingerprint::SeedFingerprint>,
    ) {
        let (network, index, fingerprint) = self.voting_derivation;
        (self.inner, network, index, fingerprint)
    }
}

#[wasm_bindgen]
impl UnifiedFullViewingKey {
    pub fn encode(&self, network: &str) -> Result<String, Error> {
        let network = Network::from_str(network)?;
        self.inner.encode(&network).map_err(|error| Error::KeyDecoding(error.to_string()))
    }

    #[wasm_bindgen(constructor)]
    pub fn new(network: &str, encoding: &str) -> Result<UnifiedFullViewingKey, Error> {
        let network = Network::from_str(network)?;
        Ok(Self {
            inner: zcash_keys::keys::UnifiedFullViewingKey::decode(&network, encoding).map_err(|error| Error::KeyDecoding(error.to_string()))?,
        })
    }
}
