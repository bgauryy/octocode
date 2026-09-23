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
use super::{ProviderError, ProviderErrorKind, RateLimit};
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

static GITHUB_CALLS: AtomicU64 = AtomicU64::new(0);
static RATE_LIMITS: AtomicU64 = AtomicU64::new(0);
static FAILURES: AtomicU64 = AtomicU64::new(0);
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Process stats: `githubCalls` counts sent HTTP requests, `rateLimits` only
/// real GitHub rate-limit events (primary, secondary, GraphQL RATE_LIMITED),
/// `failures` every other failed response or transport error.
pub fn session_snapshot() -> serde_json::Value {
    serde_json::json!({
        "githubCalls": GITHUB_CALLS.load(Ordering::Relaxed),
        "rateLimits": RATE_LIMITS.load(Ordering::Relaxed),
        "failures": FAILURES.load(Ordering::Relaxed),
        "rateLimitState": GitHubBudget::global().snapshot(),
    })
}

pub(crate) fn count_call() {
    GITHUB_CALLS.fetch_add(1, Ordering::Relaxed);
}
pub(crate) fn count_rate_limit() {
    RATE_LIMITS.fetch_add(1, Ordering::Relaxed);
}
pub(crate) fn count_failure() {
    FAILURES.fetch_add(1, Ordering::Relaxed);
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
    Graphql,
    Write,
    Auth,
}

