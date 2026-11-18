//! Keyset setup, PCZT tooling, verification, deposit and canary helpers.

use super::*;
use sha2::Digest;

/// Keyset setup: `ak` plus the recovery key to a receiving address.
#[wasm_bindgen]
pub struct Keyset;

#[wasm_bindgen]
impl Keyset {
    /// Returns `[fvk(96), ufvk, address, rawAddress(43), fingerprint(32)]`.
    #[wasm_bindgen]
    pub fn derive(group_verifying_key: &[u8], qsk: &[u8], mainnet: bool, diversifier_index: u32) -> Result<js_sys::Array, JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let qsk: [u8; 32] = qsk.try_into().map_err(|_| JsError::new("qsk must be 32 bytes"))?;
        let network = if mainnet {
            zcash_protocol::consensus::Network::MainNetwork
        } else {
            zcash_protocol::consensus::Network::TestNetwork
        };
        let keyset = vault_keyset::VaultKeyset::derive(&ak, &qsk, network, diversifier_index).map_err(err)?;
        let out = js_sys::Array::new();
        out.push(&to_js_bytes(&keyset.fvk));
        out.push(&JsValue::from(keyset.ufvk));
        out.push(&JsValue::from(keyset.address));
        out.push(&to_js_bytes(&keyset.raw_address));
        out.push(&to_js_bytes(&keyset.fingerprint));
        Ok(out)
    }

    /// A fresh `qsk`: the `nk`/`rivk` source, and the ZIP-2005 recovery candidate.
    ///
    /// Generate it **once**, on one device, then deliver it to every other device over a
    /// confidential, authenticated channel — the same one the DKG round-2 packages need. Every
    /// device must pass the identical `qsk` to [`Self::derive`] or they will land on different
    /// addresses for the same group.
    ///
    /// It cannot spend: spend authorization is `ak`, which lives only in the FROST shares. Holding
    /// `qsk` does not weaken the threshold, which is why backing it up is cheap insurance.
    #[wasm_bindgen]
    pub fn generate_qsk() -> js_sys::Uint8Array {
        let mut bytes = [0u8; 32];
        rand_core::RngCore::fill_bytes(&mut rng(), &mut bytes);
        // A spending key must be a canonical scalar; retry rather than shipping a rejected one.
        while orchard::keys::SpendingKey::from_bytes(bytes).into_option().is_none() {
            rand_core::RngCore::fill_bytes(&mut rng(), &mut bytes);
        }
        to_js_bytes(&bytes)
    }

    /// The 96-byte FVK inside a UFVK string.
    ///
    /// A recovering device receives a viewing key but not the `qsk` behind it, so it cannot re-derive
    /// anything with [`Self::derive`]. This reads the key back out.
    #[wasm_bindgen]
    pub fn fvk_from_ufvk(ufvk: &str) -> Result<js_sys::Uint8Array, JsError> {
        Ok(to_js_bytes(&vault_keyset::fvk_from_ufvk(ufvk).map_err(err)?))
    }

    /// The keyset fingerprint, recomputed from an FVK: `SHA-256(domain ‖ network_tag ‖ fvk)`.
    ///
    /// Recovery needs this: a device keeps the FVK but not the `qsk`, so [`Self::derive`] is not
    /// available to it, and the fingerprint is what the user compares against an independent device to
    /// detect a coordinator handing over a viewing key with a real `ak` and attacker-chosen
    /// `nk`/`rivk`.
    ///
    /// Do not reimplement this hash elsewhere: the domain tag and network tag are part of the preimage,
    /// and a one-byte difference makes two implementations disagree forever.
    #[wasm_bindgen]
    pub fn fingerprint_from_fvk(fvk: &[u8], mainnet: bool) -> Result<js_sys::Uint8Array, JsError> {
        let fvk: [u8; 96] = fvk.try_into().map_err(|_| JsError::new("fvk must be 96 bytes"))?;
        let network = if mainnet {
            zcash_protocol::consensus::Network::MainNetwork
        } else {
            zcash_protocol::consensus::Network::TestNetwork
        };
        Ok(to_js_bytes(&vault_keyset::fingerprint_from_fvk(&fvk, network)))
    }

    /// `fingerprint_from_fvk` for callers holding the encoded UFVK.
    #[wasm_bindgen]
    pub fn fingerprint_from_ufvk(ufvk: &str, mainnet: bool) -> Result<js_sys::Uint8Array, JsError> {
        let network = if mainnet {
            zcash_protocol::consensus::Network::MainNetwork
        } else {
            zcash_protocol::consensus::Network::TestNetwork
        };
        Ok(to_js_bytes(&vault_keyset::fingerprint_from_ufvk(ufvk, network).map_err(err)?))
    }

    /// Whether a UFVK was derived from this group key.
    ///
    /// Run this before trusting a viewing key handed to you by a coordinator or a peer: `ak` is the
    /// first 32 bytes of the FVK, and a mismatch would restore the device into a different, empty
    /// wallet.
    #[wasm_bindgen]
    pub fn ufvk_matches_ak(ufvk: &str, group_verifying_key: &[u8]) -> Result<bool, JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        vault_keyset::ufvk_matches_ak(ufvk, &ak).map_err(err)
    }

    /// Whether the derived address carries only an Orchard receiver. Must always be true; the
    /// frontend should refuse to show an address where it is not.
    #[wasm_bindgen]
    pub fn address_is_shielded_only(address: &str) -> bool {
        use zcash_address::unified::{Address as UnifiedAddress, Encoding};
        match UnifiedAddress::decode(address) {
            Ok((_, _, ua)) => {
                ua.has_receiver_of_type(zcash_protocol::PoolType::ORCHARD)
                    && !ua.has_receiver_of_type(zcash_protocol::PoolType::TRANSPARENT)
                    && !ua.has_receiver_of_type(zcash_protocol::PoolType::SAPLING)
            }
            Err(_) => false,
        }
    }
}

