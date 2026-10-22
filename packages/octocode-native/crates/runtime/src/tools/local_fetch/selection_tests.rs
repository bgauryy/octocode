use super::*;
use crate::security::ContentSecurity;
use crate::tools::local_fetch::ReadStatus;
use crate::tools::selection::{Decision, JudgeCall, Runtime, SelectionJudge};
use crate::tools::test_support::selection_fake::{self, DOWN, REFUSED, WordJudge};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

struct BudgetJudge(Arc<AtomicUsize>);

impl SelectionJudge for BudgetJudge {
    fn identity(&self) -> serde_json::Value {
        json!({"test":"budget"})
    }

    fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
        self.0.fetch_add(1, Ordering::SeqCst);
        let sent_chars = call
            .admitted
            .iter()
            .map(|&ordinal| {
                call.candidates[ordinal]
                    .state
                    .as_ref()
                    .unwrap()
                    .to_string()
                    .chars()
                    .count()
            })
            .sum::<usize>();
        assert!(
            sent_chars <= call.limits.max_chars,
            "sent {sent_chars} serialized state characters with budget {}",
            call.limits.max_chars
        );
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| (ordinal, Decision::Selected))
            .collect())
    }
}

fn budget_runtime(handle: tokio::runtime::Handle, calls: Arc<AtomicUsize>) -> Runtime {
    let mut runtime = selection_fake::runtime(BudgetJudge(calls), handle, Duration::from_secs(5));
    runtime.limits.max_chars = 4_096;
    runtime
}

#[test]
fn disjoint_spans_never_spend_candidate_slots_on_omission_markers() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = budget_runtime(executor.handle().clone(), calls);
    runtime.limits.max_candidates = 2;
    runtime.limits.max_chars = 20_000;
    let query = json!({"clasifyContext":{"classifiers":["Find the selected source"]}});
    for width in [8, 2_047] {
        let text = format!(
            "{}\n{}{}\n",
            "a".repeat(width),
            super::super::extraction::omission_marker(2, 9),
            "b".repeat(width),
        );
        let (candidates, windows) = chunks(
            &text,
            Some(&[1, 0, 10]),
            None,
            Path::new("source.txt"),
            "source.txt",
            &ContentSecurity::new(),
            Some(&runtime),
            None,
            None,
        );
        assert_eq!(
            candidates.len(),
            2,
            "one candidate per disjoint source span"
        );
        for (candidate, expected) in candidates.iter().zip([1, 10]) {
            let state = candidate.state.as_ref().expect("complete source state");
            let lines = state["sourceLines"].as_array().expect("source coordinates");
            let actual = lines
                .iter()
                .filter_map(|line| line.as_u64().filter(|line| *line > 0))
                .collect::<Vec<_>>();
            assert_eq!(actual, [expected]);
        }
        let outcome = crate::tools::selection::select(
            Some(&runtime),
            &query,
            "source",
            None,
            &candidates,
            None,
            octocode_contract_types::SelectionUnit::Chunks,
        )
        .unwrap_or_else(|_| panic!("selected source spans"));
        assert_eq!(outcome.receipt.unknown_items, 0);
        assert_eq!(outcome.receipt.selected_items, 2);
        assert_eq!(project(&text, &windows, &outcome.selected).0, text);
    }
}

