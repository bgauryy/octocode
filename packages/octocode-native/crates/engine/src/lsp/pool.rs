use crate::error::{Error, Result};
use crate::lsp::client::{LspLease, NativeLspClient};
use crate::lsp::types::LspLanguageServerConfig;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, Notify, oneshot};
use tokio::task::AbortHandle;
use tokio::time::{Duration, Instant, sleep_until};

type ClientFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
/// Largest readiness wait in [`readiness_timeout`]; the cold-start budget
/// (`lsp::MAX_COLD_LSP_EXECUTION_BUDGET_MS`) is built on it.
pub(crate) const MAX_READINESS_TIMEOUT_MS: u64 = 120_000;

/// First delay after a failed start; doubles per consecutive failure.
const RESTART_BACKOFF_BASE: Duration = Duration::from_millis(250);
/// Cap on the start backoff window.
const RESTART_BACKOFF_MAX: Duration = Duration::from_secs(30);

trait PoolClient: Clone + Send + Sync + 'static {
    fn alive(&self) -> ClientFuture<bool>;
    /// Authoritative "in use" signal. The pool never stops a busy client: the
    /// idle timer re-arms while it is busy and LRU eviction skips it (allowing a
    /// temporary overflow instead). It MUST therefore stay `true` for the whole
    /// of any operation holding the client — in-flight requests AND any lease
    /// held across document syncs, readiness waits, and the gaps between the
    /// requests of one multi-request walk — not only while a single request is
    /// outstanding.
    fn busy(&self) -> bool;
    /// Resident memory of the server tree at its last sample; `0` when
    /// unsampled (no memory cap configured), which counts against nothing.
    fn resident_bytes(&self) -> u64;
    fn stop(&self) -> ClientFuture<()>;
    /// Same underlying client (a clone of it), not merely an equal config.
    fn same(&self, other: &Self) -> bool;
    /// Runs on a live pooled client before it is handed out again. A client
    /// whose readiness wait timed out waits again here, so a stored
    /// `timeout` never fails every later call while the server indexes.
    /// Concurrent acquires of the key share this one wait.
    fn resume(&self) -> ClientFuture<()>;
    /// A project config file the server reads once and never re-reads for a
    /// document it holds changed under it (see `lsp::project_config`): the
    /// pool replaces the server when it is idle.
    fn project_config_changed(&self) -> ClientFuture<bool>;
}

impl PoolClient for NativeLspClient {
    fn alive(&self) -> ClientFuture<bool> {
        let client = self.clone();
        Box::pin(async move { client.is_alive().await })
    }

    fn busy(&self) -> bool {
        // Counts requests, syncs, diagnostic waits, and caller leases.
        self.is_busy()
    }

    fn resident_bytes(&self) -> u64 {
        self.last_resident_bytes()
    }

    fn stop(&self) -> ClientFuture<()> {
        let client = self.clone();
        Box::pin(async move {
            let _ = client.stop().await;
        })
    }

    fn same(&self, other: &Self) -> bool {
        self.same_client(other)
    }

    fn resume(&self) -> ClientFuture<()> {
        let client = self.clone();
        Box::pin(async move {
            if let Some(timeout_ms) = readiness_timeout(client.language_id()) {
                client.resume_timed_out_readiness(timeout_ms).await;
            }
        })
    }

    fn project_config_changed(&self) -> ClientFuture<bool> {
        let client = self.clone();
        Box::pin(async move { client.project_config_changed().await })
    }
}

/// Acquire attempts [`GenericPool::acquire_leased`] makes when the entry it
/// was handed is replaced before it can be leased.
const LEASE_ATTEMPTS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LspPoolOptions {
    pub idle_timeout_ms: u64,
    pub max_entries: usize,
    /// Resident memory all pooled servers may hold together, in bytes: the
    /// pool stops idle least-recently-used servers while the sampled total is
    /// over it, never the one in use or a busy one. `0` means no budget.
    pub max_resident_bytes: u64,
    /// Options every client the pool starts takes.
    pub client: super::client::LspClientOptions,
}

/// Agent think time between calls often exceeds a minute, and a cold
/// language server costs seconds; an idle server holds only memory.
impl Default for LspPoolOptions {
    fn default() -> Self {
        Self {
            idle_timeout_ms: 300_000,
            max_entries: 4,
            max_resident_bytes: super::spawn_limits::default_memory_cap_bytes(),
            client: super::client::LspClientOptions::default(),
        }
    }
}

