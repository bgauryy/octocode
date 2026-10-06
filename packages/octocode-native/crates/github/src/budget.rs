//! Keyed GitHub request executor state (Octokit plugin-throttling parity).
//!
//! Every GitHub call — REST, GraphQL, OAuth login, and `git` clone
//! subprocesses — is admitted through a [`KeyState`] keyed by
//! `(api host, short token digest | "anon")`. Per key it owns the Octokit
//! throttling groups (global, search, graphql, write, auth, git), the
//! per-resource primary buckets learned from every response
//! (`x-ratelimit-resource/remaining/reset`), a shared secondary-limit
//! cooldown, and a circuit breaker fed only by secondary limits and 5xx.
//! Blocking facts are optionally mirrored to
//! `~/.octocode/tmp/ratelimit/<host>-<token>.json` so short-lived CLI
//! processes honor each other's limits.
mod circuit;
mod classify;
mod persist;

pub(crate) use classify::{is_primary_rate_limit, is_secondary_rate_limit};

use super::{ProviderError, ProviderErrorKind, RateLimit, credential_host, retry::header_u64};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GitHubResource {
    Core,
    Search,
    CodeSearch,
    Graphql,
}

impl GitHubResource {
    pub fn classify(url: &Url) -> Self {
        let path = url.path();
        if path.contains("/graphql") {
            Self::Graphql
        } else if path.contains("/search/code") {
            Self::CodeSearch
        } else if path.contains("/search/") {
            Self::Search
        } else {
            Self::Core
        }
    }

    /// GitHub's `x-ratelimit-resource` name for this request family.
    pub fn bucket(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Search => "search",
            Self::CodeSearch => "code_search",
            Self::Graphql => "graphql",
        }
    }
}

/// Octokit throttling groups layered under the per-key global group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Group {
    Search,
    CodeSearch,
    Graphql,
    Write,
    Auth,
}

impl Group {
    fn name(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::CodeSearch => "code_search",
            Self::Graphql => "graphql",
            Self::Write => "write",
            Self::Auth => "auth",
        }
    }
}

/// Executor tuning. [`ExecutorConfig::octokit`] mirrors
/// `@octokit/plugin-throttling`; tests shrink the timings.
#[derive(Clone, Debug)]
pub struct ExecutorConfig {
    pub global_concurrency: usize,
    pub git_concurrency: usize,
    pub search_spacing: Duration,
    pub graphql_spacing: Duration,
    pub write_spacing: Duration,
    pub code_search_per_minute: usize,
    /// Longest wait for a code-search window slot taken inside the request
    /// (the 10/min window spans 60 s, beyond the general retry-after cap).
    pub code_search_wait: Duration,
    /// Added to a primary reset before retrying (Octokit waits reset + 1s).
    pub reset_grace: Duration,
    /// Secondary-limit wait when GitHub sends no `retry-after`.
    pub secondary_default: Duration,
    pub circuit_failures: u32,
    pub circuit_open: Duration,
}

impl ExecutorConfig {
    pub fn octokit() -> Self {
        Self {
            global_concurrency: 10,
            git_concurrency: 2,
            search_spacing: Duration::from_millis(2000),
            graphql_spacing: Duration::from_millis(1000),
            write_spacing: Duration::from_millis(1000),
            code_search_per_minute: 10,
            code_search_wait: Duration::from_secs(20),
            reset_grace: Duration::from_secs(1),
            secondary_default: Duration::from_secs(60),
            circuit_failures: 5,
            circuit_open: Duration::from_secs(30),
        }
    }

    /// No pacing — used by unit tests that do not exercise throttling.
    pub fn relaxed() -> Self {
        Self {
            global_concurrency: 32,
            git_concurrency: 8,
            search_spacing: Duration::ZERO,
            graphql_spacing: Duration::ZERO,
            write_spacing: Duration::ZERO,
            code_search_per_minute: usize::MAX,
            reset_grace: Duration::ZERO,
            ..Self::octokit()
        }
    }

