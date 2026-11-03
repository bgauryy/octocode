//! Admission mechanics: refusals decided before queueing, admissions queued
//! ahead, reservations handed back, and the shared state lock.
use super::*;

/// Fixed spacing between two `graphql` starts, long enough to see.
fn spaced(spacing: Duration) -> (Arc<GitHubBudget>, Arc<KeyState>) {
    limited(ExecutorConfig {
        graphql_spacing: spacing,
        ..ExecutorConfig::relaxed()
    })
}

/// A refusal the facts already decide is returned before the group permit is
/// awaited: the request does not queue behind another's permit toward its
/// deadline.
#[tokio::test]
async fn a_refusal_is_decided_before_the_group_permit() {
    let (budget, state) = spaced(Duration::from_secs(5));
    let held = admit(
        &state,
        &budget,
        Group::Graphql,
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .expect("first start holds the group permit");
    let started = Instant::now();
    let error = admit(
        &state,
        &budget,
        Group::Graphql,
        Instant::now() + Duration::from_secs(1),
    )
    .await
    .err()
    .expect("the spacing puts the start past the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(started.elapsed() < Duration::from_millis(500));
    drop(held);
}

/// The peek counts the admission queued ahead: with one start already
/// waiting for the permit, the next one plans after it and is refused when
/// that second spacing passes its deadline.
#[tokio::test]
async fn a_peek_counts_the_admissions_queued_ahead() {
    let spacing = Duration::from_millis(400);
    let (budget, state) = spaced(spacing);
    let held = admit(
        &state,
        &budget,
        Group::Graphql,
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .expect("first start");
    let queued = {
        let (budget, state) = (Arc::clone(&budget), Arc::clone(&state));
        tokio::spawn(async move {
            admit(
                &state,
                &budget,
                Group::Graphql,
                Instant::now() + Duration::from_millis(700),
            )
            .await
            .map(drop)
        })
    };
    while state
        .groups
        .get("graphql")
        .expect("lane")
        .queued
        .load(std::sync::atomic::Ordering::SeqCst)
        == 0
    {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let error = admit(
        &state,
        &budget,
        Group::Graphql,
        Instant::now() + Duration::from_millis(700),
    )
    .await
    .err()
    .expect("two spacings ahead pass the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    drop(held);
    queued.await.expect("task").expect("the queued start fits");
}

/// An admission dropped while it waits for its reserved start hands the slot
/// back through `Drop`: the next admission plans after the last admitted
/// request, not after the dropped one.
#[tokio::test]
async fn dropped_admission_releases_its_slot() {
    let spacing = Duration::from_millis(400);
    let (budget, state) = spaced(spacing);
    let deadline = Instant::now() + Duration::from_secs(10);
    drop(
        admit(&state, &budget, Group::Graphql, deadline)
            .await
            .expect("first start"),
    );
    let first = state.facts().last_start_ms["graphql"];
    let dropped = tokio::time::timeout(
        Duration::from_millis(100),
        admit(&state, &budget, Group::Graphql, deadline),
    )
    .await;
    assert!(dropped.is_err(), "the wait was dropped, not completed");
    assert_eq!(state.facts().last_start_ms["graphql"], first);
}

/// The reservation is mirrored to disk before the wait, so the release must
/// reach disk too, or other processes still count the phantom start.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dropped_wait_releases_its_persisted_slot() {
    let dir = tempfile::tempdir().expect("state directory");
    let key = LimiterKey::new("h", Some("tok"));
    let budget = GitHubBudget::with_config(ExecutorConfig {
        graphql_spacing: Duration::from_millis(400),
        ..ExecutorConfig::relaxed()
    });
    let state = budget.key_state(&key, Some(dir.path()));
    let deadline = Instant::now() + Duration::from_secs(10);
    drop(
        admit(&state, &budget, Group::Graphql, deadline)
            .await
            .expect("first start"),
    );
    let first = state.facts().last_start_ms["graphql"];
    let dropped = tokio::time::timeout(
        Duration::from_millis(100),
        admit(&state, &budget, Group::Graphql, deadline),
    )
    .await;
    assert!(dropped.is_err());
    let file = dir.path().join(key.file_name());
    let patience = Instant::now() + Duration::from_secs(5);
    loop {
        let on_disk = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<key_state::KeyFacts>(&bytes).ok())
            .and_then(|facts| facts.last_start_ms.get("graphql").copied());
        if on_disk == Some(first) {
            break;
        }
        assert!(
            Instant::now() < patience,
            "disk still holds {on_disk:?}, not {first}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A peer process that holds the state file's lock must not hold this request
/// past its deadline or its cancellation, and the request goes through as soon
/// as the lock is released.
#[tokio::test]
async fn a_held_state_lock_cannot_block_admission_past_its_deadline() {
    let dir = tempfile::tempdir().expect("state directory");
    let key = LimiterKey::new("h", None);
    let budget = GitHubBudget::with_config(quota_paced());
    let state = budget.key_state(&key, Some(dir.path()));
    let mut sidecar = dir.path().join(key.file_name()).into_os_string();
    sidecar.push(".lock");
    let peer = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&sidecar)
        .expect("lock file");
    peer.lock().expect("peer takes the lock");

    let started = Instant::now();
    let error = admit(
        &state,
        &budget,
        Group::CodeSearch,
        Instant::now() + Duration::from_millis(300),
    )
    .await
    .err()
    .expect("the held lock times the request out");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));

    let token = Cancellation::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            Instant::now() + Duration::from_secs(60),
            &token,
        )
        .await
        .err()
        .expect("cancellation ends the lock wait");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);

    peer.unlock().expect("peer releases the lock");
    admit(
        &state,
        &budget,
        Group::CodeSearch,
        Instant::now() + Duration::from_secs(10),
    )
    .await
    .expect("admitted once the lock is free");
}
