//! Executable continuations: anchored replays, identity handoffs and
//! terminal fits that keep controls and cursor identity.

use super::*;

#[test]
fn local_search_identity_continuations_validate_as_anchored_queries() {
    // The shape localSearch emits as next.references / next.callers for a
    // declaration hit: uri + symbolName + line, nothing else.
    for operation in ["references", "callers"] {
        let continuation = serde_json::json!({
            "mainGoal": "Who uses run_and_clear_commit_hooks?",
            "reasoning": "Declaration hit from localSearch.",
            "operation": operation,
            "path": "/repo/django/db/backends/base/base.py",
            "symbolName": "run_and_clear_commit_hooks",
            "line": 749
        });
        let validated = crate::contracts::validate_query("lspSearch", continuation)
            .unwrap_or_else(|error| panic!("{operation}: {error:?}"));
        let parsed: LspSearchQuery = serde_json::from_value(validated).expect("typed query");
        assert_eq!(
            lsp_search_query::anchor_line(&parsed),
            Some(749),
            "{operation}"
        );
        assert_eq!(lsp_search_query::operation(&parsed), operation);
        assert_eq!(
            lsp_search_query::symbol_name(&parsed),
            Some("run_and_clear_commit_hooks")
        );
    }
}

#[test]
fn anchored_operation_shapes_preserve_controls_and_replay() {
    for operation in [
        "definition",
        "typeDefinition",
        "implementation",
        "references",
        "callers",
        "callees",
        "supertypes",
        "subtypes",
        "hover",
    ] {
        let mut row = serde_json::json!({
            "operation": operation, "path": "/repo/a.ts", "symbolName": "run", "line": 7,
            "orderHint": 2, "page": 3, "pageSize": 4, "snapshot": "stable", "workspaceRoot": "/repo"
        });
        let locations = matches!(
            operation,
            "definition" | "typeDefinition" | "implementation" | "references"
        );
        let hierarchy = matches!(operation, "callers" | "callees" | "supertypes" | "subtypes");
        let importer = matches!(operation, "references" | "callers");
        if locations {
            row["groupByFile"] = true.into();
            row["contextLines"] = 2.into();
        }
        if hierarchy {
            row["graphDepth"] = 3.into();
        }
        if importer {
            row["importerPage"] = 4.into();
        }
        if operation == "references" {
            row["includeDeclaration"] = false.into();
        }
        if operation == "subtypes" {
            row["candidatePage"] = 5.into();
        }
        let mut parsed = query(row);
        assert_eq!(lsp_search_query::operation(&parsed), operation);
        assert_eq!(lsp_search_query::path(&parsed), Some("/repo/a.ts"));
        assert_eq!(lsp_search_query::workspace_root(&parsed), Some("/repo"));
        assert_eq!(lsp_search_query::symbol_name(&parsed), Some("run"));
        assert_eq!(lsp_search_query::anchor_line(&parsed), Some(7));
        assert_eq!(lsp_search_query::order_hint(&parsed), Some(2));
        assert_eq!(lsp_search_query::page(&parsed), Some(3));
        assert_eq!(lsp_search_query::page_size(&parsed), Some(4));
        assert_eq!(lsp_search_query::snapshot(&parsed), Some("stable"));
        assert_eq!(
            lsp_search_query::group_by_file(&parsed),
            locations.then_some(true)
        );
        assert_eq!(
            lsp_search_query::context_lines(&parsed),
            locations.then_some(2)
        );
        assert_eq!(
            lsp_search_query::graph_depth(&parsed),
            hierarchy.then_some(3)
        );
        assert_eq!(
            lsp_search_query::include_declaration(&parsed),
            (operation == "references").then_some(false)
        );
        assert_eq!(
            lsp_search_query::importer_page(&parsed),
            if importer { 4 } else { 1 }
        );
        assert_eq!(
            lsp_search_query::candidate_page(&parsed),
            if operation == "subtypes" { 5 } else { 1 }
        );
        set_path(&mut parsed, "/canonical/a.ts".to_owned());
        set_workspace_root(&mut parsed, "/canonical".to_owned());
        assert_eq!(lsp_search_query::path(&parsed), Some("/canonical/a.ts"));
        assert_eq!(
            lsp_search_query::workspace_root(&parsed),
            Some("/canonical")
        );
        assert_eq!(query(lsp_search_query::to_row(&parsed)), parsed);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn compact_reference_terminal_fit_preserves_controls_and_cursor_identity() {
    use serde_json::{Value, json};
    async fn walk(
        paths: &crate::policy::path::PathPolicy,
        initial: &LspSearchQuery,
        snippets: &[Value],
        windows: &[usize],
    ) -> Vec<Value> {
        use crate::tools::test_support::checks::{Page, Walker};
        let tool = crate::id::ToolId::LspSearch;
        let mut walker = Walker::new(
            tool.as_str(),
            json!({"queries":[lsp_search_query::to_row(initial)]}),
        );
        let mut pages = Vec::new();
        while let Some(call) = walker.next_query() {
            let query: LspSearchQuery =
                serde_json::from_value(call["queries"][0].clone()).expect("executable nextPage");
            let window = windows[pages.len().min(windows.len() - 1)];
            let budget = crate::response::page_windows::page_budget(
                Some(window),
                &lsp_search_query::to_row(&query),
            );
            let row = locations(
                &query,
                &mut SourceCache::new(paths),
                "references",
                "referencesProvider",
                snippets.to_vec(),
                budget,
                Some(window),
            )
            .await;
            let row = with_next(&query, row);
            walker.accept(Page {
                output: crate::tools::test_support::published(
                    tool,
                    &lsp_search_query::to_row(&query),
                    &row,
                ),
                rows: identities(std::slice::from_ref(&row))
                    .iter()
                    .map(u64::to_string)
                    .collect(),
            });
            pages.push(row);
            assert!(pages.len() <= snippets.len(), "finite exact-once walk");
        }
        walker.finish();
        pages
    }
    fn identities(pages: &[Value]) -> Vec<u64> {
        pages
            .iter()
            .flat_map(|page| page["files"].as_array().expect("files"))
            .flat_map(|file| file["matches"].as_array().expect("matches"))
            .map(|hit| hit["line"].as_u64().expect("line"))
            .collect()
    }
    for content in ["ordinary evidence ".repeat(7), "普通😀 evidence ".repeat(7)] {
        let (_dir, root, paths) = temp_workspace();
        let file = root.join("a.ts");
        std::fs::write(&file, format!("{content}\n").repeat(241)).expect("source");
        let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
        let query =
            query(json!({"operation":"references","path":uri,"symbolName":"ordinary","line":1}));
        let found = (0..241)
            .map(|line| {
                json!({"uri":uri,"content":content,
            "range":{"start":{"line":line,"character":0},"end":{"line":line,"character":4}}})
            })
            .collect::<Vec<_>>();
        let full = locations(
            &requery(&query, json!({"pageSize":1000})),
            &mut SourceCache::new(&paths),
            "references",
            "referencesProvider",
            found.clone(),
            1,
            None,
        )
        .await;
        // The measured public terminal row plus the existing shared envelope
        // reserve fits; the repeated-query reserve would leave a tiny tail.
        let mut low = 1;
        let mut high = octocode_engine::json_size::utf16_units(&full).unwrap_or(0) * 2;
        assert!(crate::response::page_windows::terminal_row_fits(
            Some(high),
            &full
        ));
        while low < high {
            let middle = low + (high - low) / 2;
            if crate::response::page_windows::terminal_row_fits(Some(middle), &full) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let window = low;
        let expected = (1..=241).collect::<Vec<_>>();
        let default = walk(&paths, &query, &found, &[window]).await;
        assert_eq!(default.len(), 1, "whole terminal row should fit: {window}");
        assert_eq!(identities(&default), expected);
        let small = walk(&paths, &query, &found, &[5_000]).await;
        assert!(small.len() > 1, "small windows must keep pages");
        assert_eq!(identities(&small), expected);
        let explicit = walk(
            &paths,
            &requery(&query, json!({"pageSize":17})),
            &found,
            &[window],
        )
        .await;
        assert_eq!(explicit.len(), 241usize.div_ceil(17));
        assert_eq!(identities(&explicit), expected);
        // Every call of a walk pages under the one automatic window
        // (`domain_dispatch::local_response_window`), whatever the batch, so
        // a walk is cut at one budget: any budget reaches every row once.
        let medium = walk(&paths, &query, &found, &[12_000]).await;
        assert!(
            medium.len() > 1 && medium.len() < small.len(),
            "{}",
            medium.len()
        );
        assert_eq!(identities(&medium), expected);
        let receipts = walk(
            &paths,
            &requery(&query, json!({"extras":["receipts"]})),
            &found,
            &[window],
        )
        .await;
        assert!(
            receipts.len() > 1,
            "unmeasured detailed extras retain the conservative cut"
        );
        assert_eq!(identities(&receipts), expected);
        let stale = locations(
            &requery(&query, json!({"page":2,"snapshot":"lsp-v3:not-current"})),
            &mut SourceCache::new(&paths),
            "references",
            "referencesProvider",
            found,
            1,
            Some(window),
        )
        .await;
        assert_eq!(stale["errorCode"], "staleSnapshot");
    }
}

/// lspSearch recomputes page N from the page budget: page 1 asked for an
/// absolute path over 128 characters and the `next.nextPage` its published
/// response leads to (that path respelled workspace-relative) get the same
/// budget, so the continuation starts exactly where page 1 stopped.
#[tokio::test]
async fn reference_next_page_keeps_page_one_budget() {
    use serde_json::json;
    let (_dir, root, paths) = temp_workspace();
    let deep = (0..3).fold(root.join("deep"), |dir, level| {
        let mut name = format!("level_{level}_{}", "x".repeat(60));
        name.truncate(60);
        dir.join(name)
    });
    std::fs::create_dir_all(&deep).expect("deep dir");
    let file = deep.join("a.ts");
    std::fs::write(&file, "const used = 1;\n".repeat(400)).expect("source");
    let absolute = file.to_string_lossy().into_owned();
    assert!(absolute.len() > 128, "{absolute}");
    let uri = octocode_engine::lsp::uri::path_to_uri(&absolute).expect("uri");
    let found = (0..400)
        .map(|line| {
            json!({"uri":uri,"content":"const used = 1;",
                "range":{"start":{"line":line,"character":6},"end":{"line":line,"character":10}}})
        })
        .collect::<Vec<_>>();
    let query =
        query(json!({"operation":"references","path":absolute,"symbolName":"used","line":1}));
    let window = 4_000;
    let row = locations(
        &query,
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        found,
        crate::response::page_windows::page_budget(
            Some(window),
            &lsp_search_query::to_row(&query),
        ),
        Some(window),
    )
    .await;
    let row = with_next(&query, row);
    assert_eq!(row["pagination"]["hasMore"], true, "page 1 leads on: {row}");
    crate::tools::test_support::assert_next_page_keeps_the_budget(
        crate::id::ToolId::LspSearch,
        &lsp_search_query::to_row(&query),
        &row,
        &paths,
        |window, row| {
            let typed: LspSearchQuery = serde_json::from_value(row.clone()).expect("typed row");
            crate::response::page_windows::page_budget(window, &lsp_search_query::to_row(&typed))
        },
    );
}
