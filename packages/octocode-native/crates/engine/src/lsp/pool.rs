use crate::error::{Error, Result, Status};
use crate::lsp::client::{LspLease, NativeLspClient};
use crate::lsp::types::JsLanguageServerConfig;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, Notify, oneshot};
use tokio::task::AbortHandle;
use tokio::time::{Duration, Instant, sleep_until};

type ClientFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub const MAX_READINESS_TIMEOUT_MS: u64 = 120_000;

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
    fn stop(&self) -> ClientFuture<()>;
    /// Same underlying client (a clone of it), not merely an equal config.
    fn same(&self, other: &Self) -> bool;
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

    fn stop(&self) -> ClientFuture<()> {
        let client = self.clone();
        Box::pin(async move {
            let _ = client.stop().await;
        })
    }

    fn same(&self, other: &Self) -> bool {
        self.same_client(other)
    }
}

/// Acquire attempts [`GenericPool::acquire_leased`] makes when the entry it
/// was handed is replaced before it can be leased.
const LEASE_ATTEMPTS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LspPoolOptions {
    pub idle_timeout_ms: u64,
    pub max_entries: usize,
}

impl Default for LspPoolOptions {
    fn default() -> Self {
        Self {
            idle_timeout_ms: 60_000,
            max_entries: 4,
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

struct Entry<C, M> {
    client: C,
    metadata: M,
    id: u64,
    /// Renewed on every successful acquire; the entry's one idle timer
    /// re-sleeps until `last_used + idle_timeout` instead of spawning anew.
    last_used: Instant,
    idle_timer: IdleTimerGuard,
}

/// Errors are shared between deduplicated waiters, so they are reference
/// counted instead of flattened to a string: status (and any structured
/// detail on `Error`) survives to the boundary.
type SharedError = Arc<Error>;
type SharedResult<C> = std::result::Result<Option<C>, SharedError>;

fn shared_error(status: Status, reason: impl Into<String>) -> SharedError {
    Arc::new(Error::new(status, reason))
}

/// Recover an owned `Error` at the pool boundary, preserving every field
/// (status, typed RPC detail). The last holder moves it out instead of cloning.
fn unshare_error(error: SharedError) -> Error {
    Arc::unwrap_or_clone(error)
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
        if let Ok(mut slot) = self.result.lock()
            && slot.is_none()
        {
            *slot = Some(result);
            self.notify.notify_waiters();
        }
    }

    async fn wait(&self) -> SharedResult<C> {
        loop {
            let notified = self.notify.notified();
            if let Ok(slot) = self.result.lock()
                && let Some(result) = slot.clone()
            {
                return result;
            }
            notified.await;
        }
    }

    fn cancel(&self) {
        // Wake current waiters immediately. A later acquire also observes this
        // bit and replaces the registration without racing async Drop cleanup.
        self.cancelled.store(true, Ordering::SeqCst);
        self.complete(Err(shared_error(
            Status::GenericFailure,
            "LSP client startup was cancelled",
        )));
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

struct State<C, M> {
    entries: HashMap<String, Entry<C, M>>,
    inflight: HashMap<String, Arc<InFlight<C>>>,
    backoff: HashMap<String, StartBackoff>,
    lru: VecDeque<String>,
    next_id: u64,
    #[cfg(test)]
    idle_timers_spawned: usize,
}

impl<C, M> Default for State<C, M> {
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

struct GenericPool<C, M> {
    options: LspPoolOptions,
    state: Arc<Mutex<State<C, M>>>,
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

struct StartCancellationGuard<C: PoolClient, M: Send + 'static> {
    key: String,
    state: Arc<Mutex<State<C, M>>>,
    inflight: Arc<InFlight<C>>,
    client: Option<C>,
    armed: bool,
    runtime: Option<Handle>,
}

impl<C: PoolClient, M: Send + 'static> StartCancellationGuard<C, M> {
    fn new(key: String, state: Arc<Mutex<State<C, M>>>, inflight: Arc<InFlight<C>>) -> Self {
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

impl<C: PoolClient, M: Send + 'static> Drop for StartCancellationGuard<C, M> {
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

impl<C: PoolClient, M: Clone + Send + Sync + 'static> GenericPool<C, M> {
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

    async fn acquire<F, Fut>(&self, key: String, metadata: M, factory: F) -> SharedResult<C>
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
                    let evicted = {
                        let mut state = self.state.lock().await;
                        if !inflight_is_current(&state, &key, &inflight)
                            || state.entries.get(&key).map(|entry| entry.id) != Some(entry_id)
                        {
                            None
                        } else {
                            touch_entry(&mut state, &key);
                            // A previously deferred (busy) eviction may be
                            // possible now.
                            let evicted =
                                evict_overflow(&mut state, self.options.max_entries, &key);
                            self.count.store(state.entries.len(), Ordering::SeqCst);
                            state.inflight.remove(&key);
                            inflight.complete(Ok(Some(client.clone())));
                            Some(evicted)
                        }
                    };
                    cancellation.disarm();
                    if let Some(evicted) = evicted {
                        for stale in evicted {
                            stale.stop().await;
                        }
                        return Ok(Some(client));
                    }
                    return inflight.wait().await;
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
                self.finish_start(key, metadata, inflight, factory()).await
            }
            Action::Start(inflight) => self.finish_start(key, metadata, inflight, factory()).await,
        }
    }

    /// [`acquire`](Self::acquire), then lease the client under the state lock
    /// while it is still the pooled entry. The idle timer and LRU eviction
    /// check `busy()` under the same lock, so no reaper can stop the client
    /// between acquire and lease. An entry replaced in that window is
    /// re-acquired (bounded); the last attempt leases what it got.
    async fn acquire_leased<F, Fut, L>(
        &self,
        key: String,
        metadata: M,
        factory: F,
        lease: impl Fn(&C) -> L,
    ) -> SharedResult<(C, L)>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = SharedResult<C>> + Send,
    {
        for attempt in 1..=LEASE_ATTEMPTS {
            let Some(client) = self
                .acquire(key.clone(), metadata.clone(), &factory)
                .await?
            else {
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
        metadata: M,
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
                                metadata,
                                id: entry_id,
                                last_used: now,
                                idle_timer: IdleTimerGuard(Some(timer)),
                            },
                        );
                        state.backoff.remove(&key);
                        remove_lru(&mut state.lru, &key);
                        state.lru.push_back(key.clone());
                        evicted = evict_overflow(&mut state, self.options.max_entries, &key);
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

    async fn metadata(&self) -> Vec<M> {
        self.state
            .lock()
            .await
            .entries
            .values()
            .map(|entry| entry.metadata.clone())
            .collect()
    }
}

fn inflight_is_current<C, M>(state: &State<C, M>, key: &str, expected: &Arc<InFlight<C>>) -> bool {
    state
        .inflight
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, expected))
}

fn touch_entry<C, M>(state: &mut State<C, M>, key: &str) -> bool {
    let Some(entry) = state.entries.get_mut(key) else {
        return false;
    };
    entry.last_used = Instant::now();
    remove_lru(&mut state.lru, key);
    state.lru.push_back(key.to_owned());
    true
}

/// Evict least-recently-used entries while over `max_entries`, skipping
/// `keep` and every busy client. When everything evictable is busy the pool
/// overflows temporarily; a later install or acquire retries the eviction and
/// idle expiry shrinks it too. Returned clients must be stopped by the caller
/// after the state lock is released.
fn evict_overflow<C: PoolClient, M>(
    state: &mut State<C, M>,
    max_entries: usize,
    keep: &str,
) -> Vec<C> {
    let mut evicted = Vec::new();
    while state.entries.len() > max_entries {
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
/// client; the pool's factory for both acquire flavors.
async fn start_client(config: JsLanguageServerConfig) -> SharedResult<NativeLspClient> {
    let client = NativeLspClient::new(config.clone());
    let starting_client = client.clone();
    let start =
        run_cancellation_safe_start(client.clone(), async move { starting_client.start().await })
            .await
            .map_err(|error| {
                shared_error(
                    Status::GenericFailure,
                    format!("LSP client startup task failed: {error}"),
                )
            })?;
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
    inner: GenericPool<NativeLspClient, JsLanguageServerConfig>,
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

    pub async fn acquire(&self, config: JsLanguageServerConfig) -> Result<Option<NativeLspClient>> {
        let key = canonical_lsp_key(&config)?;
        let factory_config = config.clone();
        self.inner
            .acquire(key, config, || start_client(factory_config))
            .await
            .map_err(unshare_error)
    }

    /// Acquire a client already leased: the lease is taken under the pool
    /// lock, so the idle timer cannot stop the server between acquire and the
    /// caller's first request. Hold the lease for the whole operation.
    pub async fn acquire_leased(
        &self,
        config: JsLanguageServerConfig,
    ) -> Result<Option<(NativeLspClient, LspLease)>> {
        let key = canonical_lsp_key(&config)?;
        let factory_config = config.clone();
        self.inner
            .acquire_leased(
                key,
                config,
                || start_client(factory_config.clone()),
                NativeLspClient::lease,
            )
            .await
            .map_err(unshare_error)
    }

    pub async fn clear(&self, config: &JsLanguageServerConfig) -> Result<bool> {
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

    pub async fn configs(&self) -> Vec<JsLanguageServerConfig> {
        self.inner.metadata().await
    }
}

fn readiness_timeout(language_id: Option<&str>) -> Option<u32> {
    match language_id {
        Some("go") => Some(15_000),
        Some("rust") => Some(60_000),
        Some("java") => Some(120_000),
        Some("csharp" | "swift") => Some(30_000),
        Some("shellscript") => Some(2_000),
        // tsserver/typescript-language-server and pyright/jedi build a project
        // model before they can answer navigation queries. clangd parses the
        // compilation database + preamble. Without a readiness wait a query can
        // race that indexing and return a partial/empty answer stripped of the
        // partiality signal the lifecycle contract promises.
        Some("typescript" | "typescriptreact" | "javascript" | "javascriptreact") => Some(30_000),
        Some("python") => Some(15_000),
        Some("c" | "cpp" | "cuda") => Some(20_000),
        _ => None,
    }
}

pub fn canonical_lsp_key(config: &JsLanguageServerConfig) -> Result<String> {
    let workspace_root = normalize_workspace_root(&config.workspace_root)?;
    let env = config
        .env
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let initialization_options = canonical_json(
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
    .map_err(|error| {
        Error::new(
            Status::GenericFailure,
            format!("Failed to serialize LSP pool key: {error}"),
        )
    })
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
            .map_err(|error| {
                Error::new(
                    Status::GenericFailure,
                    format!("Failed to resolve current directory: {error}"),
                )
            })?
            .join(path)
    };
    if let Ok(real) = std::fs::canonicalize(&absolute) {
        return Ok(real.to_string_lossy().into_owned());
    }
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized.to_string_lossy().into_owned())
}

/// Deep copy of `value` with every object's keys sorted, so equal JSON
/// hashes and compares equal regardless of key order.
pub fn canonical_json(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(canonical_json).collect()),
        Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect::<BTreeMap<_, _>>();
            let mut canonical = serde_json::Map::new();
            canonical.extend(sorted);
            Value::Object(canonical)
        }
        scalar => scalar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::oneshot;

    #[derive(Clone)]
    struct FakeClient {
        id: usize,
        alive: Arc<AtomicBool>,
        health_gate: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
        health_checks: Arc<AtomicUsize>,
        stops: Arc<AtomicUsize>,
        busy: Arc<AtomicBool>,
    }

    impl FakeClient {
        fn new(id: usize) -> Self {
            Self {
                id,
                alive: Arc::new(AtomicBool::new(true)),
                health_gate: Arc::new(Mutex::new(None)),
                health_checks: Arc::new(AtomicUsize::new(0)),
                stops: Arc::new(AtomicUsize::new(0)),
                busy: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl std::fmt::Debug for FakeClient {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("FakeClient")
                .field("id", &self.id)
                .finish()
        }
    }

    impl PoolClient for FakeClient {
        fn alive(&self) -> ClientFuture<bool> {
            let alive = Arc::clone(&self.alive);
            let gate = Arc::clone(&self.health_gate);
            let checks = Arc::clone(&self.health_checks);
            Box::pin(async move {
                checks.fetch_add(1, Ordering::SeqCst);
                if let Some(receiver) = gate.lock().await.take() {
                    let _ = receiver.await;
                }
                alive.load(Ordering::SeqCst)
            })
        }

        fn busy(&self) -> bool {
            self.busy.load(Ordering::SeqCst)
        }

        fn stop(&self) -> ClientFuture<()> {
            let alive = Arc::clone(&self.alive);
            let stops = Arc::clone(&self.stops);
            Box::pin(async move {
                alive.store(false, Ordering::SeqCst);
                stops.fetch_add(1, Ordering::SeqCst);
            })
        }

        fn same(&self, other: &Self) -> bool {
            Arc::ptr_eq(&self.alive, &other.alive)
        }
    }

    fn pool(max_entries: usize, idle_timeout_ms: u64) -> GenericPool<FakeClient, usize> {
        GenericPool::new(LspPoolOptions {
            max_entries,
            idle_timeout_ms,
        })
    }

    #[tokio::test]
    async fn concurrent_starts_are_deduplicated() {
        let pool = Arc::new(pool(4, 60_000));
        let starts = Arc::new(AtomicUsize::new(0));
        let (send, receive) = oneshot::channel();
        let gate = Arc::new(Mutex::new(Some(receive)));
        let first = {
            let pool = Arc::clone(&pool);
            let starts = Arc::clone(&starts);
            let gate = Arc::clone(&gate);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async move {
                    starts.fetch_add(1, Ordering::SeqCst);
                    if let Some(receiver) = gate.lock().await.take() {
                        let _ = receiver.await;
                    }
                    Ok(Some(FakeClient::new(1)))
                })
                .await
            })
        };
        tokio::task::yield_now().await;
        let second = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) })
                    .await
            })
        };
        let _ = send.send(());
        let first_client = first
            .await
            .expect("first task")
            .expect("first acquire")
            .expect("client");
        let second_client = second
            .await
            .expect("second task")
            .expect("second acquire")
            .expect("client");
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(first_client.id, second_client.id);
    }

    #[tokio::test]
    async fn cancelled_start_does_not_strand_inflight_or_block_retry() {
        let pool = Arc::new(pool(4, 60_000));
        let (entered_send, entered_receive) = oneshot::channel();
        let (_release_send, release_receive) = oneshot::channel::<()>();
        let first = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async move {
                    let _ = entered_send.send(());
                    let _ = release_receive.await;
                    Ok(Some(FakeClient::new(1)))
                })
                .await
            })
        };

        entered_receive.await.expect("startup entered");
        first.abort();
        let cancelled = first.await;
        assert!(matches!(cancelled, Err(error) if error.is_cancelled()));

        tokio::time::timeout(Duration::from_secs(1), async {
            while pool.state.lock().await.inflight.contains_key("k") {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled startup registration must be removed");

        let retried = tokio::time::timeout(
            Duration::from_secs(1),
            pool.acquire("k".into(), 2, || async { Ok(Some(FakeClient::new(2))) }),
        )
        .await
        .expect("cancelled startup must not block a retry")
        .expect("retry")
        .expect("client");
        assert_eq!(retried.id, 2);
        assert!(pool.state.lock().await.inflight.is_empty());
    }

    #[tokio::test]
    async fn cancelled_install_stops_the_uninstalled_client_and_allows_retry() {
        let pool = Arc::new(pool(4, 60_000));
        let uninstalled = FakeClient::new(1);
        let uninstalled_check = uninstalled.clone();
        let (entered_send, entered_receive) = oneshot::channel();
        let (release_send, release_receive) = oneshot::channel::<()>();
        let first = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async move {
                    let _ = entered_send.send(());
                    let _ = release_receive.await;
                    Ok(Some(uninstalled))
                })
                .await
            })
        };

        entered_receive.await.expect("startup entered");
        let state = pool.state.lock().await;
        let _ = release_send.send(());
        tokio::task::yield_now().await;
        first.abort();
        let cancelled = first.await;
        assert!(matches!(cancelled, Err(error) if error.is_cancelled()));
        drop(state);

        tokio::time::timeout(Duration::from_secs(1), async {
            while uninstalled_check.stops.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled install must stop its client");
        assert_eq!(uninstalled_check.stops.load(Ordering::SeqCst), 1);

        let retried = tokio::time::timeout(
            Duration::from_secs(1),
            pool.acquire("k".into(), 2, || async { Ok(Some(FakeClient::new(2))) }),
        )
        .await
        .expect("cancelled install must not block a retry")
        .expect("retry")
        .expect("client");
        assert_eq!(retried.id, 2);
    }

    #[tokio::test]
    async fn cancelled_health_check_does_not_strand_inflight_or_stop_live_client() {
        let pool = Arc::new(pool(4, 60_000));
        let client = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("install")
            .expect("client");
        let (_release_send, release_receive) = oneshot::channel();
        *client.health_gate.lock().await = Some(release_receive);
        let checking = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) })
                    .await
            })
        };
        tokio::time::timeout(Duration::from_secs(1), async {
            while client.health_checks.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("health check must enter its gate");
        checking.abort();
        let cancelled = checking.await;
        assert!(matches!(cancelled, Err(error) if error.is_cancelled()));

        let reused = tokio::time::timeout(
            Duration::from_secs(1),
            pool.acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) }),
        )
        .await
        .expect("cancelled health check must not block reuse")
        .expect("reuse")
        .expect("client");
        assert_eq!(reused.id, 1);
        assert_eq!(client.stops.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn cancelled_supervised_start_stops_client_after_start_finishes() {
        let client = FakeClient::new(1);
        let check = client.clone();
        let (entered_send, entered_receive) = oneshot::channel();
        let (release_send, release_receive) = oneshot::channel::<()>();
        let startup = tokio::spawn(run_cancellation_safe_start(client, async move {
            let _ = entered_send.send(());
            let _ = release_receive.await;
        }));
        entered_receive.await.expect("startup entered");
        startup.abort();
        let cancelled = startup.await;
        assert!(matches!(cancelled, Err(error) if error.is_cancelled()));
        let _ = release_send.send(());
        tokio::time::timeout(Duration::from_secs(1), async {
            while check.stops.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("orphaned startup must stop its client after it finishes");
        assert_eq!(check.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_health_checks_are_deduplicated_and_dead_clients_retry() {
        let pool = Arc::new(pool(4, 60_000));
        let old = FakeClient::new(1);
        let installed = pool
            .acquire("k".into(), 1, || async { Ok(Some(old.clone())) })
            .await
            .expect("install")
            .expect("client");
        installed.alive.store(false, Ordering::SeqCst);
        let starts = Arc::new(AtomicUsize::new(0));
        let first = {
            let pool = Arc::clone(&pool);
            let starts = Arc::clone(&starts);
            tokio::spawn(async move {
                pool.acquire("k".into(), 2, || async move {
                    starts.fetch_add(1, Ordering::SeqCst);
                    Ok(Some(FakeClient::new(2)))
                })
                .await
            })
        };
        let second = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 2, || async { Ok(Some(FakeClient::new(3))) })
                    .await
            })
        };
        let a = first
            .await
            .expect("first")
            .expect("acquire")
            .expect("client");
        let b = second
            .await
            .expect("second")
            .expect("acquire")
            .expect("client");
        assert_eq!(a.id, 2);
        assert_eq!(b.id, 2);
        assert_eq!(starts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn idle_use_renews_expiry_and_stale_timers_do_not_remove_replacements() {
        let pool = pool(4, 100);
        let first = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("install")
            .expect("client");
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(75)).await;
        let reused = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) })
            .await
            .expect("reuse")
            .expect("client");
        assert_eq!(reused.id, 1);
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 1);
        tokio::time::advance(Duration::from_millis(51)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 0);
        assert_eq!(first.stops.load(Ordering::SeqCst), 1);
    }

    /// The lease is taken while the pool state lock is held (the same lock the
    /// idle timer and eviction take to read `busy()`), so no reaper can stop
    /// the client between acquire and lease.
    #[tokio::test(start_paused = true)]
    async fn acquire_leased_takes_the_lease_under_the_pool_lock() {
        struct Busy(Arc<AtomicBool>);
        impl Drop for Busy {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let pool = pool(4, 100);
        let state = Arc::clone(&pool.state);
        let lease = |client: &FakeClient| {
            assert!(state.try_lock().is_err(), "lease taken under the pool lock");
            client.busy.store(true, Ordering::SeqCst);
            Busy(Arc::clone(&client.busy))
        };
        let (client, held) = pool
            .acquire_leased(
                "k".into(),
                1,
                || async { Ok(Some(FakeClient::new(1))) },
                lease,
            )
            .await
            .expect("install")
            .expect("client");
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(250)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 1, "a leased client outlives the idle timeout");
        assert_eq!(client.stops.load(Ordering::SeqCst), 0);
        // A reused entry is leased the same way.
        let (again, second) = pool
            .acquire_leased(
                "k".into(),
                1,
                || async { Ok(Some(FakeClient::new(2))) },
                lease,
            )
            .await
            .expect("reuse")
            .expect("client");
        assert_eq!(again.id, 1);
        drop((held, second));
        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 0);
        assert_eq!(client.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn active_request_prevents_idle_shutdown_until_request_finishes() {
        let pool = pool(4, 100);
        let client = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("install")
            .expect("client");
        client.busy.store(true, Ordering::SeqCst);
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 1);
        assert_eq!(client.stops.load(Ordering::SeqCst), 0);

        client.busy.store(false, Ordering::SeqCst);
        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 0);
        assert_eq!(client.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn successful_use_drives_lru_eviction() {
        let pool = pool(2, 60_000);
        let one = pool
            .acquire("1".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("one")
            .expect("client");
        let two = pool
            .acquire("2".into(), 2, || async { Ok(Some(FakeClient::new(2))) })
            .await
            .expect("two")
            .expect("client");
        let _ = pool
            .acquire("1".into(), 1, || async { Ok(Some(FakeClient::new(9))) })
            .await
            .expect("touch");
        let _ = pool
            .acquire("3".into(), 3, || async { Ok(Some(FakeClient::new(3))) })
            .await
            .expect("three");
        assert_eq!(one.stops.load(Ordering::SeqCst), 0);
        assert_eq!(two.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn clear_during_start_invalidates_late_completion() {
        let pool = Arc::new(pool(4, 60_000));
        let late = FakeClient::new(1);
        let late_check = late.clone();
        let (send, receive) = oneshot::channel();
        let task = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async move {
                    let _ = receive.await;
                    Ok(Some(late))
                })
                .await
            })
        };
        tokio::task::yield_now().await;
        assert!(pool.clear("k").await);
        let _ = send.send(());
        assert!(task.await.expect("task").expect("acquire").is_none());
        assert_eq!(late_check.stops.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 0);
    }

    #[tokio::test]
    async fn clear_during_health_invalidates_waiters_and_shutdown_stops_all() {
        let pool = Arc::new(pool(4, 60_000));
        let client = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("install")
            .expect("client");
        let (send, receive) = oneshot::channel();
        *client.health_gate.lock().await = Some(receive);
        let task = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move {
                pool.acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) })
                    .await
            })
        };
        tokio::task::yield_now().await;
        assert!(pool.clear("k").await);
        let _ = send.send(());
        assert!(task.await.expect("task").expect("acquire").is_none());
        assert_eq!(client.stops.load(Ordering::SeqCst), 1);
        let other = pool
            .acquire("other".into(), 2, || async { Ok(Some(FakeClient::new(3))) })
            .await
            .expect("other")
            .expect("client");
        pool.clear_all().await;
        assert_eq!(other.stops.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn readiness_timeout_gates_the_slow_indexing_languages() {
        // Languages whose servers index before answering must get a readiness
        // wait so a query cannot race indexing and return a partial/empty answer
        // without the partiality signal.
        for language in [
            "go",
            "rust",
            "java",
            "csharp",
            "swift",
            "shellscript",
            "typescript",
            "typescriptreact",
            "javascript",
            "python",
            "c",
            "cpp",
            "cuda",
        ] {
            assert!(
                readiness_timeout(Some(language)).is_some(),
                "{language} must gate on indexing readiness"
            );
        }
        // Preserve existing budgets for the already-wired set.
        assert_eq!(readiness_timeout(Some("go")), Some(15_000));
        assert_eq!(readiness_timeout(Some("rust")), Some(60_000));
        assert_eq!(readiness_timeout(Some("java")), Some(120_000));
        assert_eq!(readiness_timeout(Some("csharp")), Some(30_000));
        assert_eq!(readiness_timeout(Some("swift")), Some(30_000));
        assert_eq!(readiness_timeout(Some("shellscript")), Some(2_000));
        // Newly-gated languages.
        assert_eq!(readiness_timeout(Some("typescript")), Some(30_000));
        assert_eq!(readiness_timeout(Some("typescriptreact")), Some(30_000));
        assert_eq!(readiness_timeout(Some("javascript")), Some(30_000));
        assert_eq!(readiness_timeout(Some("python")), Some(15_000));
        assert_eq!(readiness_timeout(Some("c")), Some(20_000));
        assert_eq!(readiness_timeout(Some("cpp")), Some(20_000));
        assert_eq!(readiness_timeout(Some("cuda")), Some(20_000));
        // An unknown/unlisted language still opts out of the readiness wait.
        assert_eq!(readiness_timeout(Some("plaintext")), None);
        assert_eq!(readiness_timeout(None), None);
    }

    #[test]
    fn canonical_key_normalizes_root_and_all_effective_config() {
        let root = std::env::current_dir().expect("cwd");
        let mut env_a = HashMap::new();
        env_a.insert("B".into(), "2".into());
        env_a.insert("A".into(), "1".into());
        let mut env_b = HashMap::new();
        env_b.insert("A".into(), "1".into());
        env_b.insert("B".into(), "2".into());
        let first = JsLanguageServerConfig {
            command: "server".into(),
            args: Some(vec!["--stdio".into()]),
            workspace_root: root.join("x/../workspace").to_string_lossy().into_owned(),
            language_id: Some("rust".into()),
            initialization_options: Some(serde_json::json!({"z": 1, "nested": {"b": 2, "a": 1}})),
            env: Some(env_a),
            max_memory_mb: None,
        };
        let second = JsLanguageServerConfig {
            workspace_root: root.join("workspace").to_string_lossy().into_owned(),
            initialization_options: Some(serde_json::json!({"nested": {"a": 1, "b": 2}, "z": 1})),
            env: Some(env_b),
            max_memory_mb: None,
            ..first.clone()
        };
        assert_eq!(
            canonical_lsp_key(&first).expect("key"),
            canonical_lsp_key(&second).expect("key")
        );
        let mut changed = second;
        changed.args = Some(vec!["--socket".into()]);
        assert_ne!(
            canonical_lsp_key(&first).expect("key"),
            canonical_lsp_key(&changed).expect("changed key")
        );
        let mut capped = first.clone();
        capped.max_memory_mb = Some(1_024);
        assert_ne!(
            canonical_lsp_key(&first).expect("key"),
            canonical_lsp_key(&capped).expect("capped key"),
            "the memory cap is effective config; differently-capped servers must not share a pool entry"
        );
    }

    #[tokio::test]
    async fn lru_eviction_never_stops_a_busy_client_and_evicts_it_once_idle() {
        let pool = pool(1, 60_000);
        let busy = pool
            .acquire("busy".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("busy")
            .expect("client");
        busy.busy.store(true, Ordering::SeqCst);
        let fresh = pool
            .acquire("fresh".into(), 2, || async { Ok(Some(FakeClient::new(2))) })
            .await
            .expect("fresh")
            .expect("client");
        assert_eq!(
            busy.stops.load(Ordering::SeqCst),
            0,
            "the busy LRU entry must survive eviction"
        );
        assert_eq!(pool.len(), 2, "all-busy overflow is allowed temporarily");

        busy.busy.store(false, Ordering::SeqCst);
        let _ = pool
            .acquire("fresh".into(), 2, || async { Ok(Some(FakeClient::new(3))) })
            .await
            .expect("touch");
        assert_eq!(
            busy.stops.load(Ordering::SeqCst),
            1,
            "deferred eviction runs once idle"
        );
        assert_eq!(fresh.stops.load(Ordering::SeqCst), 0);
        assert_eq!(pool.len(), 1);
    }

    #[tokio::test]
    async fn eviction_skips_busy_oldest_and_takes_next_idle() {
        let pool = pool(2, 60_000);
        let oldest = pool
            .acquire("1".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("one")
            .expect("client");
        oldest.busy.store(true, Ordering::SeqCst);
        let second = pool
            .acquire("2".into(), 2, || async { Ok(Some(FakeClient::new(2))) })
            .await
            .expect("two")
            .expect("client");
        let _ = pool
            .acquire("3".into(), 3, || async { Ok(Some(FakeClient::new(3))) })
            .await
            .expect("three");
        assert_eq!(oldest.stops.load(Ordering::SeqCst), 0);
        assert_eq!(second.stops.load(Ordering::SeqCst), 1);
        assert_eq!(pool.len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn one_idle_timer_per_entry_across_many_acquires() {
        let pool = pool(4, 100);
        let client = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("install")
            .expect("client");
        for _ in 0..10 {
            tokio::time::advance(Duration::from_millis(20)).await;
            let _ = pool
                .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) })
                .await
                .expect("reuse");
        }
        assert_eq!(pool.state.lock().await.idle_timers_spawned, 1);
        assert_eq!(pool.len(), 1);
        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 0);
        assert_eq!(client.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn cleared_entry_aborts_its_idle_timer_and_replacement_keeps_its_own() {
        let pool = pool(4, 100);
        let first = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("install")
            .expect("client");
        assert!(pool.clear("k").await);
        tokio::time::advance(Duration::from_millis(60)).await;
        let second = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(2))) })
            .await
            .expect("replace")
            .expect("client");
        // The first entry's deadline (t=100) must not remove the replacement.
        tokio::time::advance(Duration::from_millis(60)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 1);
        assert_eq!(second.stops.load(Ordering::SeqCst), 0);
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert_eq!(pool.len(), 0);
        assert_eq!(second.stops.load(Ordering::SeqCst), 1);
        assert_eq!(first.stops.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stop_on_drop_outside_a_runtime_thread_still_stops_the_client() {
        let client = FakeClient::new(1);
        let check = client.clone();
        let guard = StopOnDrop::new(client);
        std::thread::spawn(move || drop(guard))
            .join()
            .expect("drop on a non-runtime thread");
        tokio::time::timeout(Duration::from_secs(1), async {
            while check.stops.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cleanup must use the handle captured at construction");
    }

    #[tokio::test]
    async fn start_cancellation_guard_dropped_off_runtime_releases_inflight() {
        let pool = pool(4, 60_000);
        let client = FakeClient::new(1);
        let check = client.clone();
        let inflight = Arc::new(InFlight::new());
        pool.state
            .lock()
            .await
            .inflight
            .insert("k".into(), Arc::clone(&inflight));
        let mut guard =
            StartCancellationGuard::new("k".into(), Arc::clone(&pool.state), Arc::clone(&inflight));
        guard.track_client(client);
        std::thread::spawn(move || drop(guard))
            .join()
            .expect("drop on a non-runtime thread");
        tokio::time::timeout(Duration::from_secs(1), async {
            while check.stops.load(Ordering::SeqCst) == 0
                || pool.state.lock().await.inflight.contains_key("k")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancellation cleanup must run off-runtime");
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_start_failures_back_off_exponentially_and_reset_on_success() {
        let pool = pool(4, 60_000);
        let starts = Arc::new(AtomicUsize::new(0));
        let failing = |starts: Arc<AtomicUsize>| {
            move || async move {
                starts.fetch_add(1, Ordering::SeqCst);
                Err(shared_error(Status::GenericFailure, "spawn failed"))
            }
        };

        let first = pool
            .acquire("k".into(), 1, failing(Arc::clone(&starts)))
            .await
            .expect_err("first failure");
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        // Inside the 250ms window: fail fast with the same error, no start.
        let fast = pool
            .acquire("k".into(), 1, failing(Arc::clone(&starts)))
            .await
            .expect_err("backing off");
        assert!(Arc::ptr_eq(&first, &fast));
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_millis(251)).await;
        let _ = pool
            .acquire("k".into(), 1, failing(Arc::clone(&starts)))
            .await
            .expect_err("second failure");
        assert_eq!(starts.load(Ordering::SeqCst), 2);

        // Second failure doubles the window to 500ms.
        tokio::time::advance(Duration::from_millis(300)).await;
        let _ = pool
            .acquire("k".into(), 1, failing(Arc::clone(&starts)))
            .await
            .expect_err("still backing off");
        assert_eq!(starts.load(Ordering::SeqCst), 2);

        tokio::time::advance(Duration::from_millis(201)).await;
        let client = pool
            .acquire("k".into(), 1, || async { Ok(Some(FakeClient::new(1))) })
            .await
            .expect("recovered")
            .expect("client");
        assert_eq!(client.id, 1);

        // Success resets the backoff: the next failure waits only 250ms.
        assert!(pool.clear("k").await);
        let _ = pool
            .acquire("k".into(), 1, failing(Arc::clone(&starts)))
            .await
            .expect_err("failure after reset");
        assert_eq!(starts.load(Ordering::SeqCst), 3);
        tokio::time::advance(Duration::from_millis(251)).await;
        let _ = pool
            .acquire("k".into(), 1, failing(Arc::clone(&starts)))
            .await
            .expect_err("retried after base window");
        assert_eq!(starts.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn restart_backoff_is_exponential_and_capped() {
        assert_eq!(restart_backoff_delay(1), Duration::from_millis(250));
        assert_eq!(restart_backoff_delay(2), Duration::from_millis(500));
        assert_eq!(restart_backoff_delay(3), Duration::from_secs(1));
        assert_eq!(restart_backoff_delay(8), Duration::from_secs(30));
        assert_eq!(restart_backoff_delay(u32::MAX), Duration::from_secs(30));
    }

    #[tokio::test]
    async fn start_errors_keep_their_status_through_the_pool() {
        let pool = pool(4, 60_000);
        let error = pool
            .acquire("k".into(), 1, || async {
                Err(shared_error(Status::InvalidArg, "bad server config"))
            })
            .await
            .expect_err("start failure");
        assert_eq!(error.status, Status::InvalidArg);
        assert_eq!(error.reason, "bad server config");
        let owned = unshare_error(error);
        assert_eq!(owned.status, Status::InvalidArg);
        let shared = shared_error(Status::InvalidArg, "shared");
        let _other_holder = Arc::clone(&shared);
        assert_eq!(unshare_error(shared).status, Status::InvalidArg);
    }

    #[cfg(unix)]
    #[test]
    fn canonical_key_follows_workspace_symlinks() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "octocode-pool-symlink-{}-{nanos}",
            std::process::id()
        ));
        let real = base.join("real");
        let link = base.join("link");
        std::fs::create_dir_all(&real).expect("create real root");
        std::os::unix::fs::symlink(&real, &link).expect("create symlink");
        let config = |root: &Path| JsLanguageServerConfig {
            command: "server".into(),
            args: None,
            workspace_root: root.to_string_lossy().into_owned(),
            language_id: Some("rust".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
        };
        let direct = canonical_lsp_key(&config(&real)).expect("direct key");
        let via_link = canonical_lsp_key(&config(&link)).expect("symlink key");
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(
            direct, via_link,
            "a symlinked root must share the pool entry"
        );
    }
}
