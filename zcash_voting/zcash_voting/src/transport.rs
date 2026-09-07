//! Built-in HTTP transports for client features.

#[cfg(not(target_arch = "wasm32"))]
pub use crate::http_transport::HyperTransport;
