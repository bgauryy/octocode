//! References: file grouping, budgeted pages, alias scans and compact
//! rows.

use super::*;

#[test]
fn group_by_file_preserves_reference_counts_and_unique_site_lines() {
    let root_dir = crate::tools::test_support::Fixture::new();
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
            serde_json::json!({"path": absolute("src/a.ts"), "matchCount": 3, "lines": [5, 10]}),
            serde_json::json!({"path": absolute("src/b.ts"), "matchCount": 1, "lines": [1]}),
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
        None,
    )
    .await;
    assert!(result.get("matches").is_none(), "{result}");
    assert_eq!(
        result["files"],
        serde_json::json!([{"path": root.join("a.ts").to_string_lossy(), "matchCount": 2, "lines": [1, 2]}])
    );
    assert_eq!(result["matchCount"], 2);
    crate::contracts::validate_output("lspSearch", &public_row(&result))
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
        None,
    )
    .await;
    let matches = result["matches"].as_array().expect("full locations");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[1]["source"], "recoveredAlias");
    assert!(matches[0].get("source").is_none());
    assert_eq!(result["recoveredAliasReferences"], 1);
    crate::contracts::validate_output("lspSearch", &public_row(&result))
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
        None,
    )
    .await;
    assert!(
        result.get("files").is_none(),
        "default has one explicit location form: {result}"
    );
    let found = result["matches"].as_array().expect("locations");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["displayRange"]["startLine"], 1);
    assert_eq!(found[0]["displayRange"]["endLine"], 2);
    assert!(found[0]["content"].as_str().unwrap().contains("foo"));
    crate::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("full evidence contract");
}