/// PCZT handling the frontend needs around a signing session: shrink the request, read what to sign,
/// put the signatures back.
///
/// Deliberately no proving: building and proving belongs to the wallet's own transaction path, and
/// linking Halo2 here would double the module size for something already present.
#[wasm_bindgen]
pub struct PcztTools;

#[wasm_bindgen]
impl PcztTools {
    /// Fail unless this transaction is one the vault is allowed to sign (V6, Ironwood only, no
    /// transparent or Sapling part).
    #[wasm_bindgen]
    pub fn check(bytes: &[u8]) -> Result<(), JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let shape = pczt_signer_core::gate::PcztShape::of(&pczt);
        pczt_signer_core::gate::check(&shape).map_err(err)
    }

    /// Drop the zk proof: the transport copy. The coordinator keeps its own proven copy for applying
    /// signatures.
    #[wasm_bindgen]
    pub fn strip_proofs(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let stripped = pczt_signer_core::material::strip_proofs(pczt);
        Ok(to_js_bytes(&stripped.serialize().map_err(|e| JsError::new(&format!("{e:?}")))?))
    }

    /// The shielded sighash, recomputed from the PCZT. Signers must use this value, never one handed
    /// to them by the coordinator.
    #[wasm_bindgen]
    pub fn shielded_sighash(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let sighash = pczt_signer_core::sighash::shielded_sighash(pczt).map_err(err)?;
        Ok(to_js_bytes(&sighash))
    }

    /// Every Ironwood action's `rk`, 32 bytes each, in action order.
    #[wasm_bindgen]
    pub fn action_rks(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let rks = pczt_signer_core::material::ironwood_action_rks(&pczt).map_err(err)?;
        let mut flat = Vec::with_capacity(rks.len() * 32);
        for rk in &rks {
            flat.extend_from_slice(&rk.rk);
        }
        Ok(to_js_bytes(&flat))
    }

    /// Every Ironwood action's `alpha`, 32 bytes each, in action order. This is what the coordinator
    /// hands to the signers, and what `Verification::rk` needs.
    #[wasm_bindgen]
    pub fn action_alphas(bytes: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let alphas = pczt_signer_core::material::extract_alphas(&pczt).map_err(err)?;
        let mut flat = Vec::with_capacity(alphas.len() * 32);
        for alpha in &alphas {
            flat.extend_from_slice(alpha);
        }
        Ok(to_js_bytes(&flat))
    }

    /// Whether every action's `rk` equals `ak + alpha·G`. Run this on the signer before approving.
    #[wasm_bindgen]
    pub fn check_action_bindings(bytes: &[u8], group_verifying_key: &[u8], alphas: &[u8]) -> Result<(), JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        if alphas.len() % 32 != 0 {
            return Err(JsError::new("alphas must be a multiple of 32 bytes"));
        }
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let rks = pczt_signer_core::material::ironwood_action_rks(&pczt).map_err(err)?;
        let alphas: Vec<[u8; 32]> = alphas.chunks_exact(32).map(|c| c.try_into().expect("chunk size checked")).collect();
        let material = pczt_signer_core::material::bind_alphas(&rks, &alphas).map_err(err)?;
        for m in &material {
            m.verify_rk(&ak).map_err(err)?;
        }
        Ok(())
    }

    /// The signer's own reading of the transaction: every output (recipient, amount, change, memo) and
    /// every spend, with the totals and the fee.
    ///
    /// Returns `[reviewedPcztSha256, outputs, spends, sentTotal, changeTotal, fee]` where:
    ///
    /// * `reviewedPcztSha256` — SHA-256 of **the bytes passed in**, so a caller can persist "this is the
    ///   transaction I reviewed" and compare it later. Compute it on the PCZT the user was shown, not on a
    ///   later, proved copy: proving changes the bytes.
    /// * `outputs` — array of `[recipient(43), value, isChange, memoOrNull]`. `memoOrNull` is either the
    ///   exact 512-byte memo (ZIP-302 marker and padding included) or `null` for "could not verify"; the
    ///   two must never be conflated, so a caller that needs a memo must reject `null`.
    /// * `spends` — array of `[nullifier(32), value]`, one per spend that belongs to this viewing key.
    /// * `sentTotal` / `changeTotal` / `fee` — in zats. `sentTotal + changeTotal + fee` is the spent value.
    ///
    /// Fails wholesale if any check fails: there is no "partially reviewed" result to approve.
    #[wasm_bindgen]
    pub fn review(bytes: &[u8], fvk: &[u8]) -> Result<js_sys::Array, JsError> {
        let fvk: [u8; 96] = fvk.try_into().map_err(|_| JsError::new("fvk must be 96 bytes"))?;
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let reviewed = pczt_signer_core::review::review(&pczt, &fvk).map_err(err)?;

        let mut digest = sha2::Sha256::new();
        digest.update(bytes);
        let reviewed_pczt_sha256 = digest.finalize();

        let outputs = js_sys::Array::new();
        for output in &reviewed.outputs {
            let row = js_sys::Array::new();
            row.push(&to_js_bytes(&output.recipient));
            row.push(&JsValue::from_f64(output.value as f64));
            row.push(&JsValue::from_bool(output.is_change));
            row.push(&match &output.memo {
                pczt_signer_core::review::MemoReview::Read(memo) => to_js_bytes(memo).into(),
                pczt_signer_core::review::MemoReview::NotReadable => JsValue::NULL,
            });
            outputs.push(&row);
        }

        let spends = js_sys::Array::new();
        for spend in &reviewed.spends {
            let row = js_sys::Array::new();
            row.push(&to_js_bytes(&spend.nullifier));
            row.push(&JsValue::from_f64(spend.value as f64));
            spends.push(&row);
        }

        let out = js_sys::Array::new();
        out.push(&to_js_bytes(&reviewed_pczt_sha256));
        out.push(&outputs);
        out.push(&spends);
        out.push(&JsValue::from_f64(reviewed.sent_total as f64));
        out.push(&JsValue::from_f64(reviewed.change_total as f64));
        out.push(&JsValue::from_f64(reviewed.fee as f64));
        Ok(out)
    }

    /// Write signatures back. `signatures` is 64 bytes per action, positionally matching `indices`.
    #[wasm_bindgen]
    pub fn apply_signatures(bytes: &[u8], indices: &[u8], signatures: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        if signatures.len() != indices.len() * 64 {
            return Err(JsError::new("expected 64 bytes of signature per index"));
        }
        let pczt = pczt::Pczt::parse(bytes).map_err(err)?;
        let pairs: Vec<(u32, [u8; 64])> = indices
            .iter()
            .enumerate()
            .map(|(i, index)| {
                let sig: [u8; 64] = signatures[i * 64..(i + 1) * 64].try_into().expect("slice size checked");
                (*index as u32, sig)
            })
            .collect();
        let signed = pczt_signer_core::apply::apply_all(pczt, &pairs).map_err(err)?;
        Ok(to_js_bytes(&signed.serialize().map_err(|e| JsError::new(&format!("{e:?}")))?))
    }
}

