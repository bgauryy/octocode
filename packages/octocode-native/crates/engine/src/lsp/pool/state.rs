//! Pool bookkeeping: entries, in-flight starts shared by concurrent
//! acquires, start backoff, and LRU eviction.

use super::LspPoolOptions;
use super::pool_client::PoolClient;
use crate::error::Error;
use lru::LruCache;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio::task::AbortHandle;
use tokio::time::{Duration, Instant};

/// First delay after a failed start; doubles per consecutive failure.
const RESTART_BACKOFF_BASE: Duration = Duration::from_millis(250);
/// Cap on the start backoff window.
const RESTART_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Aborts the entry's single idle-timer task when the entry is dropped
/// (clear, eviction, replacement), unless the timer itself is the remover.
pub(super) struct IdleTimerGuard(pub(super) Option<AbortHandle>);

impl IdleTimerGuard {
    pub(super) fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for IdleTimerGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

/// What [`super::acquire::GenericPool::retire_if_idle`] did with a client whose project
/// config changed.
pub(super) enum Retirement {
    /// Removed from the pool; the caller stops it and starts a fresh one.
    Retired,
    /// A clear or replacement owns the in-flight start now.
    Superseded,
    /// In use, or no longer the pooled entry: left as it is.
    Busy,
}

pub(super) struct Entry<C> {
    pub(super) client: C,
    /// Sibling-family key: entries of one workspace and server binary share
    /// it whatever their initialization options (see `canonical_lsp_family`).
    pub(super) family: String,
    pub(super) id: u64,
    /// Renewed on every successful acquire; the entry's one idle timer
    /// re-sleeps until `last_used + idle_timeout` instead of spawning anew.
    pub(super) last_used: Instant,
    pub(super) idle_timer: IdleTimerGuard,
}

/// Errors are shared between deduplicated waiters, so they are reference
/// counted instead of flattened to a string: any structured detail on
/// `Error` survives to the boundary.
pub(super) type SharedError = Arc<Error>;
pub(super) type SharedResult<C> = std::result::Result<Option<C>, SharedError>;

pub(super) fn shared_error(reason: impl Into<String>) -> SharedError {
    Arc::new(Error::new(reason))
}

pub(super) struct StartBackoff {
    pub(super) failures: u32,
    pub(super) retry_at: Instant,
    pub(super) last_error: SharedError,
}

pub(super) fn restart_backoff_delay(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(16);
    RESTART_BACKOFF_BASE
        .saturating_mul(1_u32 << exponent)
        .min(RESTART_BACKOFF_MAX)
}

pub(super) struct InFlight<C> {
    pub(super) result: Mutex<Option<SharedResult<C>>>,
    pub(super) notify: Notify,
    pub(super) cancelled: AtomicBool,
}

impl<C: Clone> InFlight<C> {
    pub(super) fn new() -> Self {
        Self {
            result: Mutex::new(None),
            notify: Notify::new(),
            cancelled: AtomicBool::new(false),
        }
    }

    pub(super) fn complete(&self, result: SharedResult<C>) {
        let mut slot = self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(result);
            self.notify.notify_waiters();
        }
    }

    pub(super) async fn wait(&self) -> SharedResult<C> {
        loop {
            let notified = self.notify.notified();
            let current = self
                .result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(result) = current {
                return result;
            }
            notified.await;
        }
    }

