//! Request shape: operation gating, document sync, definition hops and
//! the query identity of an anchor.

use super::*;

#[test]
fn definition_alias_retry_is_limited_to_an_unresolved_first_same_file_hop() {
    assert!(should_retry_definition_hop(
        0,
        "/repo/entry.ts",
        "/repo/entry.ts",
        false,
    ));
    assert!(!should_retry_definition_hop(
        1,
        "/repo/entry.ts",
        "/repo/entry.ts",
        false,
    ));
    assert!(!should_retry_definition_hop(
        0,
        "/repo/entry.ts",
        "/repo/math.ts",
        false,
    ));
    assert!(!should_retry_definition_hop(
        0,
        "/repo/entry.ts",
        "/repo/entry.ts",
        true,
    ));
}

#[test]
fn didopen_read_is_capped_to_avoid_oversized_document_sync() {
    // A source at/under the cap is opened; one above it is skipped rather
    // than read uncapped and streamed to the server under a flat deadline.
    let dir = tempfile::tempdir().expect("temp dir");
    let at_cap = dir.path().join("at_cap.ts");
    let over_cap = dir.path().join("over_cap.ts");
    let cap = usize::try_from(MAX_LSP_DIDOPEN_BYTES).expect("cap fits usize");
    std::fs::write(&at_cap, vec![b'a'; cap]).expect("write at cap");
    std::fs::write(&over_cap, vec![b'a'; cap + 1]).expect("write over cap");
    assert_eq!(
        read_bounded_source(&at_cap).map(|text| text.len()).ok(),
        Some(cap)
    );
    assert!(matches!(
        read_bounded_source(&over_cap),
        Err(SourceReadError::TooLarge(len)) if len == MAX_LSP_DIDOPEN_BYTES + 1
    ));
}

#[test]
fn document_wide_operations_do_not_require_a_position_anchor() {
    for operation in ["documentSymbols", "workspaceSymbol", "diagnostic"] {
        let mut row = serde_json::json!({
            "operation": operation, "path": "/repo/src/lib.rs",
            "page": 1
        });
        // The contract requires a query name for workspaceSymbol; it is still unanchored.
        if operation == "workspaceSymbol" {
            row["symbolQuery"] = serde_json::json!("Lib");
        }
        let query: LspSearchQuery = serde_json::from_value(row).expect("document-wide lsp query");
        assert_eq!(
            resolve_anchor(&query, "/unused", "file:///repo/src/lib.rs", None),
            Ok(Anchor {
                line: 0,
                character: 0,
                resolved_symbol: None
            })
        );
    }
}

#[test]
fn semantic_operations_require_their_advertised_lsp_capability() {
    for operation in [
        "definition",
        "references",
        "hover",
        "typeDefinition",
        "implementation",
        "documentSymbols",
        "workspaceSymbol",
        "callers",
        "callees",
        "supertypes",
        "subtypes",
    ] {
        assert_eq!(
            required_capability(operation),
            provider_for_operation(operation),
            "{operation}"
        );
    }
    assert_eq!(required_capability("diagnostic"), None);
    assert_eq!(
        provider_for_operation("diagnostic"),
        Some("diagnosticProvider")
    );
    assert_eq!(
        required_capability("references"),
        Some("referencesProvider")
    );
    assert_eq!(
        required_capability("callers"),
        Some("callHierarchyProvider")
    );
    assert_eq!(
        required_capability("subtypes"),
        Some("typeHierarchyProvider")
    );
    assert_eq!(required_capability("unknown"), None);
}

/// hover reads one position: walk controls are rejected before any server
/// starts, with the operations they apply to.
#[test]
fn hover_rejects_walk_controls_with_the_operations_that_take_them() {
    for (field, value, applies) in [
        ("graphDepth", serde_json::json!(2), "callers"),
        ("view", serde_json::json!("files"), "references"),
    ] {
        let mut row = serde_json::json!({"operation":"hover","path":"/tmp/a.rs","symbolName":"main","line":1});
        row[field] = value;
        let error =
            crate::contracts::validate("lspSearch", serde_json::json!({"queries":[row.clone()]}))
                .expect_err("hover with a walk control");
        assert!(
            error.issues.iter().any(|issue| {
                issue.rule_id == "schema.unknown-field" && issue.path == ["queries", "0", field]
            }),
            "unsupported operation field {field}: {error:?}"
        );
        let rendered = crate::contracts::format_input_error("lspSearch", &error).to_string();
        assert!(rendered.contains(applies), "{field}: {rendered}");
        row["operation"] = serde_json::json!(applies);
        crate::contracts::validate("lspSearch", serde_json::json!({"queries":[row]}))
            .expect("the stated operation accepts its control");
    }
}

#[test]
fn retired_grouping_field_is_not_accepted_as_a_view_alias() {
    let row = serde_json::json!({"operation":"references","path":"/tmp/a.rs",
        "symbolName":"main","line":1,"groupByFile":true});
    let error = crate::contracts::validate("lspSearch", serde_json::json!({"queries":[row]}))
        .expect_err("the retired grouping field is not a supported alias");
    assert!(
        error.issues.iter().any(|issue| {
            issue.rule_id == "schema.unknown-field" && issue.path == ["queries", "0", "groupByFile"]
        }),
        "{error:?}"
    );
}