#[test]
fn long_line_match_candidates_keep_original_source_coordinates() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.limits.max_chars = 20_000;
    let text = format!(
        "{}needle{}needle{}\n",
        "x".repeat(3_000),
        "x".repeat(3_000),
        "x".repeat(3_000)
    );
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt", "matchString":"needle",
        "clasifyContext":{"classifiers":["Find needle"]}
    }))
    .unwrap();
    let extracted = super::super::extraction::extract(&query, &text)
        .unwrap_or_else(|_| panic!("match extraction"));
    let source_lines = extracted.source_lines.as_ref().unwrap();
    assert_eq!(
        source_lines.len(),
        extracted.text.split_inclusive('\n').count()
    );
    let (candidates, _) = chunks(
        &extracted.text,
        Some(source_lines),
        None,
        Path::new("source.txt"),
        "source.txt",
        &ContentSecurity::new(),
        Some(&runtime),
        None,
        None,
    );
    for candidate in candidates {
        let state = candidate.state.expect("mapped evidence");
        assert!(
            state["sourceLines"]
                .as_array()
                .unwrap()
                .iter()
                .all(|line| line.as_u64().is_some_and(|line| line <= 1)),
            "{state}"
        );
    }
    let read = |query: &LocalFetchQuery, runtime: &Runtime| {
        super::super::executor::process_fetched_content_selected(
            query,
            text.as_bytes(),
            Path::new("source.txt"),
            "source.txt",
            &SourceFacts::default(),
            &ContentSecurity::new(),
            &NeverCancel,
            Some(runtime),
        )
    };
    let result = read(&query, &runtime);
    assert_eq!(result.error_code, None, "{result:?}");
    assert_eq!(result.content.as_deref(), Some(extracted.text.as_str()));
    assert_eq!(result.total_lines, Some(1));
    assert!(
        result
            .source_line_ranges
            .iter()
            .all(|range| range.start == 1 && range.end == 1),
        "{result:?}"
    );
    assert_eq!(
        result.content.as_deref().unwrap().matches("needle").count(),
        2
    );
    runtime.limits.max_candidates = 1;
    let partial = read(&query, &runtime);
    assert_eq!(partial.selection.as_ref().unwrap().unknown_items, 1);
    let recovery = partial.next.unwrap().unfiltered.unwrap().query;
    assert!(recovery.clasify_context.is_none());
    let unfiltered = read(&recovery, &runtime);
    assert_eq!(unfiltered.content.as_deref(), Some(extracted.text.as_str()));
    assert!(
        unfiltered
            .source_line_ranges
            .iter()
            .all(|range| range.start == 1 && range.end == 1)
    );
}

#[test]
fn nested_declarations_in_large_containers_keep_docs_and_every_source_byte() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.limits.max_chars = 4_096;
    let prefix = "  consume();\n".repeat(400);
    let inner = "  /// Retry the connection.\n  fn retry() {\n    connect();\n  }\n";
    let text = format!("fn container() {{\n{prefix}{inner}}}\n");
    let (_, windows) = chunks(
        &text,
        None,
        None,
        Path::new("source.rs"),
        "source.rs",
        &ContentSecurity::new(),
        Some(&runtime),
        None,
        None,
    );
    assert!(
        windows
            .iter()
            .any(|window| &text[window.bytes.clone()] == inner)
    );
    let ordinals = (0..windows.len()).collect::<Vec<_>>();
    let (recovered, anchors) = project(&text, &windows, &ordinals);
    assert_eq!(recovered, text);
    assert_eq!(anchors, (1..=text.lines().count()).collect::<Vec<_>>());
}

#[test]
fn unsupported_syntax_and_oversized_declarations_keep_complete_line_windows() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    for path in ["source.txt", "source.rs"] {
        let text = format!("fn large() {{\n{}}}\n", "    consume();\n".repeat(600));
        let (_, windows) = chunks(
            &text,
            None,
            None,
            Path::new(path),
            path,
            &ContentSecurity::new(),
            Some(&runtime),
            None,
            None,
        );
        assert!(windows.len() > 2);
        assert!(
            windows
                .iter()
                .all(|window| text[window.bytes.clone()].chars().count() <= TARGET_CHARS)
        );
        assert_eq!(
            project(&text, &windows, &(0..windows.len()).collect::<Vec<_>>()).0,
            text
        );
    }
}

#[test]
fn declaration_body_stays_with_its_header_and_neighbor_is_separate() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.limits.max_chars = 16_384;
    let body = "    consume(value);\n".repeat(150);
    let first = format!("fn process(value: u32) {{\n{body}}}\n");
    let text = format!("{first}fn other() {{}}\n");
    let (_, windows) = chunks(
        &text,
        None,
        None,
        Path::new("source.rs"),
        "source.rs",
        &ContentSecurity::new(),
        Some(&runtime),
        None,
        None,
    );
    assert_eq!(&text[windows[0].bytes.clone()], first);
    assert_eq!(windows.len(), 2);
    assert_eq!(project(&text, &windows, &[0, 1]).0, text);
}

