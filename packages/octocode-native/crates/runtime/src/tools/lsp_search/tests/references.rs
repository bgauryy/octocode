//! References: file grouping, budgeted pages, alias scans and compact
//! rows.

use super::*;

#[test]
fn group_by_file_summarizes_per_file_with_workspace_relative_paths() {
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
    // Through the envelope, base + path names the real file.
    let envelope = crate::response::rows::envelope(
        crate::id::ToolId::LspSearch,
        vec![serde_json::json!({
            "index": 0,
            "data": {"path": absolute("src/deep/anchor.ts"), "files": summaries}
        })],
    );
    let base = envelope["root"].as_str().expect("root");
    for file in envelope["results"][0]["data"]["files"]
        .as_array()
        .expect("files")
    {
        let joined = format!("{base}/{}", file["path"].as_str().expect("path"));
        assert!(
            joined == absolute("src/a.ts") || joined == absolute("src/b.ts"),
            "{joined}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn grouped_references_replace_locations_with_file_summaries() {
    let (_root_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "foo\nfoo\n").expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": uri,
        "symbolName": "foo", "line": 1,
        "groupByFile": true
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

/// Every reference page of one walk: `(rows, chars)` per page.
async fn walk_reference_pages(
    root: &std::path::Path,
    uri: &str,
    count: usize,
    page_size: Option<u32>,
    budget: usize,
) -> Vec<(Vec<(u64, u64)>, usize)> {
    let paths = crate::tools::test_support::workspace_policy(root);
    let mut q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": uri, "symbolName": "foo", "line": 1
    }));
    if let Some(size) = page_size {
        q = requery(&q, serde_json::json!({"pageSize": size}));
    }
    let found = (0..count)
        .map(|line| {
            serde_json::json!({
                "uri": uri,
                "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 3}}
            })
        })
        .collect::<Vec<_>>();
    let mut pages = Vec::new();
    for page in 1.. {
        let paged = requery(&q, serde_json::json!({"page": page}));
        let row = locations(
            &paged,
            &mut SourceCache::new(&paths),
            "references",
            "referencesProvider",
            found.clone(),
            budget,
            None,
        )
        .await;
        assert_eq!(row["matchCount"], count, "a stable total on page {page}");
        let rows = row["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["matches"].as_array().into_iter().flatten())
            .map(|hit| {
                (
                    hit["line"].as_u64().expect("line"),
                    hit["column"].as_u64().expect("column"),
                )
            })
            .collect::<Vec<(u64, u64)>>();
        let more = row["pagination"]["hasMore"] == true;
        pages.push((
            rows,
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0),
        ));
        if !more {
            break;
        }
        assert!(page < 1_000, "the walk must end");
    }
    pages
}

#[tokio::test(flavor = "current_thread")]
async fn reference_pages_fill_the_budget_and_reach_every_location_once() {
    let root_dir = crate::tools::test_support::Fixture::new();
    let root = root_dir.0.canonicalize().expect("canonical root");
    let source = "foo\n".repeat(641);
    std::fs::write(root.join("a.ts"), source).expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");

    let pages = walk_reference_pages(&root, &uri, 641, None, TEST_BUDGET).await;
    let mut seen = std::collections::BTreeSet::new();
    for (index, (rows, chars)) in pages.iter().enumerate() {
        assert!(*chars <= TEST_BUDGET, "page {index} is {chars} chars");
        for row in rows {
            assert!(seen.insert(*row), "{row:?} repeated");
        }
    }
    assert_eq!(seen.len(), 641, "every reference is reached");
    // 40 rows a page was 17 calls; a filled budget is a handful.
    assert!(pages.len() <= 3, "{} calls", pages.len());

    // An explicit count overrides the budget exactly.
    let counted = walk_reference_pages(&root, &uri, 641, Some(100), TEST_BUDGET).await;
    let sizes = counted
        .iter()
        .map(|(rows, _)| rows.len())
        .collect::<Vec<_>>();
    assert_eq!(sizes, [100, 100, 100, 100, 100, 100, 41]);
}

