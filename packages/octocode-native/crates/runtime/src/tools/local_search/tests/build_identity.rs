//! A cursor minted by another build is refused, never replayed.
use super::*;

/// The same snapshot under another build's identity is stale: the page
/// restarts with an executable lead, though the digest and the files match.
#[test]
fn a_cursor_of_another_build_is_stale_with_a_restart_lead() {
    let root = tempfile::tempdir().expect("fixture directory");
    for name in ["a.txt", "b.txt"] {
        fs::write(root.path().join(name), "needle\n".repeat(400)).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy(), "matchString": ["needle"]}),
        None,
    );
    let snapshot = search_in(&request, &policy, &security, Some(6_000))
        .expect("page 1")
        .source_snapshot
        .expect("snapshot");
    let page_two = |snapshot: &str| {
        let next = ls_query(
            serde_json::json!({"snapshot": snapshot, "page": 2}),
            Some(&request),
        );
        search_in(&next, &policy, &security, Some(6_000))
    };
    page_two(&snapshot).expect("a cursor of this build replays");
    let identity = crate::build_identity::BUILD_IDENTITY;
    assert!(snapshot.contains(identity), "{snapshot}");
    let foreign = snapshot.replacen(identity, "0123456789abcdef", 1);
    let failed = page_two(&foreign).expect_err("another build's cursor");
    assert_eq!(
        failed.code,
        octocode_contract_types::ErrorCode::StaleSnapshot,
        "{}",
        failed.message
    );
    assert!(failed.calls.is_some(), "no restart lead: {failed:?}");
}
