//! The signing session: coordinator and signer entry points.

use super::*;

/// Coordinator side of a signing session.
#[wasm_bindgen]
pub struct Coordinator {
    inner: CoordinatorSession,
}

#[wasm_bindgen]
impl Coordinator {
    /// `signers` is the selected roster slots (length must equal the threshold).
    #[wasm_bindgen(constructor)]
    pub fn new(
        session_id: &[u8],
        participants: u16,
        threshold: u16,
        signers: &[u8],
        actions: usize,
        public_key_package: &[u8],
    ) -> Result<Coordinator, JsError> {
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let mut identifiers = BTreeMap::new();
        for slot in 0..participants {
            identifiers.insert(slot as u8, threshold::slot_identifier(slot as u8).map_err(err)?);
        }
        let config = SessionConfig {
            session_id,
            participants,
            threshold,
            signers: signers.to_vec(),
            identifiers,
        };
        let pubkeys = PublicKeyPackage::deserialize(public_key_package).map_err(err)?;
        let inner = CoordinatorSession::new(config, pubkeys, actions).map_err(err)?;
        Ok(Self { inner })
    }

    /// The whole `SIGN_INIT` batch: `[[slot, envelope], ...]`. One call, because the session moves to
    /// `InitSent` as a whole.
    pub fn init_all(&mut self) -> Result<js_sys::Array, JsError> {
        let envelopes = self.inner.init().map_err(err)?;
        Ok(slot_and_bytes(envelopes))
    }

    /// Collect a `SIGN_COMMITMENTS` envelope from a signer.
    pub fn accept_commitments(&mut self, envelope: &[u8]) -> Result<(), JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        self.inner.on_commitments(&env).map_err(err)
    }

    /// Hand over the sighash and per-action alphas read from the built transaction.
    pub fn prepare_payload(&mut self, sighash: &[u8], alphas: &[u8]) -> Result<(), JsError> {
        let sighash: [u8; 32] = sighash.try_into().map_err(|_| JsError::new("sighash must be 32 bytes"))?;
        if alphas.len() % 32 != 0 {
            return Err(JsError::new("alphas must be a multiple of 32 bytes"));
        }
        let alphas: Vec<[u8; 32]> = alphas.chunks_exact(32).map(|c| c.try_into().expect("chunk size checked")).collect();
        self.inner.prepare_payload(sighash, alphas).map_err(err)
    }

    /// The whole `SIGN_PAYLOAD` batch: `[[slot, envelope], ...]`.
    pub fn payloads(&mut self) -> Result<js_sys::Array, JsError> {
        let envelopes = self.inner.payloads().map_err(err)?;
        Ok(slot_and_bytes(envelopes))
    }

    /// Collect a `SIGN_SHARE` envelope.
    pub fn accept_share(&mut self, envelope: &[u8]) -> Result<(), JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        self.inner.on_share(&env).map_err(err)
    }

    /// Aggregate once every signer has returned every share. Returns the signatures, 64 bytes each.
    pub fn aggregate(&mut self) -> Result<js_sys::Uint8Array, JsError> {
        let signatures = self.inner.aggregate().map_err(err)?;
        let mut flat = Vec::with_capacity(signatures.len() * 64);
        for signature in &signatures {
            flat.extend_from_slice(signature);
        }
        Ok(to_js_bytes(&flat))
    }

    pub fn phase(&self) -> String {
        format!("{:?}", self.inner.phase())
    }

    pub fn actions(&self) -> usize {
        self.inner.actions()
    }

    /// Accept a signer's refusal. The session ends; one signer out means this selection no longer reaches
    /// the threshold.
    pub fn accept_reject(&mut self, envelope: &[u8]) -> Result<js_sys::Array, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let (sender, code, note) = self.inner.on_reject(&env).map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&JsValue::from(sender));
        out.push(&JsValue::from(code.to_byte()));
        out.push(&JsValue::from(note));
        Ok(out)
    }

    /// Report the outcome to every signer after aggregation.
    ///
    /// `status`: 0 = broadcasted (then `txid` is required), 1 = failed. Returns `[[slot, envelope], ...]`.
    pub fn report_result(&mut self, status: u8, txid: Option<Vec<u8>>, note: &str) -> Result<js_sys::Array, JsError> {
        let status = ResultStatus::from_byte(status).ok_or_else(|| JsError::new("status must be 0 (broadcasted) or 1 (failed)"))?;
        let txid = match txid {
            Some(bytes) => Some(<[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| JsError::new("txid must be 32 bytes"))?),
            None => None,
        };
        let envelopes = self.inner.result(status, txid, note).map_err(err)?;
        Ok(slot_and_bytes(envelopes))
    }
}

