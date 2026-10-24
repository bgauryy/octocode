//! Caller pages: incoming walks, reused first pages and continuations
//! that reach every row once.

use super::*;

#[test]
fn first_pages_of_incoming_walks_reuse_responses_keyed_by_scope_fingerprint() {
    let callers = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts", "symbolName": "f", "line": 1
    }));
    let reused = response_scope(&callers, "content".into(), Some("fp"));
    assert!(reused.reuse);
    assert_ne!(reused.generation, "content");
    // Another fingerprint (any file edit) is another generation.
    assert_ne!(
        response_scope(&callers, "content".into(), Some("fp2")).generation,
        reused.generation
    );
    // No fingerprint (walk past its bounds): first pages ask the server.
    let unbounded = response_scope(&callers, "content".into(), None);
    assert!(!unbounded.reuse);
    assert_eq!(unbounded.generation, "content");
    // Hover always asks; its continuation keying is unchanged.
    let hover = query(serde_json::json!({
        "operation": "hover", "path": "/repo/a.ts", "symbolName": "f", "line": 1
    }));
    assert!(!response_scope(&hover, "content".into(), Some("fp")).reuse);
    // A continuation page still reuses.
    let page_two = requery(
        &callers,
        serde_json::json!({"page": 2, "snapshot": "lsp-v1:x"}),
    );
    assert!(response_scope(&page_two, "content".into(), None).reuse);
    // So does a later importer window: its anchor answer is the one its
    // candidate digest was cut from.
    let window_two = requery(
        &callers,
        serde_json::json!({"importerPage": 2, "snapshot": "lsp-imp:x"}),
    );
    assert!(response_scope(&window_two, "content".into(), None).reuse);
}

#[tokio::test(flavor = "current_thread")]
async fn incoming_walk_answers_level_one_before_importer_roots_join() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("a.ts");
    std::fs::write(&file, "x\n").expect("file");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    let mut graph = graph(&[
        ("target", &["a", "b"]),
        ("importer", &["c", "a"]),
        ("a", &["d"]),
    ]);
    graph.uri = Some(uri.clone());
    let at = |name: &str| node_at(name, Some(&uri));
    let cancel = crate::execution::cancel::NeverCancel;
    let mut walk = Walk::new(&paths, &[at("target")], Expansion::IncomingCalls, 2);
    assert!(walk.step(&graph, &cancel).await.expect("level 1"));
    // The level-1 answer names its files before any importer is verified.
    let canonical = file.canonicalize().expect("canonical");
    assert!(
        walk.answered_files()
            .contains(canonical.to_string_lossy().as_ref())
    );
    let before = graph.requests.get();
    walk.add_roots(
        &graph,
        vec![at("importer"), at("target")],
        std::collections::HashSet::new(),
        &cancel,
    )
    .await
    .expect("importer roots");
    // Only the new root is expanded; the anchor root is not asked twice.
    assert_eq!(graph.requests.get(), before + 1);
    let finished = walk.finish(&graph, &cancel).await.expect("walk");
    let summary = edge_summary(&finished);
    // `c` joins at level 1 from the importer root; `a` is listed for both
    // roots but expanded once; `d` is level 2.
    assert!(
        summary
            .iter()
            .any(|(name, level, _)| name == "c" && *level == 1),
        "{summary:?}"
    );
    assert_eq!(
        summary
            .iter()
            .filter(|(name, level, _)| name == "d" && *level == 2)
            .count(),
        1,
        "{summary:?}"
    );
}

/// Names of a page's rows, whichever key the kind lists them under.
fn page_names(row: &serde_json::Value) -> Vec<String> {
    ["matches", "symbols"]
        .iter()
        .filter_map(|key| row[*key].as_array())
        .flatten()
        .filter_map(|item| {
            item.get("name")
                .or_else(|| item.get("symbolName"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// Walk a result set copying each `next.nextPage` unchanged under the one
/// page `budget` every page of a walk is cut at: the walk reaches every row
/// exactly once, in order.
fn walk_pages(
    first: &LspSearchQuery,
    budget: usize,
    page: impl Fn(&LspSearchQuery, usize) -> serde_json::Value,
) -> Vec<String> {
    let mut q = first.clone();
    let mut seen = Vec::new();
    for call in 0.. {
        assert!(call < 1_000, "the walk must end");
        let row = with_next(&q, page(&q, budget));
        assert!(row.get("errorCode").is_none(), "page {call}: {row}");
        assert_ne!(row["pagination"]["outOfRange"], true, "page {call}: {row}");
        let names = page_names(&row);
        assert!(!names.is_empty(), "page {call} moved: {row}");
        seen.extend(names);
        let Some(next) = row.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        q = serde_json::from_value(next.clone()).expect("executable continuation");
    }
    seen
}

#[test]
fn a_copied_continuation_reaches_every_row_once() {
    // Rows of uneven size, so a budget cuts pages at uneven rows.
    let items = (0..60)
        .map(|index| {
            serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(index % 7 * 40)})
        })
        .collect::<Vec<_>>();
    let expected = items
        .iter()
        .map(|item| item["name"].as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "path": "file:///repo", "symbolQuery": "s"
    }));
    for budget in [600, 900, 2_400] {
        let seen = walk_pages(&q, budget, |q, budget| {
            items_payload(q, "symbols", serde_json::json!(items), budget)
        });
        assert_eq!(seen, expected, "budget {budget}");
    }

    // documentSymbols pages top-level rows the same way.
    let range = |line: u64| serde_json::json!({"start": {"line": line, "character": 0}, "end": {"line": line, "character": 1}});
    let symbols = (0..60)
        .map(|index| {
            serde_json::json!({"name": format!("s{index:02}{}", "_".repeat(index as usize % 7 * 30)),
                "kind": 12, "range": range(index), "selectionRange": range(index)})
        })
        .collect::<Vec<_>>();
    let expected = symbols
        .iter()
        .map(|item| item["name"].as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "documentSymbols", "path": "file:///repo/src/lib.rs"
    }));
    for budget in [600, 900, 2_400] {
        let seen = walk_pages(&q, budget, |q, budget| {
            items_payload(q, "documentSymbols", serde_json::json!(symbols), budget)
        });
        assert_eq!(seen, expected, "budget {budget}");
    }
}

