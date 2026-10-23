//! Installing a started client, its idle timer, and clearing the pool.

use super::acquire::GenericPool;
use super::pool_client::PoolClient;
use super::start_guards::StartCancellationGuard;
use super::state::{
    Entry, IdleTimerGuard, InFlight, SharedResult, StartBackoff, evict_overflow,
    inflight_is_current, remove_lru, restart_backoff_delay,
};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::task::AbortHandle;
use tokio::time::{Instant, sleep_until};

impl<C: PoolClient> GenericPool<C> {
    pub(super) async fn finish_start<Fut>(
        &self,
        key: String,
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
                let installed = {
                    let mut state = self.state.lock().await;
                    if !inflight_is_current(&state, &key, &inflight) {
                        false
                    } else {
                        let entry_id = state.next_id;
                        state.next_id = state.next_id.wrapping_add(1);
                        let now = Instant::now();
                        let timer = self.spawn_idle_timer(key.clone(), entry_id, now);
                        #[cfg(test)]
                        {
                            state.idle_timers_spawned += 1;
                        }
                        state.entries.insert(
                            key.clone(),
                            Entry {
                                client: client.clone(),
                                id: entry_id,
                                last_used: now,
                                idle_timer: IdleTimerGuard(Some(timer)),
                            },
                        );
                        state.backoff.remove(&key);
                        remove_lru(&mut state.lru, &key);
                        state.lru.push_back(key.clone());
                        evicted = evict_overflow(&mut state, &self.options, &key);
                        self.count.store(state.entries.len(), Ordering::SeqCst);
                        state.inflight.remove(&key);
                        inflight.complete(Ok(Some(client.clone())));
                        true
                    }
                };
                if installed {
                    cancellation.disarm();
                }
                for stale in evicted {
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
                    state.backoff.insert(
                        key.clone(),
                        StartBackoff {
                            failures,
                            retry_at: Instant::now() + restart_backoff_delay(failures),
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

    /// The single idle timer for one entry. It sleeps until the entry's
    /// `last_used + idle_timeout`, re-sleeps when a later acquire renewed
    /// `last_used` or while the client is busy, and exits once the entry is
    /// gone or replaced (entry id check). Dropping the entry aborts it.
    fn spawn_idle_timer(&self, key: String, entry_id: u64, installed_at: Instant) -> AbortHandle {
        let state = Arc::clone(&self.state);
        let count = Arc::clone(&self.count);
        let ttl = self.idle_timeout();
        tokio::spawn(async move {
            let mut deadline = installed_at + ttl;
            loop {
                sleep_until(deadline).await;
                let stale = {
                    let mut state = state.lock().await;
                    let Some(entry) = state
                        .entries
                        .get_mut(&key)
                        .filter(|entry| entry.id == entry_id)
                    else {
                        return;
                    };
                    let now = Instant::now();
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
            state.backoff.remove(key);
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

    pub(super) fn len(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}
