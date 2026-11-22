//! The generic pool and its acquire path: reuse a live entry, share one
//! in-flight start per key, back off a failing start, and lease under the
//! state lock.

use super::LspPoolOptions;
use super::pool_client::PoolClient;
use super::start_guards::StartCancellationGuard;
use super::state::{
    InFlight, Retirement, SharedError, SharedResult, State, evict_overflow, inflight_is_current,
    remove_lru, shared_error, touch_entry,
};
use crate::lsp::clock::LspClock;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;
use tokio::time::Duration;

/// Acquire attempts [`GenericPool::acquire_leased`] makes when the entry it
/// was handed is replaced before it can be leased.
const LEASE_ATTEMPTS: usize = 3;

pub(super) struct GenericPool<C> {
    pub(super) options: LspPoolOptions,
    pub(super) state: Arc<Mutex<State<C>>>,
    pub(super) count: Arc<AtomicUsize>,
    /// The pools that share this pool's memory budget, itself included
    /// ([`GenericPool::share_memory_budget`]).
    pub(super) sharers: Arc<AtomicUsize>,
    /// The time idle expiry and start backoff run on; the pool's clients
    /// get it too.
    pub(super) clock: LspClock,
}

/// `options` with the memory budget cut to one pool's part among `sharers`.
pub(super) fn budgeted_options(options: &LspPoolOptions, sharers: &AtomicUsize) -> LspPoolOptions {
    let sharers = sharers.load(Ordering::Relaxed).max(1) as u64;
    let budget = options.max_resident_bytes;
    LspPoolOptions {
        max_resident_bytes: if budget == 0 {
            0
        } else {
            (budget / sharers).max(1)
        },
        ..*options
    }
}

impl<C: PoolClient> GenericPool<C> {
    pub(super) fn new(options: LspPoolOptions) -> Self {
        Self::with_clock(options, LspClock::default())
    }

    pub(super) fn with_clock(mut options: LspPoolOptions, clock: LspClock) -> Self {
        options.idle_timeout_ms = options.idle_timeout_ms.max(1);
        options.max_entries = options.max_entries.max(1);
        Self {
            options,
            state: Arc::new(Mutex::new(State::new(
                std::num::NonZeroUsize::new(options.max_entries)
                    .unwrap_or(std::num::NonZeroUsize::MIN),
            ))),
            count: Arc::new(AtomicUsize::new(0)),
            sharers: Arc::new(AtomicUsize::new(1)),
            clock,
        }
    }

    /// State that `sharers` pools (this one included) share the memory
    /// budget: each keeps an equal part, so together they hold no more than
    /// the budget one pool alone would. A count below one is one.
    pub(super) fn share_memory_budget(&self, sharers: usize) {
        self.sharers.store(sharers.max(1), Ordering::Relaxed);
    }

    /// The options eviction runs on: the pool's own, with the memory budget
    /// cut to this pool's part. A budget stays a budget (never a part of
    /// zero, which means none), and no budget stays none.
    pub(super) fn budgeted(&self) -> LspPoolOptions {
        budgeted_options(&self.options, &self.sharers)
    }

    pub(super) fn idle_timeout(&self) -> Duration {
        Duration::from_millis(self.options.idle_timeout_ms)
    }

