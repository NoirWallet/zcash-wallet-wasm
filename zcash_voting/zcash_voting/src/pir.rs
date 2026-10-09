//! PIR endpoint selection and client re-exports.
//!
//! Wallets use this module to select an exact-height PIR snapshot endpoint
//! before delegation PIR precomputation. [`connect_pir_blocking`] /
//! [`connect_pir`] bind a caller-chosen URL to an explicit [`PirLayout`] for
//! the config/server handshake (tree split and YPIR degree). Neither path
//! hardcodes depth, tier-split, or YPIR degree constants, and neither checks
//! whether the URL appears in a resolved config's advertised endpoint list.

use std::sync::Arc;

use crate::config::{validate_and_convert_pir_layout, PirLayout};
use crate::types::VotingError;

pub use crate::pir_snapshot::{
    classify_pir_snapshot_height, matching_pir_snapshot_endpoints, select_pir_snapshot_endpoint,
    PirSnapshotEndpointDiagnostic, PirSnapshotEndpointStatus, PirSnapshotResolution,
};

/// Candidate PIR endpoint URL.
pub type PirEndpoint = String;

/// Selects an exact-height PIR endpoint from already-probed diagnostics.
///
/// `match_index` lets callers inject deterministic or random selection without
/// making endpoint probing part of the core API.
pub fn select_pir_endpoint(
    diagnostics: &[PirSnapshotEndpointDiagnostic],
    snapshot_height: u64,
    match_index: u64,
) -> Result<PirSnapshotResolution, VotingError> {
    select_pir_snapshot_endpoint(diagnostics, snapshot_height, match_index)
}

#[cfg(not(target_arch = "wasm32"))]
pub use pir_client::PirClientBlocking;
pub use pir_client::{
    ImtProofData, PirClient, RetryableTransportError, Transport, TransportFuture, TransportResponse,
};
pub use pir_types::PirLayout as NegotiatedPirLayout;

/// Bind the client's actual handshake to the trusted round before any private query.
pub async fn connect_pir_for_snapshot(
    layout: PirLayout,
    endpoint: &str,
    transport: Arc<dyn Transport>,
    network: crate::Network,
    root: &[u8],
    height: u64,
) -> Result<PirClient, VotingError> {
    connect_snapshot(layout, endpoint, transport, network, root, height)
        .await
        .map_err(|_| snapshot_identity_error())
}

fn snapshot_identity_error() -> VotingError {
    VotingError::InvalidInput {
        message: "PIR snapshot identity mismatch".into(),
    }
}

async fn connect_snapshot(
    layout: PirLayout,
    endpoint: &str,
    transport: Arc<dyn Transport>,
    network: crate::Network,
    root: &[u8],
    height: u64,
) -> anyhow::Result<PirClient> {
    if network != crate::Network::Testnet || root.len() != 32 || height == 0 {
        return Err(snapshot_identity_error().into());
    }
    let endpoint = normalize_endpoint_url(endpoint);
    anyhow::ensure!(!endpoint.is_empty(), "PIR endpoint missing");
    let negotiated = negotiated_pir_layout(layout)?;
    let bound = SnapshotTransport {
        inner: transport,
        root_url: format!("{endpoint}/root"),
        layout: negotiated,
        root: root.to_vec(),
        height,
    };
    PirClient::with_transport(&endpoint, negotiated, Arc::new(bound)).await
}

/// Ordered authenticated endpoints for one round. A replacement is connected
/// and fully bound before the unchanged client generates any private query.
/// Callers supply only endpoint URLs in the authenticated configuration order.
pub struct OrderedSnapshotPir {
    layout: PirLayout,
    endpoints: std::vec::IntoIter<String>,
    transport: Arc<dyn Transport>,
    network: crate::Network,
    root: Vec<u8>,
    height: u64,
    attempted: usize,
    client: Option<PirClient>,
}

impl OrderedSnapshotPir {
    pub async fn connect(
        layout: PirLayout,
        endpoints: Vec<String>,
        transport: Arc<dyn Transport>,
        network: crate::Network,
        root: &[u8],
        height: u64,
    ) -> Result<Self, VotingError> {
        let mut selected = Self {
            layout,
            endpoints: endpoints.into_iter(),
            transport,
            network,
            root: root.to_vec(),
            height,
            attempted: 0,
            client: None,
        };
        selected.next_endpoint().await?;
        Ok(selected)
    }

    async fn next_endpoint(&mut self) -> Result<(), VotingError> {
        self.client = None;
        for endpoint in self.endpoints.by_ref() {
            self.attempted += 1;
            match connect_snapshot(
                self.layout,
                &endpoint,
                self.transport.clone(),
                self.network,
                &self.root,
                self.height,
            )
            .await
            {
                Ok(client) => {
                    self.client = Some(client);
                    return Ok(());
                }
                Err(error) if error.is::<RetryableTransportError>() => continue,
                Err(_) => return Err(snapshot_identity_error()),
            }
        }
        Err(VotingError::Internal {
            message: format!("PIR unavailable after {} endpoints", self.attempted),
        })
    }

