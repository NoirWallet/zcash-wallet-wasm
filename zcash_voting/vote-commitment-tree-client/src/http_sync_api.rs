//! HTTP endpoint mapping for [`TreeSyncApi`] against a running Zally chain node.
//!
//! Maps the three trait methods to per-round REST endpoints:
//! - `get_tree_state()`        → `GET /shielded-vote/v1/commitment-tree/{round_id}/latest`
//! - `get_root_at_height(h)`   → `GET /shielded-vote/v1/commitment-tree/{round_id}/{h}`
//! - `get_block_commitments()` → `GET /shielded-vote/v1/commitment-tree/{round_id}/leaves?from_height=X&to_height=Y`

use std::sync::Arc;

use vote_commitment_tree::sync_api::{
    AsyncTreeSyncApi, BlockCommitmentsPage, TreeState, TreeSyncApi,
};
use voting_crypto_deps::pasta_curves::Fp;

use crate::async_transport::AsyncTransport;
use crate::transport::{Transport, TransportError, TransportResponse};
use crate::types::{
    QueryCommitmentLeavesResponse, QueryCommitmentTreeResponse, QueryLatestTreeResponse,
};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors from the HTTP sync API.
#[derive(Debug, thiserror::Error)]
pub enum HttpSyncError {
    #[error("transport request failed: {0}")]
    Transport(#[from] TransportError),

    #[error("HTTP request to {url} returned status {status}: {body}")]
    HttpStatus {
        url: String,
        status: u16,
        body: String,
    },

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("parse error: {0}")]
    Parse(#[from] crate::types::ParseError),

    #[error("server returned no tree state")]
    NoTreeState,
}

/// Browser-safe async sync errors. URLs and response bodies are deliberately
/// omitted because authenticated service metadata may be sensitive.
#[derive(Debug, thiserror::Error)]
pub enum AsyncHttpSyncError {
    #[error("tree transport request failed")]
    Transport(#[source] TransportError),

    #[error("tree HTTP request returned status {status}")]
    HttpStatus { status: u16 },

    #[error("tree response exceeds {limit} bytes")]
    ResponseTooLarge { limit: usize },

    #[error("tree JSON response is invalid")]
    Json(#[source] serde_json::Error),

    #[error("tree response fields are invalid")]
    Parse(#[source] crate::types::ParseError),

    #[error("server returned no tree state")]
    NoTreeState,

    #[error("tree root response height does not match the requested checkpoint")]
    AnchorHeightMismatch,

    #[error("tree endpoint is not a strict HTTP(S) base URL for a lowercase 32-byte round ID")]
    InvalidEndpoint,
}

/// Per-response limit used by the generic async client. Browser transports
/// enforce the same limit before allocating and this remains a second boundary.
pub const DEFAULT_ASYNC_TREE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

fn latest_url(base_url: &str, round_id: &str) -> String {
    format!("{base_url}/shielded-vote/v1/commitment-tree/{round_id}/latest")
}

fn root_url(base_url: &str, round_id: &str, height: u32) -> String {
    format!("{base_url}/shielded-vote/v1/commitment-tree/{round_id}/{height}")
}

fn leaves_url(base_url: &str, round_id: &str, from_height: u32, to_height: u32) -> String {
    format!(
        "{base_url}/shielded-vote/v1/commitment-tree/{round_id}/leaves?from_height={from_height}&to_height={to_height}"
    )
}

fn parse_tree_state(body: &[u8]) -> Result<TreeState, HttpSyncError> {
    let response: QueryLatestTreeResponse = serde_json::from_slice(body)?;
    response
        .tree
        .ok_or(HttpSyncError::NoTreeState)?
        .into_tree_state()
        .map_err(HttpSyncError::Parse)
}

fn parse_root(body: &[u8]) -> Result<Option<Fp>, HttpSyncError> {
    parse_root_state(body).map(|state| state.map(|state| state.root))
}

fn parse_root_state(body: &[u8]) -> Result<Option<TreeState>, HttpSyncError> {
    let response: QueryCommitmentTreeResponse = serde_json::from_slice(body)?;
    response
        .tree
        .map(|state| state.into_tree_state())
        .transpose()
        .map_err(HttpSyncError::Parse)
}

fn parse_page(body: &[u8]) -> Result<BlockCommitmentsPage, HttpSyncError> {
    let response: QueryCommitmentLeavesResponse = serde_json::from_slice(body)?;
    response
        .into_block_commitments_page()
        .map_err(HttpSyncError::Parse)
}

// ---------------------------------------------------------------------------
// HttpTreeSyncApi
// ---------------------------------------------------------------------------

/// HTTP-based implementation of [`TreeSyncApi`] for remote chain sync.
///
/// Uses an injected blocking transport so this crate can own URL construction
/// and JSON parsing without selecting an HTTP stack for library consumers.
/// Each instance is scoped to a specific voting round via `round_id`
/// (hex-encoded in URL paths).
pub struct HttpTreeSyncApi {
    transport: Arc<dyn Transport>,
    /// Base URL of the chain's REST API (e.g. `http://localhost:1317`).
    base_url: String,
    /// Hex-encoded round ID for per-round tree endpoints.
    round_id: String,
}

impl HttpTreeSyncApi {
    /// Create a new HTTP sync API client for a specific voting round.
    ///
    /// `base_url` should be the root of the chain's REST API, without a trailing
    /// slash (e.g. `http://localhost:1317`). `round_id` is the hex-encoded
    /// voting round identifier.
    pub fn new(
        base_url: impl Into<String>,
        round_id: impl Into<String>,
        transport: Arc<dyn Transport>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            round_id: round_id.into(),
            transport,
        }
    }

    fn success_body(
        &self,
        url: &str,
        response: TransportResponse,
    ) -> Result<Vec<u8>, HttpSyncError> {
        if (200..300).contains(&response.status) {
            Ok(response.body)
        } else {
            Err(HttpSyncError::HttpStatus {
                url: url.to_string(),
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            })
        }
    }
}

impl TreeSyncApi for HttpTreeSyncApi {
    type Error = HttpSyncError;

    fn get_tree_state(&self) -> Result<TreeState, Self::Error> {
        let url = latest_url(&self.base_url, &self.round_id);
        let body = self.success_body(&url, self.transport.get(&url)?)?;
        parse_tree_state(&body)
    }

    fn get_root_at_height(&self, height: u32) -> Result<Option<Fp>, Self::Error> {
        let url = root_url(&self.base_url, &self.round_id, height);
        let body = self.success_body(&url, self.transport.get(&url)?)?;
        parse_root(&body)
    }

    fn get_block_commitments(
        &self,
        from_height: u32,
        to_height: u32,
    ) -> Result<BlockCommitmentsPage, Self::Error> {
        let url = leaves_url(&self.base_url, &self.round_id, from_height, to_height);
        let body = self.success_body(&url, self.transport.get(&url)?)?;
        parse_page(&body)
    }
}

/// HTTP implementation of [`AsyncTreeSyncApi`] with owned response futures.
pub struct AsyncHttpTreeSyncApi {
    transport: Arc<dyn AsyncTransport>,
    base_url: String,
    round_id: String,
    max_response_bytes: usize,
}

impl AsyncHttpTreeSyncApi {
    pub fn new(
        base_url: impl AsRef<str>,
        round_id: impl AsRef<str>,
        transport: Arc<dyn AsyncTransport>,
    ) -> Result<Self, AsyncHttpSyncError> {
        Self::with_response_limit(
            base_url,
            round_id,
            transport,
            DEFAULT_ASYNC_TREE_RESPONSE_BYTES,
        )
    }

    pub fn with_response_limit(
        base_url: impl AsRef<str>,
        round_id: impl AsRef<str>,
        transport: Arc<dyn AsyncTransport>,
        max_response_bytes: usize,
    ) -> Result<Self, AsyncHttpSyncError> {
        let base_url = base_url.as_ref();
        let round_id = round_id.as_ref();
        let parsed = url::Url::parse(base_url).map_err(|_| AsyncHttpSyncError::InvalidEndpoint)?;
        let valid_round = round_id.len() == 64
            && round_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !valid_round
            || max_response_bytes == 0
        {
            return Err(AsyncHttpSyncError::InvalidEndpoint);
        }
        Ok(Self {
            transport,
            base_url: base_url.trim_end_matches('/').to_string(),
            round_id: round_id.to_string(),
            max_response_bytes,
        })
    }

    async fn response_body(&self, url: String) -> Result<Vec<u8>, AsyncHttpSyncError> {
        let response = self
            .transport
            .get(url)
            .await
            .map_err(AsyncHttpSyncError::Transport)?;
        if response.body.len() > self.max_response_bytes {
            return Err(AsyncHttpSyncError::ResponseTooLarge {
                limit: self.max_response_bytes,
            });
        }
        if !(200..300).contains(&response.status) {
            return Err(AsyncHttpSyncError::HttpStatus {
                status: response.status,
            });
        }
        Ok(response.body)
    }

    fn map_parse(error: HttpSyncError) -> AsyncHttpSyncError {
        match error {
            HttpSyncError::Json(error) => AsyncHttpSyncError::Json(error),
            HttpSyncError::Parse(error) => AsyncHttpSyncError::Parse(error),
            HttpSyncError::NoTreeState => AsyncHttpSyncError::NoTreeState,
            HttpSyncError::Transport(_) | HttpSyncError::HttpStatus { .. } => {
                unreachable!("parsers cannot produce transport errors")
            }
        }
    }
}

impl AsyncTreeSyncApi for AsyncHttpTreeSyncApi {
    type Error = AsyncHttpSyncError;

    fn get_tree_state(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TreeState, Self::Error>> + Send>>
    {
        let transport = self.transport.clone();
        let base_url = self.base_url.clone();
        let round_id = self.round_id.clone();
        let limit = self.max_response_bytes;
        Box::pin(async move {
            let api = Self {
                transport,
                base_url,
                round_id,
                max_response_bytes: limit,
            };
            let body = api
                .response_body(latest_url(&api.base_url, &api.round_id))
                .await?;
            parse_tree_state(&body).map_err(Self::map_parse)
        })
    }

    fn get_root_at_height(
        &self,
        height: u32,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Fp>, Self::Error>> + Send>>
    {
        let transport = self.transport.clone();
        let base_url = self.base_url.clone();
        let round_id = self.round_id.clone();
        let limit = self.max_response_bytes;
        Box::pin(async move {
            let api = Self {
                transport,
                base_url,
                round_id,
                max_response_bytes: limit,
            };
            let body = api
                .response_body(root_url(&api.base_url, &api.round_id, height))
                .await?;
            let state = parse_root_state(&body).map_err(Self::map_parse)?;
            if state.as_ref().is_some_and(|state| state.height != height) {
                return Err(AsyncHttpSyncError::AnchorHeightMismatch);
            }
            Ok(state.map(|state| state.root))
        })
    }

    fn get_block_commitments(
        &self,
        from_height: u32,
        to_height: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BlockCommitmentsPage, Self::Error>> + Send>,
    > {
        let transport = self.transport.clone();
        let base_url = self.base_url.clone();
        let round_id = self.round_id.clone();
        let limit = self.max_response_bytes;
        Box::pin(async move {
            let api = Self {
                transport,
                base_url,
                round_id,
                max_response_bytes: limit,
            };
            let body = api
                .response_body(leaves_url(
                    &api.base_url,
                    &api.round_id,
                    from_height,
                    to_height,
                ))
                .await?;
            parse_page(&body).map_err(Self::map_parse)
        })
    }
}
