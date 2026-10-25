use super::*;

#[tokio::test(start_paused = true)]
async fn acquire_waits_for_resume_and_reuses_the_same_client_without_starting() {
    let pool = pool(4, 60_000);
    let first = pool
        .acquire("k".into(), || async { Ok(Some(FakeClient::new(1))) })
        .await
        .expect("install")
        .expect("client");
    let (release, wait) = oneshot::channel();
    *first.resume_gate.lock().await = Some(wait);
    let next = pool.acquire("k".into(), || async {
        panic!("a live client must not restart")
    });
    tokio::pin!(next);
    std::future::poll_fn(|context| {
        assert!(
            next.as_mut().poll(context).is_pending(),
            "acquire waits for readiness resumption"
        );
        std::task::Poll::Ready(())
    })
    .await;
    assert_eq!(first.resumes.load(Ordering::SeqCst), 1);
    release.send(()).expect("release readiness");
    let second = next.await.expect("reuse").expect("client");
    assert!(
        first.same(&second),
        "resumption preserves the exact pooled client"
    );
    assert_eq!(first.stops.load(Ordering::SeqCst), 0);
    assert_eq!(first.resumes.load(Ordering::SeqCst), 1);
}

#[test]
fn idle_servers_outlive_agent_think_time() {
    assert!(LspPoolOptions::default().idle_timeout_ms >= 300_000);
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