/// Aborts the entry's single idle-timer task when the entry is dropped
/// (clear, eviction, replacement), unless the timer itself is the remover.
struct IdleTimerGuard(Option<AbortHandle>);

impl IdleTimerGuard {
    fn disarm(&mut self) {
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

/// What [`GenericPool::retire_if_idle`] did with a client whose project
/// config changed.
enum Retirement {
    /// Removed from the pool; the caller stops it and starts a fresh one.
    Retired,
    /// A clear or replacement owns the in-flight start now.
    Superseded,
    /// In use, or no longer the pooled entry: left as it is.
    Busy,
}

struct Entry<C> {
    client: C,
    id: u64,
    /// Renewed on every successful acquire; the entry's one idle timer
    /// re-sleeps until `last_used + idle_timeout` instead of spawning anew.
    last_used: Instant,
    idle_timer: IdleTimerGuard,
}

/// Errors are shared between deduplicated waiters, so they are reference
/// counted instead of flattened to a string: any structured detail on
/// `Error` survives to the boundary.
type SharedError = Arc<Error>;
type SharedResult<C> = std::result::Result<Option<C>, SharedError>;

fn shared_error(reason: impl Into<String>) -> SharedError {
    Arc::new(Error::new(reason))
}

struct StartBackoff {
    failures: u32,
    retry_at: Instant,
    last_error: SharedError,
}

fn restart_backoff_delay(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(16);
    RESTART_BACKOFF_BASE
        .saturating_mul(1_u32 << exponent)
        .min(RESTART_BACKOFF_MAX)
}

struct InFlight<C> {
    result: StdMutex<Option<SharedResult<C>>>,
    notify: Notify,
    cancelled: AtomicBool,
}

impl<C: Clone> InFlight<C> {
    fn new() -> Self {
        Self {
            result: StdMutex::new(None),
            notify: Notify::new(),
            cancelled: AtomicBool::new(false),
        }
    }

    fn complete(&self, result: SharedResult<C>) {
        let mut slot = self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(result);
            self.notify.notify_waiters();
        }
    }

    async fn wait(&self) -> SharedResult<C> {
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

    fn cancel(&self) {
        // Wake current waiters immediately. A later acquire also observes this
        // bit and replaces the registration without racing async Drop cleanup.
        self.cancelled.store(true, Ordering::SeqCst);
        self.complete(Err(shared_error("LSP client startup was cancelled")));
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

struct State<C> {
    entries: HashMap<String, Entry<C>>,
    inflight: HashMap<String, Arc<InFlight<C>>>,
    backoff: HashMap<String, StartBackoff>,
    lru: VecDeque<String>,
    next_id: u64,
    #[cfg(test)]
    idle_timers_spawned: usize,
}

impl<C> Default for State<C> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            inflight: HashMap::new(),
            backoff: HashMap::new(),
            lru: VecDeque::new(),
            next_id: 1,
            #[cfg(test)]
            idle_timers_spawned: 0,
        }
    }
}

struct GenericPool<C> {
    options: LspPoolOptions,
    state: Arc<Mutex<State<C>>>,
    count: Arc<AtomicUsize>,
}

/// Spawn async cleanup from a `Drop`. Uses the runtime handle captured when
/// the guard was built (a `Drop` can run on a non-runtime thread, e.g. a napi
/// finalizer, where `Handle::try_current()` fails), falling back to the
/// current runtime only if none was captured.
fn spawn_cleanup<F>(runtime: Option<&Handle>, future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    if let Some(runtime) = runtime {
        runtime.spawn(future);
    } else if let Ok(runtime) = Handle::try_current() {
        runtime.spawn(future);
    }
}

struct StartCancellationGuard<C: PoolClient> {
    key: String,
    state: Arc<Mutex<State<C>>>,
    inflight: Arc<InFlight<C>>,
    client: Option<C>,
    armed: bool,
    runtime: Option<Handle>,
}

impl<C: PoolClient> StartCancellationGuard<C> {
    fn new(key: String, state: Arc<Mutex<State<C>>>, inflight: Arc<InFlight<C>>) -> Self {
        Self {
            key,
            state,
            inflight,
            client: None,
            armed: true,
            runtime: Handle::try_current().ok(),
        }
    }

    fn track_client(&mut self, client: C) {
        self.client = Some(client);
    }

