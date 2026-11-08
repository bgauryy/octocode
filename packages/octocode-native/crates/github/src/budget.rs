//! Keyed GitHub request executor state (Octokit plugin-throttling parity).
//!
//! Every GitHub call — REST, GraphQL, OAuth login, and `git` clone
//! subprocesses — is admitted through a [`KeyState`] keyed by
//! `(api host, short token digest | "anon")`. Per key it owns the Octokit
//! throttling groups (global, search, graphql, write, auth, git), the
//! per-resource primary buckets learned from every response
//! (`x-ratelimit-resource/limit/remaining/reset`; search groups space their
//! starts by the quota their bucket reports, code search by the documented
//! default until the first answer reports one), a shared secondary-limit
//! cooldown, and a circuit breaker fed only by secondary limits and 5xx.
//! Blocking facts are optionally mirrored to
//! `~/.octocode/tmp/ratelimit/<host>-<token>.json` so short-lived CLI
//! processes honor each other's limits.
mod circuit;
mod classify;
mod config;
mod key_state;
mod persist;
mod slots;
mod waits;

pub(crate) use circuit::CircuitCause;
pub(crate) use classify::{is_primary_rate_limit, is_secondary_rate_limit, names_secondary_limit};
pub use config::LimiterKey;
pub(crate) use config::{ExecutorConfig, GitHubResource, Group};
pub use key_state::AuthAdmission;
pub(crate) use key_state::{Admission, KeyState};
pub(crate) use waits::ClientWaits;

/// Admission of clone subprocesses (network `git`) under the shared
/// host/token cooldown, circuit and `git` permits of one budget: the API
/// transport's, so its cooldowns hold clones back too. The optional
/// directory is the same persisted rate-limit state as HTTP. Waits land on
/// the row the limiter was made for.
#[derive(Clone)]
pub struct GitLimiter {
    budget: Arc<GitHubBudget>,
    state_dir: Option<std::path::PathBuf>,
    waits: Arc<ClientWaits>,
}

impl GitLimiter {
    /// A limiter on `budget` whose waits no row discloses.
    #[must_use]
    pub fn new(budget: Arc<GitHubBudget>, state_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            budget,
            state_dir,
            waits: Arc::default(),
        }
    }

    /// This limiter, its waits recorded on `context`'s row.
    #[must_use]
    pub fn for_row(mut self, context: &crate::RequestContext) -> Self {
        self.waits = Arc::clone(&context.waits);
        self
    }

    /// Admit one clone subprocess for `key`, waiting at most `cap` for a
    /// cooldown and never past `deadline` or a cancel.
    pub fn acquire_blocking(
        &self,
        key: &LimiterKey,
        cap: Duration,
        deadline: Instant,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<OwnedSemaphorePermit, ProviderError> {
        self.budget
            .key_state(key, self.state_dir.as_deref())
            .acquire_git_blocking(
                &self.budget.config,
                cap,
                deadline,
                is_cancelled,
                &self.waits,
            )
    }
}

use super::{ProviderError, ProviderErrorKind, RateLimit};
use octocode_io::budget::BudgetStop;
use octocode_io::cancellation::Cancellation;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use url::Url;

pub(crate) fn rate_limited_error(message: &str, rate_limit: RateLimit) -> ProviderError {
    let mut error = ProviderError::new(ProviderErrorKind::RateLimited, message);
    error.rate_limit = Some(Box::new(rate_limit));
    error.retryable = true;
    error
}

/// Whether a spent `bucket` is waited out before the next send. Only the
/// buckets the client paces are: search and code search, metered over
/// GitHub's minute windows, whose resets the search pacing already plans
/// around. Any other spent quota (core and GraphQL, metered over an hour)
/// is disclosed at once as `rateLimited` with its reset, so the caller
/// decides instead of a wait of up to an hour. Secondary-limit cooldowns
/// and `retry-after` stay waited out: GitHub prescribes those waits.
pub(crate) fn waits_out_spent(bucket: &str) -> bool {
    Group::paces_bucket(bucket)
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(millis)
        .unwrap_or(0)
}

/// Whole milliseconds of `duration`, saturating at `u64::MAX` (~584 million
/// years: no wait or epoch reaches it).
pub(crate) fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn ceil_secs(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis().div_ceil(1000)).unwrap_or(u64::MAX)
}