#[test]
fn whole_file_windows_end_before_a_declaration_once_half_full() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    // Two functions of ~1,500 chars each: a 2,048-char window would cut
    // the second one in its middle without snapping.
    let body = |name: &str| {
        let lines = (0..30)
            .map(|i| format!("    let {name}_{i} = {i}; // {}\n", "x".repeat(20)))
            .collect::<String>();
        format!("fn {name}() {{\n{lines}}}\n")
    };
    let text = format!("{}{}", body("first"), body("second"));
    let second_start = text
        .lines()
        .position(|line| line.starts_with("fn second"))
        .unwrap()
        + 1;
    let (_, windows) = chunks(
        &text,
        None,
        None,
        Path::new("source.rs"),
        "source.rs",
        &ContentSecurity::new(),
        Some(&runtime),
        None,
        None,
    );
    assert!(
        windows
            .iter()
            .any(|window| window.lines.first() == Some(&second_start)),
        "a window starts at the second declaration: {:?}",
        windows.iter().map(|w| w.lines.first()).collect::<Vec<_>>()
    );
}

#[test]
fn incomplete_source_maps_never_invent_candidate_coordinates() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    let (candidates, _) = chunks(
        "first\nsecond\n",
        Some(&[7]),
        None,
        Path::new("source.txt"),
        "source.txt",
        &ContentSecurity::new(),
        Some(&runtime),
        None,
        None,
    );
    assert!(candidates.iter().all(|candidate| candidate.state.is_none()));
}

#[test]
fn serialized_state_budget_uses_sanitized_content() {
    struct Redacted;
    impl ContentScan for Redacted {
        fn sanitize(&self, _: &str, _: &Path) -> Result<(String, Vec<String>), (String, String)> {
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
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
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
        super::super::executor::process_fetched_content_selected(
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
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
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
            super::super::executor::process_fetched_content_selected(
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
#[test]
fn context_selection_omits_unrelated_bodies_and_exposes_recovery() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
    struct Relevance;
    impl SelectionJudge for Relevance {
        fn identity(&self) -> serde_json::Value {
            json!({"test":"selection"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&i| {
                    let body = call.candidates[i].state.as_ref().unwrap()["content"]
                        .as_str()
                        .unwrap();
                    (
                        i,
                        if body.contains("REQUIRED_EVIDENCE") {
                            Decision::Selected
                        } else {
                            Decision::Excluded
                        },
                    )
                })
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.limits.max_chars = 20_000;
    runtime.judge = Arc::new(Relevance);
    let text = format!(
        "UNRELATED_SENTINEL {}\nREQUIRED_EVIDENCE retry stops after three attempts\n",
        "x".repeat(2_030)
    );
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt", "length":100,
        "clasifyContext":{"classifiers":["When do retries stop?"]},
        "mainGoal":"Explain retry termination", "reasoning":"Find the stopping condition"
    }))
    .unwrap();
    let result = super::super::executor::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    );
    let body = result.content.as_deref().unwrap();
    assert!(body.contains("REQUIRED_EVIDENCE"));
    assert!(
        !body.contains("UNRELATED_SENTINEL"),
        "Unrelated bodies must remain outside host context"
    );
    let value = serde_json::to_value(&result).unwrap();
    assert!(
        value.pointer("/next/unfiltered").is_some(),
        "Withheld evidence must be explicitly recoverable"
    );
}

fn word_read(fail: Option<&'static str>) -> crate::tools::local_fetch::LocalFetchResult {
    word_read_with(
        json!({}),
        fail,
        &crate::tools::local_fetch::SourceFacts::default(),
    )
}

/// The five-chunk word source read with `extra` query fields under
/// `facts`.
fn word_read_with(
    extra: serde_json::Value,
    fail: Option<&'static str>,
    facts: &crate::tools::local_fetch::SourceFacts,
) -> crate::tools::local_fetch::LocalFetchResult {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::local_fetch::LocalFetchQuery;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail });
    runtime.limits.max_chars = 100_000;
    // One line per chunk: two lines never fit one 2,048-character window.
    let text = ["keep", "drop", "maybe", "maybe", "keep"]
        .iter()
        .map(|word| format!("{word} {}\n", "x".repeat(1_500)))
        .collect::<String>();
    let mut query = json!({
        "path":"source.txt", "fullContent":true,
        "clasifyContext":{"classifiers":["Is it kept?"]}
    });
    for (key, value) in extra.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    let query: LocalFetchQuery = serde_json::from_value(query).unwrap();
    super::super::executor::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        facts,
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    )
}

