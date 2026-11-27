use super::*;

#[tokio::test]
async fn idle_servers_are_evicted_oldest_first_when_resident_memory_exceeds_the_budget() {
    let pool = pool_with_memory_budget(1_000);
    let mut clients = Vec::new();
    for id in 1..=3 {
        let client = pool
            .acquire(id.to_string(), id.to_string(), || async move {
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
        .acquire("1".into(), "1".into(), || async {
            Ok(Some(FakeClient::new(1).resident(900)))
        })
        .await
        .expect("start")
        .expect("client");
    busy.busy.store(true, Ordering::SeqCst);
    let _ = pool
        .acquire("2".into(), "2".into(), || async {
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
            .acquire(id.to_string(), id.to_string(), || async move {
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
    let _other = pool
        .acquire("other".into(), "other".into(), || async {
            Ok(Some(FakeClient::new(9)))
        })
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
        .acquire("1".into(), "1".into(), || async {
            Ok(Some(FakeClient::new(1)))
        })
        .await
        .expect("one")
        .expect("client");
    let two = pool
        .acquire("2".into(), "2".into(), || async {
            Ok(Some(FakeClient::new(2)))
        })
        .await
        .expect("two")
        .expect("client");
    let _ = pool
        .acquire("1".into(), "1".into(), || async {
            Ok(Some(FakeClient::new(9)))
        })
        .await
        .expect("touch");
    let _ = pool
        .acquire("3".into(), "3".into(), || async {
            Ok(Some(FakeClient::new(3)))
        })
        .await
        .expect("three");
    assert_eq!(one.stops.load(Ordering::SeqCst), 0);
    assert_eq!(two.stops.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn lru_eviction_never_stops_a_busy_client_and_evicts_it_once_idle() {
    let pool = pool(1, 60_000);
    let busy = pool
        .acquire("busy".into(), "busy".into(), || async {
            Ok(Some(FakeClient::new(1)))
        })
        .await
        .expect("busy")
        .expect("client");
    busy.busy.store(true, Ordering::SeqCst);
    let fresh = pool
        .acquire("fresh".into(), "fresh".into(), || async {
            Ok(Some(FakeClient::new(2)))
        })
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
        .acquire("fresh".into(), "fresh".into(), || async {
            Ok(Some(FakeClient::new(3)))
        })
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
        .acquire("1".into(), "1".into(), || async {
            Ok(Some(FakeClient::new(1)))
        })
        .await
        .expect("one")
        .expect("client");
    oldest.busy.store(true, Ordering::SeqCst);
    let second = pool
        .acquire("2".into(), "2".into(), || async {
            Ok(Some(FakeClient::new(2)))
        })
        .await
        .expect("two")
        .expect("client");
    let _ = pool
        .acquire("3".into(), "3".into(), || async {
            Ok(Some(FakeClient::new(3)))
        })
        .await
        .expect("three");
    assert_eq!(oldest.stops.load(Ordering::SeqCst), 0);
    assert_eq!(second.stops.load(Ordering::SeqCst), 1);
    assert_eq!(pool.len(), 2);
}

/// The pool keeps a busy server over its budget; it reports that overflow to
/// the caller until the server is idle and eviction runs again.
#[tokio::test]
async fn the_pool_reports_resident_memory_over_its_budget_while_a_busy_server_holds_it() {
    let budget = 1_000;
    let resident = budget - 100;
    let pool = pool_with_memory_budget(budget);
    let busy = pool
        .acquire("1".into(), "1".into(), || async move {
            Ok(Some(FakeClient::new(1).resident(resident)))
        })
        .await
        .expect("start")
        .expect("client");
    assert_eq!(pool.over_budget().await, None, "within the budget");
    busy.busy.store(true, Ordering::SeqCst);
    let _ = pool
        .acquire("2".into(), "2".into(), || async move {
            Ok(Some(FakeClient::new(2).resident(resident)))
        })
        .await
        .expect("start")
        .expect("client");
    assert_eq!(pool.over_budget().await, Some((resident * 2, budget)));
    busy.busy.store(false, Ordering::SeqCst);
    let _ = pool
        .acquire("3".into(), "3".into(), || async move {
            Ok(Some(FakeClient::new(3).resident(0)))
        })
        .await
        .expect("start")
        .expect("client");
    assert_eq!(pool.over_budget().await, None, "eviction ran once idle");
}

#[tokio::test]
async fn a_pool_without_a_memory_budget_is_never_over_budget() {
    let pool = pool_with_memory_budget(0);
    let _ = pool
        .acquire("1".into(), "1".into(), || async {
            Ok(Some(FakeClient::new(1).resident(u64::MAX / 2)))
        })
        .await
        .expect("start");
    assert_eq!(pool.over_budget().await, None);
}

/// Pools that share one budget (one per daemon of a user) each keep its
/// equal part: with N sharers a pool evicts at a budget of one Nth, so the
/// pools together stay within the budget one pool alone would have.
#[tokio::test]
async fn a_budget_shared_by_several_pools_evicts_at_each_pools_part() {
    const BUDGET: u64 = 4_000;
    const SERVER: u64 = 600;
    for sharers in [1_usize, 2, 4, 8] {
        let pool = pool_with_memory_budget(BUDGET);
        pool.share_memory_budget(sharers);
        for id in 1..=6_usize {
            let _ = pool
                .acquire(id.to_string(), id.to_string(), || async move {
                    Ok(Some(FakeClient::new(id).resident(SERVER)))
                })
                .await
                .expect("start")
                .expect("client");
        }
        let part = BUDGET / sharers as u64;
        // The servers a part holds, and always the one just acquired.
        let kept = (part / SERVER).clamp(1, 6) as usize;
        assert_eq!(pool.len(), kept, "{sharers} sharers");
        assert!(
            pool.len() as u64 * SERVER <= part.max(SERVER),
            "{sharers} sharers stay within their part"
        );
    }
}

/// Sharing never turns a budget off (a part of zero would read as "no
/// budget") nor on (no budget stays none), and no sharer count below one
/// applies.
#[tokio::test]
async fn sharing_keeps_a_budget_a_budget_and_no_budget_none() {
    let pool = pool_with_memory_budget(10);
    pool.share_memory_budget(1_000);
    assert_eq!(pool.budgeted().max_resident_bytes, 1);
    pool.share_memory_budget(0);
    assert_eq!(pool.budgeted().max_resident_bytes, 10);
    let unbounded = pool_with_memory_budget(0);
    unbounded.share_memory_budget(8);
    assert_eq!(unbounded.budgeted().max_resident_bytes, 0);
}

/// A busy server that keeps the pool over budget is stopped as soon as its
/// request ends, without an acquire to trigger the eviction, and it is never
/// stopped while the request runs.
#[test]
fn an_over_budget_busy_server_is_stopped_when_its_request_ends() {
    paused(|clock| async move {
        let budget = 1_000;
        let resident = 900;
        let pool = GenericPool::with_clock(
            LspPoolOptions {
                max_entries: 8,
                idle_timeout_ms: 3_600_000,
                max_resident_bytes: budget,
                ..LspPoolOptions::default()
            },
            clock.lsp(),
        );
        let busy = pool
            .acquire("1".into(), "1".into(), || async move {
                Ok(Some(FakeClient::new(1).resident(resident)))
            })
            .await
            .expect("start")
            .expect("client");
        busy.busy.store(true, Ordering::SeqCst);
        let newest = pool
            .acquire("2".into(), "2".into(), || async move {
                Ok(Some(FakeClient::new(2).resident(resident)))
            })
            .await
            .expect("start")
            .expect("client");
        assert_eq!(pool.len(), 2, "the busy server outlives the overrun");
        for _ in 0..3 {
            clock.advance(Duration::from_secs(1)).await;
        }
        assert_eq!(busy.stops.load(Ordering::SeqCst), 0, "request in flight");
        busy.busy.store(false, Ordering::SeqCst);
        clock.advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(busy.stops.load(Ordering::SeqCst), 1, "stopped at its end");
        assert_eq!(newest.stops.load(Ordering::SeqCst), 0);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.over_budget().await, None);
    });
}

/// The overrun is reported only while a busy server will end it: the newest
/// server alone over a budget is kept by design, not a temporary overrun.
#[tokio::test]
async fn a_lone_server_over_the_budget_is_not_reported_as_an_overrun() {
    let pool = pool_with_memory_budget(1_000);
    let _ = pool
        .acquire("1".into(), "1".into(), || async {
            Ok(Some(FakeClient::new(1).resident(5_000)))
        })
        .await
        .expect("start");
    assert_eq!(pool.over_budget().await, None);
}
