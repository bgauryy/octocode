//! Reads judged under `clasifyContext`: the selected plan, its scope, and
//! refusals.

use super::*;

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
    fs::write(&path, "first\nsecond\nthird\n").expect("fixture");
    let paths = Paths(temp.0.clone());
    let mut query = q(&path);
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find second"]}))
            .expect("context"),
    );
    query.length = wire_positive(1);
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = selection_runtime(executor.handle().clone());
    let read = |query: &LocalFetchQuery| {
        execute_local_fetch(query, &paths, &Safe, &NeverCancel, None, Some(&runtime))
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
    assert!(continued.selection.is_some());
    let second = read(&continued);
    assert_eq!(second.content.as_deref(), Some("second\n"));
    let mut switched_to_full = continued.clone();
    switched_to_full.full_content = Some(true);
    switched_to_full.unit = None;
    switched_to_full.offset = None;
    switched_to_full.length = None;
    assert_eq!(
        read(&switched_to_full).error_code,
        None,
        "fullContent changes output paging, not the bound extraction"
    );
    let mut changed = continued.clone();
    changed.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["a different goal"]}))
            .expect("context"),
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
    query.match_string = Some(MatchString::String("missing".parse().expect("match")));
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find missing evidence"]}))
            .expect("context"),
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
fn a_refused_classified_read_keeps_its_window_in_the_unfiltered_lead() {
    let temp = Fixture::new();
    let path = temp.0.join("evidence.txt");
    let text = numbered(40);
    fs::write(&path, &text).expect("fixture");
    let digest = hex::encode(sha2::Sha256::digest(text.as_bytes()));
    let paths = Paths(temp.0.clone());
    for read in [
        serde_json::json!({"unit": "bytes", "offset": 0, "length": 50}),
        serde_json::json!({"ranges": ["1-30"], "offset": 10}),
        serde_json::json!({"minify": "standard", "offset": 0, "length": 5}),
    ] {
        let mut query = qj(&path, read.clone());
        query.clasify_context = Some(
            serde_json::from_value(
                serde_json::json!({"classifiers":["Does this explain ownership?"]}),
            )
            .expect("context"),
        );
        let row = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, None);
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
        let mut expected = qj(Path::new(lead.path.as_str()), read.clone());
        expected.snapshot = digest.parse().ok();
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
    query.clasify_context = Some(
        serde_json::from_value(serde_json::json!({"classifiers":["find evidence"]}))
            .expect("context"),
    );
    let result = execute_local_fetch(&query, &policy, &Safe, &NeverCancel, None, Some(&selection));
    assert_eq!(result.error_code, None);
    assert_eq!(result.content.as_deref(), Some(source.as_str()));
}

/// A classified read its input alone rules out is refused before the
/// source is decoded or redacted, its lead bound to the version it saw.
#[test]
fn a_classified_read_with_an_unsupported_view_is_refused_before_any_scan() {
    struct Counting(AtomicUsize);
    impl ContentScan for Counting {
        fn sanitize(
            &self,
            text: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok((text.to_owned(), vec![]))
        }
        fn redact_key_blocks(&self, content: &str) -> (String, bool) {
            self.0.fetch_add(1, Ordering::SeqCst);
            (content.to_owned(), false)
        }
    }
    let t = Fixture::new();
    let p = t.0.join("run.rs");
    fs::write(&p, "pub fn run() {}\n").expect("fixture");
    for fields in [
        serde_json::json!({"minify": "standard"}),
        serde_json::json!({"unit": "bytes"}),
        serde_json::json!({"offset": 3}),
    ] {
        let mut fields = fields;
        fields["clasifyContext"] = serde_json::json!({"classifiers": ["find run"]});
        let scan = Counting(AtomicUsize::new(0));
        let r = execute_local_fetch(
            &qj(&p, fields.clone()),
            &Paths(t.0.clone()),
            &scan,
            &NeverCancel,
            None,
            None,
        );
        assert_eq!(
            r.error_code,
            Some(ErrorCode::InvalidInput),
            "{fields}: {r:?}"
        );
        assert!(r.source_sha256.is_some());
        assert_eq!(scan.0.into_inner(), 0, "{fields}");
    }
}
