//! Fetch and JavaScript ownership stay on the calling browser thread.

use futures_channel::oneshot;
use futures_util::future::{Either, select};
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use zcash_voting::platform::{
    HttpTransport, OwnedHttpRequest, OwnedHttpResponse, TransportError, TransportFuture,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserFetchTransport;

impl HttpTransport for BrowserFetchTransport {
    fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
        if let Err(error) = request.validate() {
            return Box::pin(async move { Err(error) });
        }
        let kind = request.kind;
        let (sender, receiver) = oneshot::channel();
        spawn_local(async move {
            let result = fetch(request).await;
            let _ = sender.send(result);
        });
        // Only owned Rust data crosses this Send future. Lost local execution
        // cannot prove that a non-idempotent request was never dispatched.
        Box::pin(async move {
            receiver
                .await
                .unwrap_or_else(|_| Err(TransportError::after_dispatch(kind)))
        })
    }
}

async fn fetch(request: OwnedHttpRequest) -> Result<OwnedHttpResponse, TransportError> {
    let window = web_sys::window().ok_or(TransportError::Definite)?;
    let controller = web_sys::AbortController::new().map_err(|_| TransportError::Definite)?;
    let init = web_sys::RequestInit::new();
    init.set_method(&request.method);
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_credentials(web_sys::RequestCredentials::Omit);
    init.set_redirect(web_sys::RequestRedirect::Error);
    init.set_signal(Some(&controller.signal()));
    let headers = web_sys::Headers::new().map_err(|_| TransportError::Definite)?;
    for (name, value) in &request.headers {
        headers
            .append(name, value)
            .map_err(|_| TransportError::Definite)?;
    }
    init.set_headers(&headers);
    if !request.body.is_empty() {
        let body = js_sys::Uint8Array::from(request.body.as_slice());
        init.set_body(&body);
    }
    let js_request = web_sys::Request::new_with_str_and_init(&request.url, &init)
        .map_err(|_| TransportError::Definite)?;
    let fetch: js_sys::Function =
        js_sys::Reflect::get(&window, &wasm_bindgen::JsValue::from_str("fetch"))
            .map_err(|_| TransportError::Definite)?
            .dyn_into()
            .map_err(|_| TransportError::Definite)?;

    let (timeout_sender, timeout_receiver) = oneshot::channel::<()>();
    let mut timeout_sender = Some(timeout_sender);
    let abort = controller.clone();
    let timeout_callback = Closure::<dyn FnMut()>::new(move || {
        abort.abort();
        if let Some(sender) = timeout_sender.take() {
            let _ = sender.send(());
        }
    });
    let timeout_id = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            timeout_callback.as_ref().unchecked_ref(),
            request.timeout_ms as i32,
        )
        .map_err(|_| TransportError::Definite)?;

    let after_dispatch = || TransportError::after_dispatch(request.kind);
    let response = async {
        // Function.call1 catches synchronous host exceptions, unlike the
        // web-sys Fetch binding. Once invocation begins, non-delivery is unknown.
        let promise: js_sys::Promise = fetch
            .call1(&window, &js_request)
            .map_err(|_| after_dispatch())?
            .dyn_into()
            .map_err(|_| after_dispatch())?;
        let value = JsFuture::from(promise)
            .await
            .map_err(|_| after_dispatch())?;
        let response: web_sys::Response = value.dyn_into().map_err(|_| after_dispatch())?;
        let content_length = response
            .headers()
            .get("Content-Length")
            .map_err(|_| after_dispatch())?;
        request.check_content_length(content_length.as_deref())?;
        let buffer = JsFuture::from(response.array_buffer().map_err(|_| after_dispatch())?)
            .await
            .map_err(|_| after_dispatch())?;
        let buffer: js_sys::ArrayBuffer = buffer.dyn_into().map_err(|_| after_dispatch())?;
        // The locked js-sys byte_length getter narrows to u32. Reflect retains
        // the complete JS number, including lengths at and above 2^32.
        let size = js_sys::Reflect::get(&buffer, &wasm_bindgen::JsValue::from_str("byteLength"))
            .map_err(|_| after_dispatch())?
            .as_f64()
            .ok_or_else(after_dispatch)?;
        request.check_response_size_number(size)?;
        Ok(OwnedHttpResponse {
            status: response.status(),
            body: js_sys::Uint8Array::new(&buffer).to_vec(),
        })
    };
    let result = match select(Box::pin(response), timeout_receiver).await {
        Either::Left((result, _)) => result,
        Either::Right((_, _)) => Err(after_dispatch()),
    };
    window.clear_timeout_with_handle(timeout_id);
    if result.is_err() {
        controller.abort();
    }
    drop(timeout_callback);
    result
}

/// Local HTTP fixture entry point, returning `[status, body]` to JavaScript.
#[cfg(debug_assertions)]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = votingTestFetch)]
pub async fn voting_test_fetch(
    url: String,
    limit: usize,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    use zcash_voting::platform::RequestKind;

    let response = BrowserFetchTransport
        .execute(OwnedHttpRequest {
            method: "GET".into(),
            url,
            headers: Vec::new(),
            body: Vec::new(),
            timeout_ms: super::constants::REQUEST_TIMEOUT_MS,
            max_response_bytes: limit,
            kind: RequestKind::Idempotent,
        })
        .await
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))?;
    serde_wasm_bindgen::to_value(&(response.status, response.body))
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}
