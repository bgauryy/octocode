//! Recovery calls of a classified read replay its source version and reject
//! a changed one.

use super::*;
use octocode_contract_types::LocalFetchQuery;

#[test]
fn uncaptured_stream_recovery_is_optional_and_reads_the_current_source() {
    let temp = Fixture::new();
    let path = temp.0.join("stream.txt");
    let line = "old evidence\n";
    let original =
        line.repeat(crate::tools::local_fetch::acquire::MAX_SOURCE_BYTES as usize / line.len() + 1);
    fs::write(&path, &original).expect("stream above whole-file ceiling");
    let paths = Paths(temp.0.clone());
    let query = qj(
        &path,
        serde_json::json!({
            "ranges": [{"startLine": 1, "endLine": 1}],
            "classification": {"predicates": ["Find evidence"]}
        }),
    );
    let refused = fetch(&query, &paths);
    assert_eq!(refused.error_code, Some(ErrorCode::InvalidInput));
    assert!(refused.source_sha256.is_none(), "no source was captured");
    let recovery = refused
        .next
        .expect("recovery")
        .unfiltered
        .expect("unfiltered");
    let call = serde_json::to_value(&recovery).expect("public call");
    assert_eq!(
        call["kind"], "related",
        "an uncaptured read starts new work"
    );
    assert!(local_fetch_query::snapshot(&recovery.query).is_none());
    assert!(!local_fetch_query::has_classification(&recovery.query));
    fs::write(&path, original.replace("old", "new")).expect("change source");
    let current = fetch(&recovery.query, &paths);
    assert_eq!(current.error_code, None);
    assert_eq!(current.content.as_deref(), Some("new evidence\n"));
}

#[test]
fn selection_recovery_keeps_fetch_snapshot_and_rejects_changed_source() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    fs::write(&path, "original evidence\n").expect("fixture");
    let paths = Paths(temp.0.clone());
    let mut query = q(&path);
    amend(
        &mut query,
        serde_json::json!({"classification":serde_json::json!({"predicates":["Does this explain ownership?"]})}),
    );
    let first = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, None);
    assert!(first.content.as_deref().unwrap_or_default().is_empty());
    let captured = first.source_sha256.expect("captured source identity");
    let recovery = first
        .next
        .expect("next")
        .unfiltered
        .expect("unfiltered")
        .query;
    assert!(!local_fetch_query::has_classification(&recovery));
    assert!(local_fetch_query::selection(&recovery).is_none());
    let recovered = execute_local_fetch(&recovery, &paths, &Safe, &NeverCancel, None, None);
    assert_eq!(recovered.error_code, None);
    assert_eq!(recovered.content.as_deref(), Some("original evidence\n"));
    fs::write(&path, "changed evidence\n").expect("change source");
    let changed = execute_local_fetch(&recovery, &paths, &Safe, &NeverCancel, None, None);
    assert_eq!(changed.error_code, Some(ErrorCode::StaleSnapshot));
    assert_eq!(
        local_fetch_query::snapshot(&recovery),
        Some(captured.as_str())
    );
}