    fn spacing(&self, group: Group) -> Duration {
        match group {
            Group::Search | Group::CodeSearch => self.search_spacing,
            Group::Graphql => self.graphql_spacing,
            Group::Write => self.write_spacing,
            Group::Auth => Duration::ZERO,
        }
    }
}

/// Limiter identity: the API host (`api.github.com` folds to `github.com`,
/// explicit ports kept) and a short SHA-256 digest of the token.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct LimiterKey {
    host: String,
    token: String,
}

impl LimiterKey {
    pub fn new(host: &str, token: Option<&str>) -> Self {
        let host = credential_host(&host.trim().to_ascii_lowercase()).to_owned();
        let token = match token.filter(|value| !value.is_empty()) {
            Some(token) => {
                let digest = Sha256::digest(token.as_bytes());
                hex::encode(&digest[..8])
            }
            None => "anon".to_owned(),
        };
        Self { host, token }
    }

    /// Key for any URL on the host (API, web origin, or git remote).
    pub fn for_url(url: &Url, token: Option<&str>) -> Self {
        let host = url.host_str().unwrap_or_default();
        match url.port() {
            Some(port) => Self::new(&format!("{host}:{port}"), token),
            None => Self::new(host, token),
        }
    }

    fn file_name(&self) -> String {
        let host: String = self
            .host
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{host}-{}.json", self.token)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Bucket {
    pub remaining: u64,
    /// Epoch seconds.
    pub reset: u64,
}

/// Blocking facts for one key. Circuit fields are process-local.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyFacts {
    #[serde(default)]
    buckets: BTreeMap<String, Bucket>,
    #[serde(default)]
    cooldown_until_ms: u64,
    #[serde(default)]
    last_start_ms: BTreeMap<String, u64>,
    #[serde(default)]
    code_search_ms: VecDeque<u64>,
    #[serde(skip)]
    circuit_failures: u32,
    #[serde(skip)]
    circuit_open_until_ms: u64,
}

impl KeyFacts {
    /// Undo one reservation made by `admit` (its start and window slot) when
    /// the request was never sent.
    fn release(&mut self, group: &str, start: u64, previous: Option<u64>, window: bool) {
        if self.last_start_ms.get(group) == Some(&start) {
            match previous {
                Some(previous) => self.last_start_ms.insert(group.to_owned(), previous),
                None => self.last_start_ms.remove(group),
            };
        }
        if window
            && let Some(index) = self
                .code_search_ms
                .iter()
                .rposition(|stamp| *stamp == start)
        {
            self.code_search_ms.remove(index);
        }
    }
}

/// What currently blocks a request on a key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Block {
    pub until_ms: u64,
    pub resource: String,
    pub remaining: Option<u64>,
    pub reset: Option<u64>,
    pub circuit: bool,
}

impl Block {
    pub fn wait(&self, now_ms: u64) -> Duration {
        Duration::from_millis(self.until_ms.saturating_sub(now_ms))
    }

    pub fn error(&self, now_ms: u64) -> ProviderError {
        let message = if self.circuit {
            "GitHub host circuit is open after repeated failures; wait and retry."
        } else if self.reset.is_some() {
            "GitHub API rate limit exhausted; request not sent until the limit resets."
        } else {
            "GitHub secondary rate limit cooldown is active; request not sent."
        };
        rate_limited_error(
            message,
            RateLimit {
                remaining: self.remaining,
                reset_epoch_seconds: self.reset,
                retry_after_seconds: Some(ceil_secs(self.wait(now_ms))),
                resource: Some(self.resource.as_str().into()),
            },
        )
    }
}

pub(crate) fn rate_limited_error(message: &str, rate_limit: RateLimit) -> ProviderError {
    let mut error = ProviderError::new(ProviderErrorKind::RateLimited, message);
    error.rate_limit = Some(rate_limit);
    error.retryable = true;
    error
}

/// Permits for one in-flight request; released on drop (before any backoff).
pub(crate) struct Admission {
    _permits: Vec<OwnedSemaphorePermit>,
}

/// Keeps the host's OAuth throttling permits until the request finishes.
#[must_use]
pub struct AuthAdmission {
    _admission: Admission,
}

