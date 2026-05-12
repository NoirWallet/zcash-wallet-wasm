use crate::Error;
use crate::bindgen::{SeedFingerprint, UnifiedSpendingKey};
use bip0039::{Count, English, Mnemonic};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Unified addresses for all Zcash pools
#[derive(Serialize, Deserialize)]
pub struct UnifiedAddresses {
    /// The full unified address (contains all available receivers)
    pub unified: String,
    /// The Orchard receiver address (if available)
    pub orchard: Option<String>,
    /// The Sapling receiver unified address (u1 format, if available)
    pub sapling: Option<String>,
    /// The Sapling receiver traditional address (zs format, if available)
    pub sapling_zs: Option<String>,
    /// The Transparent receiver address (if available)
    pub transparent: Option<String>,
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
/// console.log(mnemonic); // "abandon abandon abandon ... art"
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

// /// Convert a mnemonic phrase to its entropy bytes.
// ///
// /// # Arguments
// ///
// /// * `phrase` - The mnemonic phrase
// ///
// /// # Returns
// ///
// /// A vector of bytes representing the entropy
// ///
// /// # Example
// ///
// /// ```javascript
// /// const entropy = mnemonic_to_entropy("abandon abandon abandon ... art");
// /// console.log(entropy); // Uint8Array
// /// ```
// #[wasm_bindgen]
// pub fn mnemonic_to_entropy(phrase: &str) -> Result<Vec<u8>, Error> {
//     let mnemonic: Mnemonic<English> =
//         Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

//     Ok(mnemonic.entropy().to_vec())
// }

// /// Convert entropy bytes to a mnemonic phrase.
// ///
// /// # Arguments
// ///
// /// * `entropy` - The entropy bytes (16, 20, 24, 28, or 32 bytes)
// ///
// /// # Returns
// ///
// /// A string containing the mnemonic phrase
// ///
// /// # Example
// ///
// /// ```javascript
// /// const entropy = new Uint8Array(32); // 32 bytes for 24 words
// /// crypto.getRandomValues(entropy);
// /// const mnemonic = entropy_to_mnemonic(entropy);
// /// console.log(mnemonic);
// /// ```
// #[wasm_bindgen]
// pub fn entropy_to_mnemonic(entropy: &[u8]) -> Result<String, Error> {
//     let mnemonic: Mnemonic<English> =
//         Mnemonic::from_entropy(entropy).map_err(|_| Error::InvalidSeedPhrase)?;

//     Ok(mnemonic.phrase().to_string())
// }

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
/// console.log(seed); // Uint8Array of 64 bytes
/// ```
#[wasm_bindgen]
pub fn mnemonic_to_seed(phrase: &str, passphrase: &str) -> Result<Vec<u8>, Error> {
    let mnemonic: Mnemonic<English> =
        Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

    let seed = mnemonic.to_seed(passphrase);
    Ok(seed.to_vec())
}

/// Generate a UnifiedSpendingKey from a mnemonic phrase.
///
/// # Arguments
///
/// * `network` - Network name ("main" or "test")
/// * `phrase` - The mnemonic phrase
/// * `passphrase` - Optional passphrase (use empty string "" if none)
/// * `account_index` - The account index for HD derivation (usually 0)
///
/// # Returns
///
/// A UnifiedSpendingKey that can be used for signing transactions
///
/// # Example
///
/// ```javascript
/// const mnemonic = generate_mnemonic(24);
/// const usk = mnemonic_to_spending_key(
///     "main",
///     mnemonic,
///     "",  // no passphrase
///     0    // first account
/// );
/// ```
#[wasm_bindgen]
pub fn mnemonic_to_spending_key(
    network: &str,
    phrase: &str,
    passphrase: &str,
    account_index: u32,
) -> Result<UnifiedSpendingKey, Error> {
    // Convert mnemonic to seed
    let mnemonic: Mnemonic<English> =
        Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

    let seed = mnemonic.to_seed(passphrase);

    // Create UnifiedSpendingKey from seed
    UnifiedSpendingKey::new(network, seed.to_vec().into_boxed_slice(), account_index)
}

/// Generate a SeedFingerprint from a mnemonic phrase.
///
/// # Arguments
///
/// * `phrase` - The mnemonic phrase
/// * `passphrase` - Optional passphrase (use empty string "" if none)
///
/// # Returns
///
/// A SeedFingerprint that can be used for key derivation path identification
///
/// # Example
///
/// ```javascript
/// const mnemonic = generate_mnemonic(24);
/// const seed_fp = mnemonic_to_seed_fingerprint(
///     mnemonic,
///     ""  // no passphrase
/// );
/// ```
#[wasm_bindgen]
pub fn mnemonic_to_seed_fingerprint(
    phrase: &str,
    passphrase: &str,
) -> Result<SeedFingerprint, Error> {
    // Convert mnemonic to seed
    let mnemonic: Mnemonic<English> =
        Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

    let seed = mnemonic.to_seed(passphrase);

    // Create SeedFingerprint from seed
    SeedFingerprint::new(seed.to_vec().into_boxed_slice())
}

// /// Generate a UnifiedFullViewingKey from a mnemonic phrase.
// ///
// /// # Arguments
// ///
// /// * `network` - Network name ("main" or "test")
// /// * `phrase` - The mnemonic phrase
// /// * `passphrase` - Optional passphrase (use empty string "" if none)
// /// * `account_index` - The account index for HD derivation (usually 0)
// ///
// /// # Returns
// ///
// /// A UnifiedFullViewingKey that can be used for viewing transactions
// ///
// /// # Example
// ///
// /// ```javascript
// /// const mnemonic = generate_mnemonic(24);
// /// const ufvk = mnemonic_to_viewing_key(
// ///     "main",
// ///     mnemonic,
// ///     "",  // no passphrase
// ///     0    // first account
// /// );
// /// ```
// #[wasm_bindgen]
// pub fn mnemonic_to_viewing_key(
//     network: &str,
//     phrase: &str,
//     passphrase: &str,
//     account_index: u32,
// ) -> Result<UnifiedFullViewingKey, Error> {
//     // Convert mnemonic to seed
//     let mnemonic: Mnemonic<English> =
//         Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

//     let seed = mnemonic.to_seed(passphrase);

//     // Create UnifiedSpendingKey from seed
//     let usk = UnifiedSpendingKey::new(network, seed.to_vec().into_boxed_slice(), account_index)?;

//     // Derive UnifiedFullViewingKey from spending key
//     Ok(usk.to_unified_full_viewing_key())
// }

/// Get all addresses (unified, orchard, sapling, transparent) from a mnemonic phrase.
///
/// # Arguments
///
/// * `network` - Network name ("main" or "test")
/// * `phrase` - The mnemonic phrase
/// * `passphrase` - Optional passphrase (use empty string "" if none)
/// * `account_index` - The account index for HD derivation (usually 0)
///
/// # Returns
///
/// A JSON object containing all address types
///
/// # Example
///
/// ```javascript
/// const mnemonic = generate_mnemonic(24);
/// const addresses = mnemonic_to_addresses(
///     "main",
///     mnemonic,
///     "",  // no passphrase
///     0    // first account
/// );
/// console.log("Unified:", addresses.unified);
/// console.log("Orchard:", addresses.orchard);
/// console.log("Sapling:", addresses.sapling);
/// console.log("Transparent:", addresses.transparent);
/// ```
#[wasm_bindgen]
pub fn mnemonic_to_addresses(
    network: &str,
    phrase: &str,
    passphrase: &str,
    account_index: u32,
) -> Result<JsValue, Error> {
    use std::str::FromStr;
    use zcash_client_backend::encoding::AddressCodec;
    use zcash_keys::keys::UnifiedAddressRequest;

    // Convert mnemonic to seed
    let mnemonic: Mnemonic<English> =
        Mnemonic::from_phrase(phrase).map_err(|_| Error::InvalidSeedPhrase)?;

    let seed = mnemonic.to_seed(passphrase);

    // Create UnifiedSpendingKey from seed
    let usk = UnifiedSpendingKey::new(network, seed.to_vec().into_boxed_slice(), account_index)?;

    // Get UnifiedFullViewingKey
    let ufvk = usk.to_unified_full_viewing_key();

    // Get the network
    let net = crate::Network::from_str(network)?;

    // Get UIVK (unified incoming viewing key) from UFVK
    let uivk = ufvk.as_inner().to_unified_incoming_viewing_key();

    // Generate the first address that includes every receiver supported by the UIVK.
    let (address, _) = uivk.default_address(UnifiedAddressRequest::AllAvailableKeys)?;

    // Encode the unified address
    let unified_str = address.encode(&net);

    // Extract individual receiver addresses
    // For Orchard and Sapling, we generate unified addresses with only that receiver type
    use zcash_keys::keys::{ReceiverRequirement as RR, UnifiedAddressRequest as UAR};

    let orchard_addr = if address.orchard().is_some() {
        // Request a unified address with only Orchard receiver
        // unsafe_custom allows creating requests with any combination
        let ua_request = UAR::unsafe_custom(
            RR::Require, // Orchard
            RR::Omit,    // Sapling
            RR::Omit,    // Transparent
        );
        uivk.default_address(ua_request)
            .ok()
            .map(|(addr, _)| addr.encode(&net))
    } else {
        None
    };

    let (sapling_addr, sapling_zs_addr) = if let Some(sapling_receiver) = address.sapling() {
        // Generate unified address with only Sapling receiver
        let ua_request = UAR::unsafe_custom(
            RR::Omit,    // Orchard
            RR::Require, // Sapling
            RR::Omit,    // Transparent
        );
        let sapling_ua = uivk
            .default_address(ua_request)
            .ok()
            .map(|(addr, _)| addr.encode(&net));

        // Also generate traditional zs format address (directly encode the PaymentAddress)
        let sapling_zs = Some(sapling_receiver.encode(&net));

        (sapling_ua, sapling_zs)
    } else {
        (None, None)
    };

    let transparent_addr = address.transparent().map(|addr| addr.encode(&net));

    // Create the result structure
    let result = UnifiedAddresses {
        unified: unified_str,
        orchard: orchard_addr,
        sapling: sapling_addr,
        sapling_zs: sapling_zs_addr,
        transparent: transparent_addr,
    };

    // Convert to JsValue using serde_wasm_bindgen
    Ok(serde_wasm_bindgen::to_value(&result)?)
}

/// Parse a unified address string and extract individual receiver addresses.
///
/// # Arguments
///
/// * `network` - Network name ("main" or "test")
/// * `unified_address` - The unified address string to parse
///
/// # Returns
///
/// A JSON object containing individual address types extracted from the unified address
///
/// # Example
///
/// ```javascript
/// const addresses = parse_unified_address(
///     "main",
///     "u1..." // unified address string
/// );
/// console.log("Orchard:", addresses.orchard);
/// console.log("Sapling:", addresses.sapling);
/// console.log("Transparent:", addresses.transparent);
/// ```
#[wasm_bindgen]
pub fn parse_unified_address(network: &str, unified_address: &str) -> Result<JsValue, Error> {
    use std::str::FromStr;
    use zcash_address::unified::{self, Container};
    use zcash_client_backend::encoding::AddressCodec;
    use zcash_primitives::consensus::Parameters;
    use zcash_primitives::legacy::TransparentAddress;

    // Get the network
    let net = crate::Network::from_str(network)?;

    // Parse the unified address directly
    use zcash_address::unified::Encoding;
    let (parsed_net, ua) =
        unified::Address::decode(unified_address).map_err(|_| Error::InvalidAddress)?;

    // Verify network matches
    if parsed_net != net.network_type() {
        return Err(Error::InvalidAddress);
    }

    // Extract individual receivers and create single-receiver addresses
    let mut orchard_addr: Option<String> = None;
    let mut sapling_addr: Option<String> = None;
    let mut sapling_zs_addr: Option<String> = None;
    let mut transparent_addr: Option<String> = None;

    // Iterate through receivers to extract each type
    for receiver in ua.items() {
        match receiver {
            unified::Receiver::Orchard(data) => {
                // Create a unified address with only Orchard receiver
                let single_ua =
                    unified::Address::try_from_items(vec![unified::Receiver::Orchard(data)])
                        .map_err(|_| Error::InvalidAddress)?;
                // Encode the single-receiver UA
                orchard_addr = Some(single_ua.encode(&parsed_net));
            }
            unified::Receiver::Sapling(data) => {
                // Create a unified address with only Sapling receiver
                let single_ua =
                    unified::Address::try_from_items(vec![unified::Receiver::Sapling(data)])
                        .map_err(|_| Error::InvalidAddress)?;
                // Encode the single-receiver UA
                sapling_addr = Some(single_ua.encode(&parsed_net));

                // Also generate traditional zs format address
                use sapling::PaymentAddress;
                if let Some(payment_addr) = PaymentAddress::from_bytes(&data) {
                    sapling_zs_addr = Some(payment_addr.encode(&net));
                }
            }
            unified::Receiver::P2pkh(data) => {
                // Create a standalone transparent P2PKH address
                let t_addr = TransparentAddress::PublicKeyHash(data);
                transparent_addr = Some(t_addr.encode(&net));
            }
            unified::Receiver::P2sh(_) => {
                unreachable!();
            }
            unified::Receiver::Unknown { .. } => {
                unreachable!();
            }
        }
    }

    // Create the result structure
    let result = UnifiedAddresses {
        unified: unified_address.to_string(),
        orchard: orchard_addr,
        sapling: sapling_addr,
        sapling_zs: sapling_zs_addr,
        transparent: transparent_addr,
    };

    // Convert to JsValue using serde_wasm_bindgen
    Ok(serde_wasm_bindgen::to_value(&result)?)
}

#[cfg(test)]
mod tests {
    use super::generate_mnemonic;
    use crate::Error;

    #[test]
    fn generate_mnemonic_only_allows_24_words() {
        for word_count in [12, 15, 18, 21] {
            assert!(matches!(
                generate_mnemonic(word_count),
                Err(Error::InvalidSeedPhrase)
            ));
        }

        let mnemonic = generate_mnemonic(24).expect("24-word mnemonic generation should succeed");
        assert_eq!(mnemonic.split_whitespace().count(), 24);
    }
}
