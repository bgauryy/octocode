use crate::error::{Error, Result, Status};
use crate::lsp::client::NativeLspClient;
use crate::lsp::types::JsLanguageServerConfig;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Mutex, Notify};
use tokio::time::{sleep, Duration};

type ClientFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

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
}

impl<C: Clone> InFlight<C> {
    fn new() -> Self {
        Self {
            result: StdMutex::new(None),
            notify: Notify::new(),
        }
    }

    fn complete(&self, result: SharedResult<C>) {
        if let Ok(mut slot) = self.result.lock() {
            if slot.is_none() {
                *slot = Some(result);
                self.notify.notify_waiters();
            }
        }
    }

    async fn wait(&self) -> SharedResult<C> {
        loop {
            let notified = self.notify.notified();
            if let Ok(slot) = self.result.lock() {
                if let Some(result) = slot.clone() {
                    return result;
                }
            }
            notified.await;
        }
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
            if let Some(inflight) = state.inflight.get(&key) {
                Action::Wait(Arc::clone(inflight))
            } else {
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
                        self.spawn_idle_timer(key, entry_id, generation);
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
                    return inflight.wait().await;
                }
                client.stop().await;
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
        let result = future.await;
        match result {
            Ok(Some(client)) => {
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
                            if oldest != key {
                                if let Some(entry) = state.entries.remove(&oldest) {
                                    evicted.push(entry.client);
                                }
                            }
                        }
                        self.count.store(state.entries.len(), Ordering::SeqCst);
                        state.inflight.remove(&key);
                        inflight.complete(Ok(Some(client.clone())));
                        self.spawn_idle_timer(key.clone(), entry_id, generation);
                        true
                    }
                };
                for stale in evicted {
                    stale.stop().await;
                }
                if installed {
                    Ok(Some(client))
                } else {
                    client.stop().await;
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
                inflight.wait().await
            }
            Err(error) => {
                let mut state = self.state.lock().await;
                if inflight_is_current(&state, &key, &inflight) {
                    state.inflight.remove(&key);
                    inflight.complete(Err(error));
                }
                drop(state);
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
                if let Err(error) = client.start().await {
                    let _ = client.stop().await;
                    return Err(error.to_string());
                }
                if let Some(timeout_ms) = readiness_timeout(factory_config.language_id.as_deref()) {
                    if let Err(error) = client.wait_for_ready(Some(timeout_ms)).await {
                        let _ = client.stop().await;
                        return Err(error.to_string());
                    }
                }
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
        stops: Arc<AtomicUsize>,
        busy: Arc<AtomicBool>,
    }

    impl FakeClient {
        fn new(id: usize) -> Self {
            Self {
                id,
                alive: Arc::new(AtomicBool::new(true)),
                health_gate: Arc::new(Mutex::new(None)),
                stops: Arc::new(AtomicUsize::new(0)),
                busy: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl PoolClient for FakeClient {
        fn alive(&self) -> ClientFuture<bool> {
            let alive = Arc::clone(&self.alive);
            let gate = Arc::clone(&self.health_gate);
            Box::pin(async move {
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
        };
        let second = JsLanguageServerConfig {
            workspace_root: root.join("workspace").to_string_lossy().into_owned(),
            initialization_options: Some(serde_json::json!({"nested": {"a": 1, "b": 2}, "z": 1})),
            env: Some(env_b),
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
    }
}