/// Verification helpers the frontend needs before it broadcasts anything.
#[wasm_bindgen]
pub struct Verification;

#[wasm_bindgen]
impl Verification {
    /// `rk = ak + alpha·G`, as the transaction carries it.
    #[wasm_bindgen]
    pub fn rk(group_verifying_key: &[u8], alpha: &[u8]) -> Result<js_sys::Uint8Array, JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let alpha: [u8; 32] = alpha.try_into().map_err(|_| JsError::new("alpha must be 32 bytes"))?;
        let rk = threshold::randomized_verifying_key(&ak, &alpha).map_err(err)?;
        Ok(to_js_bytes(&rk))
    }

    /// Check an aggregated spend authorization signature. Run this before broadcasting.
    #[wasm_bindgen]
    pub fn verify_spend_auth(group_verifying_key: &[u8], alpha: &[u8], message: &[u8], signature: &[u8]) -> Result<(), JsError> {
        let ak: [u8; 32] = group_verifying_key
            .try_into()
            .map_err(|_| JsError::new("group_verifying_key must be 32 bytes"))?;
        let alpha: [u8; 32] = alpha.try_into().map_err(|_| JsError::new("alpha must be 32 bytes"))?;
        let signature: [u8; 64] = signature.try_into().map_err(|_| JsError::new("signature must be 64 bytes"))?;
        threshold::verify_with_group_key_and_alpha(&ak, &alpha, message, &signature).map_err(err)
    }
}

