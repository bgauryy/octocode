//! Code-search quota across processes that share one state file: the quota
//! one reports paces the other, and covered starts burst across both.
use super::*;

/// Two budgets with separate key states and one state file stand in for two
/// processes.
fn two_processes(dir: &std::path::Path) -> (LimiterKey, [GitHubBudget; 2], Vec<Arc<KeyState>>) {
    let key = LimiterKey::new("h", Some("tok"));
    let budgets = [
        GitHubBudget::with_config(quota_paced()),
        GitHubBudget::with_config(quota_paced()),
    ];
    let states = budgets
        .iter()
        .map(|budget| budget.key_state(&key, Some(dir)))
        .collect();
    (key, budgets, states)
}

/// Starts the reported quota covers burst across both processes: none waits
/// for an even-spreading interval, and none is refused.
#[tokio::test]
async fn two_processes_burst_within_the_reported_quota() {
    let dir = tempfile::tempdir().expect("state directory");
    let (key, budgets, states) = two_processes(dir.path());
    for state in &states {
        state.report_quota(Group::CodeSearch.bucket(), Duration::from_secs(5));
    }
    let began = Instant::now();
    for turn in 0..8 {
        let which = turn % 2;
        let deadline = Instant::now() + Duration::from_secs(10);
        drop(
            admit(&states[which], &budgets[which], Group::CodeSearch, deadline)
                .await
                .expect("covered start"),
        );
    }
    assert!(
        began.elapsed() < Duration::from_millis(500),
        "eight covered starts took {:?}",
        began.elapsed()
    );
    let disk: key_state::KeyFacts =
        serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
            .expect("json");
    assert!(disk.last_start_ms.contains_key("code_search"));
}

/// The quota one process learns reaches the other through the file: a row
/// the assumed quota would cover is refused once a peer's answer reports
/// less.
#[tokio::test]
async fn a_peers_reported_quota_paces_this_process() {
    let dir = tempfile::tempdir().expect("state directory");
    let (_, budgets, states) = two_processes(dir.path());
    let row = |which: usize| {
        states[which].code_search_batch_refusal(
            6,
            budgets[which].config(),
            Instant::now() + Duration::from_secs(5),
        )
    };
    assert!(row(1).is_none(), "assumed quota covers six searches");
    report_remaining(&states[0], 2, 60);
    // The mirror is written off the async worker: wait until it lands.
    let patience = Instant::now() + Duration::from_secs(5);
    while states[1].code_search_room(budgets[1].config()) != 2 {
        assert!(Instant::now() < patience, "the peer's quota never arrived");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(row(1).is_some(), "the peer reported two left until reset");
}

/// The shared state directory holds a token's rate-limit facts: it and every
/// parent created for it are private to the user, as the rest of the home is.
#[cfg(unix)]
#[test]
fn the_state_directory_and_its_new_parents_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("root");
    let state = root.path().join("home").join("tmp").join("ratelimit");
    let budget = GitHubBudget::with_config(quota_paced());
    let _ = budget.key_state(&LimiterKey::new("h", Some("tok")), Some(&state));
    for dir in [
        &state,
        state.parent().unwrap(),
        state.parent().unwrap().parent().unwrap(),
    ] {
        let mode = std::fs::metadata(dir)
            .expect("created")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "{}", dir.display());
    }
}