    pub async fn fetch_proofs(
        &mut self,
        nullifiers: &[crate::backend::pasta_curves::pallas::Base],
    ) -> Result<Vec<ImtProofData>, VotingError> {
        self.fetch_with(|client| {
            let nullifiers = nullifiers.to_vec();
            Box::pin(async move {
                let results = futures_util::future::join_all(
                    nullifiers
                        .iter()
                        .map(|&nullifier| client.fetch_proof(nullifier)),
                )
                .await;
                verify_pir_batch(&nullifiers, results)
            })
        })
        .await
    }

    async fn fetch_with<T>(
        &mut self,
        query: impl for<'a> Fn(
            &'a PirClient,
        ) -> futures_util::future::LocalBoxFuture<'a, anyhow::Result<T>>,
    ) -> Result<T, VotingError> {
        loop {
            let client = self.client.as_ref().ok_or_else(snapshot_identity_error)?;
            match query(client).await {
                Ok(result) => return Ok(result),
                Err(error) if error.is::<RetryableTransportError>() => self.next_endpoint().await?,
                Err(_) => {
                    return Err(VotingError::InvalidInput {
                        message: "PIR retrieval failed".into(),
                    })
                }
            }
        }
    }
}

fn verify_pir_batch(
    nullifiers: &[crate::backend::pasta_curves::pallas::Base],
    results: Vec<anyhow::Result<ImtProofData>>,
) -> anyhow::Result<Vec<ImtProofData>> {
    anyhow::ensure!(
        nullifiers.len() == results.len(),
        "PIR result count mismatch"
    );
    let mut proofs = Vec::with_capacity(results.len());
    let mut retryable = None;
    // All requests have completed. Even an assembled Ok proof must verify
    // before a retryable sibling is allowed to trigger endpoint fallback.
    for (&nullifier, result) in nullifiers.iter().zip(results) {
        match result {
            Ok(proof) => {
                anyhow::ensure!(proof.verify(nullifier), "PIR proof verification failed");
                proofs.push(proof);
            }
            Err(error) if error.is::<RetryableTransportError>() => retryable = Some(error),
            Err(error) => return Err(error),
        }
    }
    match retryable {
        Some(error) => Err(error),
        None => Ok(proofs),
    }
}

struct SnapshotTransport {
    inner: Arc<dyn Transport>,
    root_url: String,
    layout: NegotiatedPirLayout,
    root: Vec<u8>,
    height: u64,
}
impl Transport for SnapshotTransport {
    fn get<'a>(&'a self, url: &'a str) -> TransportFuture<'a> {
        Box::pin(async move {
            let response = self.inner.get(url).await?;
            if !(200..300).contains(&response.status) {
                return Err(RetryableTransportError.into());
            }
            if url == self.root_url {
                let info: pir_types::RootInfo = serde_json::from_slice(&response.body)?;
                anyhow::ensure!(
                    (200..300).contains(&response.status)
                        && info.zcash_network == pir_types::ZcashNetwork::Test
                        && pir_types::is_current_dataset(
                            &info.nullifier_pool,
                            info.dataset_version
                        )
                        && info.pir_layout == self.layout
                        && hex::decode(info.circuit_root)? == self.root
                        && info.height == Some(self.height),
                    "PIR snapshot identity mismatch"
                );
            }
            Ok(response)
        })
    }
    fn post<'a>(&'a self, url: &'a str, body: Vec<u8>) -> TransportFuture<'a> {
        self.inner.post(url, body)
    }
}

/// Converts a wallet-config [`PirLayout`] into the PIR client's negotiated layout.
///
/// Rejects the legacy summary sentinel [`PirLayout::UNKNOWN`], inconsistent
/// geometry, and layouts below the YPIR row or item-size minima.
pub fn negotiated_pir_layout(layout: PirLayout) -> Result<NegotiatedPirLayout, VotingError> {
    if layout == PirLayout::UNKNOWN {
        return Err(VotingError::InvalidInput {
            message: "pir_layout is unknown; resolve a current dynamic voting config first"
                .to_string(),
        });
    }
    validate_and_convert_pir_layout(layout).map_err(|message| VotingError::InvalidInput {
        message: format!("invalid pir_layout: {message}"),
    })
}

/// Connects a blocking PIR client using an explicit layout and endpoint URL.
///
/// Does not check whether `endpoint_url` appears in a resolved config's
/// advertised endpoint list; callers pass a layout and URL they already chose
/// (for example after exact-height snapshot probing). Fails closed before any
/// private query when the layout is unknown or the config/server handshake
/// rejects the server. Layout mismatches (including `poly_len`) surface as
/// [`VotingError::InvalidInput`]; other connect failures remain
/// [`VotingError::Internal`].
#[cfg(not(target_arch = "wasm32"))]
pub fn connect_pir_blocking(
    pir_layout: PirLayout,
    endpoint_url: &str,
    transport: Arc<dyn Transport>,
) -> Result<PirClientBlocking, VotingError> {
    let endpoint = normalize_endpoint_url(endpoint_url);
    if endpoint.is_empty() {
        return Err(VotingError::InvalidInput {
            message: "PIR endpoint URL must not be empty".to_string(),
        });
    }
    let expected_layout = negotiated_pir_layout(pir_layout)?;
    PirClientBlocking::with_transport(&endpoint, expected_layout, transport)
        .map_err(map_pir_connect_error)
}

