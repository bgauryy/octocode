//! Admission against the state file other processes share: reservations, its lock, and released
//! slots on disk.
use super::*;

/// Two processes sharing the state directory reserve under its lock: the
/// second sees the first's window slot before it reserves, so the
/// window never admits more than its limit.
#[tokio::test]
async fn processes_reserve_window_slots_against_each_other() {
    let dir = tempfile::tempdir().expect("state directory");
    let config = ExecutorConfig {
        code_search_per_minute: 1,
        ..ExecutorConfig::relaxed()
    };
    let key = LimiterKey::new("h", None);
    let first = GitHubBudget::with_config(config.clone());
    let second = GitHubBudget::with_config(config);
    let (one, two) = (
        first.key_state(&key, Some(dir.path())),
        second.key_state(&key, Some(dir.path())),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let token = CancellationToken::new();
    let taken = one
        .admit(
            Some(Group::CodeSearch),
            first.config(),
            true,
            Duration::from_secs(1),
            deadline,
            &token,
        )
        .await;
    assert!(taken.is_ok(), "{:?}", taken.err());
    let error = two
        .admit(
            Some(Group::CodeSearch),
            second.config(),
            true,
            Duration::from_secs(1),
            deadline,
            &token,
        )
        .await
        .err()
        .expect("the other process holds the only slot");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}

/// A peer process that holds the state file's lock must not hold this
/// request past its deadline or its cancellation, and the request goes
/// through as soon as the lock is released.
#[tokio::test]
async fn a_held_state_lock_cannot_block_admission_past_its_deadline() {
    let dir = tempfile::tempdir().expect("state directory");
    let config = || ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    };
    let key = LimiterKey::new("h", None);
    let budget = GitHubBudget::with_config(config());
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
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_millis(300),
            &CancellationToken::new(),
        )
        .await
        .err()
        .expect("the held lock times the request out");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}",
        started.elapsed()
    );

    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let started = Instant::now();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(60),
            &token,
        )
        .await
        .err()
        .expect("cancellation ends the lock wait");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}",
        started.elapsed()
    );

    peer.unlock().expect("peer releases the lock");
    state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await
        .expect("admitted once the lock is free");
}

/// The reservation is mirrored to disk before the wait, so the release
/// must reach disk too, or other processes (and this one, on its next
/// refresh) still count the phantom slot.
#[tokio::test]
async fn a_cancelled_window_wait_releases_its_persisted_slot() {
    let dir = tempfile::tempdir().expect("state directory");
    let config = || ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    };
    let key = LimiterKey::new("h", None);
    let budget = Arc::new(GitHubBudget::with_config(config()));
    let state = budget.key_state(&key, Some(dir.path()));
    {
        let now = now_ms();
        let mut facts = state.facts();
        facts.code_search_ms.push_back(now - 58_500);
        facts.code_search_ms.push_back(now - 10_000);
    }
    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(10),
            &token,
        )
        .await
        .err()
        .expect("cancelled");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    let other_process = GitHubBudget::with_config(config());
    let seen = other_process.key_state(&key, Some(dir.path()));
    let facts = seen.facts();
    assert_eq!(facts.code_search_ms.len(), 2, "no phantom slot on disk");
    assert!(
        facts
            .last_start_ms
            .get(Group::CodeSearch.name())
            .is_none_or(|start| *start <= now_ms()),
        "no phantom spacing reservation on disk"
    );
}
