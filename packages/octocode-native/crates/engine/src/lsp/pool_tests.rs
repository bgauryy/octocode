use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use tokio::sync::oneshot;

#[derive(Clone)]
struct FakeClient {
    id: usize,
    alive: Arc<AtomicBool>,
    health_gate: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    health_checks: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    busy: Arc<AtomicBool>,
    resumes: Arc<AtomicUsize>,
    resume_gate: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    resident: Arc<AtomicU64>,
    config_changed: Arc<AtomicBool>,
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
            resumes: Arc::new(AtomicUsize::new(0)),
            resume_gate: Arc::new(Mutex::new(None)),
            resident: Arc::new(AtomicU64::new(0)),
            config_changed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn resident(self, bytes: u64) -> Self {
        self.resident.store(bytes, Ordering::SeqCst);
        self
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

    fn resident_bytes(&self) -> u64 {
        self.resident.load(Ordering::SeqCst)
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

    fn resume(&self) -> ClientFuture<()> {
        let resumes = Arc::clone(&self.resumes);
        let gate = Arc::clone(&self.resume_gate);
        Box::pin(async move {
            resumes.fetch_add(1, Ordering::SeqCst);
            if let Some(receiver) = gate.lock().await.take() {
                let _ = receiver.await;
            }
        })
    }

    fn project_config_changed(&self) -> ClientFuture<bool> {
        let changed = self.config_changed.load(Ordering::SeqCst);
        Box::pin(async move { changed })
    }
}

fn pool(max_entries: usize, idle_timeout_ms: u64) -> GenericPool<FakeClient> {
    GenericPool::new(LspPoolOptions {
        max_entries,
        idle_timeout_ms,
        max_resident_bytes: 0,
        ..LspPoolOptions::default()
    })
}

fn pool_with_memory_budget(max_resident_bytes: u64) -> GenericPool<FakeClient> {
    GenericPool::new(LspPoolOptions {
        max_entries: 8,
        idle_timeout_ms: 60_000,
        max_resident_bytes,
        ..LspPoolOptions::default()
    })
}

#[tokio::test]
async fn idle_servers_are_evicted_oldest_first_when_resident_memory_exceeds_the_budget() {
    let pool = pool_with_memory_budget(1_000);
    let mut clients = Vec::new();
    for id in 1..=3 {
        let client = pool
            .acquire(id.to_string(), || async move {
                Ok(Some(FakeClient::new(id).resident(600)))
            })
            .await
            .expect("start")
            .expect("client");
        clients.push(client);
    }
    let stops = clients
        .iter()
        .map(|client| client.stops.load(Ordering::SeqCst))
        .collect::<Vec<_>>();
    assert_eq!(stops, [1, 1, 0], "oldest idle servers go first");
    assert_eq!(pool.len(), 1);
}

#[tokio::test]
async fn a_busy_server_is_never_evicted_for_memory() {
    let pool = pool_with_memory_budget(1_000);
    let busy = pool
        .acquire("1".into(), || async {
            Ok(Some(FakeClient::new(1).resident(900)))
        })
        .await
        .expect("start")
        .expect("client");
    busy.busy.store(true, Ordering::SeqCst);
    let _ = pool
        .acquire("2".into(), || async {
            Ok(Some(FakeClient::new(2).resident(900)))
        })
        .await
        .expect("start")
        .expect("client");
    assert_eq!(busy.stops.load(Ordering::SeqCst), 0);
    assert_eq!(
        pool.len(),
        2,
        "the pool overflows rather than stop a busy server"
    );
}

#[tokio::test]
async fn an_unbounded_memory_budget_evicts_nothing() {
    let pool = pool_with_memory_budget(0);
    for id in 1..=3 {
        let _ = pool
            .acquire(id.to_string(), || async move {
                Ok(Some(FakeClient::new(id).resident(u64::MAX / 8)))
            })
            .await
            .expect("start");
    }
    assert_eq!(pool.len(), 3);
}

#[test]
fn idle_servers_outlive_agent_think_time() {
    assert!(LspPoolOptions::default().idle_timeout_ms >= 300_000);
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
            pool.acquire("k".into(), || async move {
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
            pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
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
            pool.acquire("k".into(), || async move {
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
        pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) }),
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
            pool.acquire("k".into(), || async move {
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
        pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) }),
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
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    let (_release_send, release_receive) = oneshot::channel();
    *client.health_gate.lock().await = Some(release_receive);
    let checking = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
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
        pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) }),
    )
    .await
    .expect("cancelled health check must not block reuse")
    .expect("reuse")
    .expect("client");
    assert_eq!(reused.id, 1);
    assert_eq!(client.stops.load(Ordering::SeqCst), 0);
}

