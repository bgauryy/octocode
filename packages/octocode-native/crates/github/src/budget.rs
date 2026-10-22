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
mod config;
mod key_state;
mod persist;
mod slots;

pub(crate) use circuit::CircuitCause;
pub(crate) use classify::{is_primary_rate_limit, is_secondary_rate_limit};
pub use config::LimiterKey;
pub(crate) use config::{ExecutorConfig, GitHubResource, Group};
pub use key_state::AuthAdmission;
pub(crate) use key_state::{Admission, KeyState};

use super::{ProviderError, ProviderErrorKind, RateLimit};
use octocode_io::budget::BudgetStop;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

pub(crate) fn rate_limited_error(message: &str, rate_limit: RateLimit) -> ProviderError {
    let mut error = ProviderError::new(ProviderErrorKind::RateLimited, message);
    error.rate_limit = Some(Box::new(rate_limit));
    error.retryable = true;
    error
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn ceil_secs(duration: Duration) -> u64 {
    duration.as_millis().div_ceil(1000) as u64
}

/// Cancellation- and deadline-aware sleep. Fails fast (timeout) instead of
/// sleeping past the deadline.
pub(crate) async fn pause(
    delay: Duration,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<(), ProviderError> {
    if cancellation.is_cancelled() {
        return Err(BudgetStop::Cancelled.into());
    }
    if Instant::now() + delay >= deadline {
        return Err(BudgetStop::Deadline.into());
    }
    if delay.is_zero() {
        return Ok(());
    }
    tokio::select! {
        _ = cancellation.cancelled() => Err(BudgetStop::Cancelled.into()),
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
        _ = cancellation.cancelled() => Err(BudgetStop::Cancelled.into()),
        _ = tokio::time::sleep(remaining) => Err(BudgetStop::Deadline.into()),
        permit = semaphore.clone().acquire_owned() => permit.map_err(|_| {
            ProviderError::new(ProviderErrorKind::Cancelled, "GitHub concurrency limiter closed")
        }),
    }
}

/// How a runtime paces its GitHub requests. `Octokit` is production;
/// `Unpaced` is the test seam (through the runtime's internal limits): the
/// limits GitHub's headers report still hold, the client-side spacing and
/// code-search window do not.
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
