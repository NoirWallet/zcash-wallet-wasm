// Portions derived from ChainSafe WebZjs.
// Copyright 2024 ChainSafe Systems
// Modified by NoirWallet; see THIRD_PARTY_NOTICES.md for licensing.

use std::str::FromStr;
use wasm_bindgen::prelude::*;

mod fund_core;

use crate::Network;
use crate::error::Error;
use secrecy::Zeroize;
use zip32::AccountId;

#[wasm_bindgen]
pub struct SeedFingerprint {
    inner: zip32::fingerprint::SeedFingerprint,
}
#[wasm_bindgen]
impl SeedFingerprint {
    #[wasm_bindgen(constructor)]
    pub fn new(mut seed: Box<[u8]>) -> Result<SeedFingerprint, Error> {
        let result = zip32::fingerprint::SeedFingerprint::from_seed(&seed)
            .ok_or(Error::SeedFingerprint)
            .map(|inner| Self { inner });
        seed.zeroize();
        result
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<SeedFingerprint, Error> {
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| Error::SeedFingerprint)?;
        Ok(Self {
            inner: zip32::fingerprint::SeedFingerprint::from_bytes(bytes),
        })
    }
}

impl From<SeedFingerprint> for zip32::fingerprint::SeedFingerprint {
    fn from(value: SeedFingerprint) -> Self {
        value.inner
    }
}

impl From<zip32::fingerprint::SeedFingerprint> for SeedFingerprint {
    fn from(value: zip32::fingerprint::SeedFingerprint) -> Self {
        Self { inner: value }
    }
}

#[wasm_bindgen]
pub struct ProofGenerationKey {
    inner: sapling::ProofGenerationKey,
}

impl From<ProofGenerationKey> for sapling::ProofGenerationKey {
    fn from(value: ProofGenerationKey) -> sapling::ProofGenerationKey {
        value.inner
    }
}

impl From<sapling::ProofGenerationKey> for ProofGenerationKey {
    fn from(value: sapling::ProofGenerationKey) -> Self {
        Self { inner: value }
    }
}

#[wasm_bindgen]
pub struct UnifiedSpendingKey {
    inner: zcash_keys::keys::UnifiedSpendingKey,
    #[cfg(feature = "voting")]
    voting_derivation: (Network, AccountId, Option<zip32::fingerprint::SeedFingerprint>),
}

impl From<UnifiedSpendingKey> for zcash_keys::keys::UnifiedSpendingKey {
    fn from(value: UnifiedSpendingKey) -> Self {
        value.inner
    }
}

#[cfg(feature = "voting")]
#[cfg(all(test, feature = "voting"))]
mod voting_tests {
    #[test]
    fn delegation_key_wrapper_retains_constructor_derivation_without_seed() {
        let key = super::UnifiedSpendingKey::new("test", vec![17; 32].into_boxed_slice(), 1).unwrap();
        let (_, network, index, fingerprint) = key.into_voting_parts();
        assert!(matches!(network, crate::Network::TestNetwork));
        assert_eq!(u32::from(index), 1);
        assert_eq!(fingerprint, zip32::fingerprint::SeedFingerprint::from_seed(&[17; 32]));
    }
}

/// A Zcash viewing key
///
/// This is a wrapper around the `zcash_keys::keys::ViewingKey` type.
/// UFVKs should be generated from a spending key by calling `to_unified_full_viewing_key`
/// They can also be encoded and decoded to a canonical string representation
#[wasm_bindgen]
pub struct UnifiedFullViewingKey {
    inner: zcash_keys::keys::UnifiedFullViewingKey,
}

impl UnifiedFullViewingKey {
    /// Get a reference to the inner UnifiedFullViewingKey
    pub(crate) fn as_inner(&self) -> &zcash_keys::keys::UnifiedFullViewingKey {
        &self.inner
    }
}