/// The entry is removed (idle expiry) while Validate awaits
/// `alive()`/`resume()` and the client still reports alive. Validate must
/// restart instead of waiting on its own never-completed inflight.
#[tokio::test]
async fn entry_removed_during_validate_restarts_instead_of_hanging() {
    let pool = Arc::new(pool(4, 60_000));
    let client = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    let (release_send, release_receive) = oneshot::channel();
    *client.resume_gate.lock().await = Some(release_receive);
    let validating = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        while client.resumes.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("validate must reach resume");
    // What the idle timer does under the state lock.
    {
        let mut state = pool.state.lock().await;
        remove_lru(&mut state.lru, "k");
        state.entries.remove("k");
    }
    let _ = release_send.send(());
    let restarted = tokio::time::timeout(Duration::from_secs(1), validating)
        .await
        .expect("validate must not hang after its entry was removed")
        .expect("task")
        .expect("acquire")
        .expect("client");
    assert_eq!(restarted.id, 2);
    let later = tokio::time::timeout(
        Duration::from_secs(1),
        pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(3))) }),
    )
    .await
    .expect("later acquires must not hang")
    .expect("acquire")
    .expect("client");
    assert_eq!(later.id, 2);
    assert!(pool.state.lock().await.inflight.is_empty());
}

/// Another key's install evicts the entry
/// being validated (it is not leased, so not busy).
#[tokio::test]
async fn overflow_eviction_during_validate_restarts_instead_of_hanging() {
    let pool = Arc::new(pool(1, 60_000));
    let client = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    let (release_send, release_receive) = oneshot::channel();
    *client.resume_gate.lock().await = Some(release_receive);
    let validating = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        while client.resumes.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("validate must reach resume");
    let _other = pool
        .acquire("other".into(), || async { Ok(Some(FakeClient::new(9))) })
        .await
        .expect("other")
        .expect("client");
    assert_eq!(client.stops.load(Ordering::SeqCst), 1, "k was evicted");
    let _ = release_send.send(());
    let restarted = tokio::time::timeout(Duration::from_secs(1), validating)
        .await
        .expect("validate must not hang after eviction")
        .expect("task")
        .expect("acquire")
        .expect("client");
    assert_eq!(restarted.id, 2);
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
        .acquire("k".into(), || async { Ok(Some(old.clone())) })
        .await
        .expect("install")
        .expect("client");
    installed.alive.store(false, Ordering::SeqCst);
    let starts = Arc::new(AtomicUsize::new(0));
    let first = {
        let pool = Arc::clone(&pool);
        let starts = Arc::clone(&starts);
        tokio::spawn(async move {
            pool.acquire("k".into(), || async move {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok(Some(FakeClient::new(2)))
            })
            .await
        })
    };
    let second = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(3))) })
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

/// A server whose project config changed under it keeps the setup it
/// first saw, so the pool replaces it, once it is idle.
#[tokio::test]
async fn a_client_whose_project_config_changed_is_replaced_when_idle() {
    let pool = pool(4, 60_000);
    let old = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    let same = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
        .await
        .expect("reuse")
        .expect("client");
    assert_eq!(same.id, 1, "unchanged config keeps the pooled server");
    assert_eq!(old.stops.load(Ordering::SeqCst), 0);

    old.config_changed.store(true, Ordering::SeqCst);
    let replaced = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(3))) })
        .await
        .expect("replace")
        .expect("client");
    assert_eq!(replaced.id, 3);
    assert_eq!(old.stops.load(Ordering::SeqCst), 1, "old server stopped");
    assert_eq!(pool.len(), 1);
    let again = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(4))) })
        .await
        .expect("reuse")
        .expect("client");
    assert_eq!(again.id, 3, "the replacement is pooled");
}

#[tokio::test]
async fn a_busy_client_is_never_replaced_for_a_config_change() {
    let pool = pool(4, 60_000);
    let old = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    old.config_changed.store(true, Ordering::SeqCst);
    old.busy.store(true, Ordering::SeqCst);
    let during = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
        .await
        .expect("acquire")
        .expect("client");
    assert_eq!(during.id, 1, "in use: keeps serving");
    assert_eq!(old.stops.load(Ordering::SeqCst), 0);

    old.busy.store(false, Ordering::SeqCst);
    let after = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(3))) })
        .await
        .expect("acquire")
        .expect("client");
    assert_eq!(after.id, 3, "replaced once idle");
    assert_eq!(old.stops.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn idle_use_renews_expiry_and_stale_timers_do_not_remove_replacements() {
    let pool = pool(4, 100);
    let first = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(75)).await;
    let reused = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
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
        .acquire_leased("k".into(), || async { Ok(Some(FakeClient::new(1))) }, lease)
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
        .acquire_leased("k".into(), || async { Ok(Some(FakeClient::new(2))) }, lease)
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
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
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
        .acquire("1".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("one")
        .expect("client");
    let two = pool
        .acquire("2".into(), || async { Ok(Some(FakeClient::new(2))) })
        .await
        .expect("two")
        .expect("client");
    let _ = pool
        .acquire("1".into(), || async { Ok(Some(FakeClient::new(9))) })
        .await
        .expect("touch");
    let _ = pool
        .acquire("3".into(), || async { Ok(Some(FakeClient::new(3))) })
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
            pool.acquire("k".into(), || async move {
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
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    let (send, receive) = oneshot::channel();
    *client.health_gate.lock().await = Some(receive);
    let task = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
                .await
        })
    };
    tokio::task::yield_now().await;
    assert!(pool.clear("k").await);
    let _ = send.send(());
    assert!(task.await.expect("task").expect("acquire").is_none());
    assert_eq!(client.stops.load(Ordering::SeqCst), 1);
    let other = pool
        .acquire("other".into(), || async { Ok(Some(FakeClient::new(3))) })
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
    // Every built-in server's language waits, objective-c (clangd) and
    // scala (Metals) included.
    assert_eq!(readiness_timeout(Some("objective-c")), Some(20_000));
    assert_eq!(readiness_timeout(Some("scala")), Some(120_000));
    // An unknown/unlisted language still opts out of the readiness wait.
    assert_eq!(readiness_timeout(Some("plaintext")), None);
    assert_eq!(readiness_timeout(None), None);
}

