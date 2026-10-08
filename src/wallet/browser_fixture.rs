use std::time::{Duration, SystemTime};

use wasm_bindgen::prelude::*;

use super::transaction_request_is_due;

#[wasm_bindgen(js_name = walletTestRequestTiming)]
pub fn request_timing() -> bool {
    let future = SystemTime::from(time::OffsetDateTime::now_utc()) + Duration::from_secs(60);
    transaction_request_is_due(None)
        && transaction_request_is_due(Some(SystemTime::UNIX_EPOCH))
        && !transaction_request_is_due(Some(future))
}