/// A definition reached through a symlink and through its target is
/// one definition (canonical-path identity, like the hierarchy walk).
#[cfg(unix)]
#[test]
fn definition_identity_treats_a_symlink_and_its_target_as_one() {
    let (_dir, root, _) = temp_workspace();
    let target = root.join("impl.ts");
    std::fs::write(&target, "export function f() {}\n").expect("target");
    let link = root.join("link.ts");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let snippet = |path: &std::path::Path| octocode_engine::lsp::types::LspCodeSnippet {
        uri: octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri"),
        range: octocode_engine::lsp::types::LspRange {
            start: octocode_engine::lsp::types::LspExactPosition {
                line: 0,
                character: 16,
            },
            end: octocode_engine::lsp::types::LspExactPosition {
                line: 0,
                character: 17,
            },
        },
        content: String::new(),
        symbol_kind: None,
        display_range: None,
    };
    assert_eq!(
        snippet_identity(&snippet(&link)),
        snippet_identity(&snippet(&target))
    );
    // A missing path still has an identity (its decoded form).
    let gone = root.join("gone.ts");
    assert!(snippet_identity(&snippet(&gone)).contains("gone.ts"));
}

#[test]
fn anchor_file_uri_is_stated_once_per_row() {
    let canonical = "file:///repo/src/util.ts";
    let mut symbol = serde_json::json!({"name":"greet","uri":canonical,"foundAtLine":1});
    super::super::receipt::drop_same_uri(&mut symbol, canonical);
    assert!(symbol.get("uri").is_none(), "{symbol}");
    // A payload whose locations all sit in ANOTHER file keeps its uri.
    let mut other = serde_json::json!({"kind":"references","uri":"file:///repo/src/index.ts"});
    super::super::receipt::drop_same_uri(&mut other, canonical);
    assert_eq!(other["uri"], "file:///repo/src/index.ts");
    // Percent-encoding differences still name the same file.
    let mut encoded = serde_json::json!({"uri":"file:///repo/src/a%20b.ts"});
    super::super::receipt::drop_same_uri(&mut encoded, "file:///repo/src/a b.ts");
    assert!(encoded.get("uri").is_none(), "{encoded}");
}

#[test]
fn page_two_query_hashes_like_page_one() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
    "path":"/tmp/a.rs","symbolName":"is_alive","line":1,"operation":"references",
    }))
    .expect("page 1");
    // A continuation lists the same fields in another order.
    let second: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "page":2,"snapshot":"lsp-v1:abc","operation":"references",
        "line":1,"symbolName":"is_alive","path":"/tmp/a.rs"
    }))
    .expect("page 2");
    let items = [serde_json::json!({"uri":"file:///tmp/a.rs"})];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&second, "references", &items),
        "first={} second={}",
        lsp_search_query::to_row(&first),
        lsp_search_query::to_row(&second)
    );
}

/// The anchor file and workspace root are named by their canonical paths
/// before any snapshot is computed, so a continuation that spells them
/// relative to the workspace (as the response envelope does) replays the
/// same snapshot as the absolute first page.
#[test]
fn relative_and_absolute_anchors_share_one_query_identity() {
    let root = tempfile::tempdir().expect("fixture");
    let root = std::fs::canonicalize(root.path()).expect("canonical");
    std::fs::create_dir_all(root.join("src")).expect("dir");
    std::fs::write(root.join("src/a.ts"), "export const a = 1;\n").expect("file");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let base = |uri: String, workspace: &str| serde_json::json!({"operation":"documentSymbols","path":uri,"workspaceRoot":workspace,"page":2});
    let mut absolute = query(base(
        root.join("src/a.ts").to_string_lossy().into_owned(),
        &root.to_string_lossy(),
    ));
    let mut relative = query(base("src/a.ts".into(), "."));
    let mut uri = query(base(
        format!("file://{}", root.join("src/a.ts").display()),
        ".",
    ));
    let canonical = root.join("src/a.ts").to_string_lossy().into_owned();
    for query in [&mut absolute, &mut relative, &mut uri] {
        assert_eq!(resolve_paths(query, &paths).expect("valid"), canonical);
    }
    assert_eq!(
        lsp_search_query::to_row(&absolute),
        lsp_search_query::to_row(&relative)
    );
    assert_eq!(
        lsp_search_query::to_row(&absolute),
        lsp_search_query::to_row(&uri)
    );
    let items = [serde_json::json!({"name":"a"})];
    let snapshot = |query: &LspSearchQuery| semantic_snapshot(query, "documentSymbols", &items);
    assert_eq!(snapshot(&absolute), snapshot(&relative));
}

/// The anchor receipt shows only what the request did not say: the line a
/// symbol moved to. A column alone is no receipt.
#[test]
fn resolved_symbol_receipt_shows_only_a_moved_anchor() {
    let uri = "file:///repo/a.ts";
    let named = query(serde_json::json!({
        "operation": "references", "path": uri, "symbolName": "foo", "line": 3
    }));
    let column = serde_json::json!({"uri": uri, "foundAtCharacter": 5, "occurrence": 1});
    let moved = serde_json::json!({"uri": uri, "foundAtCharacter": 5, "foundAtLine": 4, "lineDeviation": 1});
    assert_eq!(public_resolved_symbol(&named, Some(column), uri), None);
    assert_eq!(
        public_resolved_symbol(&named, Some(moved), uri),
        Some(serde_json::json!({"foundAtCharacter": 5, "foundAtLine": 4, "lineDeviation": 1}))
    );
}