    pub(super) async fn acquire<F, Fut>(
        &self,
        key: String,
        family: String,
        factory: F,
    ) -> SharedResult<C>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = SharedResult<C>> + Send,
    {
        enum Action<C> {
            Wait(Arc<InFlight<C>>),
            Validate {
                inflight: Arc<InFlight<C>>,
                client: C,
                entry_id: u64,
            },
            Start(Arc<InFlight<C>>),
            BackingOff(SharedError),
        }

        let action = {
            let mut state = self.state.lock().await;
            let now = self.clock.now();
            let current = state.inflight.get(&key).cloned();
            let backing_off = if state.entries.contains_key(&key) {
                None
            } else {
                state
                    .backoff
                    .get(&key)
                    .filter(|backoff| now < backoff.retry_at)
                    .map(|backoff| Arc::clone(&backoff.last_error))
            };
            if let Some(inflight) = current.filter(|inflight| !inflight.is_cancelled()) {
                Action::Wait(inflight)
            } else if let Some(error) = backing_off {
                // Lazy restart with backoff: a key whose start keeps failing
                // fails fast with its last error until the window elapses.
                Action::BackingOff(error)
            } else {
                state.inflight.remove(&key);
                let inflight = Arc::new(InFlight::new());
                state.inflight.insert(key.clone(), Arc::clone(&inflight));
                match state.entries.get(&key) {
                    Some(entry) => Action::Validate {
                        inflight,
                        client: entry.client.clone(),
                        entry_id: entry.id,
                    },
                    None => Action::Start(inflight),
                }
            }
        };

        match action {
            Action::Wait(inflight) => inflight.wait().await,
            Action::BackingOff(error) => Err(error),
            Action::Validate {
                inflight,
                client,
                entry_id,
            } => {
                let mut cancellation = StartCancellationGuard::new(
                    key.clone(),
                    Arc::clone(&self.state),
                    Arc::clone(&inflight),
                );
                if client.alive().await {
                    if client.project_config_changed().await {
                        match self
                            .retire_if_idle(&key, &inflight, &client, entry_id)
                            .await
                        {
                            Retirement::Retired => {
                                cancellation.track_client(client.clone());
                                client.stop().await;
                                cancellation.disarm();
                                return self.finish_start(key, family, inflight, factory()).await;
                            }
                            Retirement::Superseded => {
                                cancellation.disarm();
                                return inflight.wait().await;
                            }
                            // In use: keep serving it; the next acquire that
                            // finds it idle replaces it.
                            Retirement::Busy => {}
                        }
                    }
                    enum Revalidated<C> {
                        Reused(Vec<C>),
                        // A clear or replacement owns the inflight now.
                        Superseded,
                        // Idle expiry or LRU eviction removed the entry while
                        // it was validated; this inflight is still ours and no
                        // one else will complete it, so start a fresh client.
                        Removed,
                    }
                    let outcome = {
                        let mut state = self.state.lock().await;
                        if !inflight_is_current(&state, &key, &inflight) {
                            Revalidated::Superseded
                        } else if state.entries.get(&key).map(|entry| entry.id) != Some(entry_id) {
                            Revalidated::Removed
                        } else {
                            touch_entry(&mut state, &key, self.clock.now());
                            // A previously deferred (busy) eviction may be
                            // possible now.
                            let evicted = evict_overflow(&mut state, &self.budgeted(), &key);
                            self.drain_busy_overflow(&mut state);
                            self.count.store(state.entries.len(), Ordering::SeqCst);
                            state.inflight.remove(&key);
                            inflight.complete(Ok(Some(client.clone())));
                            Revalidated::Reused(evicted)
                        }
                    };
                    cancellation.disarm();
                    return match outcome {
                        Revalidated::Reused(evicted) => {
                            for stale in evicted {
                                stale.stop().await;
                            }
                            Ok(Some(client))
                        }
                        Revalidated::Superseded => inflight.wait().await,
                        Revalidated::Removed => {
                            self.finish_start(key, family, inflight, factory()).await
                        }
                    };
                }

                let should_start = {
                    let mut state = self.state.lock().await;
                    if !inflight_is_current(&state, &key, &inflight) {
                        false
                    } else {
                        if state.entries.get(&key).map(|entry| entry.id) == Some(entry_id) {
                            state.entries.remove(&key);
                            self.count.store(state.entries.len(), Ordering::SeqCst);
                            remove_lru(&mut state.lru, &key);
                        }
                        true
                    }
                };
                if !should_start {
                    cancellation.disarm();
                    return inflight.wait().await;
                }
                cancellation.track_client(client.clone());
                client.stop().await;
                cancellation.disarm();
                self.finish_start(key, family, inflight, factory()).await
            }
            Action::Start(inflight) => self.finish_start(key, family, inflight, factory()).await,
        }
    }

    /// Remove `client` (the entry `entry_id` of `key`) from the pool for
    /// replacement, unless it is in use. The busy check and the removal share
    /// the state lock that leasing takes, so no lease can start on a client
    /// that is being retired.
    async fn retire_if_idle(
        &self,
        key: &str,
        inflight: &Arc<InFlight<C>>,
        client: &C,
        entry_id: u64,
    ) -> Retirement {
        let mut state = self.state.lock().await;
        if !inflight_is_current(&state, key, inflight) {
            return Retirement::Superseded;
        }
        if state.entries.get(key).map(|entry| entry.id) != Some(entry_id) || client.busy() {
            return Retirement::Busy;
        }
        state.entries.remove(key);
        self.count.store(state.entries.len(), Ordering::SeqCst);
        remove_lru(&mut state.lru, key);
        Retirement::Retired
    }

    /// [`acquire`](Self::acquire), then lease the client under the state lock
    /// while it is still the pooled entry. The idle timer and LRU eviction
    /// check `busy()` under the same lock, so no reaper can stop the client
    /// between acquire and lease. An entry replaced in that window is
    /// re-acquired (bounded); the last attempt leases what it got.
    pub(super) async fn acquire_leased<F, Fut, L>(
        &self,
        key: String,
        family: String,
        factory: F,
        lease: impl Fn(&C) -> L,
    ) -> SharedResult<(C, L)>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = SharedResult<C>> + Send,
    {
        for attempt in 1..=LEASE_ATTEMPTS {
            let Some(client) = self.acquire(key.clone(), family.clone(), &factory).await? else {
                return Ok(None);
            };
            let state = self.state.lock().await;
            let pooled = state
                .entries
                .get(&key)
                .is_some_and(|entry| entry.client.same(&client));
            if pooled {
                let held = lease(&client);
                drop(state);
                return Ok(Some((client, held)));
            }
            drop(state);
            if attempt == LEASE_ATTEMPTS {
                // Never hand out an entry the pool no longer holds: its idle
                // timer is gone and a request on it would fail unexplained.
                return Err(shared_error(
                    "the pooled language server was replaced repeatedly while being leased; retry",
                ));
            }
        }
        Ok(None)
    }
}
