//! Installing a started client, its idle timer, and clearing the pool.

use super::acquire::{GenericPool, budgeted_options};
use super::pool_client::PoolClient;
use super::start_guards::StartCancellationGuard;
use super::state::{
    Entry, IdleTimerGuard, InFlight, SharedResult, StartBackoff, State, busy_overflow_pending,
    evict_overflow, inflight_is_current, newest_key, remove_lru, resident_over_budget,
    restart_backoff_delay, retire_idle_siblings,
};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::task::AbortHandle;
use tokio::time::{Duration, Instant};

/// How often the drain task looks for a busy server whose request ended.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "the activity counter has no completion event, so this cadence rechecks overflow eviction without imposing a request or lease deadline"
    )
)]
const DRAIN_POLL: Duration = Duration::from_secs(1);

impl<C: PoolClient> GenericPool<C> {
    pub(super) async fn finish_start<Fut>(
        &self,
        key: String,
        family: String,
        inflight: Arc<InFlight<C>>,
        future: Fut,
    ) -> SharedResult<C>
    where
        Fut: Future<Output = SharedResult<C>> + Send,
    {
        let mut cancellation = StartCancellationGuard::new(
            key.clone(),
            Arc::clone(&self.state),
            Arc::clone(&inflight),
        );
        let result = future.await;
        match result {
            Ok(Some(client)) => {
                cancellation.track_client(client.clone());
                let mut evicted = Vec::new();
                let mut retired = Vec::new();
                let installed = {
                    let mut state = self.state.lock().await;
                    if !inflight_is_current(&state, &key, &inflight) {
                        false
                    } else {
                        let entry_id = state.next_id;
                        state.next_id = state.next_id.wrapping_add(1);
                        let now = self.clock.now();
                        let timer = self.spawn_idle_timer(key.clone(), entry_id, now);
                        #[cfg(test)]
                        {
                            state.idle_timers_spawned += 1;
                        }
                        state.entries.insert(
                            key.clone(),
                            Entry {
                                client: client.clone(),
                                family: family.clone(),
                                id: entry_id,
                                last_used: now,
                                idle_timer: IdleTimerGuard(Some(timer)),
                            },
                        );
                        state.backoff.pop(&key);
                        remove_lru(&mut state.lru, &key);
                        state.lru.push_back(key.clone());
                        // The new server replaces its workspace's idle
                        // siblings only once it is installed: a failed start
                        // keeps the working sibling.
                        retired = retire_idle_siblings(&mut state, &family, &key);
                        evicted = evict_overflow(&mut state, &self.budgeted(), &key);
                        self.drain_busy_overflow(&mut state);
                        self.count.store(state.entries.len(), Ordering::SeqCst);
                        state.inflight.remove(&key);
                        inflight.complete(Ok(Some(client.clone())));
                        true
                    }
                };
                if installed {
                    cancellation.disarm();
                }
                for stale in retired.into_iter().chain(evicted) {
                    stale.stop().await;
                }
                if installed {
                    Ok(Some(client))
                } else {
                    client.stop().await;
                    cancellation.disarm();
                    inflight.wait().await
                }
            }
            Ok(None) => {
                let mut state = self.state.lock().await;
                if inflight_is_current(&state, &key, &inflight) {
                    state.inflight.remove(&key);
                    inflight.complete(Ok(None));
                }
                drop(state);
                cancellation.disarm();
                inflight.wait().await
            }
            Err(error) => {
                let mut state = self.state.lock().await;
                if inflight_is_current(&state, &key, &inflight) {
                    let failures = state
                        .backoff
                        .get(&key)
                        .map_or(0, |backoff| backoff.failures)
                        .saturating_add(1);
                    state.backoff.put(
                        key.clone(),
                        StartBackoff {
                            failures,
                            retry_at: self.clock.deadline_after(restart_backoff_delay(failures)),
                            last_error: Arc::clone(&error),
                        },
                    );
                    state.inflight.remove(&key);
                    inflight.complete(Err(error));
                }
                drop(state);
                cancellation.disarm();
                inflight.wait().await
            }
        }
    }

