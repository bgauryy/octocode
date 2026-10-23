use super::*;

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
