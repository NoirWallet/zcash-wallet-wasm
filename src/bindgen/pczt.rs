use serde::{Deserialize, Serialize};

use crate::*;

#[wasm_bindgen]
#[derive(Clone, Debug, Serialize, Deserialize)]
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
        serde_wasm_bindgen::to_value(&self).map_err(Error::from)
    }

    /// Returns a Pczt from a JSON object
    pub fn from_json(s: JsValue) -> Result<Pczt, Error> {
        serde_wasm_bindgen::from_value(s).map_err(Error::from)
    }

    /// Returns the postcard serialization of the Pczt.
    pub fn serialize(&self) -> Vec<u8> {
        self.0.serialize()
    }

    /// Deserialize to a Pczt from postcard bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Pczt, Error> {
        pczt::Pczt::parse(bytes)
            .map(Self)
            .map_err(|e| Error::Generic(format!("Failed to parse PCZT bytes: {:?}", e)))
    }
}
