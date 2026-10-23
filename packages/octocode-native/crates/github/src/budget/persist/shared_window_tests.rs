//! Two processes sharing one state file: window, spacing and recovery.
use super::super::*;
use super::KeyFacts;
use std::collections::BTreeMap;

async fn admit_code_search(
    state: &Arc<KeyState>,
    budget: &GitHubBudget,
    cap: Duration,
) -> Result<Admission, ProviderError> {
    state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            cap,
            Instant::now() + Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await
}

/// Two budgets with separate key states and one state file stand in for
/// two processes: together they stay within one window, and their starts
/// keep the spacing across both. The spacing is read from the starts the
/// shared file reserved, and the wait from the last admission against the
/// loop's start: a task resumed late after one admission would shorten
/// the gap observed to the next, so observed gaps prove nothing.
#[tokio::test]
async fn two_processes_share_one_window_and_one_spacing() {
    let dir = tempfile::tempdir().expect("state directory");
    let key = LimiterKey::new("h", Some("tok"));
    let config = ExecutorConfig {
        code_search_per_minute: 4,
        search_spacing: Duration::from_millis(150),
        ..ExecutorConfig::relaxed()
    };
    let budgets = [
        GitHubBudget::with_config(config.clone()),
        GitHubBudget::with_config(config),
    ];
    let states = budgets
        .iter()
        .map(|budget| budget.key_state(&key, Some(dir.path())))
        .collect::<Vec<_>>();
    let mut starts = Vec::new();
    let mut refused = 0;
    let began = Instant::now();
    for turn in 0..6 {
        let which = turn % 2;
        match admit_code_search(&states[which], &budgets[which], Duration::from_secs(1)).await {
            Ok(admission) => {
                starts.push(Instant::now());
                drop(admission);
            }
            Err(error) => {
                assert_eq!(error.kind, ProviderErrorKind::RateLimited);
                refused += 1;
            }
        }
    }
    assert_eq!((starts.len(), refused), (4, 2), "one window of 4 for both");
    let disk: KeyFacts =
        serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
            .expect("json");
    assert_eq!(disk.code_search_ms.len(), 4);
    let reserved = Vec::from(disk.code_search_ms);
    for pair in reserved.windows(2) {
        assert!(
            pair[1] - pair[0] >= 150,
            "reserved starts {}ms apart across processes",
            pair[1] - pair[0]
        );
    }
    // Each admission waits for its reserved start: the fourth is three
    // spacings after the first, which is no earlier than the loop began
    // (a few ms for the stamps' millisecond rounding).
    assert!(
        starts[3] - began >= Duration::from_millis(445),
        "the fourth start came {:?} after the first request",
        starts[3] - began
    );
}

/// A corrupt file, stamps older than the window, and stamps from a clock
/// far ahead hold nothing back: the admission goes ahead at once and the
/// file is valid again.
#[tokio::test]
async fn a_corrupt_stale_or_future_stamped_file_is_recovered() {
    let key = LimiterKey::new("h", Some("tok"));
    let now = now_ms();
    let hour = 3_600_000;
    let full_window = |stamp: u64| KeyFacts {
        code_search_ms: vec![stamp; 10].into(),
        last_start_ms: BTreeMap::from([("code_search".to_owned(), stamp)]),
        ..KeyFacts::default()
    };
    let files = [
        b"{ not json".to_vec(),
        serde_json::to_vec(&full_window(now - 2 * 60_000)).expect("stale"),
        serde_json::to_vec(&full_window(now + hour)).expect("future"),
    ];
    for bytes in files {
        let dir = tempfile::tempdir().expect("state directory");
        std::fs::write(dir.path().join(key.file_name()), &bytes).expect("seed");
        let budget = GitHubBudget::with_config(ExecutorConfig {
            search_spacing: Duration::from_secs(2),
            ..ExecutorConfig::octokit()
        });
        let state = budget.key_state(&key, Some(dir.path()));
        let started = Instant::now();
        let admission = admit_code_search(&state, &budget, Duration::from_secs(1)).await;
        assert!(admission.is_ok(), "{:?}", admission.err());
        assert!(started.elapsed() < Duration::from_millis(500));
        let disk: KeyFacts =
            serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
                .expect("the file is valid json again");
        assert_eq!(disk.code_search_ms.len(), 1, "{disk:?}");
    }
}

/// A state directory that cannot be created degrades to this process's
/// own pacing: the request is admitted and its window still applies.
#[tokio::test]
async fn an_unusable_state_directory_degrades_to_per_process_pacing() {
    let parent = tempfile::tempdir().expect("tempdir");
    let blocker = parent.path().join("not-a-directory");
    std::fs::write(&blocker, b"file").expect("blocker");
    let budget = GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 1,
        ..ExecutorConfig::relaxed()
    });
    let state = budget.key_state(
        &LimiterKey::new("h", None),
        Some(&blocker.join("ratelimit")),
    );
    let first = admit_code_search(&state, &budget, Duration::from_secs(1)).await;
    assert!(first.is_ok(), "{:?}", first.err());
    let second = admit_code_search(&state, &budget, Duration::from_secs(1)).await;
    assert_eq!(
        second.err().map(|error| error.kind),
        Some(ProviderErrorKind::RateLimited)
    );
}

#[test]
fn peer_stamps_beyond_the_horizon_are_not_merged() {
    let now = 1_000_000_000;
    let beyond = now + super::PEER_STAMP_HORIZON_MS + 1;
    let mut facts = KeyFacts::default();
    facts.merge(
        KeyFacts {
            code_search_ms: vec![beyond, now].into(),
            last_start_ms: BTreeMap::from([("code_search".to_owned(), beyond)]),
            ..KeyFacts::default()
        },
        now,
    );
    assert_eq!(facts.code_search_ms, vec![now]);
    assert!(facts.last_start_ms.is_empty());
}
