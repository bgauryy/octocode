//! Reads judged under `classification`: the selected plan, its scope, and
//! refusals.

use super::*;
use octocode_contract_types::LocalFetchQuery;

mod recovery;

fn selection_runtime(handle: tokio::runtime::Handle) -> crate::tools::selection::Runtime {
    use crate::tools::test_support::selection_fake;
    selection_fake::runtime(
        selection_fake::SelectAll,
        handle,
        std::time::Duration::from_secs(5),
    )
}

#[test]
fn context_read_replays_selected_plan_and_rejects_source_drift() {
    let temp = Fixture::new();
    let path = temp.0.join("goals.txt");
    let source = "first\nsecond\nthird\n".repeat(500);
    fs::write(&path, &source).expect("fixture");
    let paths = Paths(temp.0.clone());
    let mut query = q(&path);
    amend(
        &mut query,
        serde_json::json!({"classification":serde_json::json!({"predicates":["find second"]})}),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let read = |query: &LocalFetchQuery| {
        execute_local_fetch(
            query,
            &paths,
            &Safe,
            &NeverCancel,
            Some(1200),
            Some(&runtime),
            &Default::default(),
        )
    };
    let first = read(&query);
    assert_eq!(
        first.selection.as_ref().map(|r| r.status),
        Some(octocode_contract_types::SelectionStatus::Complete)
    );
    let continued = first
        .next
        .expect("next")
        .continue_window
        .expect("page")
        .query;
    assert!(local_fetch_query::selection(&continued).is_some());
    let second = read(&continued);
    let first_bytes = first.content.as_deref().expect("first content").len();
    assert!(source[first_bytes..].starts_with(second.content.as_deref().expect("second content")));
    let mut changed = continued.clone();
    amend(
        &mut changed,
        serde_json::json!({"classification":serde_json::json!({"predicates":["a different goal"]})}),
    );
    assert_eq!(read(&changed).error_code, Some(ErrorCode::StaleSnapshot));
    fs::write(&path, "changed evidence\n").expect("change source");
    assert_eq!(read(&continued).error_code, Some(ErrorCode::StaleSnapshot));
}

#[test]
fn context_read_with_no_matches_reports_empty_selection_scope() {
    let temp = Fixture::new();
    let path = temp.0.join("empty-selection.txt");
    fs::write(&path, "ordinary text\n").expect("fixture");
    let mut query = q(&path);
    amend(&mut query, serde_json::json!({"matchString":["missing"]}));
    amend(
        &mut query,
        serde_json::json!({"classification":serde_json::json!({"predicates":["find missing evidence"]})}),
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let result = execute_local_fetch(
        &query,
        &Paths(temp.0.clone()),
        &Safe,
        &NeverCancel,
        None,
        Some(&runtime),
        &Default::default(),
    );
    let receipt = result.selection.expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Complete
    );
    assert_eq!(receipt.selected_items, 0);
    assert_eq!(receipt.total_items, 0);
}

/// A classified read refused before selection leads to the caller's own
/// read without the classifier: its window, ranges and view stay, bound to
/// the source version the refusal saw.
#[test]
fn a_refused_classified_read_restarts_its_unfiltered_selection() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    let text = numbered(40);
    fs::write(&path, &text).expect("fixture");
    let digest = hex::encode(sha2::Sha256::digest(text.as_bytes()));
    let paths = Paths(temp.0.clone());
    for read in [
        serde_json::json!({"readProgress":{"unit":"bytes","offset":10}}),
        serde_json::json!({"ranges":[{"startLine":1,"endLine":30}],"readProgress":{"unit":"lines","offset":10}}),
    ] {
        let mut query = qj(&path, read.clone());
        amend(
            &mut query,
            serde_json::json!({"classification":{"predicates":["Does this explain ownership?"]}}),
        );
        let row = execute_local_fetch(
            &query,
            &paths,
            &Safe,
            &NeverCancel,
            None,
            None,
            &Default::default(),
        );
        assert_eq!(
            row.error_code,
            Some(ErrorCode::InvalidInput),
            "{read}: {row:?}"
        );
        let lead = row
            .next
            .expect("next")
            .unfiltered
            .expect("unfiltered")
            .query;
        // The row names the file as the workspace does.
        let mut expected = qj(Path::new(local_fetch_query::path(&lead)), read.clone());
        local_fetch_query::set_progress(&mut expected, None);
        local_fetch_query::set_snapshot(&mut expected, Some(&digest)).expect("snapshot");
        assert_eq!(
            serde_json::to_value(&lead).expect("json"),
            serde_json::to_value(&expected).expect("json"),
            "{read}"
        );
    }
}

#[test]
fn selection_provider_sees_workspace_relative_path_and_complete_evidence() {
    use crate::tools::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::Value;

    use std::time::Duration;

    struct PathJudge;
    impl SelectionJudge for PathJudge {
        fn identity(&self) -> Value {
            serde_json::json!({"backend":"path-test"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&ordinal| {
                    let state = call.candidates[ordinal].state.as_ref().expect("state");
                    assert_eq!(state["path"], "src/evidence.txt");
                    assert!(
                        state["content"]
                            .as_str()
                            .is_some_and(|text| text.contains("evidence"))
                    );
                    (ordinal, Decision::Selected)
                })
                .collect())
        }
    }

    let t = Fixture::new();
    let folder = t.0.join("src");
    fs::create_dir_all(&folder).expect("folder");
    let file = folder.join("evidence.txt");
    let source = (0..6)
        .map(|_| format!("{}\n", "evidence ".repeat(100)))
        .collect::<String>();
    fs::write(&file, &source).expect("fixture");
    let policy = workspace_policy(&t.0);
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let selection = crate::tools::test_support::selection_fake::runtime(
        PathJudge,
        executor.handle().clone(),
        Duration::from_secs(2),
    );
    let mut query = q(&file);
    amend(
        &mut query,
        serde_json::json!({"classification":serde_json::json!({"predicates":["find evidence"]})}),
    );
    let result = execute_local_fetch(
        &query,
        &policy,
        &Safe,
        &NeverCancel,
        None,
        Some(&selection),
        &Default::default(),
    );
    assert_eq!(result.error_code, None);
    assert_eq!(result.content.as_deref(), Some(source.as_str()));
}

/// A classified read its input alone rules out is refused before the
/// source is decoded or redacted, its lead bound to the version it saw.
#[test]
fn classification_with_compact_or_symbols_is_rejected_before_execution() {
    for view in ["compact", "symbols"] {
        let row = serde_json::json!({"path":"run.rs","view":view,"classification":{"predicates":["find run"]}});
        assert!(crate::contracts::validate_query("localFetch", row).is_err());
    }
}
