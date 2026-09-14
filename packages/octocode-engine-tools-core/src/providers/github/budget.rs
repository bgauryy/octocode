//! Process-wide GitHub API budget: search pacing, concurrency, and host circuit.
use super::{ProviderError, ProviderErrorKind};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

const SEARCH_WINDOW: Duration = Duration::from_secs(60);
const CIRCUIT_OPEN: Duration = Duration::from_secs(30);
const CIRCUIT_FAILURES: u32 = 5;
const DEFAULT_CONCURRENCY: usize = 8;
static GITHUB_CALLS: AtomicU64 = AtomicU64::new(0);
static RATE_LIMITS: AtomicU64 = AtomicU64::new(0);

pub fn session_snapshot() -> serde_json::Value {
    serde_json::json!({
        "githubCalls": GITHUB_CALLS.load(Ordering::Relaxed),
        "rateLimits": RATE_LIMITS.load(Ordering::Relaxed)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubResource {
    Core,
    Search,
    CodeSearch,
    Graphql,
}

impl GitHubResource {
    pub fn classify(url: &Url) -> Self {
        let path = url.path().trim_end_matches('/');
        if path == "/graphql" || path.ends_with("/api/graphql") {
            Self::Graphql
        } else if path.contains("/search/code") {
            Self::CodeSearch
        } else if path.contains("/search/") {
            Self::Search
        } else {
            Self::Core
        }
    }

    fn per_minute(self) -> usize {
        match self {
            Self::Core => usize::MAX,
            Self::Search | Self::Graphql => 30,
            Self::CodeSearch => 10,
        }
    }
}

#[derive(Clone)]
pub struct GitHubBudget {
    inner: Arc<BudgetInner>,
}

struct BudgetInner {
    semaphore: Arc<Semaphore>,
    search: Mutex<VecDeque<Instant>>,
    code_search: Mutex<VecDeque<Instant>>,
    graphql: Mutex<VecDeque<Instant>>,
    circuit: Mutex<Circuit>,
    pace: bool,
}

struct Circuit {
    failures: u32,
    opened_at: Option<Instant>,
}

impl Default for Circuit {
    fn default() -> Self {
        Self {
            failures: 0,
            opened_at: None,
        }
    }
}

impl GitHubBudget {
    pub fn global() -> Arc<Self> {
        static BUDGET: OnceLock<Arc<GitHubBudget>> = OnceLock::new();
        BUDGET
            .get_or_init(|| Arc::new(Self::with_limits(DEFAULT_CONCURRENCY, true)))
            .clone()
    }

    pub fn relaxed() -> Arc<Self> {
        Arc::new(Self::with_limits(32, false))
    }

    pub fn with_limits(concurrency: usize, pace: bool) -> Self {
        Self {
            inner: Arc::new(BudgetInner {
                semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
                search: Mutex::new(VecDeque::new()),
                code_search: Mutex::new(VecDeque::new()),
                graphql: Mutex::new(VecDeque::new()),
                circuit: Mutex::new(Circuit::default()),
                pace,
            }),
        }
    }

    pub async fn acquire(
        &self,
        resource: GitHubResource,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        self.check_circuit()?;
        self.wait_slot(resource, deadline, cancellation).await?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        tokio::select! {
            _ = cancellation.cancelled() => Err(ProviderError::new(
                ProviderErrorKind::Cancelled,
                "GitHub request cancelled",
            )),
            _ = tokio::time::sleep(remaining) => Err(ProviderError::new(
                ProviderErrorKind::Timeout,
                "GitHub request deadline exceeded",
            )),
            permit = self.inner.semaphore.clone().acquire_owned() => {
                permit.map_err(|_| {
                    ProviderError::new(
                        ProviderErrorKind::Cancelled,
                        "GitHub concurrency limiter closed",
                    )
                })
            }
        }
    }

    pub fn record_success(&self) {
        GITHUB_CALLS.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut circuit) = self.inner.circuit.lock() {
            circuit.failures = 0;
            circuit.opened_at = None;
        }
    }

    pub fn record_failure(&self, open_circuit: bool) {
        RATE_LIMITS.fetch_add(1, Ordering::Relaxed);
        if !open_circuit {
            return;
        }
        if let Ok(mut circuit) = self.inner.circuit.lock() {
            circuit.failures = circuit.failures.saturating_add(1);
            if circuit.failures >= CIRCUIT_FAILURES {
                circuit.opened_at = Some(Instant::now());
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn circuit_is_open(&self) -> bool {
        let circuit = self
            .inner
            .circuit
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        circuit
            .opened_at
            .is_some_and(|opened| opened.elapsed() < CIRCUIT_OPEN)
    }

    fn check_circuit(&self) -> Result<(), ProviderError> {
        let mut circuit = self
            .inner
            .circuit
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(opened) = circuit.opened_at {
            if opened.elapsed() < CIRCUIT_OPEN {
                return Err(ProviderError::new(
                    ProviderErrorKind::RateLimited,
                    "GitHub host circuit is open after repeated failures; wait and retry.",
                ));
            }
            circuit.opened_at = None;
            circuit.failures = 0;
        }
        Ok(())
    }

    async fn wait_slot(
        &self,
        resource: GitHubResource,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), ProviderError> {
        loop {
            if cancellation.is_cancelled() {
                return Err(ProviderError::new(
                    ProviderErrorKind::Cancelled,
                    "GitHub request cancelled",
                ));
            }
            if Instant::now() >= deadline {
                return Err(ProviderError::new(
                    ProviderErrorKind::Timeout,
                    "GitHub request deadline exceeded",
                ));
            }
            match self.next_wait(resource) {
                None => return Ok(()),
                Some(wait) => {
                    let wait = wait.min(deadline.saturating_duration_since(Instant::now()));
                    tokio::select! {
                        _ = cancellation.cancelled() => {
                            return Err(ProviderError::new(
                                ProviderErrorKind::Cancelled,
                                "GitHub request cancelled",
                            ));
                        }
                        _ = tokio::time::sleep(wait) => {}
                    }
                }
            }
        }
    }

    fn next_wait(&self, resource: GitHubResource) -> Option<Duration> {
        if !self.inner.pace {
            return None;
        }
        let limit = resource.per_minute();
        if limit == usize::MAX {
            return None;
        }
        let mut queue = match resource {
            GitHubResource::CodeSearch => self.inner.code_search.lock(),
            GitHubResource::Graphql => self.inner.graphql.lock(),
            _ => self.inner.search.lock(),
        }
        .unwrap_or_else(|error| error.into_inner());
        let now = Instant::now();
        while queue
            .front()
            .is_some_and(|stamp| now.duration_since(*stamp) >= SEARCH_WINDOW)
        {
            queue.pop_front();
        }
        if queue.len() < limit {
            queue.push_back(now);
            return None;
        }
        queue
            .front()
            .map(|stamp| SEARCH_WINDOW.saturating_sub(now.duration_since(*stamp)))
    }
}

pub fn is_primary_rate_limit(status: u16, remaining: Option<u64>) -> bool {
    remaining == Some(0) && matches!(status, 403 | 429)
}

pub fn is_secondary_rate_limit(
    status: u16,
    remaining: Option<u64>,
    retry_after: Option<u64>,
    body: &str,
) -> bool {
    if body.to_ascii_lowercase().contains("secondary rate") {
        return true;
    }
    if status == 429 && remaining != Some(0) {
        return true;
    }
    status == 403 && retry_after.is_some() && remaining != Some(0)
}

pub fn retry_after_or_backoff(
    retry_after: Option<u64>,
    attempt: u8,
    max_retry_after: Duration,
) -> Option<Duration> {
    if let Some(seconds) = retry_after {
        let delay = Duration::from_secs(seconds);
        return (delay < max_retry_after).then_some(delay);
    }
    let delay = Duration::from_secs(1).saturating_mul(1_u32 << attempt.min(5));
    (delay < max_retry_after).then_some(delay.min(Duration::from_secs(60)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_search_resources() {
        assert_eq!(
            GitHubResource::classify(&Url::parse("https://api.github.com/search/code").unwrap()),
            GitHubResource::CodeSearch
        );
        assert_eq!(
            GitHubResource::classify(&Url::parse("https://api.github.com/search/issues").unwrap()),
            GitHubResource::Search
        );
        assert_eq!(
            GitHubResource::classify(
                &Url::parse("https://api.github.com/repos/a/b/issues").unwrap()
            ),
            GitHubResource::Core
        );
        assert_eq!(
            GitHubResource::classify(&Url::parse("https://api.github.com/graphql").unwrap()),
            GitHubResource::Graphql
        );
        assert_eq!(
            GitHubResource::classify(&Url::parse("https://ghe.example/api/graphql").unwrap()),
            GitHubResource::Graphql
        );
        assert_ne!(
            GitHubResource::classify(&Url::parse("https://api.github.com/graphql").unwrap()),
            GitHubResource::Core
        );
        assert_eq!(GitHubResource::Graphql.per_minute(), 30);
        assert_eq!(GitHubResource::Core.per_minute(), usize::MAX);
        assert_eq!(
            GitHubResource::classify(
                &Url::parse("https://api.github.com/repos/octocat/graphql").unwrap()
            ),
            GitHubResource::Core
        );
    }

    #[test]
    fn detects_secondary_limit_from_body_even_when_remaining_is_positive() {
        assert!(is_secondary_rate_limit(
            403,
            Some(21),
            None,
            "You have exceeded a secondary rate limit. Please wait a few minutes"
        ));
        assert!(!is_primary_rate_limit(403, Some(21)));
        assert!(is_primary_rate_limit(403, Some(0)));
    }

    #[test]
    fn missing_retry_after_uses_bounded_backoff() {
        let delay = retry_after_or_backoff(None, 0, Duration::from_secs(60)).expect("backoff");
        assert_eq!(delay, Duration::from_secs(1));
        assert!(retry_after_or_backoff(Some(60), 0, Duration::from_secs(60)).is_none());
    }
}
