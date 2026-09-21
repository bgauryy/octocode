use crate::error::{Error, Result, Status};
use crate::lsp::client::NativeLspClient;
use crate::lsp::types::JsLanguageServerConfig;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex, Notify, oneshot};
use tokio::time::{Duration, sleep};

type ClientFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
pub const MAX_READINESS_TIMEOUT_MS: u64 = 120_000;

trait PoolClient: Clone + Send + Sync + 'static {
    fn alive(&self) -> ClientFuture<bool>;
    fn busy(&self) -> bool;
    fn stop(&self) -> ClientFuture<()>;
}

impl PoolClient for NativeLspClient {
    fn alive(&self) -> ClientFuture<bool> {
        let client = self.clone();
        Box::pin(async move { client.is_alive().await })
    }

    fn busy(&self) -> bool {
        self.has_active_requests()
    }

    fn stop(&self) -> ClientFuture<()> {
        let client = self.clone();
        Box::pin(async move {
            let _ = client.stop().await;
        })
    }
}

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

struct Entry<C, M> {
    client: C,
    metadata: M,
    id: u64,
    idle_generation: u64,
}

type SharedResult<C> = std::result::Result<Option<C>, String>;

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
        self.complete(Err("LSP client startup was cancelled".into()));
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

struct State<C, M> {
    entries: HashMap<String, Entry<C, M>>,
    inflight: HashMap<String, Arc<InFlight<C>>>,
    lru: VecDeque<String>,
    next_id: u64,
    next_idle_generation: u64,
}

impl<C, M> Default for State<C, M> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            inflight: HashMap::new(),
            lru: VecDeque::new(),
            next_id: 1,
            next_idle_generation: 1,
        }
    }
}

struct GenericPool<C, M> {
    options: LspPoolOptions,
    state: Arc<Mutex<State<C, M>>>,
    count: Arc<AtomicUsize>,
}

struct StartCancellationGuard<C: PoolClient, M: Send + 'static> {
    key: String,
    state: Arc<Mutex<State<C, M>>>,
    inflight: Arc<InFlight<C>>,
    client: Option<C>,
    armed: bool,
}

impl<C: PoolClient, M: Send + 'static> StartCancellationGuard<C, M> {
    fn new(key: String, state: Arc<Mutex<State<C, M>>>, inflight: Arc<InFlight<C>>) -> Self {
        Self {
            key,
            state,
            inflight,
            client: None,
            armed: true,
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
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
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
}

impl<C: PoolClient> StopOnDrop<C> {
    fn new(client: C) -> Self {
        Self {
            client: Some(client),
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
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
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
        }

        let action = {
            let mut state = self.state.lock().await;
            let current = state.inflight.get(&key).cloned();
            if let Some(inflight) = current.filter(|inflight| !inflight.is_cancelled()) {
                Action::Wait(inflight)
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
                    let timer = {
                        let mut state = self.state.lock().await;
                        if !inflight_is_current(&state, &key, &inflight)
                            || state.entries.get(&key).map(|entry| entry.id) != Some(entry_id)
                        {
                            None
                        } else {
                            let generation = touch_entry(&mut state, &key);
                            state.inflight.remove(&key);
                            inflight.complete(Ok(Some(client.clone())));
                            generation
                        }
                    };
                    if let Some(generation) = timer {
                        cancellation.disarm();
                        self.spawn_idle_timer(key, entry_id, generation);
                        return Ok(Some(client));
                    }
                    cancellation.disarm();
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
                        let generation = state.next_idle_generation;
                        state.next_idle_generation = state.next_idle_generation.wrapping_add(1);
                        state.entries.insert(
                            key.clone(),
                            Entry {
                                client: client.clone(),
                                metadata,
                                id: entry_id,
                                idle_generation: generation,
                            },
                        );
                        remove_lru(&mut state.lru, &key);
                        state.lru.push_back(key.clone());
                        while state.entries.len() > self.options.max_entries {
                            let Some(oldest) = state.lru.pop_front() else {
                                break;
                            };
                            if oldest != key
                                && let Some(entry) = state.entries.remove(&oldest)
                            {
                                evicted.push(entry.client);
                            }
                        }
                        self.count.store(state.entries.len(), Ordering::SeqCst);
                        state.inflight.remove(&key);
                        inflight.complete(Ok(Some(client.clone())));
                        self.spawn_idle_timer(key.clone(), entry_id, generation);
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
                    state.inflight.remove(&key);
                    inflight.complete(Err(error));
                }
                drop(state);
                cancellation.disarm();
                inflight.wait().await
            }
        }
    }

    fn spawn_idle_timer(&self, key: String, entry_id: u64, generation: u64) {
        let state = Arc::clone(&self.state);
        let count = Arc::clone(&self.count);
        let duration = Duration::from_millis(self.options.idle_timeout_ms);
        tokio::spawn(async move {
            loop {
                sleep(duration).await;
                let stale = {
                    let mut state = state.lock().await;
                    let current = state.entries.get(&key).filter(|entry| {
                        entry.id == entry_id && entry.idle_generation == generation
                    });
                    if current.is_some_and(|entry| entry.client.busy()) {
                        continue;
                    }
                    if current.is_some() {
                        remove_lru(&mut state.lru, &key);
                        let removed = state.entries.remove(&key).map(|entry| entry.client);
                        count.store(state.entries.len(), Ordering::SeqCst);
                        removed
                    } else {
                        None
                    }
                };
                if let Some(client) = stale {
                    client.stop().await;
                }
                break;
            }
        });
    }

    async fn clear(&self, key: &str) -> bool {
        let (client, inflight) = {
            let mut state = self.state.lock().await;
            remove_lru(&mut state.lru, key);
            let client = state.entries.remove(key).map(|entry| entry.client);
            self.count.store(state.entries.len(), Ordering::SeqCst);
            let inflight = state.inflight.remove(key);
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

fn touch_entry<C, M>(state: &mut State<C, M>, key: &str) -> Option<u64> {
    let generation = state.next_idle_generation;
    state.next_idle_generation = state.next_idle_generation.wrapping_add(1);
    let entry = state.entries.get_mut(key)?;
    entry.idle_generation = generation;
    remove_lru(&mut state.lru, key);
    state.lru.push_back(key.to_owned());
    Some(generation)
}

fn remove_lru(lru: &mut VecDeque<String>, key: &str) {
    lru.retain(|candidate| candidate != key);
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
            .acquire(key, config, || async move {
                let client = NativeLspClient::new(factory_config.clone());
                let starting_client = client.clone();
                let start = run_cancellation_safe_start(client.clone(), async move {
                    starting_client.start().await
                })
                .await
                .map_err(|error| format!("LSP client startup task failed: {error}"))?;
                if let Err(error) = start {
                    return Err(error.to_string());
                }
                let mut cleanup = StopOnDrop::new(client.clone());
                if let Some(timeout_ms) = readiness_timeout(factory_config.language_id.as_deref())
                    && let Err(error) = client.wait_for_ready(Some(timeout_ms)).await
                {
                    let _ = client.stop().await;
                    cleanup.disarm();
                    return Err(error.to_string());
                }
                cleanup.disarm();
                Ok(Some(client))
            })
            .await
            .map_err(|message| Error::new(Status::GenericFailure, message))
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

fn canonical_json(value: Value) -> Value {
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
}