#[test]
fn a_capped_alias_scan_is_disclosed_with_a_text_search() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/a.ts", "symbolName": "foo", "line": 1
    }));
    let mut row = serde_json::json!({
        "type": "references",
        "kind": "references", "coverage": {}
    });
    let scope = super::super::scope::Scope::new("/repo".into(), vec!["*.ts".into()]);
    super::super::recovery::disclose_alias_cap(&mut row, &q, &scope);
    assert_eq!(row["coverage"]["aliasScan"], "capped", "{row}");
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!([super::super::recovery::ALIAS_SCAN_CAPPED_REASON])
    );
    // The alias cap is fixed per request: a terminal limit, with the
    // lexical lead as the only way past it.
    assert_eq!(row["terminalLimit"], true, "{row}");
    // The warning says why no page reaches the rest and where the lead is.
    let warning = row["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.contains("no page or retry re-checks them")
            && warning.contains("the text-search lead"),
        "{row}"
    );
    assert_eq!(row["next"]["findText"]["tool"], "localSearch", "{row}");
    assert_eq!(
        row["next"]["findText"]["query"]["queries"][0]["path"],
        "/repo"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn recovered_alias_references_are_labeled_in_output() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "foo\nbar\n").expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": uri,
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
    let files = result["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{result}");
    assert_eq!(
        files[0]["matches"].as_array().map(Vec::len),
        Some(2),
        "{result}"
    );
    // Only the recovered row (line 2) is labeled.
    assert_eq!(
        files[0]["matches"][1]["source"], "recoveredAlias",
        "{result}"
    );
    assert!(files[0]["matches"][0].get("source").is_none(), "{result}");
    assert_eq!(result["recoveredAliasReferences"], 1);
    crate::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("labeled references satisfy the output contract");
}

#[tokio::test(flavor = "current_thread")]
async fn references_default_to_compact_rows_grouped_by_file() {
    let (_dir, root, paths) = temp_workspace();
    let a = root.join("a.ts");
    let b = root.join("b.ts");
    std::fs::write(&a, "x\n").expect("a.ts");
    std::fs::write(&b, "x\n").expect("b.ts");
    let uri_a = octocode_engine::lsp::uri::path_to_uri(&a.to_string_lossy()).expect("uri");
    let uri_b = octocode_engine::lsp::uri::path_to_uri(&b.to_string_lossy()).expect("uri");
    let at = |uri: &str, line: u32, character: u32, end_line: u32, content: &str| {
        serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": line, "character": character}, "end": {"line": end_line, "character": character + 3}},
            "content": content
        })
    };
    let mut snippets = vec![at(&uri_a, 4, 13, 28, "export const foo = (\n  a,\n) => a;")];
    snippets.extend((10..30).map(|line| at(&uri_a, line, 2, line, "  foo(bar);")));
    let mut recovered = at(&uri_b, 0, 9, 0, "import { foo } from './a';");
    recovered["source"] = serde_json::json!("recoveredImporter");
    snippets.push(recovered);
    let compact = |extra: serde_json::Value| {
        let mut input = serde_json::json!({
            "operation": "references", "mainGoal": "test", "reasoning": "test",
            "path": uri_a, "symbolName": "foo", "line": 5
        });
        for (key, value) in extra.as_object().expect("object") {
            input[key] = value.clone();
        }
        query(input)
    };
    let result = locations(
        &compact(serde_json::json!({})),
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        snippets.clone(),
        TEST_BUDGET,
        None,
    )
    .await;
    let payload = &result;
    assert!(payload.get("matches").is_none(), "{result}");
    let files = payload["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{result}");
    assert_eq!(files[0]["path"], serde_json::json!(a.to_string_lossy()));
    let refs = files[0]["matches"].as_array().expect("matches");
    assert_eq!(refs.len(), 21);
    // Multi-line ranges keep their end line; text is the first trimmed line.
    assert_eq!(
        refs[0],
        serde_json::json!({"line": 5, "column": 14, "endLine": 29, "value": "export const foo = ("})
    );
    assert_eq!(
        refs[1],
        serde_json::json!({"line": 11, "column": 3, "value": "foo(bar);"})
    );
    assert_eq!(
        files[1]["matches"],
        serde_json::json!([{"line": 1, "column": 10, "value": "import { foo } from './a';",
            "source": "recoveredImporter"}])
    );
    assert_eq!(result["pagination"]["totalItems"], 22, "{result}");
    // The reference count is stated even when one page holds every row.
    assert_eq!(payload["matchCount"], 22, "{result}");
    assert_eq!(payload["matchedFiles"], 2);
    crate::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("compact references satisfy the output contract");

    // Explicit row forms keep per-location rows.
    for extra in [
        serde_json::json!({"groupByFile": false}),
        serde_json::json!({"contextLines": 1}),
    ] {
        let result = locations(
            &compact(extra.clone()),
            &mut SourceCache::new(&paths),
            "references",
            "referencesProvider",
            snippets.clone(),
            TEST_BUDGET,
            None,
        )
        .await;
        assert!(result["matches"].is_array(), "{extra}: {result}");
    }
    // A short reference list is compact too.
    let result = locations(
        &compact(serde_json::json!({})),
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        snippets[..2].to_vec(),
        TEST_BUDGET,
        None,
    )
    .await;
    assert!(result.get("matches").is_none(), "{result}");
    assert_eq!(
        result["files"],
        serde_json::json!([{"path": a.to_string_lossy(), "matches": [
            {"line": 5, "column": 14, "endLine": 29, "value": "export const foo = ("},
            {"line": 11, "column": 3, "value": "foo(bar);"}
        ]}]),
        "{result}"
    );
}
