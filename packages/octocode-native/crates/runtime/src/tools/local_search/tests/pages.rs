use super::*;

use octocode_contract_types::ErrorCode;

mod ceilings;
mod declarations;
mod redaction;
mod small_results;
mod window_fill;

#[test]
fn relevance_cap_keeps_hot_file_and_marks_first_and_final_pages_partial() {
    let root = tempfile::tempdir().expect("fixture directory");
    for i in 0..=10_000 {
        fs::write(root.path().join(format!("a{i:05}.txt")), "hit\n").expect("fixture");
    }
    fs::write(root.path().join("zzz-hot.txt"), "hit\n".repeat(40)).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "hit".to_string(), "regex": LocalSearchQueryRegex::Literal}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    assert_eq!(result.files[0].path, "zzz-hot.txt");
    let stats = &result.stats;
    assert_eq!(stats.files_searched, 10_002);
    assert_eq!(stats.files_matched, 10_002);
    assert_eq!(stats.total_occurrences, Some(10_001 + 40));
    assert!(result.is_partial);
    assert_eq!(result.status, SearchStatus::Partial);
    assert!(
        stats
            .cap_reasons
            .contains(&octocode_engine::types::CapReason::MaxCollectedFiles),
        "{stats:?}"
    );
    let last_page = result.pagination.as_ref().expect("file pages").total_pages;
    let last_request = ls_query(
        serde_json::json!({"page": last_page, "snapshot": result.source_snapshot.as_ref().expect("snapshot")}),
        Some(&request),
    );
    let last = execute_local_search(
        &last_request,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("last collected page");
    assert!(last.is_partial);
    assert!(last.terminal_limit);
    // No page continues past the cap; the only lead lists the scope's top
    // directories to split the search by.
    let next = last.next.as_ref().expect("split lead");
    assert_eq!(
        next.as_object()
            .map(|map| map.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["narrowScope".to_owned()]),
        "{next}"
    );
    assert_eq!(next["narrowScope"]["query"]["queries"][0]["maxDepth"], 1);
    assert!(
        last.warnings
            .iter()
            .any(|w| w.contains("2 of 10002 matched files")),
        "{:?}",
        last.warnings
    );
    assert_eq!(
        last.stats.cap_reasons,
        [octocode_engine::types::CapReason::MaxCollectedFiles]
    );
}