/// A permit of `semaphore`; a wait for one held by another request is
/// recorded on `waits` as `kind`.
async fn acquire(
    semaphore: &Arc<Semaphore>,
    deadline: Instant,
    cancellation: &Cancellation,
    waits: &ClientWaits,
    kind: octocode_contract_types::ClientWaitKind,
) -> Result<OwnedSemaphorePermit, ProviderError> {
    if let Ok(permit) = semaphore.clone().try_acquire_owned() {
        return Ok(permit);
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    waits
        .timed(kind, async {
            tokio::select! {
                _ = cancellation.cancelled() => Err(BudgetStop::Cancelled.into()),
                _ = tokio::time::sleep(remaining) => Err(BudgetStop::Deadline.into()),
                permit = semaphore.clone().acquire_owned() => permit.map_err(|_| {
                    ProviderError::new(ProviderErrorKind::Cancelled, "GitHub concurrency limiter closed")
                }),
            }
        })
        .await
}

/// How a runtime paces its GitHub requests. `Octokit` is production;
/// `Unpaced` is the test seam (through the runtime's internal limits): the
/// limits GitHub's headers report still hold, the client-side search pacing
/// does not.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GitHubPacing {
    #[default]
    Octokit,
    Unpaced,
}

/// Process-wide registry of per-key executor state.
/// Account keys kept before idle ones are dropped.
pub(crate) const KEY_RETENTION: usize = 256;

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

    /// The budget `pacing` names: the process-wide octokit budget, or a
    /// fresh unpaced one.
    pub fn paced(pacing: GitHubPacing) -> Arc<Self> {
        match pacing {
            GitHubPacing::Octokit => Self::global(),
            GitHubPacing::Unpaced => Self::relaxed(),
        }
    }

    pub fn relaxed() -> Arc<Self> {
        Arc::new(Self::with_config(ExecutorConfig::relaxed()))
    }

    pub(crate) fn with_config(config: ExecutorConfig) -> Self {
        Self {
            config,
            keys: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn config(&self) -> &ExecutorConfig {
        &self.config
    }

    /// Admit an OAuth attempt under the anonymous host key shared with API traffic.
    /// The returned guard holds the auth and global permits until dropped.
    pub async fn admit_auth(
        &self,
        origin: &Url,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<AuthAdmission, ProviderError> {
        let state = self.key_state(&LimiterKey::for_url(origin, None), None);
        // A login has no row to disclose its waits on.
        let waits = ClientWaits::default();
        state
            .wait_unblocked("auth", &self.config, deadline, cancellation, &waits)
            .await?;
        let admission = state
            .admit(
                Some(Group::Auth),
                &self.config,
                deadline,
                cancellation,
                &waits,
            )
            .await?;
        Ok(AuthAdmission {
            _admission: admission,
        })
    }

    /// Wait until every rate-limit fact written without awaiting (an
    /// exhausted bucket, a cooldown, an observed quota, a pull request's
    /// lifetime, a handed-back start) is on disk, in every budget of this
    /// process. A process that ends right after a call awaits this first:
    /// tokio drops a blocking write still queued at shutdown, and the next
    /// process must find the fact.
    pub async fn flush_detached_writes() {
        persist::flush_detached().await;
    }

    /// State for `key`; attaches (and loads) the on-disk mirror when a
    /// state directory is supplied.
    pub(crate) fn key_state(&self, key: &LimiterKey, state_dir: Option<&Path>) -> Arc<KeyState> {
        let state = {
            let mut keys = self
                .keys
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if keys.len() >= KEY_RETENTION && !keys.contains_key(key) {
                // Account churn must not grow the registry for the process
                // lifetime; a held or still-throttled key keeps its state.
                let now = now_ms();
                keys.retain(|_, state| {
                    Arc::strong_count(state) > 1 || !state.is_idle(&self.config, now)
                });
            }
            keys.entry(key.clone())
                .or_insert_with(|| Arc::new(KeyState::new(key.clone(), &self.config)))
                .clone()
        };
        if let Some(dir) = state_dir {
            state.attach_dir(dir);
            state.refresh_from_disk();
        }
        state
    }
}

#[cfg(test)]
mod tests;
