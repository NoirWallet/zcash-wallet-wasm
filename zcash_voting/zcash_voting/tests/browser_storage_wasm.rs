#![cfg(all(feature = "browser", target_arch = "wasm32"))]

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/browser_storage_cases.rs"]
mod browser_storage_cases;
