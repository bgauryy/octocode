//! Two processes sharing one state file: recovery from damage, an unusable
//! directory, and peer stamps from a clock far ahead.
use super::super::*;
use super::KeyFacts;
use std::collections::BTreeMap;

async fn admit_code_search(
    state: &Arc<KeyState>,
    budget: &GitHubBudget,
) -> Result<Admission, ProviderError> {
    state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            Instant::now() + Duration::from_secs(10),
            &Cancellation::new(),
        )
        .await
}

/// A corrupt file, start stamps older than the quota window, and stamps from a
/// clock far ahead hold nothing back: the admission goes ahead at once and the
/// file is valid again.
#[tokio::test]
async fn a_corrupt_stale_or_future_stamped_file_is_recovered() {
    let key = LimiterKey::new("h", Some("tok"));
    let now = now_ms();
    let hour = 3_600_000;
    let stamped = |stamp: u64| KeyFacts {
        last_start_ms: BTreeMap::from([("code_search".to_owned(), stamp)]),
        ..KeyFacts::default()
    };
    let files = [
        b"{ not json".to_vec(),
        serde_json::to_vec(&stamped(now - 2 * 60_000)).expect("stale"),
        serde_json::to_vec(&stamped(now + hour)).expect("future"),
    ];
    for bytes in files {
        let dir = tempfile::tempdir().expect("state directory");
        std::fs::write(dir.path().join(key.file_name()), &bytes).expect("seed");
        let budget = GitHubBudget::with_config(ExecutorConfig::octokit());
        let state = budget.key_state(&key, Some(dir.path()));
        let started = Instant::now();
        let admission = admit_code_search(&state, &budget).await;
        assert!(admission.is_ok(), "{:?}", admission.err());
        assert!(started.elapsed() < Duration::from_millis(500));
        let disk: KeyFacts =
            serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
                .expect("the file is valid json again");
        assert!(disk.last_start_ms.contains_key("code_search"), "{disk:?}");
    }
}

/// A state directory that cannot be created degrades to this process's own
/// pacing: the request is admitted.
#[tokio::test]
async fn an_unusable_state_directory_degrades_to_per_process_pacing() {
    let parent = tempfile::tempdir().expect("tempdir");
    let blocker = parent.path().join("not-a-directory");
    std::fs::write(&blocker, b"file").expect("blocker");
    let budget = GitHubBudget::with_config(ExecutorConfig::octokit());
    let state = budget.key_state(
        &LimiterKey::new("h", None),
        Some(&blocker.join("ratelimit")),
    );
    let first = admit_code_search(&state, &budget).await;
    assert!(first.is_ok(), "{:?}", first.err());
}

#[test]
fn peer_stamps_beyond_the_horizon_are_not_merged() {
    let now = 1_000_000_000;
    let beyond = now + super::PEER_STAMP_HORIZON_MS + 1;
    let mut facts = KeyFacts::default();
    facts.merge(
        KeyFacts {
            last_start_ms: BTreeMap::from([("code_search".to_owned(), beyond)]),
            ..KeyFacts::default()
        },
        now,
    );
    assert!(facts.last_start_ms.is_empty());
}
