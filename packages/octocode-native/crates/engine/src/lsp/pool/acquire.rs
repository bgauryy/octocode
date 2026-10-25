//! The generic pool and its acquire path: reuse a live entry, share one
//! in-flight start per key, back off a failing start, and lease under the
//! state lock.

use super::LspPoolOptions;
use super::pool_client::PoolClient;
use super::start_guards::StartCancellationGuard;
use super::state::{
    InFlight, Retirement, SharedError, SharedResult, State, evict_overflow, inflight_is_current,
    remove_lru, touch_entry,
};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

/// Acquire attempts [`GenericPool::acquire_leased`] makes when the entry it
/// was handed is replaced before it can be leased.
const LEASE_ATTEMPTS: usize = 3;

pub(super) struct GenericPool<C> {
    pub(super) options: LspPoolOptions,
    pub(super) state: Arc<Mutex<State<C>>>,
    pub(super) count: Arc<AtomicUsize>,
}

impl<C: PoolClient> GenericPool<C> {
    pub(super) fn new(mut options: LspPoolOptions) -> Self {
        options.idle_timeout_ms = options.idle_timeout_ms.max(1);
        options.max_entries = options.max_entries.max(1);
        Self {
            options,
            state: Arc::new(Mutex::new(State::default())),
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(super) fn idle_timeout(&self) -> Duration {
        Duration::from_millis(self.options.idle_timeout_ms)
    }

    pub(super) async fn acquire<F, Fut>(&self, key: String, factory: F) -> SharedResult<C>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = SharedResult<C>> + Send,
    {
        crate::lsp::diagnostics::Phase::Acquire
            .measure(self.acquire_inner(key, factory))
            .await
    }

    async fn acquire_inner<F, Fut>(&self, key: String, factory: F) -> SharedResult<C>
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
            let current = state.inflight.get(&key).cloned();
            let backing_off = if state.entries.contains_key(&key) {
                None
            } else {
                state
                    .backoff
                    .get(&key)
                    .filter(|backoff| Instant::now() < backoff.retry_at)
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
                                return self.finish_start(key, inflight, factory()).await;
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
                    client.resume().await;
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
                            touch_entry(&mut state, &key);
                            // A previously deferred (busy) eviction may be
                            // possible now.
                            let evicted = evict_overflow(&mut state, &self.options, &key);
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
                        Revalidated::Removed => self.finish_start(key, inflight, factory()).await,
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
                self.finish_start(key, inflight, factory()).await
            }
            Action::Start(inflight) => self.finish_start(key, inflight, factory()).await,
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
        factory: F,
        lease: impl Fn(&C) -> L,
    ) -> SharedResult<(C, L)>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = SharedResult<C>> + Send,
    {
        for attempt in 1..=LEASE_ATTEMPTS {
            let Some(client) = self.acquire(key.clone(), &factory).await? else {
                return Ok(None);
            };
            let state = self.state.lock().await;
            let pooled = state
                .entries
                .get(&key)
                .is_some_and(|entry| entry.client.same(&client));
            if pooled || attempt == LEASE_ATTEMPTS {
                let held = lease(&client);
                drop(state);
                return Ok(Some((client, held)));
            }
        }
        Ok(None)
    }
}