    fn disarm(&mut self) {
        self.armed = false;
        self.client = None;
    }
}

impl<C: PoolClient> Drop for StartCancellationGuard<C> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.inflight.cancel();
        let key = self.key.clone();
        let state = Arc::clone(&self.state);
        let inflight = Arc::clone(&self.inflight);
        let client = self.client.take();
        spawn_cleanup(self.runtime.as_ref(), async move {
            // Never remove a replacement installed by a retry for the same key.
            {
                let mut state = state.lock().await;
                if inflight_is_current(&state, &key, &inflight) {
                    state.inflight.remove(&key);
                }
            }
            if let Some(client) = client {
                client.stop().await;
            }
        });
    }
}

struct StopOnDrop<C: PoolClient> {
    client: Option<C>,
    runtime: Option<Handle>,
}

impl<C: PoolClient> StopOnDrop<C> {
    fn new(client: C) -> Self {
        Self {
            client: Some(client),
            runtime: Handle::try_current().ok(),
        }
    }

    fn disarm(&mut self) {
        self.client = None;
    }
}

impl<C: PoolClient> Drop for StopOnDrop<C> {
    fn drop(&mut self) {
        let Some(client) = self.client.take() else {
            return;
        };
        spawn_cleanup(self.runtime.as_ref(), async move {
            client.stop().await;
        });
    }
}

async fn run_cancellation_safe_start<C, T, Fut>(
    client: C,
    future: Fut,
) -> std::result::Result<T, oneshot::error::RecvError>
where
    C: PoolClient,
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    // Dropping a raw start future can strand a spawned process before the
    // client publishes its child handle. Let startup finish in its supervisor;
    // a closed receiver then takes the normal full-client shutdown path.
    let (send, receive) = oneshot::channel();
    tokio::spawn(async move {
        let result = future.await;
        if send.send(result).is_err() {
            client.stop().await;
        }
    });
    receive.await
}

