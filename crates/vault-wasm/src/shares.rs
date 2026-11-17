//! Custody: the key share this device holds and the marker that survives a crash.

use super::*;

/// A device's key share, bound to a group and slot.
#[wasm_bindgen]
pub struct ShareBackup {
    inner: custody::ShareBackup,
}

#[wasm_bindgen]
impl ShareBackup {
    /// Wrap a key package for storage. `device_id` and `vault_id` are opaque to the wasm and must be
    /// stable across restores.
    #[wasm_bindgen(constructor)]
    pub fn new(key_package: &[u8], slot: u8, device_id: &[u8], vault_id: &[u8], group_verifying_key: &[u8]) -> Result<ShareBackup, JsError> {
        let key_package = KeyPackage::deserialize(key_package).map_err(err)?;
        let group: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let actual: [u8; 32] = key_package
            .verifying_key()
            .serialize()
            .map_err(err)?
            .as_slice()
            .try_into()
            .map_err(|_| JsError::new("key package has no group key"))?;
        if actual != group {
            return Err(JsError::new("key package belongs to another group"));
        }
        let inner = custody::ShareBackup::new(
            &key_package,
            custody::ShareIdentity {
                slot,
                device_id: device_id.to_vec(),
                vault_id: vault_id.to_vec(),
            },
        )
        .map_err(err)?;
        Ok(Self { inner })
    }

    /// Bytes for the host to seal and store. The wasm does not encrypt.
    pub fn to_bytes(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&self.inner.to_bytes().map_err(|e| JsError::new(&format!("{e:?}")))?))
    }

    /// Rebuild a share from a 32-byte signing scalar plus the group's public data.
    ///
    /// This is the 24-word path: a BIP39 mnemonic carries 256 bits, which fits the scalar and nothing
    /// else. The identifier and verifying share are derived from `slot` and the scalar, and `ak` /
    /// `min_signers` come from the group's public state, which is not secret.
    ///
    /// **Pass `publicKeyPackage` whenever you have it.** With it, the reconstructed share is checked
    /// against the group's record for this slot, so a wrong scalar fails here instead of producing
    /// signature shares that never aggregate.
    #[wasm_bindgen]
    pub fn from_scalar(
        signing_scalar: &[u8],
        group_verifying_key: &[u8],
        slot: u8,
        min_signers: u16,
        device_id: &[u8],
        vault_id: &[u8],
        public_key_package: Option<Vec<u8>>,
    ) -> Result<ShareBackup, JsError> {
        let scalar: [u8; 32] = signing_scalar.try_into().map_err(|_| JsError::new("signing_scalar must be 32 bytes"))?;
        let group: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let pubkeys = match public_key_package {
            Some(bytes) => Some(PublicKeyPackage::deserialize(&bytes).map_err(err)?),
            None => None,
        };
        let inner = custody::ShareBackup::from_scalar(
            &scalar,
            &group,
            slot,
            min_signers,
            custody::ShareIdentity {
                slot,
                device_id: device_id.to_vec(),
                vault_id: vault_id.to_vec(),
            },
            pubkeys.as_ref(),
        )
        .map_err(err)?;
        Ok(Self { inner })
    }

    /// The 32-byte signing scalar, for a 24-word backup.
    ///
    /// This is all a BIP39 mnemonic can carry (256 bits); `from_scalar` rebuilds the rest from public
    /// data. **Drop the reference as soon as the words are encoded** — do not keep it, return it, or log
    /// it.
    pub fn signing_scalar(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&self.inner.signing_scalar().map_err(err)?))
    }

    /// Load and validate against the expected group and slot.
    pub fn from_bytes(bytes: &[u8], group_verifying_key: &[u8], expected_slot: u8) -> Result<ShareBackup, JsError> {
        let group: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let inner = custody::ShareBackup::from_bytes(bytes, &group, expected_slot).map_err(err)?;
        Ok(Self { inner })
    }

    pub fn slot(&self) -> u8 {
        self.inner.identity().slot
    }

    pub fn device_id(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.identity().device_id)
    }

    pub fn vault_id(&self) -> js_sys::Uint8Array {
        to_js_bytes(&self.inner.identity().vault_id)
    }

    pub fn group_verifying_key(&self) -> js_sys::Uint8Array {
        to_js_bytes(self.inner.group_verifying_key())
    }

    /// The key package, for handing to a [`SignerSession`]. Kept out of `to_bytes` on purpose: the
    /// backup blob is what the host stores, not what it needs in memory.
    pub fn key_package(&self) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(
            &self
                .inner
                .key_package()
                .map_err(err)?
                .serialize()
                .map_err(|e| JsError::new(&format!("{e:?}")))?,
        ))
    }
}

/// Session marker helpers: what the host persists to know a session was in flight.
#[wasm_bindgen]
pub struct SessionMarker;

#[wasm_bindgen]
impl SessionMarker {
    /// Build the marker to persist right after a session starts.
    #[wasm_bindgen]
    pub fn created(session_id: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        Ok(to_js_bytes(&custody::marker_bytes(id, custody::SessionProgress::Created)))
    }

    /// Advance a marker once commitments have left the device.
    #[wasm_bindgen]
    pub fn committed(session_id: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        Ok(to_js_bytes(&custody::marker_bytes(id, custody::SessionProgress::Committed)))
    }

    /// What to do about a persisted marker: `"clean"`, `"discardable"` or `"burned"`.
    #[wasm_bindgen]
    pub fn recover(marker: Option<Vec<u8>>) -> Result<String, JsError> {
        let recovery = custody::recover_from_marker(marker.as_deref()).map_err(err)?;
        Ok(match recovery {
            custody::Recovery::Clean => "clean".to_string(),
            custody::Recovery::Discardable { .. } => "discardable".to_string(),
            custody::Recovery::Burned { .. } => "burned".to_string(),
        })
    }

    /// The session a marker refers to, as 16 bytes, or `None` when clean.
    #[wasm_bindgen]
    pub fn session_id(marker: Option<Vec<u8>>) -> Result<Option<js_sys::Uint8Array>, JsError> {
        let recovery = custody::recover_from_marker(marker.as_deref()).map_err(err)?;
        Ok(recovery.session_id().map(|id| to_js_bytes(&id)))
    }
}
