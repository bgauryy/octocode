//! Page ceilings and caps: evidence kept, partial and terminal pages
//! disclosed, totals named in their unit.

use super::*;

#[test]
fn match_page_ceiling_preserves_evidence_and_discloses_terminal_suffix() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.txt"), "CEILING\n".repeat(1001)).expect("matches");
    fs::write(root.path().join("b.txt"), "CEILING\n").expect("other file page");
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"CEILING","regex":"literal","pageSize":1,"matchPageSize":1,"matchPage":1000}),
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
    let body = serde_json::to_value(result).expect("output");
    assert_eq!(body["files"][0]["matches"][0]["line"], 1000, "{body}");
    assert!(body["next"].get("nextMatchPage").is_none(), "{body}");
    assert!(body["next"].get("nextPage").is_some(), "{body}");
    assert_eq!(body["terminalLimit"], true, "{body}");
    assert!(
        body["warnings"]
            .to_string()
            .contains("terminalLimit: matchPage:1000 is the last a continuation may request"),
        "{body}"
    );
    assert_eq!(
        body["partialReasons"],
        serde_json::json!(["cursorMaximum"]),
        "{body}"
    );
}

/// Rows left in the only file page are stated by the row's pagination
/// (`hasMore` beside `nextMatchPage`), so the shared rule does not restate
/// them with `isPartial`.
#[test]
fn leftover_matches_on_the_last_file_page_state_has_more() {
    let root = tempfile::tempdir().expect("fixture");
    fs::write(root.path().join("a.txt"), "HIT\n".repeat(3)).expect("matches");
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"HIT","regex":"literal","pageSize":5,"matchPageSize":1}),
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
    let pagination = result.pagination.as_ref().expect("row pagination");
    assert_eq!((pagination.total_pages, pagination.has_more), (1, true));
    let body = serde_json::to_value(&result).expect("output");
    assert!(body["next"]["nextMatchPage"].is_object(), "{body}");
    let published = crate::tools::test_support::published(
        crate::id::ToolId::LocalSearch,
        &serde_json::to_value(&request).expect("query"),
        &body,
    );
    let data = &published["results"][0]["data"];
    assert!(data.get("isPartial").is_none(), "{published}");
    assert_eq!(data["pagination"]["hasMore"], true, "{published}");
}

/// `pagination.unit` names what `totalItems` counts: files on grid and list
/// pages, hit lines on streamed line pages; matchOnly span rows leave it
/// absent (items).
#[test]
fn pagination_names_the_unit_its_total_counts() {
    let root = tempfile::tempdir().expect("fixture");
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.path().join(name), "HIT one\nHIT two\n".repeat(200)).expect("hits");
    }
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let unit = |fields: serde_json::Value| {
        let mut query = serde_json::json!({"path":root.path(),"matchString":"HIT"});
        query
            .as_object_mut()
            .expect("query")
            .extend(fields.as_object().expect("fields").clone());
        let result = execute_local_search(
            &ls_query(query, None),
            &policy,
            &ContentSecurity::new(),
            &NeverCancel,
            None,
            Some(4_000),
            None,
        )
        .expect("search");
        let body = serde_json::to_value(&result).expect("output");
        assert_eq!(body["pagination"]["hasMore"], true, "{body}");
        body["pagination"]["unit"].clone()
    };
    assert_eq!(unit(serde_json::json!({"pageSize": 1})), "files");
    assert_eq!(
        unit(serde_json::json!({"resultView": "files", "pageSize": 1})),
        "files"
    );
    assert_eq!(unit(serde_json::json!({})), "lines");
    assert!(unit(serde_json::json!({"resultView": "matchOnly"})).is_null());
}

