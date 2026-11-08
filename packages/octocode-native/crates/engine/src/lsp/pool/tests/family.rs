use super::*;

const WORKSPACE: &str = "workspace-w";

/// Start `key` in `family` with `client`, as the production pool does when an
/// option change gives the same server another full key.
async fn start(
    pool: &GenericPool<FakeClient>,
    key: &str,
    family: &str,
    client: FakeClient,
) -> FakeClient {
    pool.acquire(key.into(), family.into(), || async { Ok(Some(client)) })
        .await
        .expect("start")
        .expect("client")
}

#[tokio::test]
async fn starting_a_server_retires_the_idle_sibling_of_its_workspace() {
    let pool = pool(8, 60_000);
    let old = start(&pool, "options-a", WORKSPACE, FakeClient::new(1)).await;
    let new = start(&pool, "options-b", WORKSPACE, FakeClient::new(2)).await;
    assert_eq!(pool.len(), 1, "peak one server per workspace");
    assert_eq!(old.stops.load(Ordering::SeqCst), 1);
    assert_eq!(new.stops.load(Ordering::SeqCst), 0);
    let state = pool.state.lock().await;
    assert!(!state.entries.contains_key("options-a"));
    assert!(!state.lru.iter().any(|key| key == "options-a"));
}

#[tokio::test]
async fn a_failed_start_keeps_the_idle_sibling() {
    let pool = pool(8, 60_000);
    let old = start(&pool, "options-a", WORKSPACE, FakeClient::new(1)).await;
    let failed = pool
        .acquire("options-b".into(), WORKSPACE.into(), || async {
            Err(shared_error("bad init options"))
        })
        .await;
    assert!(failed.is_err());
    assert_eq!(old.stops.load(Ordering::SeqCst), 0, "sibling still serving");
    assert_eq!(pool.len(), 1);
    assert!(pool.state.lock().await.entries.contains_key("options-a"));
}

#[tokio::test]
async fn the_sibling_is_retired_once_the_new_server_is_installed() {
    let pool = pool(8, 60_000);
    let old = start(&pool, "options-a", WORKSPACE, FakeClient::new(1)).await;
    let stops_when_started = Arc::new(AtomicUsize::new(usize::MAX));
    let observed = Arc::clone(&stops_when_started);
    let watched = old.clone();
    pool.acquire("options-b".into(), WORKSPACE.into(), || async move {
        observed.store(watched.stops.load(Ordering::SeqCst), Ordering::SeqCst);
        Ok(Some(FakeClient::new(2)))
    })
    .await
    .expect("start");
    assert_eq!(stops_when_started.load(Ordering::SeqCst), 0);
    assert_eq!(old.stops.load(Ordering::SeqCst), 1);
    assert_eq!(pool.len(), 1);
}

/// Options flapping in one workspace restarts the server on each switch: the
/// pool bounds a workspace to one resident server (memory) and a cold start is
/// the price of the option change, not a leak of two warm servers.
#[tokio::test]
async fn alternating_options_restart_the_server_and_keep_one_resident() {
    let pool = pool(8, 60_000);
    let first = start(&pool, "options-a", WORKSPACE, FakeClient::new(1)).await;
    start(&pool, "options-b", WORKSPACE, FakeClient::new(2)).await;
    let again = start(&pool, "options-a", WORKSPACE, FakeClient::new(3)).await;
    assert_ne!(again.id, first.id, "a cold restart, not the retired client");
    assert_eq!(first.stops.load(Ordering::SeqCst), 1);
    assert_eq!(pool.len(), 1);
}

#[tokio::test]
async fn a_busy_sibling_keeps_serving_its_request() {
    let pool = pool(8, 60_000);
    let busy = start(&pool, "options-a", WORKSPACE, FakeClient::new(1)).await;
    busy.busy.store(true, Ordering::SeqCst);
    start(&pool, "options-b", WORKSPACE, FakeClient::new(2)).await;
    assert_eq!(busy.stops.load(Ordering::SeqCst), 0, "no request lost");
    assert_eq!(pool.len(), 2, "overflow lasts while the sibling is busy");
    busy.busy.store(false, Ordering::SeqCst);
    start(&pool, "options-c", WORKSPACE, FakeClient::new(3)).await;
    assert_eq!(busy.stops.load(Ordering::SeqCst), 1, "retired once idle");
    assert_eq!(pool.len(), 1);
}

#[tokio::test]
async fn servers_of_other_workspaces_are_not_siblings() {
    let pool = pool(8, 60_000);
    let other = start(&pool, "other", "workspace-x", FakeClient::new(1)).await;
    start(&pool, "options-a", WORKSPACE, FakeClient::new(2)).await;
    assert_eq!(other.stops.load(Ordering::SeqCst), 0);
    assert_eq!(pool.len(), 2);
}

#[tokio::test]
async fn concurrent_acquires_across_an_option_change_lose_no_request() {
    let pool = Arc::new(pool(8, 60_000));
    let old = start(&pool, "options-a", WORKSPACE, FakeClient::new(1)).await;
    let reuse = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("options-a".into(), WORKSPACE.into(), || async {
                Ok(Some(FakeClient::new(9)))
            })
            .await
        })
    };
    let change = {
        let pool = Arc::clone(&pool);
        tokio::spawn(async move {
            pool.acquire("options-b".into(), WORKSPACE.into(), || async {
                Ok(Some(FakeClient::new(2)))
            })
            .await
        })
    };
    let served = reuse
        .await
        .expect("join")
        .expect("acquire")
        .expect("client");
    change.await.expect("join").expect("start");
    // The reuse got a live client: the old one, or its restart if the change
    // retired it first. Either way exactly one server remains.
    assert!(served.id == old.id || served.id == 9);
    assert!(pool.len() <= 2);
    start(&pool, "options-c", WORKSPACE, FakeClient::new(3)).await;
    assert_eq!(pool.len(), 1);
}
