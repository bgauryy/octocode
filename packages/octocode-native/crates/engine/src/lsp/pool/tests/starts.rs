use super::*;

/// Failure metadata is retained work too: visiting many unavailable workspaces
/// must not retain every key and error for the lifetime of the server.
#[test]
fn failed_start_cache_uses_the_pool_capacity_and_keeps_recent_keys() {
    paused(|clock| async move {
        let pool = pool_on(&clock, 3, 60_000);
        let capacity = pool.options.max_entries;
        for index in 0..capacity {
            let key = format!("failed-{index}");
            assert!(
                pool.acquire(key.clone(), key, || async {
                    Err(shared_error("unavailable"))
                })
                .await
                .is_err()
            );
        }
        // Refresh the oldest failure before inserting a new one.
        assert!(
            pool.acquire("failed-0".into(), "failed-0".into(), || async {
                panic!("the active backoff must be reused")
            })
            .await
            .is_err()
        );
        assert!(
            pool.acquire("new-failure".into(), "new-failure".into(), || async {
                Err(shared_error("unavailable"))
            })
            .await
            .is_err()
        );
        let state = pool.state.lock().await;
        assert_eq!(state.backoff.len(), capacity);
        assert!(state.backoff.contains("failed-0"));
        assert!(!state.backoff.contains("failed-1"));
        assert!(state.backoff.contains("new-failure"));
    });
}