/// A large page keeps every run's owner, with its last line.
#[test]
fn large_page_keeps_enclosing_labels() {
    let source: String = (0..6)
        .map(|f| format!("fn f{f}() {{\n{}}}\n", "    helper();\n".repeat(10)))
        .collect();
    let body = search_fixture(
        &[("src/m.rs", &source)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let rows = body["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 60, "{body}");
    for index in 0..rows.len() {
        let enclosing = &owner_of(&body["files"][0], index).expect("owner");
        assert_eq!(
            enclosing["symbolName"],
            format!("f{}", index / 10),
            "row {index}: {body}"
        );
        assert_eq!(enclosing["kind"], "function", "row {index}: {body}");
        assert!(
            enclosing["endLine"].as_u64() > enclosing["line"].as_u64(),
            "row {index}: {body}"
        );
    }
}

/// Declaration rows carry `declaration:true`; other rows carry nothing.
#[test]
fn declaration_rows_are_marked_sparsely() {
    let body = search_fixture(
        &[(
            "m.rs",
            "fn target() {}\n// target in prose\nfn caller() { target(); }\n",
        )],
        ls_query(serde_json::json!({"matchString": "target"}), None),
    );
    let rows = body["files"][0]["matches"].as_array().expect("rows");
    let by_line = |line: u64| {
        rows.iter()
            .find(|row| row["line"] == line)
            .unwrap_or_else(|| panic!("line {line}: {body}"))
    };
    assert_eq!(by_line(1)["declaration"], true, "{body}");
    assert!(by_line(2).get("declaration").is_none(), "{body}");
    assert!(by_line(3).get("declaration").is_none(), "{body}");
}

/// Count views rank the most hits first under the default sort; equal
/// counts keep relevance order. Other views keep relevance.
#[test]
fn count_views_order_by_count_under_the_default_sort() {
    use super::executor::order_files;
    let file = |path: &str, match_count| octocode_engine::types::TextSearchFile {
        path: path.into(),
        match_count,
        matches: Vec::new(),
        source: None,
    };
    // Engine relevance order: the declaring files first.
    let relevance = || {
        vec![
            file("decl.rs", 3),
            file("tie_b.rs", 38),
            file("big.rs", 155),
            file("tie_a.rs", 38),
        ]
    };
    for view in ["countMatches", "countLines"] {
        let mut files = relevance();
        order_files(
            &ls_query(serde_json::json!({"resultView": view}), None),
            &mut files,
        );
        let order: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            order,
            ["big.rs", "tie_b.rs", "tie_a.rs", "decl.rs"],
            "{view}"
        );
    }
    let mut files = relevance();
    order_files(&ls_query(serde_json::json!({}), None), &mut files);
    assert_eq!(files[0].path, "decl.rs", "a line view keeps relevance");
}

fn context_fixture() -> (tempfile::TempDir, PathPolicy, ContentSecurity) {
    let root = tempfile::tempdir().expect("fixture directory");
    // Nine ~37-char lines around one hit: a ±4 window is ~334 chars.
    let lines: Vec<String> = (1..=9)
        .map(|i| {
            if i == 5 {
                format!("let needle_{i} = compute_value_number_{i}();")
            } else {
                format!("let other_{i} = compute_value_number_{i}();")
            }
        })
        .collect();
    fs::write(root.path().join("ctx.rs"), lines.join("\n") + "\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    (root, policy, security)
}

fn goal_search_replay_with_window(response_window: Option<usize>) {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(root.path().join("a.txt"), "needle\n").expect("fixture");
    std::fs::write(root.path().join("b.txt"), "needle\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"needle","pageSize":1,"clasifyContext":{"classifiers":["find needle"]}}),
        None,
    );
    let first = select_all_search(
        &request,
        &policy,
        &security,
        &NeverCancel,
        None,
        response_window,
    )
    .expect("page");
    assert_eq!(
        first.selection.as_ref().map(|r| r.status),
        Some(octocode_contract_types::SelectionStatus::Complete)
    );
    let continued: LocalSearchQuery = serde_json::from_value(
        first.next.expect("next")["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("continuation");
    assert!(continued.selection.is_some());
    let second = select_all_search(
        &continued,
        &policy,
        &security,
        &NeverCancel,
        None,
        response_window,
    )
    .expect("page");
    assert_eq!(second.files.len(), 1);
    let changed = ls_query(
        serde_json::json!({"clasifyContext":{"classifiers":["another goal"]}}),
        Some(&continued),
    );
    assert_eq!(
        select_all_search(
            &changed,
            &policy,
            &security,
            &NeverCancel,
            None,
            response_window
        )
        .expect_err("stale")
        .code,
        ErrorCode::StaleSnapshot
    );
}

#[test]
fn goal_search_replays_skipped_order_and_rejects_changed_goals() {
    for response_window in [None, Some(20_000)] {
        goal_search_replay_with_window(response_window);
    }
}

#[test]
fn context_lines_scale_the_default_match_content_length() {
    let (root, policy, security) = context_fixture();
    let base = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle".to_string(), "contextLines": 4}),
        None,
    );
    let run = |request: &LocalSearchQuery| {
        let result =
            execute_local_search(request, &policy, &security, &NeverCancel, None, None, None)
                .expect("search");
        serde_json::to_value(result).expect("serialize")
    };
    let scaled = run(&base);
    let matched = &scaled["files"][0]["matches"][0];
    assert!(matched.get("originalChars").is_none(), "{scaled}");
    assert!(matched["value"].as_str().expect("value").chars().count() > 300);

    let explicit = run(&ls_query(
        serde_json::json!({"matchContentLength": 200}),
        Some(&base.clone()),
    ));
    assert!(explicit["files"][0]["matches"][0]["originalChars"].is_u64());

    let detailed = run(&ls_query(
        serde_json::json!({"contextLines": 3, "resultView": LocalSearchQueryResultView::Paginated}),
        Some(&base.clone()),
    ));
    assert!(
        detailed["files"][0]["matches"][0]
            .get("originalChars")
            .is_none(),
        "{detailed}"
    );
}

/// Source directories with build-ish names are searched like any other;
/// only dependency/build caches are pruned.
#[test]
fn source_dirs_named_output_cache_vendor_are_searched_but_node_modules_is_not() {
    let root = tempfile::tempdir().expect("fixture directory");
    for dir in [
        "detail/output",
        "core/cache",
        "vendor/lib",
        "node_modules/pkg",
    ] {
        fs::create_dir_all(root.path().join(dir)).expect("dir");
        fs::write(root.path().join(dir).join("x.txt"), "needle\n").expect("file");
    }
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "resultView": "files"}),
        None,
    );
    let result = execute_local_search(
        &request,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("search");
    let body = serde_json::to_string(&serde_json::to_value(&result).expect("json")).expect("text");
    for found in [
        "detail/output/x.txt",
        "core/cache/x.txt",
        "vendor/lib/x.txt",
    ] {
        assert!(body.contains(found), "{found} missing: {body}");
    }
    assert!(
        result
            .files
            .iter()
            .all(|file| !file.path.contains("node_modules")),
        "{body}"
    );
    // Not searched, but never silently: named once, in `skippedPaths`
    // (no warning restates it as a count), with the escape flag.
    assert!(
        result
            .warnings
            .iter()
            .all(|warning| !warning.contains("Default excludes")),
        "{body}"
    );
    let disclosed = serde_json::to_value(&result).expect("result");
    assert_eq!(
        disclosed["skippedPaths"]["entries"],
        serde_json::json!([{"path":skipped_source_path(root.path(),"node_modules"),"cause":"default-excluded"}]),
        "{body}"
    );
    // The lead that searches them is the one skipped-paths lead.
    let lead = &result.next.as_ref().expect("next")["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["defaultExcludes"], false, "{body}");
}