/// Deposit denomination policy.
#[wasm_bindgen]
pub struct Deposit;

#[wasm_bindgen]
impl Deposit {
    /// Split an amount into canonical outputs. Returns the output values (`Float64Array`) or an error
    /// when the amount cannot be sent.
    #[wasm_bindgen]
    pub fn plan(amount: f64) -> Result<js_sys::Float64Array, JsError> {
        if amount < 0.0 || amount > u64::MAX as f64 {
            return Err(JsError::new("amount out of range"));
        }
        let plan = vault_deposit::plan_deposit(amount as u64).map_err(err)?;
        let out = js_sys::Float64Array::new_with_length(plan.outputs.len() as u32);
        for (i, value) in plan.outputs.iter().enumerate() {
            out.set_index(i as u32, *value as f64);
        }
        Ok(out)
    }

    /// Residual left behind, in zatoshi.
    #[wasm_bindgen]
    pub fn residual(amount: f64) -> Result<f64, JsError> {
        let plan = vault_deposit::plan_deposit(amount as u64).map_err(err)?;
        Ok(plan.residual as f64)
    }

    #[wasm_bindgen]
    pub fn is_canonical(value: f64) -> bool {
        vault_deposit::is_canonical(value as u64)
    }

    #[wasm_bindgen]
    pub fn denom_cap() -> f64 {
        vault_deposit::DENOM_CAP_ZATS as f64
    }

    #[wasm_bindgen]
    pub fn canary_amount() -> f64 {
        vault_deposit::CANARY_AMOUNT_ZATS as f64
    }
}

/// Canary gate. `phase` is one of `"notStarted"`, `"awaitingArrival"`, `"awaitingSpend"`,
/// `"proven"`; `allows` answers whether a deposit may be started now.
#[wasm_bindgen]
pub struct Canary {
    inner: vault_deposit::Canary,
}

#[wasm_bindgen]
impl Canary {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: vault_deposit::Canary::new(),
        }
    }

    pub fn phase(&self) -> String {
        match self.inner.phase() {
            vault_deposit::CanaryPhase::NotStarted => "notStarted".to_string(),
            vault_deposit::CanaryPhase::AwaitingArrival => "awaitingArrival".to_string(),
            vault_deposit::CanaryPhase::AwaitingSpend => "awaitingSpend".to_string(),
            vault_deposit::CanaryPhase::Proven => "proven".to_string(),
        }
    }

    /// Register the small first deposit.
    pub fn begin(&mut self, amount: f64) -> Result<(), JsError> {
        self.inner.begin(amount as u64).map_err(err)
    }

    pub fn arrival_confirmed(&mut self) -> Result<(), JsError> {
        self.inner.arrival_confirmed().map_err(err)
    }

    pub fn spend_confirmed(&mut self) -> Result<(), JsError> {
        self.inner.spend_confirmed().map_err(err)
    }

    pub fn allows(&self, amount: f64) -> bool {
        self.inner.allows(amount as u64)
    }
}

impl Default for Canary {
    fn default() -> Self {
        Self::new()
    }
}