pub(crate) struct KeyState {
    key: LimiterKey,
    global: Arc<Semaphore>,
    groups: HashMap<&'static str, Arc<Semaphore>>,
    git: Arc<Semaphore>,
    facts: Mutex<KeyFacts>,
    persist: Mutex<Option<persist::Persist>>,
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn ceil_secs(duration: Duration) -> u64 {
    duration.as_millis().div_ceil(1000) as u64
}

fn cancelled() -> ProviderError {
    ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")
}
fn timed_out() -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Timeout,
        "GitHub request deadline exceeded",
    )
}

/// Cancellation- and deadline-aware sleep. Fails fast (timeout) instead of
/// sleeping past the deadline.
pub(crate) async fn pause(
    delay: Duration,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<(), ProviderError> {
    if cancellation.is_cancelled() {
        return Err(cancelled());
    }
    if Instant::now() + delay >= deadline {
        return Err(timed_out());
    }
    if delay.is_zero() {
        return Ok(());
    }
    tokio::select! {
        _ = cancellation.cancelled() => Err(cancelled()),
        _ = tokio::time::sleep(delay) => Ok(()),
    }
}

async fn acquire(
    semaphore: &Arc<Semaphore>,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<OwnedSemaphorePermit, ProviderError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        _ = cancellation.cancelled() => Err(cancelled()),
        _ = tokio::time::sleep(remaining) => Err(timed_out()),
        permit = semaphore.clone().acquire_owned() => permit.map_err(|_| {
            ProviderError::new(ProviderErrorKind::Cancelled, "GitHub concurrency limiter closed")
        }),
    }
}

impl KeyState {
    fn new(key: LimiterKey, config: &ExecutorConfig) -> Self {
        // Code search gets its own lane: GitHub meters it in a separate
        // bucket, and a code-search window wait must not stall repo/issue
        // search behind it.
        let groups = [
            Group::Search,
            Group::CodeSearch,
            Group::Graphql,
            Group::Write,
            Group::Auth,
        ]
        .into_iter()
        .map(|group| (group.name(), Arc::new(Semaphore::new(1))))
        .collect();
        Self {
            key,
            global: Arc::new(Semaphore::new(config.global_concurrency.max(1))),
            groups,
            git: Arc::new(Semaphore::new(config.git_concurrency.max(1))),
            facts: Mutex::new(KeyFacts::default()),
            persist: Mutex::new(None),
        }
    }

    fn facts(&self) -> std::sync::MutexGuard<'_, KeyFacts> {
        self.facts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Current blocker for `resource`, if any: open circuit, secondary
    /// cooldown, or an exhausted primary bucket (until reset + grace).
    pub fn blocked(&self, resource: &str, config: &ExecutorConfig) -> Option<Block> {
        let now = now_ms();
        let mut facts = self.facts();
        if facts.circuit_open_until_ms > now {
            return Some(Block {
                until_ms: facts.circuit_open_until_ms,
                resource: resource.to_owned(),
                remaining: None,
                reset: None,
                circuit: true,
            });
        }
        if facts.circuit_open_until_ms != 0 {
            facts.circuit_open_until_ms = 0;
            facts.circuit_failures = 0;
        }
        let mut block: Option<Block> = None;
        if facts.cooldown_until_ms > now {
            block = Some(Block {
                until_ms: facts.cooldown_until_ms,
                resource: resource.to_owned(),
                remaining: None,
                reset: None,
                circuit: false,
            });
        }
        if let Some(bucket) = facts.buckets.get(resource)
            && bucket.remaining == 0
        {
            let until = bucket
                .reset
                .saturating_mul(1000)
                .saturating_add(config.reset_grace.as_millis() as u64);
            if until > now && block.as_ref().is_none_or(|b| b.until_ms < until) {
                block = Some(Block {
                    until_ms: until,
                    resource: resource.to_owned(),
                    remaining: Some(0),
                    reset: Some(bucket.reset),
                    circuit: false,
                });
            }
        }
        block
    }

