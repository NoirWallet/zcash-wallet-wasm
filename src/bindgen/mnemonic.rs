pub use super::mnemonic_core::*;

use crate::Error;
use bip0039::{Count, English, Mnemonic};
use secrecy::Zeroize;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Addresses for all Zcash pools.
#[derive(Serialize, Deserialize)]
pub struct UnifiedAddresses {
    /// The shielded-only unified address (contains Orchard/Sapling receivers only)
    pub unified: String,
    /// The full unified address (contains all available receivers)
    pub full_unified: String,
    /// The Orchard receiver address (if available)
    pub orchard: Option<String>,
    /// The Sapling receiver unified address (u1 format, if available)
    pub sapling: Option<String>,
    /// The Sapling receiver traditional address (zs format, if available)
    pub sapling_zs: Option<String>,
    /// The Transparent receiver address (if available)
    pub transparent: Option<String>,
    /// Transparent child address index corresponding to the full unified address.
    pub address_index: Option<u32>,
}

/// Generate a new 24-word BIP-39 mnemonic phrase.
///
/// # Arguments
///
/// * `word_count` - Number of words in the mnemonic. Must be 24 for new wallet generation.
///
/// # Returns
///
/// A string containing the mnemonic phrase
///
/// # Example
///
/// ```javascript
/// const mnemonic = generate_mnemonic(24);
/// // Show the phrase only in the wallet's protected backup flow; never log it.
/// ```
#[wasm_bindgen]
pub fn generate_mnemonic(word_count: u32) -> Result<String, Error> {
    let count = match word_count {
        24 => Count::Words24,
        _ => return Err(Error::InvalidSeedPhrase),
    };

    let mnemonic: Mnemonic<English> = Mnemonic::generate(count);
    Ok(mnemonic.phrase().to_string())
}

/// Validate a BIP-39 mnemonic phrase.
///
/// # Arguments
///
/// * `phrase` - The mnemonic phrase to validate
///
/// # Returns
///
/// Returns Ok(()) if valid, or an error if invalid
///
/// # Example
///
/// ```javascript
/// try {
///     validate_mnemonic("abandon abandon abandon ... art");
///     console.log("Valid mnemonic");
/// } catch (e) {
///     console.error("Invalid mnemonic:", e);
/// }
/// ```
#[wasm_bindgen]
pub fn validate_mnemonic(phrase: &str) -> Result<(), Error> {
    Mnemonic::<English>::validate(phrase).map_err(|_| Error::InvalidSeedPhrase)
}

/// Convert a mnemonic phrase to a seed with an optional passphrase.
///
/// # Arguments
///
/// * `phrase` - The mnemonic phrase
/// * `passphrase` - Optional passphrase (use empty string for no passphrase)
///
/// # Returns
///
/// A 64-byte seed that can be used for key derivation
///
/// # Example
///
/// ```javascript
/// const seed = mnemonic_to_seed("abandon abandon abandon ... art", "");
/// // Use the seed for local key derivation; never log or persist it as plaintext.
/// ```
#[wasm_bindgen]
pub fn mnemonic_to_seed(phrase: &str, passphrase: &str) -> Result<Vec<u8>, Error> {
    let mnemonic: Mnemonic<English> = Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

    let mut seed = mnemonic.to_seed(passphrase);
    let result = seed.to_vec();
    seed.zeroize();
    Ok(result)
}

/// Transparent address data for a specific external address index.
#[derive(Serialize, Deserialize)]
pub struct TransparentAddressInfo {
    /// The transparent P2PKH address for the given index.
    pub address: String,
    /// Hex-encoded uncompressed public key (65 bytes: 04 + x + y).
    pub pubkey: String,
}

#[cfg(test)]
mod tests {
    use super::generate_mnemonic;
    use crate::Error;

    #[test]
    fn generate_mnemonic_only_allows_24_words() {
        for word_count in [12, 15, 18, 21] {
            assert!(matches!(generate_mnemonic(word_count), Err(Error::InvalidSeedPhrase)));
        }

        let mnemonic = generate_mnemonic(24).expect("24-word mnemonic generation should succeed");
        assert_eq!(mnemonic.split_whitespace().count(), 24);
    }
}
