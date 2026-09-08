use futures_util::future::LocalBoxFuture;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use zcash_voting::{
    helper::client::local::{CheckpointHook, LocalHelperClient, Timer},
    platform::{HttpTransport, OwnedHttpRequest, OwnedHttpResponse, TransportFuture},
    VotingError,
};

struct Clock(tokio::time::Instant);
impl Timer for Clock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, delay: Duration) -> LocalBoxFuture<'_, ()> {
        Box::pin(tokio::time::sleep(delay))
    }
}
struct Checkpoint;
impl CheckpointHook for Checkpoint {
    fn checkpoint(&self) -> LocalBoxFuture<'_, Result<(), VotingError>> {
        Box::pin(async { panic!("preflight must not checkpoint") })
    }
}
struct Probe {
    types: Vec<Option<&'static str>>,
    delays: Vec<Duration>,
    dropped: Arc<AtomicUsize>,
    calls: AtomicUsize,
}
impl HttpTransport for Probe {
    fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
        assert_eq!(request.method, "GET");
        assert_eq!(request.max_response_bytes, 256 * 1024);
        assert_eq!(request.timeout_ms, 30_000);
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let content_type = self.types[index].map(str::to_owned);
        let delay = self.delays[index];
        struct OnDrop(Arc<AtomicUsize>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let guard = OnDrop(self.dropped.clone());
        Box::pin(async move {
            let _guard = guard;
            tokio::time::sleep(delay).await;
            Ok(OwnedHttpResponse {
                status: 200,
                body: br#"{"status":"ok"}"#.to_vec(),
                content_type,
            })
        })
    }
}

#[tokio::test(start_paused = true)]
async fn helper_preflight_uses_actual_metadata_and_preserves_full_roster() {
    let clock = Clock(tokio::time::Instant::now());
    let transport = Probe {
        types: vec![
            None,
            Some("text/html"),
            Some("application/json; charset=utf-8"),
            Some("application/json"),
        ],
        delays: vec![Duration::ZERO; 4],
        dropped: Default::default(),
        calls: AtomicUsize::new(0),
    };
    let urls = (0..4)
        .map(|index| format!("https://helper-{index}.example"))
        .collect::<Vec<_>>();
    let client = LocalHelperClient::new(&transport, &clock, &Checkpoint, &|| 100, 200);
    let fleet = client.preflight_fleet(&urls).await.unwrap();
    assert_eq!(fleet.configured_server_urls(), urls);
    assert_eq!(fleet.ready_server_count(), 2);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 4);
    assert_eq!(transport.dropped.load(Ordering::SeqCst), 4);
}

#[tokio::test(start_paused = true)]
async fn helper_preflight_keeps_soft_hard_windows_and_drops_pending_probes() {
    for (second_ready, expected_seconds, expected_ready) in [(0, 2, 2), (3, 3, 2), (60, 30, 1)] {
        let clock = Clock(tokio::time::Instant::now());
        let transport = Probe {
            types: vec![Some("application/json"); 4],
            delays: [0, second_ready, 60, 60].map(Duration::from_secs).to_vec(),
            dropped: Default::default(),
            calls: AtomicUsize::new(0),
        };
        let urls = (0..4)
            .map(|index| format!("https://helper-{index}.example"))
            .collect::<Vec<_>>();
        let client = LocalHelperClient::new(&transport, &clock, &Checkpoint, &|| 100, 200);
        let fleet = client.preflight_fleet(&urls).await.unwrap();
        assert_eq!(fleet.ready_server_count(), expected_ready);
        assert_eq!(clock.now().as_secs(), expected_seconds);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 4);
        assert_eq!(transport.dropped.load(Ordering::SeqCst), 4);
    }
}

#[tokio::test(start_paused = true)]
async fn helper_preflight_rejects_invalid_fleet_before_transport_construction() {
    let clock = Clock(tokio::time::Instant::now());
    let transport = Probe {
        types: vec![],
        delays: vec![],
        dropped: Default::default(),
        calls: AtomicUsize::new(0),
    };
    let client = LocalHelperClient::new(&transport, &clock, &Checkpoint, &|| 100, 200);
    for urls in [
        vec![],
        vec![
            "https://helper.example".into(),
            "https://helper.example/".into(),
        ],
        vec!["ftp://helper.example".into()],
    ] {
        assert!(client.preflight_fleet(&urls).await.is_err());
    }
    assert_eq!(transport.calls.load(Ordering::SeqCst), 0);
}