#[test]
fn blocked_start_does_not_block_another_key_and_cancelled_waiter_keeps_owner() {
    paused(|clock| async move {
        let pool = Arc::new(pool_on(&clock, 4, 60_000));
        let (entered, observed) = oneshot::channel();
        let (release, wait) = oneshot::channel();
        let owner = tokio::spawn({
            let pool = Arc::clone(&pool);
            async move {
                pool.acquire("a".into(), "a".into(), || async {
                    entered.send(()).unwrap();
                    wait.await.unwrap();
                    Ok(Some(FakeClient::new(1)))
                })
                .await
            }
        });
        observed.await.unwrap();
        let waiter = tokio::spawn({
            let pool = Arc::clone(&pool);
            async move {
                pool.acquire("a".into(), "a".into(), || async {
                    panic!("the existing start must be shared")
                })
                .await
            }
        });
        tokio::task::yield_now().await;
        let independent = pool
            .acquire("b".into(), "b".into(), || async {
                Ok(Some(FakeClient::new(2)))
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(independent.id, 2);
        assert!(!owner.is_finished());
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        assert_eq!(owner.await.unwrap().unwrap().unwrap().id, 1);
        let reused = pool
            .acquire("a".into(), "a".into(), || async {
                panic!("must reuse the surviving owner")
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reused.id, 1);
    });
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
            pool.acquire("k".into(), "k".into(), || async move {
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
            pool.acquire("k".into(), "k".into(), || async {
                Ok(Some(FakeClient::new(2)))
            })
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

/// The entry is removed (idle expiry) while Validate awaits
/// `alive()` and the client still reports alive. Validate must
/// restart instead of waiting on its own never-completed inflight.
#[tokio::test]
async fn entry_removed_during_validate_restarts_instead_of_hanging() {
    let pool = Arc::new(pool(4, 60_000));
    let client = pool
        .acquire("k".into(), "k".into(), || async {
            Ok(Some(FakeClient::new(1)))
        })
        .await
        .expect("install")
        .expect("client");
    let (release_send, release_receive) = oneshot::channel();
    *client.health_gate.lock().await = Some(release_receive);
    let validating = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), "k".into(), || async {
                Ok(Some(FakeClient::new(2)))
            })
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        while client.health_checks.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("validate must reach the health check");
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
        pool.acquire("k".into(), "k".into(), || async {
            Ok(Some(FakeClient::new(3)))
        }),
    )
    .await
    .expect("later acquires must not hang")
    .expect("acquire")
    .expect("client");
    assert_eq!(later.id, 2);
    assert!(pool.state.lock().await.inflight.is_empty());
}

#[tokio::test]
async fn cancelled_supervised_start_stops_client_after_start_finishes() {
    let client = FakeClient::new(1);
    let check = client.clone();
    let (entered_send, entered_receive) = oneshot::channel();
    let (release_send, release_receive) = oneshot::channel::<()>();
    let startup = tokio::spawn(run_cancellation_safe_start(client, |_| async move {
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

/// A start whose caller left hears it: a wait inside the start that only
/// progress bounds ends then, and the client is stopped.
#[tokio::test]
async fn a_supervised_start_hears_that_its_caller_left() {
    let client = FakeClient::new(1);
    let check = client.clone();
    let (entered_send, entered_receive) = oneshot::channel();
    let (heard_send, heard_receive) = oneshot::channel();
    let startup = tokio::spawn(run_cancellation_safe_start(
        client,
        |abandoned| async move {
            let _ = entered_send.send(());
            abandoned.wait().await;
            let _ = heard_send.send(());
        },
    ));
    entered_receive.await.expect("startup entered");
    assert_eq!(check.stops.load(Ordering::SeqCst), 0);
    startup.abort();
    let _ = startup.await;
    tokio::time::timeout(Duration::from_secs(5), heard_receive)
        .await
        .expect("the start hears that its caller left")
        .expect("heard");
    tokio::time::timeout(Duration::from_secs(5), async {
        while check.stops.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the abandoned start's client is stopped");
}

/// A start whose caller still waits never hears that it was abandoned.
#[tokio::test]
async fn a_supervised_start_with_its_caller_is_not_abandoned() {
    let client = FakeClient::new(1);
    let answer = run_cancellation_safe_start(client, |abandoned| async move {
        let abandoned = abandoned.wait();
        tokio::select! {
            biased;
            () = abandoned => "abandoned",
            () = tokio::task::yield_now() => "finished",
        }
    })
    .await
    .expect("the caller receives the result");
    assert_eq!(answer, "finished");
}

#[tokio::test]
async fn concurrent_health_checks_are_deduplicated_and_dead_clients_retry() {
    let pool = Arc::new(pool(4, 60_000));
    let old = FakeClient::new(1);
    let installed = pool
        .acquire("k".into(), "k".into(), || async { Ok(Some(old.clone())) })
        .await
        .expect("install")
        .expect("client");
    installed.alive.store(false, Ordering::SeqCst);
    let starts = Arc::new(AtomicUsize::new(0));
    let first = {
        let pool = Arc::clone(&pool);
        let starts = Arc::clone(&starts);
        tokio::spawn(async move {
            pool.acquire("k".into(), "k".into(), || async move {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok(Some(FakeClient::new(2)))
            })
            .await
        })
    };
    let second = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("k".into(), "k".into(), || async {
                Ok(Some(FakeClient::new(3)))
            })
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

#[test]
fn repeated_start_failures_back_off_exponentially_and_reset_on_success() {
    paused(|clock| async move {
        let pool = pool_on(&clock, 4, 60_000);
        let starts = Arc::new(AtomicUsize::new(0));
        let failing = |starts: Arc<AtomicUsize>| {
            move || async move {
                starts.fetch_add(1, Ordering::SeqCst);
                Err(shared_error("spawn failed"))
            }
        };

        let first = pool
            .acquire("k".into(), "k".into(), failing(Arc::clone(&starts)))
            .await
            .expect_err("first failure");
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        // Inside the 250ms window: fail fast with the same error, no start.
        let fast = pool
            .acquire("k".into(), "k".into(), failing(Arc::clone(&starts)))
            .await
            .expect_err("backing off");
        assert!(Arc::ptr_eq(&first, &fast));
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        clock.advance(Duration::from_millis(251)).await;
        let _ = pool
            .acquire("k".into(), "k".into(), failing(Arc::clone(&starts)))
            .await
            .expect_err("second failure");
        assert_eq!(starts.load(Ordering::SeqCst), 2);

        // Second failure doubles the window to 500ms.
        clock.advance(Duration::from_millis(300)).await;
        let _ = pool
            .acquire("k".into(), "k".into(), failing(Arc::clone(&starts)))
            .await
            .expect_err("still backing off");
        assert_eq!(starts.load(Ordering::SeqCst), 2);

        clock.advance(Duration::from_millis(201)).await;
        let client = pool
            .acquire("k".into(), "k".into(), || async {
                Ok(Some(FakeClient::new(1)))
            })
            .await
            .expect("recovered")
            .expect("client");
        assert_eq!(client.id, 1);

        // Success resets the backoff: the next failure waits only 250ms.
        assert!(pool.clear("k").await);
        let _ = pool
            .acquire("k".into(), "k".into(), failing(Arc::clone(&starts)))
            .await
            .expect_err("failure after reset");
        assert_eq!(starts.load(Ordering::SeqCst), 3);
        clock.advance(Duration::from_millis(251)).await;
        let _ = pool
            .acquire("k".into(), "k".into(), failing(Arc::clone(&starts)))
            .await
            .expect_err("retried after base window");
        assert_eq!(starts.load(Ordering::SeqCst), 4);
    });
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
        .acquire("k".into(), "k".into(), || async {
            Err(shared_error("bad server config"))
        })
        .await
        .expect_err("start failure");
    assert_eq!(error.reason, "bad server config");
}