    pub(super) fn cancel(&self) {
        // Wake current waiters immediately. A later acquire also observes this
        // bit and replaces the registration without racing async Drop cleanup.
        self.cancelled.store(true, Ordering::SeqCst);
        self.complete(Err(shared_error("LSP client startup was cancelled")));
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub(super) struct State<C> {
    pub(super) entries: HashMap<String, Entry<C>>,
    pub(super) inflight: HashMap<String, Arc<InFlight<C>>>,
    /// Failed starts are optional memoized state, bounded by the same owner
    /// capacity as the clients. A cold failure can be retried after eviction.
    pub(super) backoff: LruCache<String, StartBackoff>,
    pub(super) lru: VecDeque<String>,
    pub(super) next_id: u64,
    #[cfg(test)]
    pub(super) idle_timers_spawned: usize,
}

impl<C> State<C> {
    pub(super) fn new(max_entries: NonZeroUsize) -> Self {
        Self {
            entries: HashMap::new(),
            inflight: HashMap::new(),
            backoff: LruCache::new(max_entries),
            lru: VecDeque::new(),
            next_id: 1,
            #[cfg(test)]
            idle_timers_spawned: 0,
        }
    }
}

pub(super) fn inflight_is_current<C>(
    state: &State<C>,
    key: &str,
    expected: &Arc<InFlight<C>>,
) -> bool {
    state
        .inflight
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, expected))
}

pub(super) fn touch_entry<C>(state: &mut State<C>, key: &str) -> bool {
    let Some(entry) = state.entries.get_mut(key) else {
        return false;
    };
    entry.last_used = Instant::now();
    remove_lru(&mut state.lru, key);
    state.lru.push_back(key.to_owned());
    true
}

/// The servers' summed resident bytes and the budget when they exceed it. A
/// budget of 0 means none.
pub(super) fn resident_over_budget<C: PoolClient>(
    entries: &HashMap<String, Entry<C>>,
    options: &LspPoolOptions,
) -> Option<(u64, u64)> {
    if options.max_resident_bytes == 0 {
        return None;
    }
    let resident = entries
        .values()
        .map(|entry| entry.client.resident_bytes())
        .fold(0_u64, u64::saturating_add);
    (resident > options.max_resident_bytes).then_some((resident, options.max_resident_bytes))
}

/// Evict least-recently-used entries while over `max_entries` or, when the
/// pool has a memory budget, while the servers' sampled resident memory
/// together exceeds it, skipping `keep` and every busy client. When everything evictable is busy the pool
/// overflows temporarily; a later install or acquire retries the eviction and
/// idle expiry shrinks it too. Returned clients must be stopped by the caller
/// after the state lock is released.
pub(super) fn evict_overflow<C: PoolClient>(
    state: &mut State<C>,
    options: &LspPoolOptions,
    keep: &str,
) -> Vec<C> {
    let mut evicted = Vec::new();
    while state.entries.len() > options.max_entries
        || resident_over_budget(&state.entries, options).is_some()
    {
        let State { entries, lru, .. } = &mut *state;
        let Some(position) = lru.iter().position(|candidate| {
            candidate != keep
                && entries
                    .get(candidate)
                    .is_some_and(|entry| !entry.client.busy())
        }) else {
            break;
        };
        let Some(victim) = lru.remove(position) else {
            break;
        };
        if let Some(entry) = entries.remove(&victim) {
            evicted.push(entry.client);
        }
    }
    evicted
}

/// Remove the idle siblings of `key` (entries of `family` under another key)
/// from the pool. A busy sibling stays: the pool never stops a client that
/// has a request or lease in flight. Returned clients must be stopped by the
/// caller after the state lock is released.
pub(super) fn retire_idle_siblings<C: PoolClient>(
    state: &mut State<C>,
    family: &str,
    key: &str,
) -> Vec<C> {
    let State { entries, lru, .. } = state;
    let siblings = entries
        .iter()
        .filter(|(candidate, entry)| {
            candidate.as_str() != key && entry.family == family && !entry.client.busy()
        })
        .map(|(candidate, _)| candidate.clone())
        .collect::<Vec<_>>();
    siblings
        .into_iter()
        .filter_map(|sibling| {
            remove_lru(lru, &sibling);
            entries.remove(&sibling).map(|entry| entry.client)
        })
        .collect()
}

pub(super) fn remove_lru(lru: &mut VecDeque<String>, key: &str) {
    lru.retain(|candidate| candidate != key);
}
