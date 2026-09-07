//! Compile the real boundary in isolation from the wallet's cryptographic code.

extern crate self as zcash_voting;

#[path = "../../src/voting/constants.rs"]
pub mod constants;
#[path = "../../zcash_voting/zcash_voting/src/platform.rs"]
pub mod platform;
#[path = "../../src/voting/transport.rs"]
pub mod transport;

use platform::{HttpTransport, OwnedHttpRequest, RequestKind};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn request(post: bool, limit: usize) -> Result<JsValue, JsValue> {
    let result = transport::BrowserFetchTransport
        .execute(OwnedHttpRequest {
            method: if post { "POST" } else { "GET" }.into(),
            url: "https://example.invalid/fixture".into(),
            headers: Vec::new(),
            body: Vec::new(),
            timeout_ms: 100,
            max_response_bytes: limit,
            kind: if post {
                RequestKind::NonIdempotent
            } else {
                RequestKind::Idempotent
            },
        })
        .await
        .map_err(|error| JsValue::from_str(&format!("{error:?}")))?;
    serde_wasm_bindgen::to_value(&(result.status, result.body))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}
