//! Local futures over the same helper wire, retry, and durable tracking policy.
use super::*;
use crate::helper::runtime::{timeout_at, LocalPolls, Moment, RuntimeClient};
use crate::platform::{HttpTransport, OwnedHttpRequest, RequestKind, TransportError};
use futures_util::{future::LocalBoxFuture, stream::FuturesUnordered, StreamExt};

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "local_tests.rs"]
mod tests;

/// Host-owned local monotonic timer. No browser object enters a worker.
pub trait Timer: Send + Sync {
    fn now(&self) -> Duration;
    fn sleep(&self, delay: Duration) -> LocalBoxFuture<'_, ()>;
}

/// Flushes sensitive owner-bound SQLite bytes before a network effect.
pub trait CheckpointHook {
    fn checkpoint(&self) -> LocalBoxFuture<'_, Result<(), VotingError>>;
}

pub struct LocalHelperClient<'a> {
    transport: &'a dyn HttpTransport,
    pub(crate) timer: &'a dyn Timer,
    checkpoint: &'a dyn CheckpointHook,
    wall_time: &'a dyn Fn() -> u64,
    vote_end: u64,
    health: HelperHealth,
    config: HelperClientConfig,
    checkpoint_failed: std::cell::Cell<bool>,
    checkpoint_lock: futures_util::lock::Mutex<()>,
    dispatched: std::cell::Cell<bool>,
}

impl<'a> LocalHelperClient<'a> {
    pub fn new(
        transport: &'a dyn HttpTransport,
        timer: &'a dyn Timer,
        checkpoint: &'a dyn CheckpointHook,
        wall_time: &'a dyn Fn() -> u64,
        vote_end: u64,
    ) -> Self {
        Self {
            transport,
            timer,
            checkpoint,
            wall_time,
            vote_end,
            health: HelperHealth::default(),
            config: HelperClientConfig::default(),
            checkpoint_failed: std::cell::Cell::new(false),
            checkpoint_lock: futures_util::lock::Mutex::new(()),
            dispatched: std::cell::Cell::new(false),
        }
    }

    pub fn has_dispatched(&self) -> bool {
        self.dispatched.get()
    }

    /// Shares process-local advisory scores across borrowed checkpoint scopes.
    pub fn with_health(mut self, health: HelperHealth) -> Self {
        self.health = health;
        self
    }

    async fn flush(&self) -> Result<(), VotingError> {
        // Serialize host snapshots without retaining a SQLite or JS guard.
        let _checkpoint = self.checkpoint_lock.lock().await;
        if self.checkpoint_failed.get() {
            return Err(crate::helper::runtime::checkpoint_error());
        }
        let result = self.checkpoint.checkpoint().await;
        if result.is_err() {
            self.checkpoint_failed.set(true);
        }
        result
    }

    pub async fn preflight_fleet(
        &self,
        urls: &[String],
    ) -> Result<HelperFleetPreflight, VotingError> {
        let fleet = HelperFleetPreflight::from_readiness(urls, &[])?;
        let urls = fleet.configured_server_urls();
        let target = share_submission_target_count(urls.len());
        let start = self.now();
        let soft = start + self.config.preflight_soft_timeout;
        let hard = start + self.config.preflight_hard_timeout;
        let mut probes = FuturesUnordered::new();
        for (index, base) in urls.iter().enumerate() {
            probes.push(async move {
                let ready = match join_helper_url(base, &["status"]) {
                    Ok(url) => self
                        .request(&url, None, hard.saturating_duration_since(self.now()))
                        .await
                        .ok()
                        .filter(|response| {
                            response.is_success() && validate_json_response(response).is_ok()
                        })
                        .and_then(|response| json_field(&response, "status"))
                        .is_some_and(|status| status.trim().eq_ignore_ascii_case("ok")),
                    Err(_) => false,
                };
                (index, ready)
            });
        }
        let mut ready = vec![false; urls.len()];
        let mut count = 0;
        let mut soft_elapsed = false;
        while !probes.is_empty() {
            if soft_elapsed && count >= target {
                break;
            }
            match timeout_at(self, if soft_elapsed { hard } else { soft }, probes.next()).await {
                Ok(Some((index, is_ready))) => {
                    ready[index] = is_ready;
                    count += usize::from(is_ready);
                }
                Ok(None) => break,
                Err(_) if !soft_elapsed => soft_elapsed = true,
                Err(_) => break,
            }
        }
        drop(probes);
        let ready = urls
            .iter()
            .zip(ready)
            .filter_map(|(url, ready)| ready.then_some(url.clone()))
            .collect::<Vec<_>>();
        HelperFleetPreflight::from_readiness(urls, &ready)
    }

