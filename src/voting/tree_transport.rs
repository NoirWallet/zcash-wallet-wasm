//! Vote-tree HTTP adapts the owned Fetch boundary without retaining JS values.

use super::transport::BrowserFetchTransport;
use zcash_voting::platform::{HttpTransport, OwnedHttpRequest, RequestKind};

impl zcash_voting::tree_sync::AsyncTransport for BrowserFetchTransport {
    fn get(
        &self,
        url: String,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        zcash_voting::tree_sync::AsyncTransportResponse,
                        zcash_voting::tree_sync::AsyncTransportError,
                    >,
                > + Send,
        >,
    > {
        let response = self.execute(OwnedHttpRequest {
            method: "GET".into(),
            url,
            headers: Vec::new(),
            body: Vec::new(),
            timeout_ms: super::constants::REQUEST_TIMEOUT_MS,
            max_response_bytes: super::constants::API_LIMIT,
            kind: RequestKind::Idempotent,
        });
        Box::pin(async move {
            response
                .await
                .map(|response| zcash_voting::tree_sync::AsyncTransportResponse {
                    status: response.status,
                    body: response.body,
                })
                .map_err(|_| {
                    zcash_voting::tree_sync::AsyncTransportError::Request(
                        "browser tree request failed".into(),
                    )
                })
        })
    }
}
