use super::*;
use crate::platform::{OwnedHttpResponse, TransportFuture};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

struct Clock(tokio::time::Instant);

#[tokio::test]
async fn local_borrowed_clients_share_advisory_health_without_persistence() {
    let clock = Clock(tokio::time::Instant::now());
    let transport = Transport::default();
    let hook = Hook {
        delay: Duration::ZERO,
        action: &|| (),
        fail: false,
    };
    let health = HelperHealth::default();
    let urls: Vec<String> = vec![
        "https://helper-1.example".into(),
        "https://helper-2.example".into(),
    ];
    {
        let client = LocalHelperClient::new(&transport, &clock, &hook, &|| 100, 200)
            .with_health(health.clone());
        for _ in 0..3 {
            client.health().record_failure(&urls[0], 100);
        }
    }
    let client =
        LocalHelperClient::new(&transport, &clock, &hook, &|| 100, 200).with_health(health);
    assert_eq!(
        client.health().candidate_servers(&urls, 100),
        [urls[1].clone(), urls[0].clone()]
    );
}
impl Timer for Clock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn sleep(&self, delay: Duration) -> LocalBoxFuture<'_, ()> {
        Box::pin(tokio::time::sleep(delay))
    }
}
struct Hook<'a> {
    delay: Duration,
    action: &'a dyn Fn(),
    fail: bool,
}
impl CheckpointHook for Hook<'_> {
    fn checkpoint(&self) -> LocalBoxFuture<'_, Result<(), VotingError>> {
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            (self.action)();
            if self.fail {
                Err(crate::helper::runtime::checkpoint_error())
            } else {
                Ok(())
            }
        })
    }
}
#[derive(Default)]
struct Transport {
    calls: AtomicUsize,
    requests: Mutex<Vec<OwnedHttpRequest>>,
    delay: Duration,
    fail: bool,
    confirmed: bool,
}
impl HttpTransport for Transport {
    fn execute(&self, request: OwnedHttpRequest) -> TransportFuture {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(request);
        let delay = self.delay;
        let fail = self.fail;
        let confirmed = self.confirmed;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            if fail {
                Err(TransportError::Definite)
            } else {
                Ok(OwnedHttpResponse {
                    status: 200,
                    body: if confirmed {
                        br#"{"status":"confirmed"}"#.as_slice()
                    } else {
                        br#"{"status":"accepted"}"#.as_slice()
                    }
                    .to_vec(),
                    content_type: Some("application/json".into()),
                })
            }
        })
    }
}

#[tokio::test(start_paused = true)]
async fn local_poll_abort_preserves_boundary_confirmations_after_cancellation() {
    use crate::helper::runtime::PollSet;
    let clock = Clock(tokio::time::Instant::now());
    let transport = Transport {
        delay: Duration::from_secs(10),
        confirmed: true,
        ..Default::default()
    };
    let hook = Hook {
        delay: Duration::ZERO,
        action: &|| (),
        fail: false,
    };
    let mut client = LocalHelperClient::new(&transport, &clock, &hook, &|| 100, 200);
    client.config.request_timeout = Duration::from_secs(12);
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let deadline = client.now() + Duration::from_secs(10);
    let mut polls = client.new_polls();
    for index in 0..2 {
        client.start_poll(
            &mut polls,
            format!("https://helper-{index}.example"),
            "01".repeat(32),
            "02".repeat(32),
            100,
            deadline,
            cancelled.clone(),
        );
    }
    assert!(polls.try_join_next().is_none());
    tokio::time::advance(Duration::from_secs(10)).await;
    cancelled.store(true, Ordering::SeqCst);
    polls.abort_all();
    for _ in 0..2 {
        assert!(matches!(
            polls.try_join_next(),
            Some(Ok((_, Ok(ShareStatus::Confirmed))))
        ));
    }
    assert!(polls.is_empty());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn local_post_revalidates_after_checkpoint_before_eager_construction() {
    for mode in 0..5 {
        let clock = Clock(tokio::time::Instant::now());
        let transport = Transport::default();
        let cancelled = AtomicBool::new(false);
        let generation_current = std::cell::Cell::new(true);
        let wall = std::cell::Cell::new(100);
        let action = || match mode {
            1 => cancelled.store(true, Ordering::SeqCst),
            2 => generation_current.set(false),
            3 => wall.set(200),
            _ => (),
        };
        let hook = Hook {
            delay: Duration::from_secs(if mode == 0 { 61 } else { 0 }),
            action: &action,
            fail: mode == 4,
        };
        let now = || wall.get();
        let client = LocalHelperClient::new(&transport, &clock, &hook, &now, 200);
        let result = client
            .submit(
                "https://helper.example",
                &super::super::tests::valid_share_json(),
                100,
                &|| cancelled.load(Ordering::SeqCst),
                Duration::from_secs(5),
                client.now() + Duration::from_secs(60),
                &|| generation_current.get(),
            )
            .await;
        assert!(result.is_err(), "mode {mode}");
        assert_eq!(transport.calls.load(Ordering::SeqCst), 0, "mode {mode}");
        assert!(!client.has_dispatched());
    }
}

#[tokio::test(start_paused = true)]
async fn local_status_does_not_start_retry_after_outer_poll_deadline() {
    let clock = Clock(tokio::time::Instant::now());
    let transport = Transport {
        delay: Duration::from_millis(9900),
        fail: true,
        ..Default::default()
    };
    let hook = Hook {
        delay: Duration::ZERO,
        action: &|| (),
        fail: false,
    };
    let mut client = LocalHelperClient::new(&transport, &clock, &hook, &|| 100, 200);
    client.config.request_timeout = Duration::from_secs(12);
    let result = client
        .status(
            "https://helper.example",
            &"01".repeat(32),
            &"02".repeat(32),
            100,
            client.now() + Duration::from_secs(10),
            &|| false,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert!(clock.now() <= Duration::from_secs(10));
    assert_eq!(transport.requests.lock().unwrap()[0].timeout_ms, 10_000);
}
