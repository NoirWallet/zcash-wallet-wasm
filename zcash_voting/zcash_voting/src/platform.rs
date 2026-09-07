//! Owned HTTP boundary for host-supplied transports, including browser Fetch.

use std::{future::Future, pin::Pin};

pub type TransportFuture =
    Pin<Box<dyn Future<Output = Result<OwnedHttpResponse, TransportError>> + Send>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestKind {
    Idempotent,
    NonIdempotent,
}

pub struct OwnedHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout_ms: u32,
    pub max_response_bytes: usize,
    pub kind: RequestKind,
}

pub struct OwnedHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("request failed before dispatch")]
    Definite,
    #[error("request outcome is unknown after dispatch")]
    Ambiguous,
    #[error("response exceeds {limit} bytes")]
    ResponseTooLarge { limit: usize },
}

impl TransportError {
    /// Fetch cannot distinguish a refused connection from a lost response.
    /// For idempotent operations, `Definite` also denotes safe retry after
    /// dispatch; it does not prove that the server received no request.
    pub fn after_dispatch(kind: RequestKind) -> Self {
        match kind {
            RequestKind::Idempotent => Self::Definite,
            RequestKind::NonIdempotent => Self::Ambiguous,
        }
    }

    /// Safe to retry under the endpoint's policy. Oversized responses are a
    /// permanent limit failure, so repeating them unchanged is not useful.
    pub fn is_safe_to_retry(&self) -> bool {
        matches!(self, Self::Definite)
    }
}

impl OwnedHttpRequest {
    /// Reject malformed inputs before creating or dispatching a browser request.
    /// Request kind is a protocol property: a read-only PIR POST can be idempotent.
    pub fn validate(&self) -> Result<(), TransportError> {
        let url = url::Url::parse(&self.url).map_err(|_| TransportError::Definite)?;
        let method = http::Method::from_bytes(self.method.as_bytes())
            .map_err(|_| TransportError::Definite)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || self.url.bytes().any(|b| b.is_ascii_control() || b == b' ')
            || ["CONNECT", "TRACE", "TRACK"]
                .iter()
                .any(|forbidden| self.method.eq_ignore_ascii_case(forbidden))
            || ((method.as_str().eq_ignore_ascii_case("GET")
                || method.as_str().eq_ignore_ascii_case("HEAD"))
                && !self.body.is_empty())
            || self.timeout_ms == 0
            || self.timeout_ms > i32::MAX as u32
        {
            return Err(TransportError::Definite);
        }
        for (name, value) in &self.headers {
            http::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| TransportError::Definite)?;
            http::header::HeaderValue::from_str(value).map_err(|_| TransportError::Definite)?;
        }
        Ok(())
    }

    fn oversized_response(&self) -> TransportError {
        match self.kind {
            RequestKind::Idempotent => TransportError::ResponseTooLarge {
                limit: self.max_response_bytes,
            },
            RequestKind::NonIdempotent => TransportError::Ambiguous,
        }
    }

    /// Check decoded body bytes, regardless of whether Content-Length was usable.
    pub fn check_response_size(&self, size: usize) -> Result<(), TransportError> {
        if size > self.max_response_bytes {
            Err(self.oversized_response())
        } else {
            Ok(())
        }
    }

    /// Reject a usable oversized Content-Length before buffering the body.
    /// Missing or malformed values never replace the actual byte-length check.
    pub fn check_content_length(&self, value: Option<&str>) -> Result<(), TransportError> {
        if let Some(value) = value.map(str::trim) {
            if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
                let size = value.parse::<u128>();
                if size.map_or(true, |size| size > self.max_response_bytes as u128) {
                    return Err(self.oversized_response());
                }
            }
        }
        Ok(())
    }
}

/// HTTP status codes pass through unchanged for protocol-layer interpretation.
/// A non-idempotent failure after dispatch must always be `Ambiguous`, including
/// timeouts, rejected promises, oversized responses and body decoding failures.
pub trait HttpTransport: Send + Sync {
    fn execute(&self, request: OwnedHttpRequest) -> TransportFuture;
}
