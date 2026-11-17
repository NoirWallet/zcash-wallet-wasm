//! Synthetic transactions for the JS test harness (feature `test-fixtures` only).

#[cfg(feature = "test-fixtures")]
use super::*;

/// Synthetic transactions for the JS test harness. **Not part of the shipping API.**
///
/// Available only when the `test-fixtures` feature is on, which the `test-vault-wasm` target enables.
/// It exists so the frontend's own harness can run the success path — build, sign, apply, verify —
/// without a synced wallet.
#[cfg(feature = "test-fixtures")]
#[wasm_bindgen]
pub struct Fixtures;

#[cfg(feature = "test-fixtures")]
#[wasm_bindgen]
impl Fixtures {
    /// An unproven, unsigned Ironwood PCZT spending from the vault's own address.
    ///
    /// `fvk` is the 96-byte viewing key from [`Keyset::derive`]; `spend` and `pay` are zatoshi.
    #[wasm_bindgen]
    pub fn ironwood_pczt(fvk: &[u8], spend: f64, pay: f64) -> Result<js_sys::Uint8Array, JsError> {
        let fvk: [u8; 96] = fvk.try_into().map_err(|_| JsError::new("fvk must be 96 bytes"))?;
        let pczt = vault_fixtures::ironwood_pczt(&fvk, orchard::ValuePool::Ironwood, spend as u64, pay as u64, rand::rng())
            .ok_or_else(|| JsError::new("could not build the fixture transaction"))?;
        Ok(to_js_bytes(&pczt.serialize().map_err(|e| JsError::new(&format!("{e:?}")))?))
    }
}
