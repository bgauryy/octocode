use super::*;

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