impl<C: PoolClient> GenericPool<C> {
    fn new(mut options: LspPoolOptions) -> Self {
        options.idle_timeout_ms = options.idle_timeout_ms.max(1);
        options.max_entries = options.max_entries.max(1);
        Self {
            options,
            state: Arc::new(Mutex::new(State::default())),
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn idle_timeout(&self) -> Duration {
        Duration::from_millis(self.options.idle_timeout_ms)
    }

    async fn acquire<F, Fut>(&self, key: String, factory: F) -> SharedResult<C>
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
    async fn acquire_leased<F, Fut, L>(
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

    async fn finish_start<Fut>(
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

    async fn clear(&self, key: &str) -> bool {
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

    async fn clear_all(&self) {
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

    fn len(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

fn inflight_is_current<C>(state: &State<C>, key: &str, expected: &Arc<InFlight<C>>) -> bool {
    state
        .inflight
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, expected))
}

fn touch_entry<C>(state: &mut State<C>, key: &str) -> bool {
    let Some(entry) = state.entries.get_mut(key) else {
        return false;
    };
    entry.last_used = Instant::now();
    remove_lru(&mut state.lru, key);
    state.lru.push_back(key.to_owned());
    true
}

/// Evict least-recently-used entries while over `max_entries` or, when the
/// pool has a memory budget, while the servers' sampled resident memory
/// together exceeds it, skipping `keep` and every busy client. When everything evictable is busy the pool
/// overflows temporarily; a later install or acquire retries the eviction and
/// idle expiry shrinks it too. Returned clients must be stopped by the caller
/// after the state lock is released.
fn evict_overflow<C: PoolClient>(
    state: &mut State<C>,
    options: &LspPoolOptions,
    keep: &str,
) -> Vec<C> {
    let mut evicted = Vec::new();
    let over_budget = |entries: &HashMap<String, Entry<C>>| {
        options.max_resident_bytes > 0
            && entries
                .values()
                .map(|entry| entry.client.resident_bytes())
                .fold(0_u64, u64::saturating_add)
                > options.max_resident_bytes
    };
    while state.entries.len() > options.max_entries || over_budget(&state.entries) {
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

fn remove_lru(lru: &mut VecDeque<String>, key: &str) {
    lru.retain(|candidate| candidate != key);
}

/// Start and (for languages that need it) wait for readiness of one pooled
/// client; the pool's factory for both acquire flavors. A readiness wait
/// that ends in `timeout` still installs the started server (it keeps
/// indexing); the next acquire waits again through [`PoolClient::resume`]
/// instead of failing on the stored `timeout`.
async fn start_client(
    config: LspLanguageServerConfig,
    options: super::client::LspClientOptions,
) -> SharedResult<NativeLspClient> {
    let client = NativeLspClient::with_options(config.clone(), options);
    let starting_client = client.clone();
    let start =
        run_cancellation_safe_start(client.clone(), async move { starting_client.start().await })
            .await
            .map_err(|error| shared_error(format!("LSP client startup task failed: {error}")))?;
    start.map_err(Arc::new)?;
    let mut cleanup = StopOnDrop::new(client.clone());
    if let Some(timeout_ms) = readiness_timeout(config.language_id.as_deref())
        && let Err(error) = client.wait_for_ready(Some(timeout_ms)).await
    {
        let _ = client.stop().await;
        cleanup.disarm();
        return Err(Arc::new(error));
    }
    cleanup.disarm();
    Ok(Some(client))
}

pub struct LspClientPool {
    inner: GenericPool<NativeLspClient>,
}

impl Default for LspClientPool {
    fn default() -> Self {
        Self::new(LspPoolOptions::default())
    }
}

impl LspClientPool {
    pub fn new(options: LspPoolOptions) -> Self {
        Self {
            inner: GenericPool::new(options),
        }
    }

    pub async fn acquire(
        &self,
        config: LspLanguageServerConfig,
    ) -> Result<Option<NativeLspClient>> {
        let key = canonical_lsp_key(&config)?;
        let options = self.inner.options.client;
        self.inner
            .acquire(key, || start_client(config, options))
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    /// Acquire a client already leased: the lease is taken under the pool
    /// lock, so the idle timer cannot stop the server between acquire and the
    /// caller's first request. Hold the lease for the whole operation.
    pub async fn acquire_leased(
        &self,
        config: LspLanguageServerConfig,
    ) -> Result<Option<(NativeLspClient, LspLease)>> {
        let key = canonical_lsp_key(&config)?;
        let options = self.inner.options.client;
        self.inner
            .acquire_leased(
                key,
                || start_client(config.clone(), options),
                NativeLspClient::lease,
            )
            .await
            .map_err(Arc::unwrap_or_clone)
    }

    pub async fn clear(&self, config: &LspLanguageServerConfig) -> Result<bool> {
        let key = canonical_lsp_key(config)?;
        Ok(self.inner.clear(&key).await)
    }

    pub async fn clear_all(&self) {
        self.inner.clear_all().await;
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How long a first request waits for the server's project load, per
/// language. Built-in servers and the languages a user's `lsp-servers.json`
/// commonly adds (sourcekit-lsp for Swift, bash-language-server) are listed.
fn readiness_timeout(language_id: Option<&str>) -> Option<u32> {
    use super::family::{is_c_family, is_typescript_family};
    // tsserver/typescript-language-server and pyright/jedi build a project
    // model before they can answer navigation queries; clangd parses the
    // compilation database + preamble; Metals and jdtls import the build.
    // Without a readiness wait a query can race that indexing and return a
    // partial/empty answer stripped of the partiality signal the lifecycle
    // contract promises.
    if is_typescript_family(language_id) {
        return Some(30_000);
    }
    if is_c_family(language_id) {
        return Some(20_000);
    }
    match language_id {
        Some("go" | "python") => Some(15_000),
        Some("rust") => Some(60_000),
        Some("java" | "scala") => Some(120_000),
        Some("csharp" | "swift") => Some(30_000),
        Some("shellscript") => Some(2_000),
        _ => None,
    }
}

pub fn canonical_lsp_key(config: &LspLanguageServerConfig) -> Result<String> {
    let workspace_root = normalize_workspace_root(&config.workspace_root)?;
    let env = config
        .env
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let initialization_options = crate::digest::canonical_json_keep_nulls(
        config
            .initialization_options
            .clone()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
    );
    serde_json::to_string(&serde_json::json!({
        "workspaceRoot": workspace_root,
        "command": config.command,
        "args": config.args.clone().unwrap_or_default(),
        "env": env,
        "languageId": config.language_id,
        "initializationOptions": initialization_options,
        "maxMemoryMb": config.max_memory_mb,
    }))
    .map_err(|error| Error::new(format!("Failed to serialize LSP pool key: {error}")))
}

/// Pool-key form of a workspace root: the real path (symlinks resolved) when
/// it exists, so a symlinked and a direct path share one server; otherwise the
/// lexical normalization of the absolute path.
fn normalize_workspace_root(root: &str) -> Result<String> {
    let path = Path::new(root);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| Error::new(format!("Failed to resolve current directory: {error}")))?
            .join(path)
    };
    if let Ok(real) = std::fs::canonicalize(&absolute) {
        return Ok(real.to_string_lossy().into_owned());
    }
    Ok(crate::text::lexical(&absolute)
        .to_string_lossy()
        .into_owned())
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