    /// Pre-send gate: wait out a short block (≤ `cap` and inside the
    /// deadline) or fail fast without sending.
    pub async fn wait_unblocked(
        &self,
        resource: &str,
        config: &ExecutorConfig,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), ProviderError> {
        self.refresh_from_disk();
        for _ in 0..4 {
            let Some(block) = self.blocked(resource, config) else {
                return Ok(());
            };
            let now = now_ms();
            let wait = block.wait(now);
            if block.circuit || wait > cap || Instant::now() + wait >= deadline {
                return Err(block.error(now));
            }
            pause(wait, deadline, cancellation).await?;
        }
        match self.blocked(resource, config) {
            None => Ok(()),
            Some(block) => Err(block.error(now_ms())),
        }
    }

    /// Admit one attempt: group permit, group spacing (minTime), optional
    /// code-search window slot (counted once per logical request), then the
    /// per-key global permit.
    pub async fn admit(
        &self,
        group: Option<Group>,
        config: &ExecutorConfig,
        count_code_search: bool,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Admission, ProviderError> {
        let mut permits = Vec::with_capacity(2);
        if let Some(group) = group {
            if let Some(semaphore) = self.groups.get(group.name()) {
                permits.push(acquire(semaphore, deadline, cancellation).await?);
            }
            let spacing = config.spacing(group).as_millis() as u64;
            let window = count_code_search && config.code_search_per_minute != usize::MAX;
            if spacing > 0 || window {
                let now = now_ms();
                let start = {
                    let mut facts = self.facts();
                    let mut start = now;
                    if let Some(last) = facts.last_start_ms.get(group.name()) {
                        start = start.max(last.saturating_add(spacing));
                    }
                    if window {
                        while facts
                            .code_search_ms
                            .front()
                            .is_some_and(|stamp| stamp.saturating_add(60_000) <= now)
                        {
                            facts.code_search_ms.pop_front();
                        }
                        if facts.code_search_ms.len() >= config.code_search_per_minute
                            && let Some(oldest) = facts.code_search_ms.front()
                        {
                            start = start.max(oldest.saturating_add(60_000));
                        }
                    }
                    let wait = Duration::from_millis(start.saturating_sub(now));
                    let limit = if window {
                        cap.max(config.code_search_wait)
                    } else {
                        cap
                    };
                    if wait > limit.max(config.spacing(group)) || Instant::now() + wait >= deadline
                    {
                        return Err(rate_limited_error(
                            "GitHub code search window is full; request not sent.",
                            RateLimit {
                                remaining: Some(0),
                                reset_epoch_seconds: Some(start.div_ceil(1000)),
                                retry_after_seconds: Some(ceil_secs(wait)),
                                resource: Some(GitHubResource::CodeSearch.bucket().into()),
                            },
                        ));
                    }
                    let previous = facts.last_start_ms.insert(group.name().to_owned(), start);
                    if window {
                        facts.code_search_ms.push_back(start);
                    }
                    (start, previous)
                };
                let (start, previous) = start;
                let persisted = matches!(group, Group::Search | Group::CodeSearch | Group::Graphql);
                if persisted {
                    self.persist();
                }
                if let Err(error) = pause(
                    Duration::from_millis(start.saturating_sub(now_ms())),
                    deadline,
                    cancellation,
                )
                .await
                {
                    // Nothing was sent: hand back the reserved start and window
                    // slot so they do not throttle later requests.
                    self.release_reservation(group, start, previous, window, persisted);
                    return Err(error);
                }
            }
        }
        permits.push(acquire(&self.global, deadline, cancellation).await?);
        Ok(Admission { _permits: permits })
    }

    /// Hand back a reservation in memory and, when mirrored, on disk: the
    /// disk copy was written before the wait and a plain merge would restore it.
    fn release_reservation(
        &self,
        group: Group,
        start: u64,
        previous: Option<u64>,
        window: bool,
        persisted: bool,
    ) {
        self.facts().release(group.name(), start, previous, window);
        if persisted {
            self.persist_with(|view| view.release(group.name(), start, previous, window));
        }
    }

    /// Record the primary bucket carried by any response.
    pub fn observe(&self, headers: &HeaderMap, fallback_resource: &str) {
        let (Some(remaining), Some(reset)) = (
            header_u64(headers, "x-ratelimit-remaining"),
            header_u64(headers, "x-ratelimit-reset"),
        ) else {
            return;
        };
        let resource = headers
            .get("x-ratelimit-resource")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_resource)
            .to_owned();
        self.facts()
            .buckets
            .insert(resource, Bucket { remaining, reset });
        if remaining == 0 {
            self.persist();
        }
    }

