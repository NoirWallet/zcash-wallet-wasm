//! Owned asynchronous transport abstraction for vote commitment tree sync.

use std::{future::Future, pin::Pin};

use crate::transport::{TransportError, TransportResponse};

/// Async GET-only transport used by the browser-safe tree sync API.
pub trait AsyncTransport: Send + Sync {
    fn get(
        &self,
        url: String,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>>;
}
