use pczt::roles::{combiner::Combiner, redactor::Redactor};

use crate::*;

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct Pczt(pczt::Pczt);

impl From<pczt::Pczt> for Pczt {
    fn from(pczt: pczt::Pczt) -> Self {
        Self(pczt)
    }
}

impl From<Pczt> for pczt::Pczt {
    fn from(pczt: Pczt) -> Self {
        pczt.0
    }
}

#[wasm_bindgen]
impl Pczt {
    /// Returns a JSON object with the details of the Pczt.
    pub fn to_json(&self) -> Result<JsValue, Error> {
        serde_wasm_bindgen::to_value(&self.serialize()?).map_err(Error::from)
    }

    /// Returns a Pczt from a JSON object
    pub fn from_json(s: JsValue) -> Result<Pczt, Error> {
        let bytes: Vec<u8> = serde_wasm_bindgen::from_value(s).map_err(Error::from)?;
        Self::from_bytes(&bytes)
    }

    /// Returns the postcard serialization of the Pczt.
    pub fn serialize(&self) -> Result<Vec<u8>, Error> {
        self.0
            .clone()
            .serialize()
            .map_err(|e| Error::Generic(format!("Failed to serialize PCZT: {:?}", e)))
    }

    /// Deserialize to a Pczt from postcard bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Pczt, Error> {
        pczt::Pczt::parse(bytes)
            .map(Self)
            .map_err(|e| Error::Generic(format!("Failed to parse PCZT bytes: {:?}", e)))
    }

    /// Strips prover-only data so the PCZT is small enough to transport to an
    /// external signer (e.g. Keystone) as an animated QR.
    ///
    /// Mirrors the "redact for signer" role used by Zashi: removes spend
    /// witnesses (the per-note Merkle authentication paths that dominate the
    /// serialized size) and the `zcash_client_backend` proprietary metadata.
    /// The signer only needs the spend randomizers, values and derivation
    /// paths, so the redacted PCZT still produces valid signatures. The
    /// original (full) PCZT must be retained and merged back with [`combine`]
    /// after signing to restore the witnesses required for proving.
    ///
    /// [`combine`]: Pczt::combine
    pub fn redact_for_signer(&self) -> Pczt {
        let redacted = Redactor::new(self.0.clone())
            .redact_global_with(|mut r| r.redact_proprietary("zcash_client_backend:proposal_info"))
            .redact_orchard_with(|mut r| {
                r.redact_actions(|mut ar| {
                    ar.clear_spend_witness();
                    ar.redact_output_proprietary("zcash_client_backend:output_info");
                });
            })
            .redact_sapling_with(|mut r| {
                r.redact_spends(|mut sr| sr.clear_witness());
                r.redact_outputs(|mut or| {
                    or.redact_proprietary("zcash_client_backend:output_info")
                });
            })
            .redact_transparent_with(|mut r| {
                r.redact_outputs(|mut or| {
                    or.redact_proprietary("zcash_client_backend:output_info")
                });
            })
            .finish();
        Pczt(redacted)
    }

    /// Combines this PCZT with another representing the same transaction.
    ///
    /// Used after external signing: `self` is the full PCZT kept by the wallet
    /// (with witnesses) and `other` is the redacted PCZT returned by the signer
    /// (with signatures). The result carries both, ready for proving and
    /// extraction.
    pub fn combine(&self, other: &Pczt) -> Result<Pczt, Error> {
        Combiner::new(vec![self.0.clone(), other.0.clone()])
            .combine()
            .map(Pczt)
            .map_err(|e| Error::Generic(format!("Failed to combine PCZTs: {:?}", e)))
    }
}
