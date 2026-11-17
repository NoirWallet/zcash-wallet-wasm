//! JS bindings for the FROST Secure Vault.
//!
//! Everything crosses the boundary as `Uint8Array` or a plain number, per the handoff contract: the
//! frontend owns transport, storage and UI, and the wasm owns protocol state. That means there are
//! no JS-implemented traits here — anything the host must remember is a byte blob it can store
//! however it likes.
//!
//! Design notes worth knowing before reading further:
//!
//! * Roster slots are 0-based; the FROST identifier for a slot is derived inside the crate. The
//!   frontend never sees or carries identifiers.
//! * Secrets (key package, DKG round secrets) are held inside the Rust objects, not passed back and
//!   forth. The frontend gets a blob only when it needs to persist or transmit one.
//! * A failed call returns `Err`; a wasm panic would abort the instance, so nothing here is allowed
//!   to panic on hostile input. The decoders underneath are covered by adversarial tests.

use std::collections::BTreeMap;

use frost_vault_core::custody;
use frost_vault_core::noise;
use frost_vault_core::protocol::{self, Envelope, MsgType, RejectCode, ResultStatus, COORDINATOR_SLOT};
use frost_vault_core::roster;
use frost_vault_core::seal;
use frost_vault_core::session::{CoordinatorSession, SessionConfig, SignerSession};
use frost_vault_core::state::{NonceState, SignPhase};
use frost_vault_core::threshold::{self, DkgFinal, DkgRound1, DkgRound2, KeyPackage, PublicKeyPackage};
use rand_core::OsRng;
use wasm_bindgen::prelude::*;

type Rng = OsRng;

fn rng() -> Rng {
    OsRng
}

fn err(e: impl core::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn to_js_bytes(bytes: &[u8]) -> js_sys::Uint8Array {
    js_sys::Uint8Array::from(bytes)
}

fn slot_and_bytes(envelopes: Vec<Envelope>) -> js_sys::Array {
    let out = js_sys::Array::new();
    for env in envelopes {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from(env.recipient));
        pair.push(&to_js_bytes(&env.encode()));
        out.push(&pair);
    }
    out
}

mod codec;
mod dkg;
mod fixtures;
mod keyset;
mod shares;
mod signing;
mod transport;

pub use codec::*;
pub use dkg::*;
#[cfg(feature = "test-fixtures")]
pub use fixtures::*;
pub use keyset::*;
pub use shares::*;
pub use signing::*;
pub use transport::*;
