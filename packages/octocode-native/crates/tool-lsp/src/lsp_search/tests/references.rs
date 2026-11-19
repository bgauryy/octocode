//! References: file grouping, budgeted pages, alias scans and compact
//! rows.

use super::*;

#[test]
fn group_by_file_preserves_reference_counts_and_unique_site_lines() {
    let root_dir = octocode_runtime_base::test_support::Fixture::new();
    let root = root_dir.0.clone();
    let root_str = root.to_string_lossy().into_owned();
    let uri = |name: &str| {
        octocode_engine::lsp::uri::path_to_uri(&root.join(name).to_string_lossy()).expect("uri")
    };
    let location = |name: &str, line: u64| {
        serde_json::json!({
            "uri": uri(name),
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 3}}
        })
    };
    // Two references on line 10 count twice and name the line once.
    let summaries = group_by_file(&[
        location("src/a.ts", 9),
        location("src/a.ts", 4),
        location("src/a.ts", 9),
        location("src/b.ts", 0),
    ]);
    let absolute = |name: &str| format!("{root_str}/{name}");
    assert_eq!(
        summaries,
        vec![
            serde_json::json!({"path": absolute("src/a.ts"), "matchCount": 3, "matchedLines": [5, 10]}),
            serde_json::json!({"path": absolute("src/b.ts"), "matchCount": 1, "matchedLines": [1]}),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn grouped_references_replace_locations_with_file_summaries() {
    let (_root_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "foo\nfoo\n").expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "path": uri,
        "symbolName": "foo", "line": 1,
        "view": "files"
    }));
    let snippets = (0..2)
        .map(|line| {
            serde_json::json!({
                "uri": uri,
                "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 3}}
            })
        })
        .collect::<Vec<_>>();
    let result = locations(
        &q,
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        snippets,
        TEST_BUDGET,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await;
    assert!(result.get("matches").is_none(), "{result}");
    assert_eq!(
        result["files"],
        serde_json::json!([{"path": root.join("a.ts").to_string_lossy(), "matchCount": 2, "matchedLines": [1, 2]}])
    );
    assert_eq!(result["matchCount"], 2);
    octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("grouped references satisfy the output contract");
}

#[tokio::test(flavor = "current_thread")]
async fn recovered_alias_references_are_labeled_in_output() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "foo\nbar\n").expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "path": uri,
        "symbolName": "foo", "line": 1
    }));
    let at = |line: u32| {
        serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 3}}
        })
    };
    let mut recovered = at(1);
    recovered["source"] = serde_json::json!(RECOVERED_ALIAS);
    let result = locations(
        &q,
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        vec![at(0), recovered],
        TEST_BUDGET,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await;
    let matches = location_rows(&result);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[1]["source"], "recoveredAlias");
    assert!(matches[0].get("source").is_none());
    assert_eq!(result["recoveredAliasReferences"], 1);
    octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("labeled references satisfy the output contract");
}

#[tokio::test(flavor = "current_thread")]
async fn default_references_preserve_full_location_geometry_and_content() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "foo\nfoo\n").unwrap();
    let uri = octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).unwrap();
    let q =
        query(serde_json::json!({"operation":"references","path":uri,"symbolName":"foo","line":1}));
    let raw = vec![
        serde_json::json!({"uri":uri,"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":3}},"content":"foo\nfoo\n"}),
    ];
    let result = locations(
        &q,
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        raw,
        TEST_BUDGET,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await;
    assert!(
        result.get("matches").is_none(),
        "default has one explicit location form: {result}"
    );
    let found = location_rows(&result);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["line"], 1);
    assert_eq!(found[0]["endLine"], 2);
    assert!(found[0]["content"].as_str().unwrap().contains("foo"));
    octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("full evidence contract");
}

/// The files view is grouped by directory only as a judged row is served
/// (`render::served_forms`, whose proof only `server_text::judge` gives), and
/// only where shorter; rows that list matches per file keep their paths.
#[test]
fn a_judged_files_view_is_served_grouped_by_directory_where_shorter() {
    let judged = super::super::server_text::judged_for_tests();
    let mut row = serde_json::json!({"files": [
        {"path": "/repo/packages/mcp-host/src/octocode/mcp.service.ts", "matchedLines": [16, 180]},
        {"path": "/repo/packages/mcp-host/src/octocode/routes.ts", "matchedLines": [3]},
        {"path": "/repo/packages/mcp-host/src/octocode/server.ts", "matchedLines": [9, 12]}
    ]});
    super::super::render::served_forms(&mut row, &judged);
    assert_eq!(
        row["files"][0]["dir"], "/repo/packages/mcp-host/src/octocode",
        "{row}"
    );
    assert_eq!(
        row["files"][0]["files"][0]["name"], "mcp.service.ts",
        "{row}"
    );
    assert_eq!(
        row["files"][0]["files"][0]["matchedLines"],
        serde_json::json!([16, 180]),
        "{row}"
    );
    let callers = serde_json::json!({"files": [
        {"path": "/repo/a/x.ts", "matches": [{"symbolName": "f", "line": 1}]},
        {"path": "/repo/a/y.ts", "matches": [{"symbolName": "g", "line": 2}]}
    ]});
    let mut kept = callers.clone();
    super::super::render::served_forms(&mut kept, &judged);
    assert_eq!(kept, callers);
}
