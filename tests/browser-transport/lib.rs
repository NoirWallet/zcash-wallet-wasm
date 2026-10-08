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
pub async fn cancel_request() {
    let future = transport::BrowserFetchTransport.execute(OwnedHttpRequest {
        method: "POST".into(),
        url: "https://example.invalid/cancel".into(),
        headers: Vec::new(),
        body: Vec::new(),
        timeout_ms: 100,
        max_response_bytes: 4,
        kind: RequestKind::NonIdempotent,
    });
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
        .await
        .unwrap();
    drop(future);
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
        .await
        .unwrap();
}

#[wasm_bindgen]
pub async fn network_request(url: String, limit: usize) -> Result<JsValue, JsValue> {
    let response = transport::BrowserFetchTransport
        .execute(OwnedHttpRequest {
            method: "GET".into(),
            url,
            headers: Vec::new(),
            body: Vec::new(),
            timeout_ms: 1000,
            max_response_bytes: limit,
            kind: RequestKind::Idempotent,
        })
        .await
        .map_err(|error| JsValue::from_str(&format!("{error:?}")))?;
    serde_wasm_bindgen::to_value(&(response.status, response.body, response.content_type))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub async fn cancel_batches(dispatched: js_sys::Function) {
    let mut previous = Vec::new();
    for batch in 0..3 {
        drop(std::mem::take(&mut previous));
        previous = (0..16)
            .map(|_| {
                transport::BrowserFetchTransport.execute(OwnedHttpRequest {
                    method: "POST".into(),
                    url: "https://example.invalid/cancel".into(),
                    headers: Vec::new(),
                    body: Vec::new(),
                    timeout_ms: 100,
                    max_response_bytes: 4,
                    kind: RequestKind::NonIdempotent,
                })
            })
            .collect();
        let barrier = dispatched
            .call1(&JsValue::NULL, &JsValue::from((batch + 1) * 16))
            .unwrap();
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&barrier))
            .await
            .unwrap();
    }
    drop(previous);
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
        .await
        .unwrap();
}

#[wasm_bindgen]
pub async fn cancellation_queue_case(mode: u32) -> String {
    let make = |timeout_ms| {
        transport::BrowserFetchTransport.execute(OwnedHttpRequest {
            method: "POST".into(),
            url: "https://example.invalid/queued".into(),
            headers: Vec::new(),
            body: Vec::new(),
            timeout_ms,
            max_response_bytes: 4,
            kind: RequestKind::NonIdempotent,
        })
    };
    if mode == 0 {
        drop(make(100));
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
            .await
            .unwrap();
        return "unpolled".into();
    }
    let active = (0..16).map(|_| make(1000)).collect::<Vec<_>>();
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
        .await
        .unwrap();
    let queued = make(10);
    let result = if mode == 1 {
        format!("{:?}", queued.await.err().unwrap())
    } else {
        wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
            .await
            .unwrap();
        drop(queued);
        "cancelled".into()
    };
    drop(active);
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL))
        .await
        .unwrap();
    result
}

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