/// Connects an async PIR client using an explicit layout and endpoint URL.
///
/// See [`connect_pir_blocking`] for layout and URL rules.
pub async fn connect_pir(
    pir_layout: PirLayout,
    endpoint_url: &str,
    transport: Arc<dyn Transport>,
) -> Result<PirClient, VotingError> {
    let endpoint = normalize_endpoint_url(endpoint_url);
    if endpoint.is_empty() {
        return Err(VotingError::InvalidInput {
            message: "PIR endpoint URL must not be empty".to_string(),
        });
    }
    let expected_layout = negotiated_pir_layout(pir_layout)?;
    PirClient::with_transport(&endpoint, expected_layout, transport)
        .await
        .map_err(map_pir_connect_error)
}

fn normalize_endpoint_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn map_pir_connect_error(err: impl std::fmt::Display) -> VotingError {
    let detail = err.to_string();
    let message = format!("PIR connect failed: {detail}");
    // Config/server layout or poly_len disagreement is a caller/config
    // incompatibility, not an unexpected internal failure.
    if detail.contains("PIR layout mismatch") || detail.contains("PIR poly_len mismatch") {
        VotingError::InvalidInput { message }
    } else {
        VotingError::Internal { message }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        resolve_dynamic_voting_config, resolve_static_voting_config, ResolveVotingConfigOptions,
    };
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use pir_types::{
        RootInfo, YpirScenario, COMPILED_PIR_LAYOUT, DATASET_VERSION, DEFAULT_YPIR_POLY_LEN,
        NULLIFIER_POOL, PIR_DEPTH, TIER0_LAYERS,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const ROUND_ID: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const TEST_POLY_LEN: u32 = DEFAULT_YPIR_POLY_LEN as u32;

    #[derive(Clone, Copy, Debug)]
    enum AssetOutcome {
        Valid,
        Unavailable,
        Malformed,
        Oversized,
        InvalidProtocol,
    }

    struct AssetTransport {
        inner: RecordingTransport,
        outcomes: [AssetOutcome; 2],
        slow: usize,
        completed: Mutex<Vec<usize>>,
    }
    impl Transport for AssetTransport {
        fn get<'a>(&'a self, url: &'a str) -> TransportFuture<'a> {
            Box::pin(async move {
                let response = self.inner.get(url).await?;
                let index = if url == "https://first/tier0" {
                    0
                } else if url == "https://first/params/tier1" {
                    1
                } else {
                    return Ok(response);
                };
                if index == self.slow {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                self.completed.lock().unwrap().push(index);
                match self.outcomes[index] {
                    AssetOutcome::Valid => Ok(response),
                    AssetOutcome::Unavailable => Ok(TransportResponse {
                        status: 503,
                        headers: vec![],
                        body: b"untrusted diagnostic".to_vec(),
                    }),
                    AssetOutcome::Malformed => Ok(TransportResponse {
                        body: vec![0],
                        ..response
                    }),
                    AssetOutcome::Oversized => {
                        Err(crate::platform::TransportError::ResponseTooLarge {
                            limit: 64 * 1024 * 1024,
                        }
                        .into())
                    }
                    AssetOutcome::InvalidProtocol => {
                        let mut response = response;
                        if index == 0 {
                            response.body[0] = 1;
                        } else {
                            let mut params: serde_json::Value =
                                serde_json::from_slice(&response.body).unwrap();
                            params["num_items"] = json!(0);
                            response.body = serde_json::to_vec(&params).unwrap();
                        }
                        Ok(response)
                    }
                }
            })
        }
        fn post<'a>(&'a self, _: &'a str, _: Vec<u8>) -> TransportFuture<'a> {
            panic!("public asset validation must not query")
        }
    }

    #[tokio::test]
    async fn ordered_pir_assets_permanent_failure_dominates_in_both_completion_orders() {
        for permanent in [
            AssetOutcome::Malformed,
            AssetOutcome::Oversized,
            AssetOutcome::InvalidProtocol,
        ] {
            for index in 0..2 {
                for slow in 0..2 {
                    let mut outcomes = [AssetOutcome::Unavailable; 2];
                    outcomes[index] = permanent;
                    let transport = Arc::new(AssetTransport {
                        inner: RecordingTransport::matching_root(),
                        outcomes,
                        slow,
                        completed: Mutex::new(vec![]),
                    });
                    let result = OrderedSnapshotPir::connect(
                        compiled_wallet_layout(),
                        vec!["https://first".into(), "https://second".into()],
                        transport.clone(),
                        crate::Network::Testnet,
                        &[0; 32],
                        100,
                    )
                    .await;
                    assert!(
                        result.is_err(),
                        "masked {permanent:?} at {index}, delayed {slow}"
                    );
                    assert_eq!(*transport.completed.lock().unwrap(), [1 - slow, slow]);
                    assert_eq!(transport.inner.count_hits("/root"), 1);
                    let error = result.err().unwrap().to_string();
                    assert!(!error.contains("untrusted") && !error.contains("https://"));
                }
            }
        }
    }

    #[tokio::test]
    async fn ordered_pir_assets_cancellation_drops_both_pending_loads() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Active(Arc<AtomicUsize>);
        impl Drop for Active {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        struct PendingAssets(RecordingTransport, Arc<AtomicUsize>);
        impl Transport for PendingAssets {
            fn get<'a>(&'a self, url: &'a str) -> TransportFuture<'a> {
                Box::pin(async move {
                    let response = self.0.get(url).await?;
                    if !url.ends_with("/root") {
                        self.1.fetch_add(1, Ordering::SeqCst);
                        let _active = Active(self.1.clone());
                        std::future::pending::<()>().await;
                    }
                    Ok(response)
                })
            }
            fn post<'a>(&'a self, _: &'a str, _: Vec<u8>) -> TransportFuture<'a> {
                panic!("unexpected private query")
            }
        }
        let active = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(PendingAssets(
            RecordingTransport::matching_root(),
            active.clone(),
        ));
        let mut connecting = Box::pin(OrderedSnapshotPir::connect(
            compiled_wallet_layout(),
            vec!["https://first".into(), "https://second".into()],
            transport.clone(),
            crate::Network::Testnet,
            &[0; 32],
            100,
        ));
        assert!(futures_util::poll!(connecting.as_mut()).is_pending());
        assert_eq!(active.load(Ordering::SeqCst), 2);
        drop(connecting);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(transport.0.count_hits("/root"), 1);
    }

    #[tokio::test]
    async fn ordered_pir_assets_both_retryable_and_valid_success() {
        for outcomes in [
            [AssetOutcome::Unavailable; 2],
            [AssetOutcome::Valid; 2],
            [AssetOutcome::Valid, AssetOutcome::Unavailable],
            [AssetOutcome::Unavailable, AssetOutcome::Valid],
        ] {
            for slow in 0..2 {
                let transport = Arc::new(AssetTransport {
                    inner: RecordingTransport::matching_root(),
                    outcomes,
                    slow,
                    completed: Mutex::new(vec![]),
                });
                assert!(OrderedSnapshotPir::connect(
                    compiled_wallet_layout(),
                    vec!["https://first".into(), "https://second".into()],
                    transport.clone(),
                    crate::Network::Testnet,
                    &[0; 32],
                    100,
                )
                .await
                .is_ok());
                assert_eq!(*transport.completed.lock().unwrap(), [1 - slow, slow]);
                let valid = outcomes.iter().all(|v| matches!(v, AssetOutcome::Valid));
                assert_eq!(
                    transport.inner.count_hits("/root"),
                    if valid { 1 } else { 2 }
                );
            }
        }
    }

    #[test]
    fn ordered_pir_invalid_assembled_proof_blocks_retryable_sibling() {
        for invalid_first in [false, true] {
            let proof = ImtProofData {
                root: 0.into(),
                nf_bounds: [0.into(), 1.into(), 3.into()],
                leaf_pos: 0,
                path: [0.into(); 29],
            };
            let mut results = vec![Err(RetryableTransportError.into()), Ok(proof)];
            if invalid_first {
                results.reverse();
            }
            let error = verify_pir_batch(&[2.into(), 2.into()], results).unwrap_err();
            assert!(!error.is::<RetryableTransportError>());
            assert_eq!(error.to_string(), "PIR proof verification failed");
        }
    }

    #[test]
    fn ordered_pir_accepts_valid_proofs_in_request_order() {
        use voting_circuits::delegation::{ImtProvider, SpacedLeafImtProvider};
        let imt = SpacedLeafImtProvider::new();
        let nullifiers = [1.into(), 4.into()];
        let results = nullifiers
            .iter()
            .map(|&nf| {
                let proof = imt.non_membership_proof(nf).unwrap();
                Ok(ImtProofData {
                    root: proof.root,
                    nf_bounds: proof.nf_bounds,
                    leaf_pos: proof.leaf_pos,
                    path: proof.path,
                })
            })
            .collect();
        let proofs = verify_pir_batch(&nullifiers, results).unwrap();
        assert_eq!(proofs.len(), 2);
        for (proof, nf) in proofs.iter().zip(nullifiers) {
            assert!(proof.verify(nf));
        }
    }

    #[tokio::test]
    async fn ordered_pir_revalidates_replacement_before_success() {
        let transport = Arc::new(RecordingTransport::matching_root());
        let mut client = OrderedSnapshotPir::connect(
            compiled_wallet_layout(),
            vec!["https://first".into(), "https://second".into()],
            transport.clone(),
            crate::Network::Testnet,
            &[0; 32],
            100,
        )
        .await
        .unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = client
            .fetch_with(|_| {
                let attempt = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let roots = transport.count_hits("/root");
                Box::pin(async move {
                    assert_eq!(roots, attempt + 1, "query before replacement handshake");
                    if attempt == 0 {
                        Err(pir_client::RetryableTransportError.into())
                    } else {
                        Ok(17u32)
                    }
                })
            })
            .await
            .unwrap();
        assert_eq!(result, 17);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(transport.count_hits("/tier0"), 2);
        assert_eq!(transport.count_hits("/params/tier1"), 2);
        let hits = transport.urls.lock().unwrap();
        assert!(hits[..3]
            .iter()
            .all(|url| url.starts_with("https://first/")));
        assert!(hits[3..]
            .iter()
            .all(|url| url.starts_with("https://second/")));
    }

    #[tokio::test]
    async fn ordered_pir_skips_unavailable_replacement_handshake_without_querying_it() {
        let transport = Arc::new(RecordingTransport::matching_root());
        transport.gets.lock().unwrap().insert(
            "https://second/root".into(),
            TransportResponse {
                status: 503,
                headers: vec![],
                body: b"untrusted diagnostic".to_vec(),
            },
        );
        let mut client = OrderedSnapshotPir::connect(
            compiled_wallet_layout(),
            vec![
                "https://first".into(),
                "https://second".into(),
                "https://third".into(),
            ],
            transport.clone(),
            crate::Network::Testnet,
            &[0; 32],
            100,
        )
        .await
        .unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        client
            .fetch_with(|_| {
                let attempt = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let roots = transport.count_hits("/root");
                Box::pin(async move {
                    if attempt == 0 {
                        Err(RetryableTransportError.into())
                    } else {
                        assert_eq!(roots, 3);
                        Ok(())
                    }
                })
            })
            .await
            .unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        let urls = transport.urls.lock().unwrap();
        assert_eq!(
            urls.iter()
                .filter(|url| url.starts_with("https://second/"))
                .count(),
            1
        );
        assert!(urls[4..]
            .iter()
            .all(|url| url.starts_with("https://third/")));
    }

    #[tokio::test]
    async fn ordered_pir_exhausts_transport_failures_but_stops_on_protocol_errors() {
        for retryable in [true, false] {
            let transport = Arc::new(RecordingTransport::matching_root());
            let mut client = OrderedSnapshotPir::connect(
                compiled_wallet_layout(),
                vec!["https://first".into(), "https://second".into()],
                transport.clone(),
                crate::Network::Testnet,
                &[0; 32],
                100,
            )
            .await
            .unwrap();
            let error = client
                .fetch_with::<()>(|_| {
                    Box::pin(async move {
                        if retryable {
                            Err(pir_client::RetryableTransportError.into())
                        } else {
                            Err(anyhow::anyhow!("private protocol material must not escape"))
                        }
                    })
                })
                .await
                .unwrap_err();
            assert_eq!(transport.count_hits("/root"), if retryable { 2 } else { 1 });
            assert!(!error.to_string().contains("private"));
            assert!(!error.to_string().contains("https://"));
            if retryable {
                assert!(error.to_string().contains("2 endpoints"));
            }
        }
    }

    #[tokio::test]
    async fn ordered_pir_rejects_changed_identity_before_replacement_query() {
        let transport = Arc::new(RecordingTransport::matching_root());
        let mut client = OrderedSnapshotPir::connect(
            compiled_wallet_layout(),
            vec![
                "https://first".into(),
                "https://second".into(),
                "https://third".into(),
            ],
            transport.clone(),
            crate::Network::Testnet,
            &[0; 32],
            100,
        )
        .await
        .unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let error = client
            .fetch_with::<()>(|_| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut gets = transport.gets.lock().unwrap();
                let root = gets.get_mut("/root").unwrap();
                let mut value: serde_json::Value = serde_json::from_slice(&root.body).unwrap();
                value["height"] = json!(101);
                root.body = serde_json::to_vec(&value).unwrap();
                Box::pin(async { Err(pir_client::RetryableTransportError.into()) })
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("identity mismatch"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(transport.count_hits("/root"), 2);
        assert_eq!(transport.count_hits("/tier0"), 1);
    }

    #[derive(Default)]
    struct RecordingTransport {
        gets: Mutex<HashMap<String, TransportResponse>>,
        hits: Mutex<Vec<String>>,
        posts: Mutex<usize>,
        urls: Mutex<Vec<String>>,
    }

    impl RecordingTransport {
        fn matching_root() -> Self {
            let transport = Self::default();
            transport.insert_matching_assets(COMPILED_PIR_LAYOUT, DEFAULT_YPIR_POLY_LEN);
            transport
        }

        fn with_root_layout(layout: NegotiatedPirLayout) -> Self {
            let transport = Self::default();
            transport.insert_matching_assets(layout, layout.poly_len);
            transport
        }

        fn insert_matching_assets(&self, layout: NegotiatedPirLayout, poly_len: usize) {
            let rows = layout.tier1_rows().unwrap();
            let row_bytes = layout.tier1_row_bytes().unwrap();
            let item_bits = layout.tier1_item_bits().unwrap();
            let tier0_bytes = layout.tier0_bytes().unwrap();
            let root = RootInfo {
                zcash_network: pir_types::ZcashNetwork::Test,
                nullifier_pool: NULLIFIER_POOL.to_owned(),
                dataset_version: DATASET_VERSION,
                circuit_root: hex::encode([0u8; 32]),
                pir_root: hex::encode([0u8; 32]),
                num_ranges: 1,
                pir_layout: layout,
                pir_depth: layout.pir_depth,
                tier1_rows: rows,
                tier1_row_bytes: row_bytes,
                height: Some(100),
            };
            let tier1 = YpirScenario {
                num_items: rows,
                item_size_bits: item_bits,
                poly_len,
            };
            let mut gets = self.gets.lock().unwrap();
            gets.insert("/tier0".to_string(), response(vec![0; tier0_bytes]));
            gets.insert(
                "/params/tier1".to_string(),
                response(serde_json::to_vec(&tier1).unwrap()),
            );
            gets.insert(
                "/root".to_string(),
                response(serde_json::to_vec(&root).unwrap()),
            );
        }

        fn count_hits(&self, path: &str) -> usize {
            self.hits
                .lock()
                .unwrap()
                .iter()
                .filter(|hit| hit.as_str() == path)
                .count()
        }

        fn post_count(&self) -> usize {
            *self.posts.lock().unwrap()
        }
    }

    impl Transport for RecordingTransport {
        fn get<'a>(&'a self, url: &'a str) -> TransportFuture<'a> {
            Box::pin(async move {
                self.urls.lock().unwrap().push(url.to_owned());
                let path = request_path(url);
                self.hits.lock().unwrap().push(path.to_owned());
                let gets = self.gets.lock().unwrap();
                gets.get(url)
                    .or_else(|| gets.get(path))
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("unexpected GET {path}"))
            })
        }

        fn post<'a>(&'a self, url: &'a str, _body: Vec<u8>) -> TransportFuture<'a> {
            Box::pin(async move {
                let path = request_path(url);
                self.hits.lock().unwrap().push(path.to_owned());
                *self.posts.lock().unwrap() += 1;
                Err(anyhow::anyhow!(
                    "unexpected POST {path}; layout handshake must fail before queries"
                ))
            })
        }
    }

    fn response(body: Vec<u8>) -> TransportResponse {
        TransportResponse {
            status: 200,
            headers: Vec::new(),
            body,
        }
    }

    fn request_path(url: &str) -> &str {
        let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        without_scheme
            .find('/')
            .map(|idx| &without_scheme[idx..])
            .unwrap_or("/")
    }

    fn expect_connect_err(result: Result<PirClientBlocking, VotingError>) -> VotingError {
        match result {
            Ok(_) => panic!("expected PIR connect failure"),
            Err(err) => err,
        }
    }

    fn compiled_wallet_layout() -> PirLayout {
        PirLayout {
            pir_depth: u32::try_from(COMPILED_PIR_LAYOUT.pir_depth).unwrap(),
            tier0_layers: u32::try_from(COMPILED_PIR_LAYOUT.tier0_layers).unwrap(),
            tier1_layers: u32::try_from(COMPILED_PIR_LAYOUT.tier1_layers).unwrap(),
            poly_len: TEST_POLY_LEN,
        }
    }

    #[test]
    fn connect_rejects_unknown_layout_before_transport() {
        let transport = Arc::new(RecordingTransport::matching_root());

        let err = expect_connect_err(connect_pir_blocking(
            PirLayout::UNKNOWN,
            "https://pir.example.com",
            transport.clone(),
        ));

        assert!(matches!(err, VotingError::InvalidInput { .. }), "{err}");
        assert!(err.to_string().contains("pir_layout is unknown"), "{err}");
        assert_eq!(transport.count_hits("/root"), 0);
        assert_eq!(transport.post_count(), 0);
    }

    #[test]
    fn connect_rejects_unusable_ypir_layouts_before_transport() {
        for (layout, expected) in [
            (
                PirLayout {
                    pir_depth: 19,
                    tier0_layers: 10,
                    tier1_layers: 9,
                    poly_len: 4096,
                },
                "Tier 1 rows 1024 below YPIR minimum 2048",
            ),
            (
                PirLayout {
                    pir_depth: 19,
                    tier0_layers: 14,
                    tier1_layers: 5,
                    poly_len: 4096,
                },
                "Tier 1 item bits 24576 below YPIR minimum 28672",
            ),
            (
                PirLayout {
                    pir_depth: 19,
                    tier0_layers: 0,
                    tier1_layers: 19,
                    poly_len: 4096,
                },
                "PIR layout tiers must be non-zero",
            ),
        ] {
            let transport = Arc::new(RecordingTransport::matching_root());
            let err = expect_connect_err(connect_pir_blocking(
                layout,
                "https://pir.example.com",
                transport.clone(),
            ));

            assert!(matches!(err, VotingError::InvalidInput { .. }), "{err}");
            assert!(err.to_string().contains(expected), "{err}");
            assert_eq!(transport.count_hits("/tier0"), 0);
            assert_eq!(transport.count_hits("/params/tier1"), 0);
            assert_eq!(transport.count_hits("/root"), 0);
            assert_eq!(transport.post_count(), 0);
        }
    }

    #[test]
    fn connect_rejects_server_layout_mismatch_before_query() {
        let mismatched = NegotiatedPirLayout {
            pir_depth: PIR_DEPTH,
            tier0_layers: TIER0_LAYERS - 1,
            tier1_layers: COMPILED_PIR_LAYOUT.tier1_layers + 1,
            poly_len: DEFAULT_YPIR_POLY_LEN,
        };
        let transport = Arc::new(RecordingTransport::with_root_layout(mismatched));

        let err = expect_connect_err(connect_pir_blocking(
            compiled_wallet_layout(),
            "https://pir.example.com/",
            transport.clone(),
        ));

        assert!(matches!(err, VotingError::InvalidInput { .. }), "{err}");
        assert!(err.to_string().contains("PIR layout mismatch"), "{err}");
        assert_eq!(transport.post_count(), 0);
    }

    #[test]
    fn connect_rejects_config_layout_mismatch_before_query() {
        let layout = PirLayout {
            pir_depth: 18,
            tier0_layers: 11,
            tier1_layers: 7,
            poly_len: 4096,
        };
        let transport = Arc::new(RecordingTransport::matching_root());

        let err = expect_connect_err(connect_pir_blocking(
            layout,
            "https://pir.example.com",
            transport.clone(),
        ));

        assert!(matches!(err, VotingError::InvalidInput { .. }), "{err}");
        assert!(err.to_string().contains("PIR layout mismatch"), "{err}");
        assert_eq!(transport.count_hits("/root"), 1);
        assert_eq!(transport.count_hits("/tier0"), 0);
        assert_eq!(transport.count_hits("/params/tier1"), 0);
        assert_eq!(transport.post_count(), 0);
    }

    #[test]
    fn connect_succeeds_for_matching_alternate_layouts() {
        for (pir_depth, tier0_layers, tier1_layers, poly_len) in
            [(19, 11, 8, 2048), (19, 13, 6, 4096), (20, 12, 8, 4096)]
        {
            let negotiated = NegotiatedPirLayout {
                pir_depth,
                tier0_layers,
                tier1_layers,
                poly_len,
            };
            let wallet_layout = PirLayout {
                pir_depth: u32::try_from(negotiated.pir_depth).unwrap(),
                tier0_layers: u32::try_from(negotiated.tier0_layers).unwrap(),
                tier1_layers: u32::try_from(negotiated.tier1_layers).unwrap(),
                poly_len: u32::try_from(negotiated.poly_len).unwrap(),
            };
            let transport = Arc::new(RecordingTransport::with_root_layout(negotiated));

            let _client = connect_pir_blocking(
                wallet_layout,
                "https://pir.example.com/",
                transport.clone(),
            )
            .unwrap_or_else(|err| {
                panic!(
                    "matching layout {pir_depth}/{tier0_layers}/{tier1_layers}/{poly_len} failed: {err}"
                )
            });

            assert_eq!(transport.count_hits("/tier0"), 1);
            assert_eq!(transport.count_hits("/params/tier1"), 1);
            assert_eq!(transport.count_hits("/root"), 1);
            assert_eq!(transport.post_count(), 0);
        }
    }

    #[test]
    fn connect_rejects_poly_len_mismatch_before_query() {
        // poly_len is part of PirLayout, so /root rejects before /params/tier1.
        let transport = Arc::new(RecordingTransport::matching_root());

        let err = expect_connect_err(connect_pir_blocking(
            PirLayout {
                poly_len: 2048,
                ..compiled_wallet_layout()
            },
            "https://pir.example.com",
            transport.clone(),
        ));

        assert!(matches!(err, VotingError::InvalidInput { .. }), "{err}");
        assert!(err.to_string().contains("PIR layout mismatch"), "{err}");
        assert_eq!(transport.count_hits("/root"), 1);
        assert_eq!(transport.count_hits("/params/tier1"), 0);
        assert_eq!(transport.post_count(), 0);
    }

    #[test]
    fn connect_succeeds_when_config_and_server_layouts_match() {
        let transport = Arc::new(RecordingTransport::matching_root());

        let _client = connect_pir_blocking(
            compiled_wallet_layout(),
            "https://pir.example.com/",
            transport.clone(),
        )
        .unwrap();

        assert_eq!(transport.count_hits("/tier0"), 1);
        assert_eq!(transport.count_hits("/params/tier1"), 1);
        assert_eq!(transport.count_hits("/root"), 1);
        assert_eq!(transport.post_count(), 0);
    }

    #[test]
    fn connect_accepts_caller_chosen_endpoint_url() {
        let transport = Arc::new(RecordingTransport::matching_root());

        let _client = connect_pir_blocking(
            compiled_wallet_layout(),
            "https://unlisted-pir.example.com/",
            transport.clone(),
        )
        .unwrap();

        assert_eq!(transport.count_hits("/tier0"), 1);
        assert_eq!(transport.count_hits("/params/tier1"), 1);
        assert_eq!(transport.count_hits("/root"), 1);
        assert_eq!(transport.post_count(), 0);
    }

    #[test]
    fn connect_rejects_empty_url() {
        let transport = Arc::new(RecordingTransport::matching_root());
        let err = expect_connect_err(connect_pir_blocking(
            compiled_wallet_layout(),
            "   ",
            transport.clone(),
        ));
        assert!(err.to_string().contains("must not be empty"), "{err}");
        assert_eq!(transport.count_hits("/root"), 0);
    }

    #[test]
    fn signed_static_to_dynamic_to_pir_connect_handshake() {
        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let pubkey = signing_key.verifying_key().to_bytes();
        let static_bytes = json!({
            "static_config_version": 1,
            "dynamic_config_url": "https://example.com/dynamic.json",
            "trusted_keys": [{
                "key_id": "k1",
                "alg": "ed25519",
                "pubkey": BASE64.encode(pubkey),
                "notes": null
            }]
        })
        .to_string()
        .into_bytes();
        let source = "https://example.com/static.json";
        let resolved_static = resolve_static_voting_config(source, &static_bytes).unwrap();

        let ea_pk = [7u8; 32];
        let wallet_layout = compiled_wallet_layout();
        let mut preimage = b"zcash-shielded-vote:round-auth:v2".to_vec();
        preimage.extend_from_slice(&hex::decode(ROUND_ID).unwrap());
        preimage.extend_from_slice(&ea_pk);
        preimage.extend_from_slice(&wallet_layout.pir_depth.to_le_bytes());
        preimage.extend_from_slice(&wallet_layout.tier0_layers.to_le_bytes());
        preimage.extend_from_slice(&wallet_layout.tier1_layers.to_le_bytes());
        preimage.extend_from_slice(&TEST_POLY_LEN.to_le_bytes());
        let sig = signing_key.sign(&preimage).to_bytes();
        let dynamic_bytes = json!({
            "config_version": 1,
            "vote_servers": [{"url": "https://vote.example.com", "label": "vote"}],
            "pir_endpoints": [{"url": "https://pir.example.com", "label": "pir"}],
            "pir_layout": {
                "pir_depth": COMPILED_PIR_LAYOUT.pir_depth,
                "tier0_layers": COMPILED_PIR_LAYOUT.tier0_layers,
                "tier1_layers": COMPILED_PIR_LAYOUT.tier1_layers,
                "poly_len": TEST_POLY_LEN
            },
            "supported_versions": {
                "pir": ["v0"],
                "vote_protocol": "v0",
                "tally": "v0",
                "vote_server": "v1"
            },
            "rounds": {
                ROUND_ID: {
                    "auth_version": 2,
                    "ea_pk": BASE64.encode(ea_pk),
                    "signatures": [{
                        "key_id": "k1",
                        "alg": "ed25519",
                        "sig": BASE64.encode(sig)
                    }]
                }
            }
        })
        .to_string()
        .into_bytes();
        let resolved = resolve_dynamic_voting_config(
            resolved_static,
            &dynamic_bytes,
            ResolveVotingConfigOptions::default(),
        )
        .unwrap();

        assert_eq!(resolved.pir_layout, compiled_wallet_layout());
        assert_eq!(resolved.pir_endpoints[0].url, "https://pir.example.com");

        let matching = Arc::new(RecordingTransport::matching_root());
        let _client = connect_pir_blocking(
            resolved.pir_layout,
            "https://pir.example.com",
            matching.clone(),
        )
        .unwrap();
        assert_eq!(matching.post_count(), 0);

        let mismatched = Arc::new(RecordingTransport::with_root_layout(NegotiatedPirLayout {
            pir_depth: 18,
            tier0_layers: 11,
            tier1_layers: 7,
            poly_len: DEFAULT_YPIR_POLY_LEN,
        }));
        let err = expect_connect_err(connect_pir_blocking(
            resolved.pir_layout,
            "https://pir.example.com",
            mismatched.clone(),
        ));
        assert!(matches!(err, VotingError::InvalidInput { .. }), "{err}");
        assert!(err.to_string().contains("PIR layout mismatch"), "{err}");
        assert_eq!(mismatched.post_count(), 0);
    }
}