    /// Mark a resource exhausted until `reset` (epoch seconds).
    pub fn exhaust(&self, resource: &str, reset: u64) {
        self.facts().buckets.insert(
            resource.to_owned(),
            Bucket {
                remaining: 0,
                reset,
            },
        );
        self.persist();
    }

    /// Secondary limit: every caller of this key waits until `until_ms`.
    pub fn cool_down(&self, until_ms: u64) {
        {
            let mut facts = self.facts();
            facts.cooldown_until_ms = facts.cooldown_until_ms.max(until_ms);
        }
        self.persist();
    }

    /// Blocking `git` admission for clone subprocesses: honors the key's
    /// secondary cooldown / circuit, then takes a `git` group permit.
    pub fn acquire_git_blocking(
        &self,
        config: &ExecutorConfig,
        cap: Duration,
        deadline: Instant,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        const TICK: Duration = Duration::from_millis(25);
        self.refresh_from_disk();
        loop {
            if is_cancelled() {
                return Err(cancelled());
            }
            if Instant::now() >= deadline {
                return Err(timed_out());
            }
            // Clone traffic is not metered by an API bucket; only the shared
            // cooldown/circuit apply.
            let block = self
                .blocked("git", config)
                .filter(|block| block.reset.is_none());
            if let Some(block) = block {
                let now = now_ms();
                let wait = block.wait(now);
                if block.circuit || wait > cap || Instant::now() + wait >= deadline {
                    return Err(block.error(now));
                }
                std::thread::sleep(wait.min(TICK));
                continue;
            }
            if let Ok(permit) = self.git.clone().try_acquire_owned() {
                return Ok(permit);
            }
            std::thread::sleep(TICK);
        }
    }
}

/// Process-wide registry of per-key executor state.
pub struct GitHubBudget {
    config: ExecutorConfig,
    keys: Mutex<HashMap<LimiterKey, Arc<KeyState>>>,
}

impl GitHubBudget {
    pub fn global() -> Arc<Self> {
        static BUDGET: OnceLock<Arc<GitHubBudget>> = OnceLock::new();
        BUDGET
            .get_or_init(|| Arc::new(Self::with_config(ExecutorConfig::octokit())))
            .clone()
    }

    pub fn relaxed() -> Arc<Self> {
        Arc::new(Self::with_config(ExecutorConfig::relaxed()))
    }