    /// When a busy server keeps the pool over its limits, start the drain
    /// task (one per pool) that stops it once its request ends: eviction
    /// skips busy servers, and without this the overrun would last until the
    /// next acquire or idle expiry. The server is never stopped while busy,
    /// so no in-flight answer is lost. Call under the state lock.
    pub(super) fn drain_busy_overflow(&self, state: &mut State<C>) {
        if state.draining || !busy_overflow_pending(state, &self.budgeted()) {
            return;
        }
        state.draining = true;
        let state = Arc::clone(&self.state);
        let count = Arc::clone(&self.count);
        let sharers = Arc::clone(&self.sharers);
        let options = self.options;
        let clock = self.clock.clone();
        tokio::spawn(async move {
            loop {
                clock.sleep(DRAIN_POLL).await;
                let (evicted, pending) = {
                    let mut state = state.lock().await;
                    let options = budgeted_options(&options, &sharers);
                    let keep = newest_key(&state).to_owned();
                    let evicted = evict_overflow(&mut state, &options, &keep);
                    count.store(state.entries.len(), Ordering::SeqCst);
                    let pending = busy_overflow_pending(&state, &options);
                    state.draining = pending;
                    (evicted, pending)
                };
                for client in evicted {
                    client.stop().await;
                }
                if !pending {
                    return;
                }
            }
        });
    }

    /// The single idle timer for one entry. It sleeps until the entry's
    /// `last_used + idle_timeout`, re-sleeps when a later acquire renewed
    /// `last_used` or while the client is busy, and exits once the entry is
    /// gone or replaced (entry id check). Dropping the entry aborts it.
    fn spawn_idle_timer(&self, key: String, entry_id: u64, installed_at: Instant) -> AbortHandle {
        let state = Arc::clone(&self.state);
        let count = Arc::clone(&self.count);
        let clock = self.clock.clone();
        let ttl = self.idle_timeout();
        tokio::spawn(async move {
            let mut deadline = installed_at + ttl;
            loop {
                clock.sleep_until(deadline).await;
                let stale = {
                    let mut state = state.lock().await;
                    let Some(entry) = state
                        .entries
                        .get_mut(&key)
                        .filter(|entry| entry.id == entry_id)
                    else {
                        return;
                    };
                    let now = clock.now();
                    let expires = entry.last_used + ttl;
                    if now < expires {
                        deadline = expires;
                        continue;
                    }
                    if entry.client.busy() {
                        entry.last_used = now;
                        deadline = now + ttl;
                        continue;
                    }
                    remove_lru(&mut state.lru, &key);
                    let removed = state.entries.remove(&key).map(|mut entry| {
                        // This task is the remover: don't abort ourselves
                        // before the client is stopped.
                        entry.idle_timer.disarm();
                        entry.client
                    });
                    count.store(state.entries.len(), Ordering::SeqCst);
                    removed
                };
                if let Some(client) = stale {
                    client.stop().await;
                }
                return;
            }
        })
        .abort_handle()
    }

    pub(super) async fn clear(&self, key: &str) -> bool {
        let (client, inflight) = {
            let mut state = self.state.lock().await;
            remove_lru(&mut state.lru, key);
            let client = state.entries.remove(key).map(|entry| entry.client);
            self.count.store(state.entries.len(), Ordering::SeqCst);
            let inflight = state.inflight.remove(key);
            state.backoff.pop(key);
            (client, inflight)
        };
        let removed = inflight.is_some() || client.is_some();
        if let Some(inflight) = inflight {
            inflight.complete(Ok(None));
        }
        if let Some(client) = client {
            client.stop().await;
        }
        removed
    }

    pub(super) async fn clear_all(&self) {
        let (clients, inflight) = {
            let mut state = self.state.lock().await;
            let clients = state
                .entries
                .drain()
                .map(|(_, entry)| entry.client)
                .collect::<Vec<_>>();
            let inflight = state
                .inflight
                .drain()
                .map(|(_, item)| item)
                .collect::<Vec<_>>();
            state.lru.clear();
            state.backoff.clear();
            self.count.store(0, Ordering::SeqCst);
            (clients, inflight)
        };
        for item in inflight {
            item.complete(Ok(None));
        }
        for client in clients {
            client.stop().await;
        }
    }

    /// The resident bytes and the budget while a busy server other than the
    /// newest holds the pool over it: the drain task stops that server once
    /// its request ends. None otherwise, also for a newest server alone over
    /// the budget, which eviction keeps.
    pub(super) async fn over_budget(&self) -> Option<(u64, u64)> {
        let state = self.state.lock().await;
        let options = self.budgeted();
        resident_over_budget(&state.entries, &options)
            .filter(|_| busy_overflow_pending(&state, &options))
    }

    pub(super) fn len(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}