/// Signer side of a signing session. Holds the key share and the nonces.
#[wasm_bindgen]
pub struct Signer {
    inner: SignerSession,
    key_package: KeyPackage,
}

#[wasm_bindgen]
impl Signer {
    #[wasm_bindgen(constructor)]
    pub fn new(session_id: &[u8], my_slot: u8, participants: u16, key_package: &[u8]) -> Result<Signer, JsError> {
        let session_id: [u8; 16] = session_id.try_into().map_err(|_| JsError::new("session_id must be 16 bytes"))?;
        let mut identifiers = BTreeMap::new();
        for slot in 0..participants {
            identifiers.insert(slot as u8, threshold::slot_identifier(slot as u8).map_err(err)?);
        }
        let key_package = KeyPackage::deserialize(key_package).map_err(err)?;
        Ok(Self {
            inner: SignerSession::new(session_id, my_slot, identifiers),
            key_package,
        })
    }

    /// Consume `SIGN_INIT`, produce the `SIGN_COMMITMENTS` envelope to send back.
    pub fn init(&mut self, envelope: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let out = self.inner.on_init(&env, &self.key_package, &mut rng()).map_err(err)?;
        Ok(to_js_bytes(&out.encode()))
    }

    /// Consume `SIGN_PAYLOAD`. Returns `[sighash: Uint8Array, alphas: Uint8Array]` for the caller to
    /// verify against the PCZT **before** approving.
    pub fn payload(&mut self, envelope: &[u8]) -> Result<js_sys::Array, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let material = self.inner.on_payload(&env).map_err(err)?;
        let mut alphas = Vec::with_capacity(material.alphas.len() * 32);
        for alpha in &material.alphas {
            alphas.extend_from_slice(alpha);
        }
        let out = js_sys::Array::new();
        out.push(&to_js_bytes(&material.sighash));
        out.push(&to_js_bytes(&alphas));
        Ok(out)
    }

    /// Record that the user approved. Only after this can a share exist.
    pub fn approve(&mut self) -> Result<(), JsError> {
        self.inner.approve().map_err(err)
    }

    /// Produce the `SIGN_SHARE` envelope. Consumes the nonces.
    pub fn share(&mut self) -> Result<js_sys::Uint8Array, JsError> {
        let out = self.inner.share(&self.key_package).map_err(err)?;
        Ok(to_js_bytes(&out.encode()))
    }

    pub fn phase(&self) -> String {
        format!("{:?}", self.inner.phase())
    }

    /// Refuse to sign, and end the session. `code`: 1 = user declined, 2 = verification failed,
    /// 3 = malformed payload, 4 = expired.
    pub fn reject(&mut self, code: u8, note: &str) -> Result<js_sys::Uint8Array, JsError> {
        let code = RejectCode::from_byte(code).ok_or_else(|| JsError::new("code must be 1..=4"))?;
        let envelope = self.inner.reject(code, note).map_err(err)?;
        Ok(to_js_bytes(&envelope.encode()))
    }

    /// Accept the coordinator's outcome report: `[status, txid | null, note]`.
    pub fn on_result(&mut self, envelope: &[u8]) -> Result<js_sys::Array, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let (status, txid, note) = self.inner.on_result(&env).map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&JsValue::from(status.to_byte()));
        match txid {
            Some(bytes) => {
                out.push(&to_js_bytes(&bytes));
            }
            None => {
                out.push(&JsValue::NULL);
            }
        }
        out.push(&JsValue::from(note));
        Ok(out)
    }

    /// `"committed"` / `"consumed"` per action, so the host can persist the nonce state.
    pub fn nonce_states(&self) -> Vec<u8> {
        self.inner
            .nonce_states()
            .iter()
            .map(|s| match s {
                NonceState::Created => 0,
                NonceState::Committed => 1,
                NonceState::Consumed => 2,
                NonceState::Destroyed => 3,
            })
            .collect()
    }
}

/// Which signing phase a session is in, for logging.
#[wasm_bindgen]
pub fn signing_phases() -> js_sys::Array {
    let out = js_sys::Array::new();
    for phase in [
        SignPhase::Created,
        SignPhase::InitSent,
        SignPhase::CommitmentsCollected,
        SignPhase::PcztReady,
        SignPhase::Verifying,
        SignPhase::Approved,
        SignPhase::SharesCollected,
        SignPhase::Aggregated,
    ] {
        out.push(&JsValue::from(format!("{phase:?}")));
    }
    out
}