    pub fn with_config(config: ExecutorConfig) -> Self {
        Self {
            config,
            keys: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &ExecutorConfig {
        &self.config
    }

    /// Admit an OAuth attempt under the anonymous host key shared with API traffic.
    /// The returned guard holds the auth and global permits until dropped.
    pub async fn admit_auth(
        &self,
        origin: &Url,
        cap: Duration,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<AuthAdmission, ProviderError> {
        let state = self.key_state(&LimiterKey::for_url(origin, None), None);
        state
            .wait_unblocked("auth", &self.config, cap, deadline, cancellation)
            .await?;
        let admission = state
            .admit(
                Some(Group::Auth),
                &self.config,
                false,
                cap,
                deadline,
                cancellation,
            )
            .await?;
        Ok(AuthAdmission {
            _admission: admission,
        })
    }

    /// Admit a clone subprocess under the shared host/token cooldown and circuit.
    /// The optional directory uses the same persisted rate-limit state as HTTP.
    pub fn acquire_git_blocking(
        &self,
        key: &LimiterKey,
        state_dir: Option<&Path>,
        cap: Duration,
        deadline: Instant,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        self.key_state(key, state_dir).acquire_git_blocking(
            &self.config,
            cap,
            deadline,
            is_cancelled,
        )
    }

    /// State for `key`; attaches (and loads) the on-disk mirror when a
    /// state directory is supplied.
    pub(crate) fn key_state(&self, key: &LimiterKey, state_dir: Option<&Path>) -> Arc<KeyState> {
        let state = self
            .keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(key.clone())
            .or_insert_with(|| Arc::new(KeyState::new(key.clone(), &self.config)))
            .clone();
        if let Some(dir) = state_dir {
            state.attach_dir(dir);
            state.refresh_from_disk();
        }
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_search_resources() {
        assert_eq!(
            GitHubResource::classify(
                &Url::parse("https://api.github.com/search/code")
                    .expect("GitHub test URL should parse")
            ),
            GitHubResource::CodeSearch
        );
        assert_eq!(
            GitHubResource::classify(
                &Url::parse("https://api.github.com/search/issues")
                    .expect("GitHub test URL should parse")
            ),
            GitHubResource::Search
        );
        assert_eq!(
            GitHubResource::classify(
                &Url::parse("https://api.github.com/repos/a/b/issues")
                    .expect("GitHub test URL should parse")
            ),
            GitHubResource::Core
        );
        assert_eq!(
            GitHubResource::classify(
                &Url::parse("https://api.github.com/graphql")
                    .expect("GitHub test URL should parse")
            ),
            GitHubResource::Graphql
        );
    }

    #[test]
    fn limiter_keys_fold_api_host_and_hash_tokens() {
        let api = LimiterKey::for_url(
            &Url::parse("https://api.github.com/").expect("url"),
            Some("t1"),
        );
        let web = LimiterKey::for_url(
            &Url::parse("https://github.com/a/b.git").expect("url"),
            Some("t1"),
        );
        assert_eq!(api, web);
        assert_ne!(api, LimiterKey::new("github.com", Some("t2")));
        assert_eq!(LimiterKey::new("github.com", None).token, "anon");
        assert!(!api.file_name().contains("t1"));
        let ported = LimiterKey::for_url(
            &Url::parse("http://127.0.0.1:8080/api/v3").expect("url"),
            None,
        );
        assert_eq!(ported.file_name(), "127.0.0.1_8080-anon.json");
    }

    #[test]
    fn primary_bucket_blocks_until_reset_and_isolates_keys() {
        let budget = GitHubBudget::relaxed();
        let one = budget.key_state(&LimiterKey::new("github.com", Some("one")), None);
        let two = budget.key_state(&LimiterKey::new("github.com", Some("two")), None);
        let reset = now_ms() / 1000 + 120;
        one.exhaust("core", reset);
        let block = one
            .blocked("core", budget.config())
            .expect("exhausted bucket blocks");
        assert_eq!(block.reset, Some(reset));
        assert!(one.blocked("search", budget.config()).is_none());
        assert!(two.blocked("core", budget.config()).is_none());
        let error = block.error(now_ms());
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        let rate = error.rate_limit.expect("metadata");
        assert_eq!(rate.resource.as_deref(), Some("core"));
        assert!(rate.retry_after_seconds.unwrap_or_default() >= 119);
    }

    /// A budget under `config` and its state for one host key.
    fn limited(config: ExecutorConfig) -> (Arc<GitHubBudget>, Arc<KeyState>) {
        let budget = Arc::new(GitHubBudget::with_config(config));
        let state = budget.key_state(&LimiterKey::new("h", None), None);
        (budget, state)
    }

    #[tokio::test]
    async fn search_spacing_orders_starts() {
        let (budget, state) = limited(ExecutorConfig {
            search_spacing: Duration::from_millis(150),
            ..ExecutorConfig::relaxed()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let token = CancellationToken::new();
        let started = Instant::now();
        for _ in 0..3 {
            let _admission = state
                .admit(
                    Some(Group::Search),
                    budget.config(),
                    false,
                    Duration::from_secs(10),
                    deadline,
                    &token,
                )
                .await
                .expect("admitted");
        }
        assert!(started.elapsed() >= Duration::from_millis(290));
    }

    #[tokio::test]
    async fn a_short_code_search_window_wait_is_taken_inside_the_request() {
        let (budget, state) = limited(ExecutorConfig {
            code_search_per_minute: 2,
            ..ExecutorConfig::relaxed()
        });
        // The window is full, and its oldest slot frees in ~1.5 s: longer
        // than the 1 s retry-after cap, inside the code-search wait.
        {
            let now = now_ms();
            let mut facts = state.facts();
            facts.code_search_ms.push_back(now - 58_500);
            facts.code_search_ms.push_back(now - 10_000);
        }
        let started = Instant::now();
        let admitted = state
            .admit(
                Some(Group::CodeSearch),
                budget.config(),
                true,
                Duration::from_secs(1),
                Instant::now() + Duration::from_secs(10),
                &CancellationToken::new(),
            )
            .await;
        assert!(admitted.is_ok(), "{:?}", admitted.err());
        assert!(started.elapsed() >= Duration::from_millis(1_000));
    }

    /// Seed a full 2/min window whose oldest slot frees in ~1.5 s.
    fn full_code_search_window() -> (Arc<GitHubBudget>, Arc<KeyState>) {
        let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
            code_search_per_minute: 2,
            ..ExecutorConfig::relaxed()
        }));
        let state = budget.key_state(&LimiterKey::new("h", None), None);
        {
            let now = now_ms();
            let mut facts = state.facts();
            facts.code_search_ms.push_back(now - 58_500);
            facts.code_search_ms.push_back(now - 10_000);
        }
        (budget, state)
    }

    #[tokio::test]
    async fn a_code_search_window_wait_does_not_block_other_searches() {
        let (budget, state) = full_code_search_window();
        let deadline = Instant::now() + Duration::from_secs(10);
        let token = CancellationToken::new();
        let waiting = {
            let (budget, state, token) = (budget.clone(), state.clone(), token.clone());
            tokio::spawn(async move {
                state
                    .admit(
                        Some(Group::CodeSearch),
                        budget.config(),
                        true,
                        Duration::from_secs(1),
                        deadline,
                        &token,
                    )
                    .await
                    .map(drop)
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        let started = Instant::now();
        let other = state
            .admit(
                Some(Group::Search),
                budget.config(),
                false,
                Duration::from_secs(1),
                deadline,
                &token,
            )
            .await;
        assert!(other.is_ok(), "{:?}", other.err());
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "repo/issue search waited {:?} behind the code-search window",
            started.elapsed()
        );
        drop(other);
        waiting.await.expect("join").expect("code search admitted");
    }

    #[tokio::test]
    async fn a_cancelled_window_wait_releases_its_reserved_slot() {
        let (budget, state) = full_code_search_window();
        let token = CancellationToken::new();
        let canceller = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            canceller.cancel();
        });
        let error = state
            .admit(
                Some(Group::CodeSearch),
                budget.config(),
                true,
                Duration::from_secs(1),
                Instant::now() + Duration::from_secs(10),
                &token,
            )
            .await
            .err()
            .expect("cancelled");
        assert_eq!(error.kind, ProviderErrorKind::Cancelled);
        let facts = state.facts();
        assert_eq!(facts.code_search_ms.len(), 2, "no phantom window slot");
        assert!(
            facts
                .last_start_ms
                .get(Group::CodeSearch.name())
                .is_none_or(|start| *start <= now_ms()),
            "no phantom spacing reservation"
        );
    }

    /// The reservation is mirrored to disk before the wait, so the release
    /// must reach disk too, or other processes (and this one, on its next
    /// refresh) still count the phantom slot.
    #[tokio::test]
    async fn a_cancelled_window_wait_releases_its_persisted_slot() {
        let dir = tempfile::tempdir().expect("state directory");
        let config = || ExecutorConfig {
            code_search_per_minute: 2,
            ..ExecutorConfig::relaxed()
        };
        let key = LimiterKey::new("h", None);
        let budget = Arc::new(GitHubBudget::with_config(config()));
        let state = budget.key_state(&key, Some(dir.path()));
        {
            let now = now_ms();
            let mut facts = state.facts();
            facts.code_search_ms.push_back(now - 58_500);
            facts.code_search_ms.push_back(now - 10_000);
        }
        let token = CancellationToken::new();
        let canceller = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            canceller.cancel();
        });
        state
            .admit(
                Some(Group::CodeSearch),
                budget.config(),
                true,
                Duration::from_secs(1),
                Instant::now() + Duration::from_secs(10),
                &token,
            )
            .await
            .err()
            .expect("cancelled");
        let other_process = GitHubBudget::with_config(config());
        let seen = other_process.key_state(&key, Some(dir.path()));
        let facts = seen.facts();
        assert_eq!(facts.code_search_ms.len(), 2, "no phantom slot on disk");
        assert!(
            facts
                .last_start_ms
                .get(Group::CodeSearch.name())
                .is_none_or(|start| *start <= now_ms()),
            "no phantom spacing reservation on disk"
        );
    }

    #[tokio::test]
    async fn code_search_window_fails_fast_when_full() {
        let (budget, state) = limited(ExecutorConfig {
            code_search_per_minute: 2,
            ..ExecutorConfig::relaxed()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let token = CancellationToken::new();
        for _ in 0..2 {
            drop(
                state
                    .admit(
                        Some(Group::CodeSearch),
                        budget.config(),
                        true,
                        Duration::from_secs(1),
                        deadline,
                        &token,
                    )
                    .await
                    .expect("admitted"),
            );
        }
        let error = state
            .admit(
                Some(Group::CodeSearch),
                budget.config(),
                true,
                Duration::from_secs(1),
                deadline,
                &token,
            )
            .await
            .err()
            .expect("window full");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        assert_eq!(
            error.rate_limit.and_then(|r| r.resource).as_deref(),
            Some("code_search")
        );
    }

    #[tokio::test]
    async fn oauth_admission_holds_permits_and_honors_cancellation_and_shared_cooldown() {
        let budget = GitHubBudget::with_config(ExecutorConfig {
            global_concurrency: 1,
            ..ExecutorConfig::relaxed()
        });
        let origin = Url::parse("https://api.github.com").expect("origin");
        let cap = Duration::from_secs(1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let cancellation = CancellationToken::new();
        let permit = budget
            .admit_auth(&origin, cap, deadline, &cancellation)
            .await
            .expect("first admission");
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let error = budget
            .admit_auth(&origin, cap, deadline, &cancelled)
            .await
            .err()
            .expect("cancelled while waiting for a permit");
        assert_eq!(error.kind, ProviderErrorKind::Cancelled);
        let error = budget
            .admit_auth(
                &origin,
                cap,
                Instant::now() + Duration::from_millis(20),
                &cancellation,
            )
            .await
            .err()
            .expect("permit stays held");
        assert_eq!(error.kind, ProviderErrorKind::Timeout);
        drop(permit);
        drop(
            budget
                .admit_auth(&origin, cap, deadline, &cancellation)
                .await
                .expect("permit released"),
        );
        budget
            .key_state(&LimiterKey::new("github.com", None), None)
            .cool_down(now_ms() + 60_000);
        let error = budget
            .admit_auth(&origin, cap, deadline, &cancellation)
            .await
            .err()
            .expect("shared anonymous cooldown");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    }

    #[test]
    fn git_gate_fails_fast_on_long_cooldown() {
        let budget = GitHubBudget::relaxed();
        let dir = tempfile::tempdir().expect("state directory");
        let key = LimiterKey::new("github.com", None);
        let state = budget.key_state(&key, Some(dir.path()));
        let deadline = Instant::now() + Duration::from_secs(5);
        let permit = budget
            .acquire_git_blocking(
                &key,
                Some(dir.path()),
                Duration::from_secs(1),
                deadline,
                &|| false,
            )
            .expect("free git slot");
        drop(permit);
        state.cool_down(now_ms() + 60_000);
        let error = GitHubBudget::relaxed()
            .acquire_git_blocking(
                &key,
                Some(dir.path()),
                Duration::from_secs(1),
                deadline,
                &|| false,
            )
            .expect_err("cooldown");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    }
}
