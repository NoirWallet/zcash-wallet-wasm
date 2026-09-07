#![cfg(all(feature = "browser", not(target_arch = "wasm32")))]

#[path = "support/browser_storage_cases.rs"]
mod browser_storage_cases;
