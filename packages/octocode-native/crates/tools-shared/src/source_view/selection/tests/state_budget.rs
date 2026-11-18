//! The serialized selection state budget, oversized lines, and the bytes
//! and anchors selected windows keep.
use super::*;
use octocode_contract_types::LocalFetchQuery;
use octocode_test_support::json_fixture;

#[test]
fn serialized_state_budget_uses_sanitized_content() {
    struct Redacted;
    impl ContentScan for Redacted {
        fn sanitize(
            &self,
            _: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Ok(("é\n\"".into(), vec![]))
        }
    }
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = budget_runtime(executor.handle().clone(), calls.clone());
    runtime.limits.max_chars = 1_024;
    let text = format!("{}\n", "x".repeat(2_047)).repeat(3);
    let (candidates, windows) = chunks(
        &text,
        None,
        None,
        Path::new("source.txt"),
        "source.txt",
        &Redacted,
        Some(&runtime),
        None,
        None,
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.state.as_ref().unwrap()["content"] == "é\n\"")
    );
    let outcome = crate::selection::select(
        Some(&runtime),
        octocode_runtime_base::id::ToolId::LocalFetch,
        &json_fixture(r#"{"classification": {"predicates": ["Find evidence"]}}"#),
        "source",
        None,
        &candidates,
        None,
        octocode_contract_types::SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("sanitized order"));
    assert_eq!(
        outcome.receipt.status,
        octocode_contract_types::SelectionStatus::Complete
    );
    assert_eq!(outcome.receipt.selected_items, 3);
    assert_eq!(outcome.selected, [0, 1, 2]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(project(&text, &windows, &outcome.selected).0, text);
}

/// A page runs its first chunk whatever its size, and replays with no call.
#[test]
fn a_chunk_over_the_page_budget_runs_alone_and_replays() {
    use octocode_contract_types::SelectionStatus;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = budget_runtime(executor.handle().clone(), calls.clone());
    runtime.limits.max_chars = 32;
    let text = "x\n".repeat(4_096);
    let query = json_fixture(r#"{"classification": {"predicates": ["Find x"]}}"#);
    let split = |prior: Option<&str>| {
        chunks(
            &text,
            None,
            None,
            Path::new("source.txt"),
            "source.txt",
            &ContentSecurity::new(),
            Some(&runtime),
            prior,
            None,
        )
    };
    let (candidates, windows) = split(None);
    assert_eq!(windows.len(), 4);
    let select = |prior: Option<&str>, candidates: &[crate::selection::Candidate]| {
        crate::selection::select(
            Some(&runtime),
            octocode_runtime_base::id::ToolId::LocalFetch,
            &query,
            "source",
            prior,
            candidates,
            None,
            octocode_contract_types::SelectionUnit::Chunks,
        )
        .unwrap_or_else(|_| panic!("budgeted order"))
    };
    let partial = select(None, &candidates);
    assert_eq!(partial.receipt.status, SelectionStatus::Partial);
    assert_eq!(partial.receipt.reason.as_deref(), Some("candidateBudget"));
    assert_eq!(partial.receipt.selected_items, 1);
    assert_eq!(partial.receipt.total_items, 4);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        project(&text, &windows, &partial.selected),
        (text[..2_048].to_owned(), (1..=1_024).collect())
    );
    let (replayed, _) = split(Some(&partial.token));
    let replay = select(Some(&partial.token), &replayed);
    assert_eq!(replay.selected, partial.selected);
    assert_eq!(replay.receipt, partial.receipt);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// Walking the candidate pages of a read reaches every line once, each chunk
/// judged once; the unfiltered lead reads the same lines with no call.
#[test]
fn a_walk_reaches_every_line_and_unfiltered_recovers_them() {
    use crate::source_view::SourceFacts;
    use octocode_runtime_base::execution::cancel::NeverCancel;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = budget_runtime(executor.handle().clone(), calls.clone());
    runtime.limits.max_chars = 1;
    let text = "x\n".repeat(4_096);
    let read = |query: &LocalFetchQuery| {
        crate::source_view::process_fetched_content_selected(
            query,
            text.as_bytes(),
            Path::new("source.txt"),
            "source.txt",
            &SourceFacts::default(),
            &ContentSecurity::new(),
            &NeverCancel,
            Some(&runtime),
        )
    };
    let mut query: LocalFetchQuery = serde_json::from_value(json_fixture(
        r#"{
            "path": "source.txt",
            "ranges": [{"startLine": 1, "endLine": 4096}],
            "classification": {"predicates": ["Find x"]}
        }"#,
    ))
    .unwrap();
    let mut pages = 0;
    let mut unfiltered = None;
    loop {
        let page = read(&query);
        assert_ne!(page.status, ReadStatus::Error, "{page:?}");
        pages += 1;
        assert!(pages < 10, "the walk ends");
        let Some(next) = page.next else { break };
        unfiltered = unfiltered.or(next.unfiltered);
        match next.next_candidate_page {
            Some(call) => query = call.query,
            None => break,
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), pages);
    let mut query = unfiltered.expect("whole view").query;
    let judged = calls.load(Ordering::SeqCst);
    let mut contents = String::new();
    loop {
        let result = read(&query);
        contents.push_str(result.content.as_deref().unwrap_or_default());
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
    }
    assert_eq!(contents.len(), text.len());
    assert_eq!(calls.load(Ordering::SeqCst), judged, "no inference");
}

#[test]
fn complete_long_lines_and_unterminated_tail_are_eligible_in_source_order() {
    struct Uncalled;
    impl SelectionJudge for Uncalled {
        fn identity(&self) -> serde_json::Value {
            json_fixture(r#"{"test": "chunks"}"#)
        }
        fn select(&self, _: JudgeCall<'_>) -> crate::selection::Judgments {
            panic!("capture only")
        }
    }
    let executor = tokio::runtime::Runtime::new().unwrap();
    let runtime =
        selection_fake::runtime(Uncalled, executor.handle().clone(), Duration::from_secs(1));
    let text = format!("{}\nshort\ntail", "a".repeat(4_096));
    let (candidates, windows) = chunks(
        &text,
        None,
        None,
        Path::new("source.txt"),
        "source.txt",
        &ContentSecurity::new(),
        Some(&runtime),
        None,
        None,
    );
    assert_eq!(windows.len(), 2);
    assert_eq!(
        candidates[0].state.as_ref().unwrap()["content"]
            .as_str()
            .unwrap()
            .len(),
        4_097
    );
    assert!(
        candidates[1].state.is_some(),
        "the final unterminated window is complete evidence"
    );
    assert_eq!(project(&text, &windows, &[0, 1]).0, text);
}

#[test]
fn selected_windows_preserve_original_bytes_and_nonadjacent_line_anchors() {
    let text = format!("{}\n{}\nlast", "a".repeat(2_047), "b".repeat(2_047));
    let (candidates, chunks) = chunks(
        &text,
        None,
        None,
        Path::new("source.txt"),
        "source.txt",
        &ContentSecurity::new(),
        None,
        None,
        None,
    );
    assert_eq!(chunks.len(), 3);
    assert!(candidates[0].state.is_none());
    assert!(candidates[2].state.is_none());
    let (ordered, anchors) = project(&text, &chunks, &[0, 2]);
    assert_eq!(anchors, vec![1, 0, 3], "a marker separates the gap");
    assert_eq!(
        ordered,
        format!("{}\n... [line 2 not selected] ...\nlast", "a".repeat(2_047))
    );
}