/// A classified page leaves room in its window for what rides beside
/// its text (plan token, receipt, both unfiltered leads): the same read
/// cut for the same window returns less text than an unclassified one,
/// by at least those chars.
#[test]
fn a_classified_page_leaves_room_for_its_selection_fields() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail: None });
    runtime.limits.max_chars = 1_000_000;
    let text = (0..400)
        .map(|n| {
            let word = if n % 7 == 3 { "maybe" } else { "keep" };
            format!("{word} {n} {}\n", "y".repeat(60))
        })
        .collect::<String>();
    let facts = SourceFacts {
        window: Some(8_000),
        ..SourceFacts::default()
    };
    let read = |classified: bool| {
        let mut query = json!({"path":"source.txt", "fullContent":true});
        if classified {
            query["clasifyContext"] = json!({"classifiers":["Is it kept?"]});
        }
        let query: LocalFetchQuery = serde_json::from_value(query).unwrap();
        super::super::executor::process_fetched_content_selected(
            &query,
            text.as_bytes(),
            Path::new("source.txt"),
            "source.txt",
            &facts,
            &ContentSecurity::new(),
            &NeverCancel,
            classified.then_some(&runtime),
        )
    };
    let plain = read(false);
    let classified = read(true);
    assert!(classified.selection.is_some(), "{classified:?}");
    let chars = |result: &crate::tools::local_fetch::LocalFetchResult| {
        result.content.as_deref().map_or(0, str::len)
    };
    let beside = serde_json::to_value(&classified)
        .map(|wire| {
            let mut extras = 0;
            for key in ["selection", "next"] {
                extras += octocode_engine::json_size::utf16_units(&wire[key]).unwrap_or(0);
            }
            extras
        })
        .unwrap_or(0);
    assert!(beside > 0);
    assert!(
        chars(&classified) + beside <= chars(&plain) + 400,
        "classified {} + beside {beside} vs plain {}",
        chars(&classified),
        chars(&plain)
    );
}

#[test]
fn unfiltered_unknown_reads_exactly_the_unknown_chunks_beside_the_whole_view() {
    let result = word_read(None);
    // The filtered-evidence notice is the response stage's, said once
    // (`selection_errors::decorate`), not the reader's.
    assert!(
        !result
            .warnings
            .iter()
            .any(|warning| warning.contains("not proof of absence")),
        "{:?}",
        result.warnings
    );
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        (
            receipt.selected_items,
            receipt.excluded_items,
            receipt.unknown_items
        ),
        (2, 1, 2)
    );
    let next = result.next.expect("recovery");
    let unknown = next.unfiltered_unknown.expect("unknown chunks").query;
    assert_eq!(
        serde_json::to_value(&unknown.ranges).unwrap(),
        json!(["3-4"])
    );
    assert!(unknown.clasify_context.is_none() && unknown.full_content.is_none());
    let whole = next.unfiltered.expect("whole view").query;
    assert!(whole.clasify_context.is_none());
    assert_eq!(whole.full_content, Some(true));
    assert!(whole.ranges.is_empty());
}

