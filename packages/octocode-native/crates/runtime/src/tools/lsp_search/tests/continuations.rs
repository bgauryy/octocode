use super::*;
use serde_json::json;

#[test]
fn every_operation_retains_its_canonical_controls_and_saved_progress() {
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
        let mut row = json!({"operation":operation,"path":"/repo/a.ts","symbolName":"run","line":7,"occurrence":2,"workspaceRoot":"/repo"});
        if matches!(operation, "callers" | "callees" | "supertypes" | "subtypes") {
            row["graphDepth"] = json!(3);
        }
        if operation == "references" {
            row["includeDeclaration"] = json!(false);
        }
        let normalized =
            crate::contracts::validate_query("lspSearch", row.clone()).expect("first call");
        let mut parsed = query(normalized);
        assert_eq!(lsp_search_query::operation(&parsed), operation);
        assert_eq!(lsp_search_query::occurrence(&parsed), Some(2));
        set_path(&mut parsed, "/canonical/a.ts".into());
        set_workspace_root(&mut parsed, "/canonical".into());
        assert_eq!(lsp_search_query::path(&parsed), Some("/canonical/a.ts"));
        assert_eq!(
            lsp_search_query::workspace_root(&parsed),
            Some("/canonical")
        );
        let mut saved = lsp_search_query::to_row(&parsed);
        saved["page"] = json!(3);
        saved["snapshot"] = json!("lsp-v3:saved");
        assert!(crate::contracts::validate_query("lspSearch", saved.clone()).is_err());
        let built = crate::contracts::validate_built_query("lspSearch", saved.clone())
            .expect("trusted saved progress");
        assert_eq!(query(built), query(saved));
        for alias in [
            "orderHint",
            "pageSize",
            "extras",
            "mainGoal",
            "reasoning",
            "groupByFile",
        ] {
            let mut bad = row.clone();
            bad[alias] = json!(1);
            assert!(
                crate::contracts::validate_query("lspSearch", bad).is_err(),
                "{operation}: {alias}"
            );
        }
    }
    for row in [
        json!({"operation":"documentSymbols","path":"a.ts"}),
        json!({"operation":"diagnostic","path":"a.ts"}),
        json!({"operation":"workspaceSymbol","path":"src","symbolQuery":"run"}),
        json!({"operation":"workspaceSymbol","workspaceRoot":".","symbolQuery":"run"}),
    ] {
        let parsed =
            query(crate::contracts::validate_query("lspSearch", row.clone()).expect("operation"));
        assert_eq!(lsp_search_query::operation(&parsed), row["operation"]);
        if row["operation"] == "workspaceSymbol" {
            assert_eq!(lsp_search_query::symbol_query(&parsed), Some("run"));
            assert_eq!(lsp_search_query::symbol_name(&parsed), None);
        }
    }
}

#[test]
fn content_and_files_views_have_distinct_context_admission() {
    let base = json!({"operation":"references","path":"a.ts","symbolName":"run","line":1});
    let mut detailed = base.clone();
    detailed["view"] = json!("content");
    detailed["contextLines"] = json!({"before":2,"after":5});
    let parsed =
        query(crate::contracts::validate_query("lspSearch", detailed.clone()).expect("context"));
    assert_eq!(lsp_search_query::view(&parsed), "content");
    let context = lsp_search_query::context_lines(&parsed).expect("context");
    assert_eq!((context.before, context.after), (2, 5));
    detailed["view"] = json!("files");
    assert!(crate::contracts::validate_query("lspSearch", detailed).is_err());
    let mut grouped = base;
    grouped["view"] = json!("files");
    let parsed = query(crate::contracts::validate_query("lspSearch", grouped).expect("grouped"));
    assert_eq!(lsp_search_query::view(&parsed), "files");
    assert!(lsp_search_query::context_lines(&parsed).is_none());
}

#[tokio::test]
async fn reference_pages_reach_every_location_once_and_preserve_the_saved_request() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("a.ts");
    std::fs::write(&file, "const run = 1; // 普通😀\n".repeat(90)).expect("source");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    let found = (0..90).map(|line| json!({"uri":uri,"content":"const run = 1; // 普通😀","range":{"start":{"line":line,"character":6},"end":{"line":line,"character":9}}})).collect::<Vec<_>>();
    let initial = query(crate::contracts::validate_query("lspSearch", json!({"operation":"references","path":file,"symbolName":"run","line":1,"occurrence":0,"includeDeclaration":false,"contextLines":{"before":0,"after":0}})).expect("first query"));
    for budget in [700, 2500, 16000] {
        let mut current = initial.clone();
        let mut lines = Vec::new();
        let mut pages = 0;
        loop {
            let row = locations(
                &current,
                &mut SourceCache::new(&paths),
                "references",
                "referencesProvider",
                found.clone(),
                budget,
                None,
            )
            .await;
            pages += 1;
            lines.extend(
                row["matches"]
                    .as_array()
                    .expect("locations")
                    .iter()
                    .map(|item| {
                        item["displayRange"]["startLine"]
                            .as_u64()
                            .expect("source line")
                    }),
            );
            crate::tools::test_support::assert_row_contract(
                ToolId::LspSearch,
                &lsp_search_query::to_row(&current),
                &row,
            );
            let Some(call) = row.pointer("/next/nextPage") else {
                break;
            };
            assert_eq!(call["kind"], "continue");
            let saved = &call["input"]["queries"][0];
            assert_eq!(saved["includeDeclaration"], false);
            assert_eq!(saved["contextLines"], json!({"before":0,"after":0}));
            current = query(
                crate::contracts::validate_built_query("lspSearch", saved.clone())
                    .expect("saved row"),
            );
            assert!(pages < 91, "finite pages");
        }
        assert_eq!(lines, (1..=90).collect::<Vec<_>>());
        assert!(pages > 1);
    }
}
