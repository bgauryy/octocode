//! Per-key state: the blocking facts (buckets, cooldown, starts, circuit),
//! the lanes and permits, and every admission and release on them.
use super::slots::Lane;
use super::{
    CircuitCause, ExecutorConfig, Group, LimiterKey, ceil_secs, now_ms, persist, rate_limited_error,
};
use crate::{ProviderError, ProviderErrorKind, RateLimit};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod admission;
mod facts;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Bucket {
    pub remaining: u64,
    /// Epoch seconds.
    pub reset: u64,
}

/// Blocking facts for one key. Circuit fields are process-local.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct KeyFacts {
    #[serde(default)]
    pub(super) buckets: BTreeMap<String, Bucket>,
    #[serde(default)]
    pub(super) cooldown_until_ms: u64,
    #[serde(default)]
    pub(super) last_start_ms: BTreeMap<String, u64>,
    #[serde(default)]
    pub(super) code_search_ms: VecDeque<u64>,
    #[serde(skip)]
    pub(super) circuit_failures: u32,
    #[serde(skip)]
    pub(super) circuit_open_until_ms: u64,
    /// What the last counted failure was: the open circuit reports it.
    #[serde(skip)]
    pub(super) circuit_cause: Option<CircuitCause>,
}

impl KeyFacts {
    /// Undo one reservation made by `admit` (its start and window slot) when
    /// the request was never sent.
    pub(super) fn release(&mut self, group: &str, start: u64, previous: Option<u64>, window: bool) {
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
    /// Set when the block is an open circuit: the failure that opened it.
    pub circuit: Option<CircuitCause>,
}

impl Block {
    pub(crate) fn wait(&self, now_ms: u64) -> Duration {
        Duration::from_millis(self.until_ms.saturating_sub(now_ms))
    }

    pub(crate) fn error(&self, now_ms: u64) -> ProviderError {
        let retry_after_seconds = Some(ceil_secs(self.wait(now_ms)));
        if let Some(cause) = self
            .circuit
            .filter(|cause| *cause != CircuitCause::Secondary)
        {
            // An outage is not a rate limit: name the real cause, retryable
            // once the circuit closes.
            let (kind, message) = match cause {
                CircuitCause::Transport => (
                    ProviderErrorKind::Transport,
                    "GitHub is unavailable after repeated connection failures; request not sent until the circuit closes.",
                ),
                _ => (
                    ProviderErrorKind::Server,
                    "GitHub is unavailable after repeated server errors; request not sent until the circuit closes.",
                ),
            };
            let mut error = ProviderError::new(kind, message);
            error.rate_limit = Some(Box::new(RateLimit {
                remaining: None,
                reset_epoch_seconds: None,
                retry_after_seconds,
                resource: None,
            }));
            return error;
        }
        let message = if self.circuit.is_some() {
            "GitHub secondary rate limits repeated; requests are paused, wait and retry."
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
                retry_after_seconds,
                resource: Some(self.resource.as_str().into()),
            },
        )
    }
}

/// Permits for one in-flight request; released on drop (before any backoff).
pub(crate) struct Admission {
    pub(super) _permits: Vec<OwnedSemaphorePermit>,
}

/// Keeps the host's OAuth throttling permits until the request finishes.
#[must_use]
pub struct AuthAdmission {
    pub(super) _admission: Admission,
}

pub(crate) struct KeyState {
    pub(super) key: LimiterKey,
    pub(super) global: Arc<Semaphore>,
    pub(super) groups: HashMap<&'static str, Lane>,
    pub(super) git: Arc<Semaphore>,
    pub(super) facts: Mutex<KeyFacts>,
    pub(super) persist: Mutex<Option<persist::Persist>>,
}

impl KeyState {
    pub(super) fn new(key: LimiterKey, config: &ExecutorConfig) -> Self {
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
        .map(|group| {
            let lane = Lane {
                permit: Arc::new(Semaphore::new(1)),
                queued: Arc::default(),
            };
            (group.name(), lane)
        })
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

    /// Nothing holds or throttles this key: no permit is out, no admission
    /// queued, and no cooldown, circuit, exhausted bucket, spacing or
    /// code-search window still constrains the next request. Dropping such a
    /// key and recreating it later admits exactly what keeping it would.
    pub(super) fn is_idle(&self, config: &ExecutorConfig, now: u64) -> bool {
        let permits_free = self.global.available_permits() == config.global_concurrency.max(1)
            && self.git.available_permits() == config.git_concurrency.max(1)
            && self.groups.values().all(|lane| {
                lane.permit.available_permits() == 1
                    && lane.queued.plain.load(Ordering::SeqCst) == 0
                    && lane.queued.windowed.load(Ordering::SeqCst) == 0
            });
        if !permits_free {
            return false;
        }
        let spacing = super::millis(
            config
                .search_spacing
                .max(config.graphql_spacing)
                .max(config.write_spacing),
        );
        let facts = self.facts();
        facts.cooldown_until_ms <= now
            && facts.circuit_failures == 0
            && facts.circuit_open_until_ms <= now
            && facts
                .buckets
                .values()
                .all(|bucket| bucket.remaining > 0 || config.reset_until_ms(bucket.reset) <= now)
            && facts
                .code_search_ms
                .iter()
                .all(|stamp| stamp.saturating_add(60_000) <= now)
            && facts
                .last_start_ms
                .values()
                .all(|start| start.saturating_add(spacing) <= now)
    }

    pub(super) fn facts(&self) -> std::sync::MutexGuard<'_, KeyFacts> {
        self.facts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Current blocker for `resource`, if any: open circuit, secondary
    /// cooldown, or an exhausted primary bucket (until reset + grace).
    pub(crate) fn blocked(&self, resource: &str, config: &ExecutorConfig) -> Option<Block> {
        let now = now_ms();
        let mut facts = self.facts();
        if facts.circuit_open_until_ms > now {
            return Some(Block {
                until_ms: facts.circuit_open_until_ms,
                resource: resource.to_owned(),
                remaining: None,
                reset: None,
                circuit: Some(facts.circuit_cause.unwrap_or(CircuitCause::Server)),
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
                circuit: None,
            });
        }
        if let Some(bucket) = facts.buckets.get(resource)
            && bucket.remaining == 0
        {
            let until = config.reset_until_ms(bucket.reset);
            if until > now && block.as_ref().is_none_or(|b| b.until_ms < until) {
                block = Some(Block {
                    until_ms: until,
                    resource: resource.to_owned(),
                    remaining: Some(0),
                    reset: Some(bucket.reset),
                    circuit: None,
                });
            }
        }
        block
    }
}