/// A classified read of `text` under a word judge, `max` candidates
/// per call.
fn budgeted_read(
    text: &str,
    query: &serde_json::Value,
    max: usize,
) -> crate::tools::local_fetch::LocalFetchResult {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail: None });
    runtime.limits.max_chars = 1_000_000;
    runtime.limits.max_candidates = max;
    let query: LocalFetchQuery = serde_json::from_value(query.clone()).unwrap();
    super::super::executor::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    )
}

/// One line per chunk: two lines never fit one 2,048-character window.
fn word_lines(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| format!("{word} {}\n", "x".repeat(1_500)))
        .collect()
}

/// Chunks past the candidate budget are judged by `nextCandidatePage`,
/// the same classified read; the first page says how many were judged.
/// Following the chain judges every chunk once and selects exactly what
/// one unbudgeted read selects.
#[test]
fn a_chain_of_candidate_pages_judges_every_chunk_once() {
    let words = [
        "keep", "drop", "maybe", "keep", "drop", "keep", "maybe", "keep",
    ];
    let text = word_lines(&words);
    let query = json!({"path":"source.txt", "fullContent":true,
        "clasifyContext":{"classifiers":["Is it kept?"]}});
    let unbudgeted = budgeted_read(&text, &query, 64);
    let mut result = budgeted_read(&text, &query, 3);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("judged 3 of 8 chunks") && w.contains("5 were not judged")),
        "{:?}",
        result.warnings
    );
    let (mut totals, mut selected) = (Vec::new(), String::new());
    loop {
        assert_eq!(result.error_code, None, "{result:?}");
        let receipt = result.selection.as_ref().expect("receipt");
        totals.push(receipt.total_items);
        selected.push_str(result.content.as_deref().unwrap_or_default());
        let next = result.next.clone().expect("leads");
        let Some(page) = next.next_candidate_page else {
            break;
        };
        assert!(page.query.clasify_context.is_some() && page.query.selection.is_some());
        assert!(totals.len() < 5, "the chain ends");
        result = budgeted_read(&text, &serde_json::to_value(&page.query).unwrap(), 3);
    }
    assert_eq!(totals, [8, 5, 2]);
    let kept = |content: &str| {
        let mut lines = content
            .lines()
            .filter(|line| !line.starts_with("..."))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        lines.sort();
        lines
    };
    assert_eq!(
        kept(&selected),
        kept(unbudgeted.content.as_deref().unwrap()),
        "every selected chunk once"
    );
}

/// Unknown chunks past one row's `ranges` are not dropped:
/// `unfilteredUnknown` reads the first ranges and `unfilteredUnknown2`,
/// … read the rest.
#[test]
fn every_unknown_range_has_a_read_past_one_row() {
    let words = (0..30)
        .map(|i| if i % 2 == 0 { "maybe" } else { "keep" })
        .collect::<Vec<_>>();
    let text = word_lines(&words);
    let query = json!({"path":"source.txt", "fullContent":true,
        "clasifyContext":{"classifiers":["Is it kept?"]}});
    let result = budgeted_read(&text, &query, 64);
    let next = serde_json::to_value(result.next.expect("leads")).unwrap();
    let mut ranges = Vec::new();
    for name in [
        "unfilteredUnknown",
        "unfilteredUnknown2",
        "unfilteredUnknown3",
    ] {
        for row in next[name]["query"]["queries"]
            .as_array()
            .into_iter()
            .flatten()
        {
            ranges.extend(row["ranges"].as_array().unwrap().iter().cloned());
        }
    }
    let expected = (0..30)
        .step_by(2)
        .map(|i| json!(format!("{}-{}", i + 1, i + 1)))
        .collect::<Vec<_>>();
    assert_eq!(ranges, expected, "{next}");
    assert_eq!(
        next["unfiltered"]["query"]["queries"][0]["fullContent"],
        true
    );
}