#[test]
fn a_page_past_the_end_or_of_another_answer_never_skips_rows() {
    let items = (0..30)
        .map(|index| serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(80)}))
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "path": "file:///repo", "symbolQuery": "s"
    }));
    let first = with_next(
        &q,
        items_payload(&q, "symbols", serde_json::json!(items), 600),
    );
    let next: LspSearchQuery =
        serde_json::from_value(first["next"]["nextPage"]["input"]["queries"][0].clone())
            .expect("continuation");
    // The answer changed: the digest no longer matches.
    let mut changed = items.clone();
    changed.pop();
    let stale = items_payload(&next, "symbols", serde_json::json!(changed), 600);
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    // A page past the end is out of range, not a clamped repeat.
    let past = requery(&next, serde_json::json!({"page": 99}));
    let row = items_payload(&past, "symbols", serde_json::json!(items), 600);
    assert_eq!(row["pagination"]["outOfRange"], true, "{row}");
    assert_eq!(row["matches"], serde_json::json!([]), "{row}");
}

/// A later page whose snapshot is another answer's is a stale snapshot with
/// the restart lead, never a page that silently skips or repeats rows.
#[test]
fn a_forged_snapshot_is_a_stale_snapshot() {
    let items = (0..30)
        .map(|index| serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(80)}))
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "path": "file:///repo", "symbolQuery": "s"
    }));
    let second: LspSearchQuery = serde_json::from_value(
        with_next(
            &q,
            items_payload(&q, "symbols", serde_json::json!(items), 600),
        )["next"]["nextPage"]["input"]["queries"][0]
            .clone(),
    )
    .expect("continuation");
    assert_eq!(lsp_search_query::page(&second), Some(2));
    let forged = requery(&second, serde_json::json!({"snapshot": "lsp-v3:forged"}));
    let row = items_payload(&forged, "symbols", serde_json::json!(items), 600);
    assert_eq!(row["errorCode"], "staleSnapshot", "{row}");
    let restart = &crate::tools::test_support::checks::related_call(&row, "lspSearch", |_| true)["input"]
        ["queries"][0];
    for cursor in crate::contracts::state_fields(crate::id::ToolId::LspSearch) {
        assert!(restart.get(cursor).is_none(), "{cursor}: {row}");
    }
    assert!(row.get("matches").is_none(), "{row}");
}

/// A callers page is cut on the compact call rows it prints, so it fills
/// the budget instead of a third of it, and still reaches every edge once.
#[test]
fn caller_pages_fill_the_budget_with_compact_rows() {
    let items = (0..240)
        .map(|n| {
            serde_json::json!({
                "from": {"symbolName": format!("caller{n}"), "kind": "function",
                    "path": format!("/repo/src/module_{}/file.rs", n / 4),
                    "displayRange": {"startLine": n + 10, "startCharacter": 8, "endLine": n + 30, "endCharacter": 1}},
                "fromRanges": [{"startLine": n + 12, "startCharacter": 5, "endLine": n + 12, "endCharacter": 9}],
                "level": 1
            })
        })
        .collect::<Vec<_>>();
    let q = query(serde_json::json!({
        "operation": "callers", "path": "file:///repo/a.rs", "symbolName": "foo", "line": 1
    }));
    let budget = 4_000;
    let paths = repo_policy();
    let mut q = q;
    let mut seen = Vec::new();
    let mut fills = Vec::new();
    for call in 0.. {
        assert!(call < 100, "the walk must end");
        let row = with_next(&q, call_items_payload(&q, &items, None, budget, &paths));
        let chars = octocode_engine::json_size::utf16_units(&row["files"]).unwrap_or(0);
        assert!(chars <= budget, "page {call} is {chars} chars");
        fills.push(chars);
        seen.extend(
            row["files"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|file| file["matches"].as_array().into_iter().flatten())
                .map(|call| call["symbolName"].as_str().expect("name").to_owned()),
        );
        let Some(next) = row.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        q = serde_json::from_value(next.clone()).expect("continuation");
    }
    let expected = (0..240).map(|n| format!("caller{n}")).collect::<Vec<_>>();
    assert_eq!(seen, expected);
    // Every page but the last is at least 80% full.
    for fill in &fills[..fills.len() - 1] {
        assert!(*fill * 5 >= budget * 4, "{fills:?}");
    }
}