#[test]
fn file_page_ceiling_keeps_later_files_partial_and_terminal() {
    let root = tempfile::tempdir().expect("fixture");
    for index in 0..1001 {
        fs::write(root.path().join(format!("a{index:04}.txt")), "hit\n").expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path":root.path(), "matchString":"hit", "sort":"path", "pageSize":1,
            "page":1000
        }),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("last admitted file page");
    assert_eq!(result.status, SearchStatus::Partial, "{result:?}");
    assert!(result.terminal_limit, "{result:?}");
    assert_eq!(result.files[0].path, "a0999.txt");
    assert_eq!(result.files[0].matches.as_ref().expect("match")[0].line, 1);
    let body = serde_json::to_value(result).expect("output");
    assert_eq!(body["pagination"]["hasMore"], true, "{body}");
    assert!(body["next"].get("nextPage").is_none(), "{body}");
    // The ceiling is the shared terminal-limit disclosure (`paging::next_cursor`):
    // it names the cursor and its maximum; the page count stays a page fact.
    assert!(
        body["warnings"]
            .as_array()
            .expect("ceiling disclosure")
            .iter()
            .any(|warning| warning.as_str().is_some_and(
                |text| text.contains("page:1000 is the last a continuation may request")
            )),
        "{body}"
    );
    assert_eq!(body["pagination"]["totalPages"], 1001, "{body}");
}

#[test]
fn capped_or_bound_results_are_partial_and_terminal_when_next_is_impossible() {
    use super::executor::classify_search;
    assert_eq!(
        classify_search(true, false, false, false, false, true),
        (SearchStatus::Empty, false)
    );
    assert_eq!(
        classify_search(true, false, false, false, true, true),
        (SearchStatus::Partial, true),
        "an empty result with unsearched content is partial, not empty"
    );
    assert_eq!(
        classify_search(false, true, false, false, false, true),
        (SearchStatus::Partial, true)
    );
    assert_eq!(
        classify_search(false, false, true, false, false, false),
        (SearchStatus::Partial, false)
    );
    assert_eq!(
        classify_search(false, false, true, false, false, true),
        (SearchStatus::Partial, false),
        "later files keep global coverage partial without capping an exhausted branch"
    );
    assert_eq!(
        classify_search(false, true, true, false, false, true),
        (SearchStatus::Partial, true),
        "a collection cap stays terminal even when sibling file pages remain"
    );
    assert_eq!(
        classify_search(false, false, true, false, true, true),
        (SearchStatus::Partial, true),
        "unsearched content stays terminal even when sibling file pages remain"
    );
    assert_eq!(
        classify_search(false, false, false, true, false, true),
        (SearchStatus::Partial, true),
        "unreachable match rows are still a terminal limit"
    );
    assert_eq!(
        classify_search(false, false, false, false, false, true),
        (SearchStatus::Success, false)
    );
}

/// Explicitly targeting a single file that the engine skips (over the
/// per-file byte ceiling → capped:true, capReason:"maxFileSize",
/// filesScanned:0) must explain the skip instead of returning a silent
/// "no matches" false negative.
#[test]
fn skipped_single_file_target_explains_the_cap_instead_of_silent_empty() {
    let root = tempfile::tempdir().expect("fixture directory");
    let oversized = root.path().join("huge.txt");
    // Sparse file over the engine's 512 MiB line-search ceiling: the skip
    // is decided on metadata length, so no bytes need to be written.
    let file = fs::File::create(&oversized).expect("fixture");
    file.set_len(512 * 1024 * 1024 + 1).expect("sparse length");
    drop(file);
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": oversized.to_string_lossy().into_owned(), "matchString": "needle".to_string()}),
        None,
    );

    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let body = serde_json::to_value(&result).expect("serialize");

    assert_eq!(body["filesScanned"], 0, "{body}");
    assert!(
        body["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|reason| reason == "maxFileSize")),
        "{body}"
    );
    let hint = body["hints"][0].as_str().expect("skip hint");
    assert!(hint.contains("skipped"), "{hint}");
    assert!(hint.contains("maxFileSize"), "{hint}");
    assert!(hint.contains("localFetch"), "{hint}");
}