#[test]
fn a_first_classified_call_with_an_offset_is_refused_input_not_a_stale_cursor() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::local_fetch::{LocalFetchQuery, SourceFacts};
    let executor = tokio::runtime::Runtime::new().unwrap();
    let runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    let query: LocalFetchQuery = serde_json::from_value(json!({
        "path":"source.txt", "offset":3, "length":5, "unit":"lines",
        "clasifyContext":{"classifiers":["Is it kept?"]}
    }))
    .unwrap();
    let result = super::super::executor::process_fetched_content_selected(
        &query,
        "a\nb\nc\nd\ne\nf\n".as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    );
    assert_eq!(
        result.error_code.as_deref(),
        Some("invalidInput"),
        "{result:?}"
    );
    let recovery = result
        .next
        .expect("recovery")
        .unfiltered
        .expect("unfiltered")
        .query;
    // The caller's window stays: only the classifier is dropped.
    assert!(recovery.clasify_context.is_none(), "{recovery:?}");
    assert_eq!((recovery.offset, recovery.unit), (query.offset, query.unit));
}

#[test]
fn a_provider_failing_every_chunk_fails_the_read_with_its_typed_error() {
    let result = word_read(REFUSED);
    assert_eq!(
        result.error_code.as_deref(),
        Some("classificationProviderError")
    );
    assert!(
        result
            .hints
            .iter()
            .any(|hint| hint.contains("OCTOCODE_CLASSIFICATION_API"))
    );
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Unavailable
    );
    assert_eq!(
        receipt.reason.as_deref(),
        Some("classificationProviderError")
    );
    assert_eq!(receipt.unknown_items, 5);
    assert!(result.content.is_none(), "no unjudged body as a fallback");
    let recovery = result
        .next
        .expect("recovery")
        .unfiltered
        .expect("unfiltered")
        .query;
    assert!(recovery.clasify_context.is_none());
}

/// A provider that cannot answer (down, overloaded, unreachable) never
/// withholds a read that succeeded: the whole view is shown under an
/// `unavailable` receipt naming the provider's code, the warning states
/// its error and hint, no unfiltered lead repeats the view, and a later
/// page replays that plan without another provider call.
#[test]
fn a_provider_that_is_down_shows_the_unfiltered_view_as_unavailable() {
    let result = word_read(DOWN);
    assert!(result.error_code.is_none(), "{result:?}");
    let receipt = result.selection.as_ref().expect("receipt");
    assert_eq!(
        receipt.status,
        octocode_contract_types::SelectionStatus::Unavailable
    );
    assert_eq!(receipt.reason.as_deref(), DOWN);
    assert_eq!((receipt.total_items, receipt.unknown_items), (5, 5));
    let content = result.content.as_deref().expect("the view is shown");
    for word in ["keep", "drop", "maybe"] {
        assert!(content.contains(word), "{word} is shown: {content}");
    }
    assert_eq!(content.lines().count(), 5, "every chunk, no marker");
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("HTTP 503")
                && warning.contains("shown unfiltered")
                && warning.contains("OCTOCODE_CLASSIFICATION_API")),
        "{:?}",
        result.warnings
    );
    let next = result.next.as_ref();
    assert!(
        next.is_none_or(|next| next.unfiltered.is_none() && next.next_candidate_page.is_none()),
        "the view is the unfiltered read: {next:?}"
    );
    // Paged one chunk at a time, the recorded plan replays on the next
    // page: the provider (now answering) is not asked, so the view stays
    // unavailable and whole.
    let facts = crate::tools::local_fetch::SourceFacts {
        window: Some(2_048),
        ..crate::tools::local_fetch::SourceFacts::default()
    };
    let first = word_read_with(json!({}), DOWN, &facts);
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("a next page")
        .query;
    assert!(next.selection.is_some(), "the plan rides the next page");
    let replay = word_read_with(
        serde_json::to_value(&next).expect("next query"),
        None,
        &facts,
    );
    assert_eq!(
        replay.selection.as_ref().map(|receipt| receipt.status),
        Some(octocode_contract_types::SelectionStatus::Unavailable)
    );
    assert!(
        replay.error_code.is_none() && replay.content.as_deref().is_some_and(|t| !t.is_empty()),
        "the next page is shown: {replay:?}"
    );
}
