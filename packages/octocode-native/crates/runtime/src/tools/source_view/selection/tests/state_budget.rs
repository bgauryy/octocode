//! The serialized selection state budget, oversized lines, and the bytes
//! and anchors selected windows keep.
use super::*;

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
    let outcome = crate::tools::selection::select(
        Some(&runtime),
        &json!({"clasifyContext":{"classifiers":["Find evidence"]}}),
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

#[test]
fn serialized_state_budget_preserves_all_short_lines_and_replay() {
    use octocode_contract_types::SelectionStatus;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = budget_runtime(executor.handle().clone(), calls.clone());
    let text = "x\n".repeat(4_096);
    let query = json!({"clasifyContext":{"classifiers":["Find x"]}});
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
    assert_eq!(windows.len(), 4);
    let first_state_chars = candidates[0]
        .state
        .as_ref()
        .unwrap()
        .to_string()
        .chars()
        .count();
    assert!(first_state_chars > runtime.limits.max_chars);
    let skipped = crate::tools::selection::select(
        Some(&runtime),
        &query,
        "source",
        None,
        &candidates,
        None,
        octocode_contract_types::SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("budgeted order"));
    assert_eq!(skipped.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        skipped.receipt.reason.as_deref(),
        Some("candidateStateTooLarge")
    );
    assert_eq!(skipped.receipt.selected_items, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        project(&text, &windows, &skipped.selected),
        (String::new(), Vec::new())
    );

    runtime.limits.max_chars = first_state_chars;
    let partial = crate::tools::selection::select(
        Some(&runtime),
        &query,
        "source",
        None,
        &candidates,
        None,
        octocode_contract_types::SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("partial order"));
    assert_eq!(partial.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        partial.receipt.reason.as_deref(),
        Some("candidateStateTooLarge")
    );
    assert_eq!(partial.receipt.selected_items, 1);
    assert_eq!(partial.receipt.total_items, 4);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        project(&text, &windows, &partial.selected),
        (text[..2_048].to_owned(), (1..=1_024).collect())
    );
    let (replayed_candidates, _) = chunks(
        &text,
        None,
        None,
        Path::new("source.txt"),
        "source.txt",
        &ContentSecurity::new(),
        Some(&runtime),
        Some(&partial.token),
        None,
    );
    let replay = crate::tools::selection::select(
        Some(&runtime),
        &query,
        "source",
        Some(&partial.token),
        &replayed_candidates,
        None,
        octocode_contract_types::SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("replay order"));
    assert_eq!(replay.selected, partial.selected);
    assert_eq!(replay.receipt, partial.receipt);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn exhausted_state_budget_withholds_bodies_and_unfiltered_recovers_every_line() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::source_view::{LocalFetchQuery, SourceFacts};
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = budget_runtime(executor.handle().clone(), calls.clone());
    let text = "x\n".repeat(4_096);
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt", "ranges":["1-4096"],
        "unit":"lines", "length":333, "clasifyContext":{"classifiers":["Find x"]}
    }))
    .unwrap();
    let read = |query: &LocalFetchQuery| {
        crate::tools::source_view::process_fetched_content_selected(
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
    let selected = read(&query);
    assert_ne!(selected.status, ReadStatus::Error, "{selected:?}");
    let receipt = selected.selection.as_ref().expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Partial
    );
    assert_eq!(receipt.reason.as_deref(), Some("candidateStateTooLarge"));
    assert_eq!(receipt.unknown_items, 4);
    assert_eq!(receipt.excluded_items, 0);
    assert!(selected.content.as_deref().unwrap_or_default().is_empty());
    let mut query = selected
        .next
        .expect("recovery")
        .unfiltered
        .expect("unfiltered")
        .query;
    assert!(query.clasify_context.is_none());
    assert!(query.selection.is_none());
    let mut contents = String::new();
    let mut pages = 0;
    loop {
        let result = read(&query);
        assert_ne!(result.status, ReadStatus::Error, "{result:?}");
        assert!(result.selection.is_none());
        contents.push_str(result.content.as_deref().unwrap_or_default());
        pages += 1;
        assert!(pages < 50, "unfiltered recovery must make progress");
        let Some(next) = result.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
    }
    assert_eq!(contents, text);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn oversized_line_terminates_selection_and_unfiltered_recovers_all_bytes() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::source_view::{LocalFetchQuery, SourceFacts};
    let executor = tokio::runtime::Runtime::new().unwrap();
    for surrounding in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut runtime = budget_runtime(executor.handle().clone(), calls.clone());
        runtime.limits = crate::execution::RuntimeLimits::DEFAULT.selection;
        let large_line = format!("{}\n", "x".repeat(140_000));
        let text = if surrounding {
            format!("first evidence\n{large_line}second evidence\n")
        } else {
            large_line
        };
        let query: LocalFetchQuery = serde_json::from_value(json!({
            "path":"source.txt", "fullContent":true,
            "clasifyContext":{"classifiers":["Find evidence"]}
        }))
        .unwrap();
        let read = |query: &LocalFetchQuery| {
            crate::tools::source_view::process_fetched_content_selected(
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
        let selected = read(&query);
        assert_ne!(selected.status, ReadStatus::Error, "{selected:?}");
        let receipt = selected.selection.as_ref().expect("receipt");
        assert_eq!(receipt.unknown_items, 1);
        assert_eq!(receipt.excluded_items, 0);
        assert_eq!(receipt.selected_items, if surrounding { 2 } else { 0 });
        assert_eq!(receipt.reason.as_deref(), Some("candidateStateTooLarge"));
        let next = selected.next.expect("raw recovery");
        assert!(next.next_candidate_page.is_none());
        let mut query = next.unfiltered.expect("whole view").query;
        let mut recovered = String::new();
        let mut pages = 0;
        loop {
            let page = read(&query);
            assert_ne!(page.status, ReadStatus::Error, "{page:?}");
            assert!(page.selection.is_none());
            recovered.push_str(page.content.as_deref().unwrap_or_default());
            pages += 1;
            assert!(pages < 50, "raw recovery makes progress");
            let Some(next) = page.next.and_then(|next| next.continue_window) else {
                break;
            };
            query = next.query;
        }
        assert_eq!(recovered, text);
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(surrounding));
    }
}

#[test]
fn complete_long_lines_and_unterminated_tail_are_eligible_in_source_order() {
    struct Uncalled;
    impl SelectionJudge for Uncalled {
        fn identity(&self) -> serde_json::Value {
            json!({"test":"chunks"})
        }
        fn select(&self, _: JudgeCall<'_>) -> crate::tools::selection::Judgments {
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
