//! Candidate chunks: declaration boundaries, line windows and source
//! coordinates.
use super::*;

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
            crate::tools::source_view::extraction::omission_marker(2, 9),
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
    use crate::tools::source_view::{LocalFetchQuery, SourceFacts};
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
    let extracted = crate::tools::source_view::extraction::extract(&query, &text)
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
        crate::tools::source_view::process_fetched_content_selected(
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