#[test]
fn unknown_and_whole_selection_recovery_reject_source_drift() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    // The word judge keeps `keep` and answers `maybe` unsure: the unknown
    // recovery lists judged-unsure chunks (budget-pending ones are
    // `nextCandidatePage`'s).
    let lines =
        ["keep", "maybe one", "maybe two"].map(|label| format!("{label} {}\n", "x".repeat(1_200)));
    let original = lines.concat();
    fs::write(&path, &original).expect("fixture");
    let paths = Paths(temp.0.clone());
    let query = qj(
        &path,
        serde_json::json!({
            "classification": {"predicates": ["Find direct evidence"]}
        }),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = crate::tools::test_support::selection_fake::runtime(
        crate::tools::test_support::selection_fake::WordJudge { fail: None },
        executor.handle().clone(),
        std::time::Duration::from_secs(5),
    );
    let first = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, Some(&runtime));
    let receipt = first.selection.expect("selection receipt");
    assert_eq!((receipt.selected_items, receipt.unknown_items), (1, 2));
    let captured = first.source_sha256.expect("captured source");
    let next = first.next.expect("recovery");
    let recoveries = [
        (
            next.unfiltered_unknown.expect("unknown chunks").query,
            lines[1..].concat(),
        ),
        (next.unfiltered.expect("whole view").query, original),
    ];
    for (index, (recovery, expected)) in recoveries.iter().enumerate() {
        // The unknown chunks replay the selection (unbilled); the whole view
        // is an ordinary read.
        assert_eq!(
            local_fetch_query::selection(recovery).is_some(),
            index == 0,
            "{recovery:?}"
        );
        assert_eq!(local_fetch_query::has_classification(recovery), index == 0);
        assert_eq!(
            local_fetch_query::snapshot(recovery),
            Some(captured.as_str())
        );
        let unchanged =
            execute_local_fetch(recovery, &paths, &Safe, &NeverCancel, None, Some(&runtime));
        assert_eq!(unchanged.error_code, None);
        assert_eq!(unchanged.content.as_deref(), Some(expected.as_str()));
    }
    let changed_source = lines
        .iter()
        .map(|line| line.replace("maybe", "changed"))
        .collect::<String>();
    fs::write(&path, changed_source).expect("change source");
    for (recovery, _) in &recoveries {
        let changed =
            execute_local_fetch(recovery, &paths, &Safe, &NeverCancel, None, Some(&runtime));
        assert_eq!(changed.error_code, Some(ErrorCode::StaleSnapshot));
        assert!(changed.content.as_deref().unwrap_or_default().is_empty());
    }
}

/// The unread rest of a block longer than one page is the read's next page:
/// it pins the source bytes its first page read and refuses a changed source.
#[test]
fn the_unread_rest_of_a_long_block_rejects_source_drift() {
    let temp = Fixture::new();
    let paths = Paths(temp.0.clone());
    let original = format!(
        "fn work() {{\n{}}}\n",
        (0..3_000)
            .map(|n| format!("    let value_{n} = {n};\n"))
            .collect::<String>()
    );
    let path = temp.0.join("block.rs");
    fs::write(&path, &original).expect("fixture");
    let first = fetch(
        &qj(
            &path,
            serde_json::json!({"ranges":[{"startLine":1,"endLine":1}],"block":true}),
        ),
        &paths,
    );
    let shown = first.content.clone().expect("page 1");
    let next = serde_json::to_value(first.next.expect("unread evidence")).expect("next");
    let recovery: LocalFetchQuery =
        serde_json::from_value(next["nextPage"]["input"]["queries"][0].clone()).expect("page 2");
    let captured = octocode_engine::digest::sha256(original.as_bytes());
    assert_eq!(
        local_fetch_query::snapshot(&recovery),
        Some(captured.as_str()),
        "the page pins the source bytes"
    );
    let unchanged = fetch(&recovery, &paths);
    assert_eq!(unchanged.error_code, None);
    let second = unchanged.content.expect("page 2");
    assert!(!second.is_empty());
    assert!(
        original.starts_with(&format!("{shown}{second}")),
        "page 2 continues page 1"
    );
    fs::write(
        &path,
        original.replace("let value_2999 = 2999;", "let changed_evidence = 2999;"),
    )
    .expect("change source");
    let stale = fetch(&recovery, &paths);
    assert_eq!(stale.error_code, Some(ErrorCode::StaleSnapshot));
    assert!(stale.content.as_deref().unwrap_or_default().is_empty());
}

#[test]
fn context_full_content_bounded_continuation_replays_its_selection() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("bounded-goals.txt");
    fs::write(&file, "let value = 1;\n".repeat(1_500)).expect("fixture");
    let mut full = q(&file);

    amend(
        &mut full,
        serde_json::json!({"classification":serde_json::json!({"predicates":["find value"]})}),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let first = execute_local_fetch(
        &full,
        &paths,
        &Safe,
        &NeverCancel,
        Some(10_000),
        Some(&runtime),
    );
    assert_eq!(first.error_code, None);
    let next = first
        .next
        .expect("bounded next")
        .continue_window
        .expect("page")
        .query;
    assert!(local_fetch_query::selection(&next).is_some());
    let second = execute_local_fetch(
        &next,
        &paths,
        &Safe,
        &NeverCancel,
        Some(10_000),
        Some(&runtime),
    );
    assert_eq!(second.error_code, None);
    assert_eq!(
        second.selection.as_ref().map(|receipt| receipt.status),
        Some(octocode_contract_types::SelectionStatus::Complete)
    );
}