    async fn request(
        &self,
        url: &str,
        body: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Result<HelperResponse, HelperError> {
        let kind = if body.is_some() {
            RequestKind::NonIdempotent
        } else {
            RequestKind::Idempotent
        };
        let timeout_ms = u32::try_from(timeout.as_millis().min(i32::MAX as u128))
            .map_err(|_| HelperError::DeadlineExceeded)?;
        if timeout_ms == 0 {
            return Err(HelperError::DeadlineExceeded);
        }
        let request = OwnedHttpRequest {
            method: if body.is_some() { "POST" } else { "GET" }.into(),
            url: url.into(),
            headers: if body.is_some() {
                vec![("Content-Type".into(), "application/json".into())]
            } else {
                vec![]
            },
            body: body.unwrap_or_default(),
            timeout_ms,
            max_response_bytes: MAX_HELPER_RESPONSE_BYTES,
            kind,
        };
        if kind == RequestKind::NonIdempotent {
            self.dispatched.set(true);
        }
        let response = timeout_at(self, self.now() + timeout, self.transport.execute(request))
            .await
            .map_err(|_| HelperError::Transport(HelperTransportError::Timeout))?
            .map_err(|error| {
                HelperError::Transport(match error {
                    TransportError::Definite => {
                        HelperTransportError::Transport("helper transport failed".into())
                    }
                    TransportError::Ambiguous => {
                        HelperTransportError::Ambiguous("helper outcome unknown".into())
                    }
                    TransportError::ResponseTooLarge { .. } => {
                        HelperTransportError::Response("helper response exceeds limit".into())
                    }
                })
            })?;
        Ok(HelperResponse::new(
            response.status,
            response.body,
            response.content_type,
        ))
    }

    async fn status(
        &self,
        base: &str,
        round: &str,
        share: &str,
        now: u64,
        deadline: Moment,
        cancel: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<ShareStatus, HelperError> {
        let base = canonicalize_helper_base_url(base).map_err(invalid_request)?;
        let round = normalize_round_id(round).map_err(invalid_request)?;
        let share = validate_hex_path_segment(share, "share_id").map_err(invalid_request)?;
        let url =
            join_helper_url(&base, &["share-status", &round, &share]).map_err(invalid_request)?;
        let start = self.now();
        let result = self
            .retry(cancel, true, false, Some(deadline), || async {
                let timeout = self
                    .config
                    .request_timeout
                    .min(deadline.saturating_duration_since(self.now()));
                let response = require_success(self.request(&url, None, timeout).await?)?;
                validate_json_response(&response)?;
                parse_share_status(&response)
            })
            .await;
        self.score(&base, &result, now, start);
        result
    }

    async fn post(
        &self,
        base: &str,
        body: &str,
        now: u64,
        cancel: &(dyn Fn() -> bool + Send + Sync),
        timeout: Duration,
        deadline: Option<Moment>,
        recovery: bool,
        before_dispatch: &dyn Fn() -> bool,
    ) -> Result<ShareSubmissionStatus, HelperError> {
        let base = canonicalize_helper_base_url(base).map_err(invalid_request)?;
        let body = validate_share_body(body)?;
        let url = join_helper_url(&base, &["shares"]).map_err(invalid_request)?;
        let start = self.now();
        let result = self
            .retry(cancel, false, recovery, deadline, || async {
                self.flush()
                    .await
                    .map_err(|_| HelperError::CheckpointFailed)?;
                if cancel() {
                    return Err(HelperError::Cancelled);
                }
                if !before_dispatch() {
                    return Err(HelperError::Cancelled);
                }
                if (self.wall_time)() >= self.vote_end {
                    return Err(HelperError::DeadlineExceeded);
                }
                let remaining =
                    deadline.map(|deadline| deadline.saturating_duration_since(self.now()));
                if remaining.is_some_and(|left| {
                    left < Duration::from_millis(
                        crate::share_policy::SHARE_DELIVERY_MIN_ATTEMPT_BUDGET_MILLISECONDS,
                    )
                }) {
                    return Err(HelperError::DeadlineExceeded);
                }
                let timeout = remaining
                    .map(|left| timeout.min(left))
                    .unwrap_or(timeout)
                    .min(self.config.post_timeout);
                parse_submission_response(self.request(&url, Some(body.clone()), timeout).await?)
            })
            .await;
        self.score(&base, &result, now, start);
        result
    }

    async fn retry<T, F, Fut>(
        &self,
        cancel: &(dyn Fn() -> bool + Send + Sync),
        idempotent: bool,
        single: bool,
        deadline: Option<Moment>,
        operation: F,
    ) -> Result<T, HelperError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, HelperError>>,
    {
        let attempts = if single {
            0
        } else {
            self.config.retry_delays.len()
        };
        let mut held = None;
        for attempt in 0..=attempts {
            if cancel() {
                return Err(HelperError::Cancelled);
            }
            if deadline.is_some_and(|end| self.now() >= end) {
                return Err(held.unwrap_or(HelperError::DeadlineExceeded));
            }
            match operation().await {
                Ok(value) => return Ok(value),
                Err(error) => {
                    if attempt == attempts || !error.retryable_for(idempotent) {
                        return Err(error);
                    }
                    if cancel() {
                        return Err(HelperError::Cancelled);
                    }
                    let Some(wake) = self.now().checked_add(self.config.retry_delays[attempt])
                    else {
                        return Err(error);
                    };
                    if deadline.is_some_and(|end| wake >= end) {
                        return Err(error);
                    }
                    self.sleep_until(wake).await;
                    held = Some(error);
                }
            }
        }
        Err(HelperError::DeadlineExceeded)
    }

    fn score<T>(&self, base: &str, result: &Result<T, HelperError>, now: u64, start: Moment) {
        match result {
            Ok(_) => self.health.record_success(base),
            Err(
                HelperError::Cancelled
                | HelperError::DeadlineExceeded
                | HelperError::CheckpointFailed
                | HelperError::InvalidRequest { .. },
            ) => {}
            Err(_) => self.health.record_failure(
                base,
                now.saturating_add(self.now().saturating_duration_since(start).as_secs()),
            ),
        }
    }
}

impl RuntimeClient for LocalHelperClient<'_> {
    type Future<'a, T: 'a>
        = LocalBoxFuture<'a, T>
    where
        Self: 'a;
    type Polls<'a>
        = LocalPolls<'a>
    where
        Self: 'a;
    fn health(&self) -> &HelperHealth {
        &self.health
    }
    fn now(&self) -> Moment {
        Moment::Local(self.timer.now())
    }
    fn sleep_until(&self, deadline: Moment) -> Self::Future<'_, ()> {
        self.timer
            .sleep(deadline.saturating_duration_since(self.now()))
    }
    fn checkpoint(&self) -> Self::Future<'_, Result<(), VotingError>> {
        Box::pin(self.flush())
    }
    fn submit<'a>(
        &'a self,
        url: &'a str,
        body: &'a str,
        now: u64,
        cancel: &'a (dyn Fn() -> bool + Send + Sync),
        timeout: Duration,
        deadline: Moment,
        before_dispatch: &'a dyn Fn() -> bool,
    ) -> Self::Future<'a, Result<ShareSubmissionStatus, HelperError>> {
        Box::pin(self.post(
            url,
            body,
            now,
            cancel,
            timeout,
            Some(deadline),
            false,
            before_dispatch,
        ))
    }
    fn resubmit<'a>(
        &'a self,
        url: &'a str,
        body: &'a str,
        now: u64,
        cancel: &'a (dyn Fn() -> bool + Send + Sync),
        before_dispatch: &'a dyn Fn() -> bool,
    ) -> Self::Future<'a, Result<ShareSubmissionStatus, HelperError>> {
        Box::pin(self.post(
            url,
            body,
            now,
            cancel,
            self.config.post_timeout,
            None,
            true,
            before_dispatch,
        ))
    }
    fn new_polls(&self) -> Self::Polls<'_> {
        LocalPolls::new()
    }
    fn start_poll<'a>(
        &'a self,
        polls: &mut Self::Polls<'a>,
        url: String,
        round: String,
        share: String,
        now: u64,
        deadline: Moment,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        polls.push(Box::pin(async move {
            let result = self
                .status(&url, &round, &share, now, deadline, &|| {
                    cancelled.load(std::sync::atomic::Ordering::Relaxed)
                })
                .await;
            (url, result)
        }));
    }
}