#[test]
fn readiness_maximum_is_the_largest_configured_wait() {
    let longest = [
        "go",
        "rust",
        "java",
        "csharp",
        "swift",
        "shellscript",
        "typescript",
        "typescriptreact",
        "javascript",
        "javascriptreact",
        "python",
        "c",
        "cpp",
        "cuda",
    ]
    .into_iter()
    .filter_map(|language| readiness_timeout(Some(language)))
    .max();
    assert_eq!(longest.map(u64::from), Some(MAX_READINESS_TIMEOUT_MS));
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
    let first = LspLanguageServerConfig {
        command: "server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: root.join("x/../workspace").to_string_lossy().into_owned(),
        language_id: Some("rust".into()),
        initialization_options: Some(serde_json::json!({"z": 1, "nested": {"b": 2, "a": 1}})),
        env: Some(env_a),
        max_memory_mb: None,
        source: Default::default(),
    };
    let second = LspLanguageServerConfig {
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
        .acquire("busy".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("busy")
        .expect("client");
    busy.busy.store(true, Ordering::SeqCst);
    let fresh = pool
        .acquire("fresh".into(), || async { Ok(Some(FakeClient::new(2))) })
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
        .acquire("fresh".into(), || async { Ok(Some(FakeClient::new(3))) })
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
        .acquire("1".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("one")
        .expect("client");
    oldest.busy.store(true, Ordering::SeqCst);
    let second = pool
        .acquire("2".into(), || async { Ok(Some(FakeClient::new(2))) })
        .await
        .expect("two")
        .expect("client");
    let _ = pool
        .acquire("3".into(), || async { Ok(Some(FakeClient::new(3))) })
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
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    for _ in 0..10 {
        tokio::time::advance(Duration::from_millis(20)).await;
        let _ = pool
            .acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
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
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    assert!(pool.clear("k").await);
    tokio::time::advance(Duration::from_millis(60)).await;
    let second = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(2))) })
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
            Err(shared_error("spawn failed"))
        }
    };

    let first = pool
        .acquire("k".into(), failing(Arc::clone(&starts)))
        .await
        .expect_err("first failure");
    assert_eq!(starts.load(Ordering::SeqCst), 1);

    // Inside the 250ms window: fail fast with the same error, no start.
    let fast = pool
        .acquire("k".into(), failing(Arc::clone(&starts)))
        .await
        .expect_err("backing off");
    assert!(Arc::ptr_eq(&first, &fast));
    assert_eq!(starts.load(Ordering::SeqCst), 1);

    tokio::time::advance(Duration::from_millis(251)).await;
    let _ = pool
        .acquire("k".into(), failing(Arc::clone(&starts)))
        .await
        .expect_err("second failure");
    assert_eq!(starts.load(Ordering::SeqCst), 2);

    // Second failure doubles the window to 500ms.
    tokio::time::advance(Duration::from_millis(300)).await;
    let _ = pool
        .acquire("k".into(), failing(Arc::clone(&starts)))
        .await
        .expect_err("still backing off");
    assert_eq!(starts.load(Ordering::SeqCst), 2);

    tokio::time::advance(Duration::from_millis(201)).await;
    let client = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("recovered")
        .expect("client");
    assert_eq!(client.id, 1);

    // Success resets the backoff: the next failure waits only 250ms.
    assert!(pool.clear("k").await);
    let _ = pool
        .acquire("k".into(), failing(Arc::clone(&starts)))
        .await
        .expect_err("failure after reset");
    assert_eq!(starts.load(Ordering::SeqCst), 3);
    tokio::time::advance(Duration::from_millis(251)).await;
    let _ = pool
        .acquire("k".into(), failing(Arc::clone(&starts)))
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
        .acquire("k".into(), || async {
            Err(shared_error("bad server config"))
        })
        .await
        .expect_err("start failure");
    assert_eq!(error.reason, "bad server config");
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
    let config = |root: &Path| LspLanguageServerConfig {
        command: "server".into(),
        args: None,
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("rust".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let direct = canonical_lsp_key(&config(&real)).expect("direct key");
    let via_link = canonical_lsp_key(&config(&link)).expect("symlink key");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(
        direct, via_link,
        "a symlinked root must share the pool entry"
    );
}