impl Group {
    fn name(self) -> &'static str {
        match self {
            Self::Search => "search",
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
            Group::Search => self.search_spacing,
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
        let host = match host.trim().to_ascii_lowercase().as_str() {
            "api.github.com" => "github.com".to_owned(),
            other => other.to_owned(),
        };
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

    fn label(&self) -> String {
        format!("{}/{}", self.host, self.token)
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
    /// Keep the most restrictive of two views (memory vs. disk).
    fn merge(&mut self, other: KeyFacts, now_ms: u64) {
        for (name, bucket) in other.buckets {
            if bucket.remaining != 0 || bucket.reset.saturating_mul(1000) <= now_ms {
                continue;
            }
            let replace = self
                .buckets
                .get(&name)
                .is_none_or(|current| current.reset < bucket.reset || current.remaining > 0);
            if replace {
                self.buckets.insert(name, bucket);
            }
        }
        self.cooldown_until_ms = self.cooldown_until_ms.max(other.cooldown_until_ms);
        for (group, stamp) in other.last_start_ms {
            let entry = self.last_start_ms.entry(group).or_default();
            *entry = (*entry).max(stamp);
        }
        let mut window: Vec<u64> = self
            .code_search_ms
            .iter()
            .chain(other.code_search_ms.iter())
            .copied()
            .filter(|stamp| stamp.saturating_add(60_000) > now_ms)
            .collect();
        window.sort_unstable();
        window.dedup();
        self.code_search_ms = window.into();
    }

    /// Only facts that can block a future request are persisted.
    fn blocking_view(&self, now_ms: u64) -> KeyFacts {
        KeyFacts {
            buckets: self
                .buckets
                .iter()
                .filter(|(_, b)| b.remaining == 0 && b.reset.saturating_mul(1000) > now_ms)
                .map(|(name, b)| (name.clone(), *b))
                .collect(),
            cooldown_until_ms: if self.cooldown_until_ms > now_ms {
                self.cooldown_until_ms
            } else {
                0
            },
            last_start_ms: self
                .last_start_ms
                .iter()
                .filter(|(_, stamp)| stamp.saturating_add(60_000) > now_ms)
                .map(|(group, stamp)| (group.clone(), *stamp))
                .collect(),
            code_search_ms: self
                .code_search_ms
                .iter()
                .copied()
                .filter(|stamp| stamp.saturating_add(60_000) > now_ms)
                .collect(),
            circuit_failures: 0,
            circuit_open_until_ms: 0,
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

pub(crate) struct KeyState {
    key: LimiterKey,
    global: Arc<Semaphore>,
    groups: HashMap<&'static str, Arc<Semaphore>>,
    git: Arc<Semaphore>,
    facts: Mutex<KeyFacts>,
    persist: Mutex<Option<Persist>>,
}

struct Persist {
    path: PathBuf,
    seen: Option<SystemTime>,
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

/// Full-jitter exponential backoff: uniform in `[0, base * 2^attempt]`,
/// bounded by `cap`.
pub(crate) fn full_jitter(base: Duration, attempt: u8, cap: Duration) -> Duration {
    let ceiling = base
        .saturating_mul(1_u32 << attempt.min(16))
        .min(cap)
        .as_millis() as u64;
    if ceiling == 0 {
        return Duration::ZERO;
    }
    let mut bytes = [0_u8; 8];
    let random = if getrandom::fill(&mut bytes).is_ok() {
        u64::from_le_bytes(bytes)
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.subsec_nanos() as u64)
            .unwrap_or(0)
    };
    Duration::from_millis(random % (ceiling + 1))
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

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.trim().parse().ok()
}

impl KeyState {
    fn new(key: LimiterKey, config: &ExecutorConfig) -> Self {
        let groups = [Group::Search, Group::Graphql, Group::Write, Group::Auth]
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

    fn attach_dir(&self, dir: &Path) {
        let mut persist = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if persist.is_none() {
            *persist = Some(Persist {
                path: dir.join(self.key.file_name()),
                seen: None,
            });
        }
    }

    /// Merge the on-disk view when another process has written it since we
    /// last looked (one `stat` per logical request).
    pub fn refresh_from_disk(&self) {
        let mut persist = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(persist) = persist.as_mut() else {
            return;
        };
        let Ok(modified) = std::fs::metadata(&persist.path).and_then(|meta| meta.modified()) else {
            return;
        };
        if persist.seen == Some(modified) {
            return;
        }
        persist.seen = Some(modified);
        let Some(disk) = std::fs::read(&persist.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(&bytes).ok())
        else {
            return;
        };
        self.facts().merge(disk, now_ms());
    }

    /// Load-merge-write via a unique temp file + atomic rename.
    fn persist(&self) {
        let path = {
            let persist = self
                .persist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match persist.as_ref() {
                Some(persist) => persist.path.clone(),
                None => return,
            }
        };
        let now = now_ms();
        let mut view = self.facts().blocking_view(now);
        if let Some(disk) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(&bytes).ok())
        {
            view.merge(disk, now);
        }
        let Ok(bytes) = serde_json::to_vec(&view) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let tmp = parent.join(format!(
            ".{}.{}.{}.tmp",
            self.key.file_name(),
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        if let Ok(modified) = std::fs::metadata(&path).and_then(|meta| meta.modified())
            && let Some(persist) = self
                .persist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
        {
            persist.seen = Some(modified);
        }
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
                    if wait > cap.max(config.spacing(group)) || Instant::now() + wait >= deadline {
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
                    facts.last_start_ms.insert(group.name().to_owned(), start);
                    if window {
                        facts.code_search_ms.push_back(start);
                    }
                    start
                };
                if matches!(group, Group::Search | Group::Graphql) {
                    self.persist();
                }
                pause(
                    Duration::from_millis(start.saturating_sub(now_ms())),
                    deadline,
                    cancellation,
                )
                .await?;
            }
        }
        permits.push(acquire(&self.global, deadline, cancellation).await?);
        Ok(Admission { _permits: permits })
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

    pub fn record_success(&self) {
        let mut facts = self.facts();
        facts.circuit_failures = 0;
        facts.circuit_open_until_ms = 0;
    }

    /// Only secondary limits and 5xx/transport failures open the circuit.
    pub fn record_circuit_failure(&self, config: &ExecutorConfig) {
        let mut facts = self.facts();
        facts.circuit_failures = facts.circuit_failures.saturating_add(1);
        if facts.circuit_failures >= config.circuit_failures {
            facts.circuit_open_until_ms =
                now_ms().saturating_add(config.circuit_open.as_millis() as u64);
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        let now = now_ms();
        let facts = self.facts();
        serde_json::json!({
            "key": self.key.label(),
            "buckets": facts.buckets.iter().map(|(name, bucket)| (name.clone(), serde_json::json!({
                "remaining": bucket.remaining,
                "resetEpochSeconds": bucket.reset,
            }))).collect::<serde_json::Map<_, _>>(),
            "cooldownRemainingMs": facts.cooldown_until_ms.saturating_sub(now),
            "circuitOpen": facts.circuit_open_until_ms > now,
        })
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

    fn snapshot(&self) -> serde_json::Value {
        let keys: Vec<Arc<KeyState>> = self
            .keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        serde_json::Value::Array(keys.iter().map(|state| state.snapshot()).collect())
    }
}

/// Octokit: `/\bsecondary rate\b/i` on the error message.
pub fn mentions_secondary_rate(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.match_indices("secondary rate").any(|(index, _)| {
        let before_ok = index == 0
            || !lower.as_bytes()[index - 1].is_ascii_alphanumeric()
                && lower.as_bytes()[index - 1] != b'_';
        let end = index + "secondary rate".len();
        let after_ok = lower
            .as_bytes()
            .get(end)
            .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
        before_ok && after_ok
    })
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
    if !matches!(status, 403 | 429) {
        return false;
    }
    if mentions_secondary_rate(body) {
        return true;
    }
    if status == 429 && remaining != Some(0) {
        return true;
    }
    status == 403 && retry_after.is_some() && remaining != Some(0)
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
    fn detects_secondary_limit_from_body_even_when_remaining_is_positive() {
        assert!(is_secondary_rate_limit(
            403,
            Some(21),
            None,
            "You have exceeded a secondary rate limit. Please wait a few minutes"
        ));
        assert!(!is_primary_rate_limit(403, Some(21)));
        assert!(is_primary_rate_limit(403, Some(0)));
        // Word boundary: "secondary rates" / 404 bodies do not qualify.
        assert!(!mentions_secondary_rate("nonsecondary ratex"));
        assert!(!is_secondary_rate_limit(
            404,
            None,
            None,
            "secondary rate limit"
        ));
    }

    #[test]
    fn full_jitter_stays_within_bounds() {
        for attempt in 0..6 {
            let delay = full_jitter(Duration::from_millis(100), attempt, Duration::from_secs(1));
            assert!(delay <= Duration::from_millis(100 << attempt).min(Duration::from_secs(1)));
        }
        assert_eq!(
            full_jitter(Duration::ZERO, 3, Duration::from_secs(1)),
            Duration::ZERO
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

    #[test]
    fn disk_state_round_trips_blocking_facts_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let reset = now_ms() / 1000 + 300;
        {
            let budget = GitHubBudget::relaxed();
            let state = budget.key_state(&key, Some(dir.path()));
            let mut headers = HeaderMap::new();
            headers.insert("x-ratelimit-remaining", "42".parse().expect("header"));
            headers.insert(
                "x-ratelimit-reset",
                reset.to_string().parse().expect("header"),
            );
            headers.insert("x-ratelimit-resource", "core".parse().expect("header"));
            state.observe(&headers, "core");
            state.exhaust("search", reset);
            state.cool_down(now_ms() + 30_000);
        }
        let file = dir.path().join(key.file_name());
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).expect("state file")).expect("json");
        assert!(raw["buckets"].get("core").is_none(), "{raw}");
        assert_eq!(raw["buckets"]["search"]["remaining"], 0);
        // A fresh process (new registry) sees the other process's facts.
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let search = state.blocked("search", budget.config()).expect("search");
        assert_eq!(search.reset, Some(reset));
        let core = state.blocked("core", budget.config()).expect("cooldown");
        assert_eq!(core.reset, None);
        let leftovers = std::fs::read_dir(dir.path())
            .expect("dir")
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[tokio::test]
    async fn search_spacing_orders_starts() {
        let config = ExecutorConfig {
            search_spacing: Duration::from_millis(150),
            ..ExecutorConfig::relaxed()
        };
        let budget = Arc::new(GitHubBudget::with_config(config));
        let state = budget.key_state(&LimiterKey::new("h", None), None);
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
    async fn code_search_window_fails_fast_when_full() {
        let config = ExecutorConfig {
            code_search_per_minute: 2,
            ..ExecutorConfig::relaxed()
        };
        let budget = Arc::new(GitHubBudget::with_config(config));
        let state = budget.key_state(&LimiterKey::new("h", None), None);
        let deadline = Instant::now() + Duration::from_secs(5);
        let token = CancellationToken::new();
        for _ in 0..2 {
            drop(
                state
                    .admit(
                        Some(Group::Search),
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
                Some(Group::Search),
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

    #[test]
    fn git_gate_fails_fast_on_long_cooldown() {
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&LimiterKey::new("github.com", None), None);
        let deadline = Instant::now() + Duration::from_secs(5);
        let permit = state
            .acquire_git_blocking(budget.config(), Duration::from_secs(1), deadline, &|| false)
            .expect("free git slot");
        drop(permit);
        state.cool_down(now_ms() + 60_000);
        let error = state
            .acquire_git_blocking(budget.config(), Duration::from_secs(1), deadline, &|| false)
            .expect_err("cooldown");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    }
}
