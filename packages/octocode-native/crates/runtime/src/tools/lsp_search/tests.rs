//! Unit tests for `lspSearch` (runtime side): anchors, coordinates,
//! pagination, source access, error mapping, and hierarchy walks over a
//! fake graph.

use super::anchor::*;
use super::failure::*;
use super::locations::*;
use super::ops::*;
use super::receipt::*;
use super::recovery::*;
use super::render::*;
use super::source::*;
use super::walk::*;
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
fn canonical_nested_position_survives_deserialization_and_resolves_exactly() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "definition", "goal": "test", "reasoning": "test",
        "uri": "/repo/src/lib.rs",
        "position": { "line": 7, "character": 11 },
        "page": 1,
        "pageSize": 40,
        "includeDeclaration": true
    }))
    .expect("canonical lsp query");
    let anchor = resolve_anchor(&query, "/unused", "file:///repo/src/lib.rs", None)
        .expect("explicit anchor");
    assert_eq!((anchor.line, anchor.character), (7, 11));
    assert_eq!(
        serde_json::to_value(query).expect("serialize")["position"],
        serde_json::json!({ "line": 7, "character": 11 })
    );
}

#[test]
fn explicit_position_is_zero_based_lsp_while_presentation_is_one_based() {
    // Pinned contract (see the `position` field doc): an explicit `position`
    // input is consumed as ZERO-based LSP coordinates (fed straight through
    // `resolve_anchor`), while every emitted coordinate — including
    // `resolvedSymbol` — is ONE-based (lines and UTF-16 columns). The receipt
    // does not echo the zero-based input back.
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "definition", "goal": "test", "reasoning": "test",
        "uri": "file:///repo/src/lib.rs",
        "position": { "line": 7, "character": 11 }
    }))
    .expect("position query");

    // Input consumed as-is (zero-based) for the LSP request, and the
    // presentation reports the same anchor one-based (line + 1).
    let anchor = resolve_anchor(&query, "/unused", "file:///repo/src/lib.rs", None)
        .expect("explicit anchor");
    assert_eq!((anchor.line, anchor.character), (7, 11));
    let resolved = anchor
        .resolved_symbol
        .expect("explicit position presents a resolved anchor");
    assert_eq!(resolved["foundAtLine"], 8);
    assert_eq!(resolved["foundAtCharacter"], 12);
    assert!(resolved.get("position").is_none(), "{resolved}");
}

#[test]
fn pagination_omits_nullable_next_page_at_the_terminal_page() {
    let items = vec![serde_json::json!({"name": "one"})];
    let (_, terminal) = paginate(&items, 1, 20);
    assert_eq!(terminal["hasMore"], false);
    assert!(terminal.get("nextPage").is_none());

    let items = vec![
        serde_json::json!({"name": "one"}),
        serde_json::json!({"name": "two"}),
    ];
    let (_, partial) = paginate(&items, 1, 1);
    assert_eq!(partial["nextPage"], 2);
}

#[test]
fn semantic_pagination_continuations_cover_the_full_result_fixture() {
    let expected = (0..7)
        .map(|index| serde_json::json!({"name": format!("symbol-{index}")}))
        .collect::<Vec<_>>();
    let mut query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "goal": "test", "reasoning": "test",
        "uri": "/repo/src/lib.rs",
        "page": 1,
        "pageSize": 3
    }))
    .expect("canonical lsp query");
    let snapshot = semantic_snapshot(&query, "symbols", &expected);
    query = requery(&query, serde_json::json!({"snapshot": snapshot}));
    let mut actual = Vec::new();

    loop {
        let (page, pagination) = paginate(&expected, query.page().unwrap_or(1), query.page_size());
        actual.extend(page);
        let response = with_next(
            &query,
            serde_json::json!({
                "snapshot": snapshot,
                "pagination": pagination
            }),
        );
        if response["pagination"]["hasMore"] != true {
            assert!(response.get("next").is_none());
            break;
        }
        let continuation = &response["next"]["nextPage"];
        assert_eq!(continuation["tool"], "lspSearch");
        query = serde_json::from_value(continuation["query"].clone())
            .expect("executable continuation query");
        assert_eq!(query.snapshot(), Some(snapshot.as_str()));
    }

    assert_eq!(actual, expected);
}

#[test]
fn didopen_read_is_capped_to_avoid_oversized_document_sync() {
    // A source at/under the cap is opened; one above it is skipped rather
    // than read uncapped and streamed to the server under a flat deadline.
    assert!(!didopen_exceeds_cap(0));
    assert!(!didopen_exceeds_cap(MAX_LSP_DIDOPEN_BYTES));
    assert!(didopen_exceeds_cap(MAX_LSP_DIDOPEN_BYTES + 1));
}

#[test]
fn document_wide_operations_do_not_require_a_position_anchor() {
    for operation in ["documentSymbols", "workspaceSymbol", "diagnostic"] {
        let mut row = serde_json::json!({
            "operation": operation, "goal": "test", "reasoning": "test",
            "uri": "/repo/src/lib.rs",
            "page": 1,
            "pageSize": 40
        });
        // The contract requires a query name for workspaceSymbol; it is still unanchored.
        if operation == "workspaceSymbol" {
            row["symbolName"] = serde_json::json!("Lib");
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
fn later_pages_require_the_semantic_snapshot_and_carry_it_forward() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "goal": "test", "reasoning": "test",
        "uri": "/repo/src/lib.rs",
        "page": 2,
        "pageSize": 1
    }))
    .expect("canonical lsp query");
    let items = vec![
        serde_json::json!({"name":"a"}),
        serde_json::json!({"name":"b"}),
    ];
    let snapshot = semantic_snapshot(&query, "symbols", &items);
    assert!(snapshot_mismatch(&query, &snapshot));
    let changed = snapshot_changed(&query, snapshot.clone());
    assert_eq!(changed["errorCode"], "lsp.snapshot.changed");
    assert_eq!(changed["next"]["restart"]["query"]["page"], 1);
    assert!(
        changed["next"]["restart"]["query"]
            .get("snapshot")
            .is_none()
    );

    let continued_query = requery(&query, serde_json::json!({"snapshot": snapshot}));
    let continued = with_next(
        &continued_query,
        serde_json::json!({
            "snapshot": snapshot,
            "pagination": {"hasMore": true, "nextPage": 3}
        }),
    );
    assert_eq!(continued["next"]["nextPage"]["query"]["page"], 3);
    assert!(continued["next"]["nextPage"]["query"]["snapshot"].is_string());
}

#[test]
fn semantic_snapshot_ignores_paging_and_workflow_metadata() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "goal": "test", "reasoning": "Find every reference.",
        "debug": false,
        "uri": "/repo/src/lib.rs",
        "symbolName": "run",
        "lineHint": 4,
        "pageSize": 1
    }))
    .expect("first page query");
    let continued: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "goal": "test", "reasoning": "Continuation metadata may be normalized.",
        "uri": "/repo/src/lib.rs",
        "symbolName": "run",
        "lineHint": 4,
        "page": 2,
        "pageSize": 1,
        "snapshot": "prior"
    }))
    .expect("continuation query");
    let items = vec![serde_json::json!({"uri":"file:///repo/src/lib.rs"})];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&continued, "references", &items)
    );

    let resized = requery(&continued, serde_json::json!({"pageSize": 100}));
    assert_ne!(
        semantic_snapshot(&continued, "references", &items),
        semantic_snapshot(&resized, "references", &items),
        "changing the page size can skip or repeat locations"
    );
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
        "callHierarchy",
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

#[test]
fn document_symbols_use_the_canonical_flat_one_based_public_shape() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "goal": "test", "reasoning": "test",
        "uri": "file:///repo/src/lib.rs"
    }))
    .expect("document symbols query");
    let raw = serde_json::json!([{
        "name": "Greeter",
        "kind": 5,
        "range": {
            "start": {"line": 2, "character": 0},
            "end": {"line": 8, "character": 1}
        },
        "selectionRange": {
            "start": {"line": 2, "character": 7},
            "end": {"line": 2, "character": 14}
        },
        "children": [{
            "name": "greet",
            "kind": 6,
            "range": {
                "start": {"line": 3, "character": 2},
                "end": {"line": 5, "character": 3}
            },
            "selectionRange": {
                "start": {"line": 3, "character": 5},
                "end": {"line": 3, "character": 10}
            }
        }]
    }, {
        // Doc comment / attribute lines are part of `range` but the name
        // (and a usable lineHint) is on `selectionRange`.
        "name": "documented",
        "kind": 12,
        "range": {
            "start": {"line": 10, "character": 0},
            "end": {"line": 14, "character": 1}
        },
        "selectionRange": {
            "start": {"line": 12, "character": 7},
            "end": {"line": 12, "character": 17}
        }
    }]);

    let envelope = items_payload(&query, "documentSymbols", raw);
    assert_eq!(envelope["lsp"]["source"], "lsp");
    assert_eq!(envelope["payload"]["kind"], "documentSymbols");
    assert_eq!(
        envelope["payload"]["symbols"]
            .as_array()
            .expect("symbols should be an array")
            .len(),
        3
    );
    assert_eq!(envelope["payload"]["symbols"][0]["kind"], "class");
    assert_eq!(envelope["payload"]["symbols"][0]["line"], 3);
    assert_eq!(envelope["payload"]["symbols"][0]["character"], 8);
    assert_eq!(envelope["payload"]["symbols"][1]["kind"], "method");
    assert_eq!(envelope["payload"]["symbols"][1]["character"], 6);
    let documented = &envelope["payload"]["symbols"][2];
    assert_eq!(documented["name"], "documented");
    assert_eq!(documented["line"], 13, "line must come from selectionRange");
    assert_eq!(documented["character"], 8, "one-based UTF-16 column");
    assert_eq!(documented["endLine"], 15, "endLine keeps the full range");
    assert_eq!(
        envelope["summary"],
        serde_json::json!({
            "totalSymbols": 3,
            "returnedSymbols": 3,
            "topLevelSymbols": 2,
            "kinds": {"class": 1, "method": 1, "function": 1}
        })
    );
}

#[test]
fn locations_are_compact_camel_case_and_one_based() {
    let location = compact_location(serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {
            "start": {"line": 5, "character": 3},
            "end": {"line": 5, "character": 8}
        },
        "content": "pub fn greet() {}\n",
        "symbol_kind": "function"
    }));
    assert_eq!(location["uri"], "file:///repo/src/lib.rs");
    assert_eq!(location["range"]["start"]["line"], 5);
    assert_eq!(
        location["displayRange"],
        serde_json::json!({
            "startLine": 6,
            "endLine": 6
        })
    );
    assert!(location.get("symbolKind").is_none());
    assert!(location.get("path").is_none());
    assert!(location.get("line").is_none());

    let public = public_location(location);
    assert!(public.get("range").is_none(), "one coordinate form only");
    assert_eq!(
        public["displayRange"],
        serde_json::json!({"startLine": 6, "startCharacter": 4, "endLine": 6})
    );
    assert!(public.get("contentStartLine").is_none());
}

#[test]
fn widened_content_reports_its_first_line_and_single_file_pages_hoist_the_uri() {
    let widened = public_location(serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {
            "start": {"line": 5, "character": 3},
            "end": {"line": 5, "character": 8}
        },
        "content": "a\nb\nc\n",
        "displayRange": {"startLine": 5, "endLine": 7}
    }));
    assert_eq!(widened["displayRange"]["startLine"], 6);
    assert_eq!(widened["contentStartLine"], 5);

    let same = [widened.clone(), widened.clone()];
    assert_eq!(
        shared_location_uri(&same).as_deref(),
        Some("file:///repo/src/lib.rs")
    );
    let mut other = widened.clone();
    other["uri"] = serde_json::json!("file:///repo/src/main.rs");
    assert!(shared_location_uri(&[widened.clone(), other]).is_none());
    assert!(
        shared_location_uri(&[widened]).is_none(),
        "a single location keeps its own uri"
    );
}

#[test]
fn provider_receipt_exposes_effective_invocation_and_omits_empty_optional_fields() {
    let config = octocode_engine::lsp::types::JsLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
    };
    let client = octocode_engine::lsp::client::NativeLspClient::new(config.clone());
    let receipt = resolved_server_receipt(&config, &client);

    assert_eq!(receipt["command"], "rust-analyzer");
    assert_eq!(receipt["argv"], serde_json::json!(["--stdio"]));
    assert_eq!(receipt["source"], "path");
    assert_eq!(receipt["workspaceRoot"], "/repo");
    assert_eq!(
        receipt["capabilities"]
            .as_object()
            .expect("capabilities should be an object")
            .len(),
        10
    );
    assert_eq!(receipt["capabilities"]["definitionProvider"], false);
    assert!(receipt.get("identity").is_none());
    assert!(
        receipt["workspaceFingerprint"]
            .as_str()
            .is_some_and(|value| value.len() == 64)
    );
    assert!(
        receipt["configurationFingerprint"]
            .as_str()
            .is_some_and(|value| value.len() == 64)
    );
    assert!(receipt.get("readiness").is_none());

    let config_without_args = octocode_engine::lsp::types::JsLanguageServerConfig {
        args: Some(Vec::new()),
        ..config
    };
    let client = octocode_engine::lsp::client::NativeLspClient::new(config_without_args.clone());
    let receipt = resolved_server_receipt(&config_without_args, &client);
    assert!(receipt.get("argv").is_none());
}

#[test]
fn empty_and_unavailable_rows_expose_status_and_recovery_next() {
    let query = query(serde_json::json!({
        "operation": "definition", "goal": "test", "reasoning": "test",
        "uri": "/repo/src/lib.rs",
        "symbolName": "execute",
        "lineHint": 10
    }));
    let empty = with_next(&query, empty(&query, "noLocations", "none", true));
    assert_eq!(empty["status"], "empty");
    assert_eq!(empty["next"]["readFile"]["confidence"], "exact");
    assert!(
        empty["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("Verify the symbol and anchor")),
        "{empty}"
    );
    let down = failure(
        &query,
        "file:///repo/src/lib.rs",
        "lsp.serverUnavailable",
        "missing",
        false,
    );
    assert_eq!(down["status"], "error");
    assert_eq!(down["errorCode"], "lsp.serverUnavailable");
    assert_eq!(down["next"]["readFile"]["tool"], "localFetch");
    assert!(
        down["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("astSearch symbols/match")),
        "{down}"
    );

    let timeout = failure(
        &query,
        "file:///repo/src/lib.rs",
        "lsp.timeout",
        "timed out",
        false,
    );
    assert_eq!(timeout["next"]["retry"]["tool"], "lspSearch");
    assert_eq!(timeout["next"]["readFile"]["tool"], "localFetch");

    let explicitly_scoped = requery(&query, serde_json::json!({"workspaceRoot": "/repo"}));
    let scoped = failure(
        &explicitly_scoped,
        "file:///repo/src/lib.rs",
        "lsp.capabilityUnavailable",
        "unsupported",
        true,
    );
    assert_eq!(scoped["next"]["readFile"]["tool"], "localFetch");
}

#[test]
fn workspace_root_failures_emit_a_string_uri_without_directory_read_recovery() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "workspaceSymbol", "goal": "test", "reasoning": "test",
        "workspaceRoot": "/repo",
        "symbolName": "execute"
    }))
    .expect("workspace-symbol query");

    let down = failure(
        &query,
        "file:///repo",
        "lsp.serverUnavailable",
        "missing",
        false,
    );

    assert!(down["uri"].is_string(), "{down}");
    assert_ne!(down["uri"], serde_json::Value::Null, "{down}");
    assert!(down.get("next").is_none(), "{down}");
    assert!(
        down["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("Provide uri")),
        "{down}"
    );
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":down}]}),
    )
    .expect("workspace-root failure must satisfy the internal output contract");
}

#[test]
fn workspace_root_empty_results_do_not_emit_a_pathless_read_recovery() {
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "goal": "test", "reasoning": "test",
        "workspaceRoot": "/repo",
        "symbolName": "nothing"
    }));
    let mut row = with_next(&q, items_payload(&q, "symbols", serde_json::json!([])));
    assert_eq!(row["status"], "empty");
    assert!(row.get("next").is_none(), "{row}");
    // execute() stamps the canonical root URI on every row.
    row["uri"] = serde_json::json!("file:///repo");
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":row}]}),
    )
    .expect("empty workspace-root row satisfies the output contract");
}

#[test]
fn server_controlled_item_uris_are_authorized_before_emission() {
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    // A policy with no configured roots authorizes no real path, so any
    // server-supplied file URI must be rejected.
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");

    let workspace_symbol =
        serde_json::json!({ "name": "Foo", "location": { "uri": "file:///etc/passwd" } });
    assert!(!item_uri_is_authorized(&workspace_symbol, &paths));

    let call = serde_json::json!({ "from": { "uri": "file:///etc/hosts" } });
    assert!(!item_uri_is_authorized(&call, &paths));

    let type_item = serde_json::json!({ "name": "Base", "uri": "file:///etc/group" });
    assert!(!item_uri_is_authorized(&type_item, &paths));

    // An item that embeds no file URI carries no path to leak and is kept.
    let no_uri = serde_json::json!({ "name": "Local" });
    assert!(item_uri_is_authorized(&no_uri, &paths));

    // filter_authorized_items removes only the unauthorized entries and leaves
    // non-array values untouched for downstream `as_array` handling.
    let filtered = filter_authorized_items(serde_json::json!([workspace_symbol, no_uri]), &paths);
    assert_eq!(filtered.as_array().map(Vec::len), Some(1));
    assert_eq!(filtered[0]["name"], "Local");
    assert_eq!(
        filter_authorized_items(serde_json::Value::Null, &paths),
        serde_json::Value::Null
    );
}

fn query(value: serde_json::Value) -> LspSearchQuery {
    serde_json::from_value(value).expect("lsp query")
}

/// `query` with `fields` layered over its row.
fn requery(query: &LspSearchQuery, fields: serde_json::Value) -> LspSearchQuery {
    let mut row = query.to_row();
    if let (Some(row), Some(fields)) = (row.as_object_mut(), fields.as_object()) {
        row.extend(fields.clone());
    }
    serde_json::from_value(row).expect("lsp query")
}

#[test]
fn diagnostic_reports_are_listed_as_individual_diagnostics() {
    let q = query(
        serde_json::json!({"operation": "diagnostic", "goal": "test", "reasoning": "test", "uri": "file:///repo/a.ts"}),
    );
    let report = serde_json::json!({
        "kind": "full",
        "items": [
            {"message": "first", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}},
            {"message": "second", "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 1}}}
        ],
        "version": 3,
        "source": "push",
        "truncated": false
    });
    let (items, truncated) = diagnostic_items(Some(report));
    assert!(!truncated);
    let envelope = items_payload(&q, "diagnostics", items);
    let listed = envelope["payload"]["items"].as_array().expect("items");
    assert_eq!(listed.len(), 2, "{envelope}");
    assert_eq!(listed[0]["message"], "first");
    assert_eq!(envelope["pagination"]["totalResults"], 2);

    // A clean file (or a pull `unchanged` report) is an empty result, not
    // a one-element list wrapping the report object.
    for report in [
        Some(serde_json::json!({"kind": "full", "items": []})),
        Some(serde_json::json!({"kind": "unchanged", "resultId": "1"})),
    ] {
        let (items, _) = diagnostic_items(report);
        let envelope = with_next(&q, items_payload(&q, "diagnostics", items));
        assert_eq!(envelope["status"], "empty", "{envelope}");
        assert_eq!(envelope["payload"]["category"], "noDiagnostics");
    }
    let (_, truncated) = diagnostic_items(Some(serde_json::json!({
        "kind": "full", "items": [{"message": "x"}], "truncated": true
    })));
    assert!(truncated);
}

#[test]
fn diagnostics_use_the_one_based_public_coordinates() {
    let report = serde_json::json!({"kind": "full", "items": [{
        "message": "m",
        "code": "macro-error",
        "range": {"start": {"line": 973, "character": 4}, "end": {"line": 1015, "character": 5}},
        "relatedInformation": [{"message": "r", "location": {"uri": "file:///repo/a.rs",
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 2}}}}]
    }]});
    let (items, _) = diagnostic_items(Some(report));
    let item = &items[0];
    assert!(item.get("range").is_none(), "{item}");
    assert_eq!(
        item["displayRange"],
        serde_json::json!({"startLine": 974, "startCharacter": 5, "endLine": 1016})
    );
    let related = &item["relatedInformation"][0]["location"];
    assert!(related.get("range").is_none(), "{related}");
    assert_eq!(related["displayRange"]["startLine"], 1);
    assert!(disabled_macro_diagnostics(&items));
    assert!(!disabled_macro_diagnostics(
        &serde_json::json!([{"code": "E0308"}])
    ));
}

#[test]
fn pages_past_the_end_are_empty_and_flagged_out_of_range() {
    let items = vec![
        serde_json::json!({"name": "one"}),
        serde_json::json!({"name": "two"}),
    ];
    let (page, pagination) = paginate(&items, 5, 1);
    assert!(page.is_empty(), "must not clamp to the last page");
    assert_eq!(pagination["currentPage"], 5);
    assert_eq!(pagination["totalPages"], 2);
    assert_eq!(pagination["hasMore"], false);
    assert_eq!(pagination["outOfRange"], true);
    assert!(pagination.get("nextPage").is_none());

    let (page, pagination) = paginate(&items, 2, 1);
    assert_eq!(page.len(), 1);
    assert!(pagination.get("outOfRange").is_none());
}

#[test]
fn group_by_file_summarizes_per_file_with_workspace_relative_paths() {
    let root = std::env::temp_dir().join(format!("octocode-lsp-group-{}", std::process::id()));
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
    let summaries = group_by_file(&[
        location("src/a.ts", 4),
        location("src/a.ts", 9),
        location("src/b.ts", 0),
    ]);
    let absolute = |name: &str| format!("{root_str}/{name}");
    assert_eq!(
        summaries,
        vec![
            serde_json::json!({"path": absolute("src/a.ts"), "references": 2, "lines": [5, 10]}),
            serde_json::json!({"path": absolute("src/b.ts"), "references": 1, "lines": [1]}),
        ]
    );
    // Through the envelope, base + path names the real file.
    let envelope = crate::runtime::response::envelope(vec![serde_json::json!({
        "index": 0,
        "data": {"path": absolute("src/deep/anchor.ts"), "payload": {"byFile": summaries}}
    })]);
    let base = envelope["base"].as_str().expect("base");
    for file in envelope["results"][0]["data"]["payload"]["byFile"]
        .as_array()
        .expect("byFile")
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
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    let root = std::env::temp_dir().join(format!("octocode-lsp-grouped-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    std::fs::write(root.join("a.ts"), "foo\nfoo\n").expect("a.ts");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("path policy");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "goal": "test", "reasoning": "test",
        "uri": uri,
        "position": {"line": 0, "character": 0},
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
    )
    .await;
    assert!(result["payload"].get("locations").is_none(), "{result}");
    assert_eq!(
        result["payload"]["byFile"],
        serde_json::json!([{"path": root.join("a.ts").to_string_lossy(), "references": 2, "lines": [1, 2]}])
    );
    assert_eq!(result["payload"]["totalReferences"], 2);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn recovered_alias_references_are_labeled_in_output() {
    let (root, paths) = temp_workspace("alias-label");
    std::fs::write(root.join("a.ts"), "foo\nbar\n").expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "goal": "test", "reasoning": "test",
        "uri": uri,
        "position": {"line": 0, "character": 0}
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
    )
    .await;
    let rows = result["payload"]["locations"]
        .as_array()
        .expect("locations");
    assert!(rows[0].get("source").is_none(), "{result}");
    assert_eq!(rows[1]["source"], "recoveredAlias", "{result}");
    assert_eq!(result["payload"]["recoveredAliasReferences"], 1);
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":result}]}),
    )
    .expect("labeled references satisfy the output contract");
    let _ = std::fs::remove_dir_all(root);
}

fn temp_workspace(tag: &str) -> (std::path::PathBuf, crate::policy::path::PathPolicy) {
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    let root = std::env::temp_dir().join(format!("octocode-lsp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.clone()),
        ..Default::default()
    })
    .expect("path policy");
    (root, paths)
}

#[tokio::test(flavor = "current_thread")]
async fn source_cache_reads_each_authorized_bounded_file_once() {
    let (root, paths) = temp_workspace("hop");
    let inside = root.join("inside.ts");
    std::fs::write(&inside, "export const x = 1;\n").expect("inside");
    let large = root.join("large.ts");
    std::fs::write(&large, vec![b'a'; (MAX_LSP_DIDOPEN_BYTES + 1) as usize]).expect("large");
    let inside = inside.to_string_lossy().into_owned();

    let mut sources = SourceCache::new(&paths);
    assert!(sources.get(&inside).await.is_some());
    assert!(sources.get(&inside).await.is_some());
    assert_eq!(sources.reads(), 1, "a file is read once per request");
    assert!(
        sources.get(&large.to_string_lossy()).await.is_none(),
        "oversized hop targets must not be read or synced"
    );
    let reads = sources.reads();
    assert!(
        sources.get("/etc/hosts").await.is_none(),
        "server-supplied targets outside the read policy must not be read"
    );
    assert_eq!(sources.reads(), reads, "a refused path is never read");
    assert!(!sources.uri_authorized("file:///etc/hosts"));
    assert!(sources.uri_authorized(&inside));
    assert!(matches!(
        read_bounded_source(&large),
        Err(SourceReadError::TooLarge(_))
    ));
    assert!(matches!(
        read_bounded_source(&root),
        Err(SourceReadError::Unreadable(_))
    ));
    let _ = std::fs::remove_dir_all(root);
}

/// An in-policy file whose content cannot be loaded (oversize, not
/// UTF-8) is still authorized, so server locations in it are kept.
#[tokio::test(flavor = "current_thread")]
async fn unreadable_in_policy_files_stay_authorized_and_keep_their_locations() {
    let (root, paths) = temp_workspace("unreadable-authorized");
    let large = root.join("large.ts");
    std::fs::write(&large, vec![b'a'; (MAX_LSP_DIDOPEN_BYTES + 1) as usize]).expect("large");
    let binary = root.join("binary.ts");
    std::fs::write(&binary, [0xff_u8, 0xfe, 0x00, 0x80]).expect("binary");
    let mut sources = SourceCache::new(&paths);
    for file in [&large, &binary] {
        let path = file.to_string_lossy().into_owned();
        assert!(sources.get(&path).await.is_none(), "{path} has no content");
        let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
        assert!(
            sources.uri_authorized(&uri),
            "unreadable content must not memoize {path} as unauthorized"
        );
    }
    let q = query(serde_json::json!({
        "operation": "references", "goal": "test", "reasoning": "test",
        "uri": large.to_string_lossy(),
        "position": {"line": 0, "character": 0},
        "contextLines": 2
    }));
    let unavailable = "[content unavailable — could not read: file too large]";
    let snippets = [&large, &binary]
        .iter()
        .map(|file| {
            serde_json::json!({
                "uri": octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri"),
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                "content": unavailable
            })
        })
        .collect::<Vec<_>>();
    let result = locations(
        &q,
        &mut sources,
        "references",
        "referencesProvider",
        snippets,
    )
    .await;
    assert_eq!(result["payload"]["totalReferences"], 2, "{result}");
    let rows = result["payload"]["locations"].as_array().expect("rows");
    assert!(
        rows.iter().all(|row| row["content"] == unavailable),
        "the unavailable reason is kept as content: {result}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn bounded_source_reads_refuse_fifos_without_blocking() {
    let (root, paths) = temp_workspace("fifo");
    let fifo = root.join("pipe.ts");
    let path = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).expect("c path");
    // SAFETY: `path` is a valid NUL-terminated C string for the call.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(matches!(
        read_bounded_source(&fifo),
        Err(SourceReadError::Unreadable(_))
    ));
    // The async read and the cache go through the O_NONBLOCK open and
    // the handle's fstat, so a FIFO (no writer) never blocks them.
    let bounded = std::time::Duration::from_secs(5);
    let read = tokio::time::timeout(bounded, read_bounded_source_async(fifo.clone()))
        .await
        .expect("async read of a FIFO must not block");
    assert!(matches!(read, Err(SourceReadError::Unreadable(_))));
    let mut sources = SourceCache::new(&paths);
    let cached = tokio::time::timeout(bounded, sources.get(&fifo.to_string_lossy()))
        .await
        .expect("cache read of a FIFO must not block");
    assert!(cached.is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn source_line_index_matches_split_inclusive() {
    for text in ["", "a", "a\n", "a\nb", "a\r\nb\r\n", "\n\n"] {
        let source = Source::new(text.to_owned());
        let expected = text.split_inclusive('\n').collect::<Vec<_>>();
        assert_eq!(source.line_count(), expected.len(), "{text:?}");
        for start in 0..=expected.len() {
            for end in start..=expected.len() + 1 {
                assert_eq!(
                    source.lines(start, end),
                    expected[start..end.min(expected.len())].concat(),
                    "{text:?} {start}..{end}"
                );
            }
        }
    }
}

#[test]
fn source_line_index_breaks_on_a_lone_cr() {
    // `\r` alone ends a line, as the server counts it.
    let source = Source::new("a\rb\r\nc\nd\r".to_owned());
    assert_eq!(source.line_count(), 4);
    assert_eq!(source.lines(0, 1), "a\r");
    assert_eq!(source.lines(1, 2), "b\r\n");
    assert_eq!(source.lines(2, 4), "c\nd\r");
}

#[test]
fn explicit_positions_on_lone_cr_lines_are_in_bounds() {
    // A lone-CR file has two lines, not one.
    let source = "export const a = 1;\rexport const b = 2;\r";
    let q = query(serde_json::json!({
        "operation": "hover", "goal": "test", "reasoning": "test",
        "uri": "/repo/cr.ts",
        "position": {"line": 1, "character": 0}
    }));
    let anchor = resolve_anchor(&q, "/repo/cr.ts", "file:///repo/cr.ts", Some(source))
        .expect("line 1 of a lone-CR file is in bounds");
    assert_eq!((anchor.line, anchor.character), (1, 0));
    let at = |line, character| (line, character);
    assert_eq!(position_bounds_error(source, at(1, 19)), None);
    assert!(position_bounds_error(source, at(1, 20)).is_some());
    // The final break opens an empty last line; past it is out of bounds.
    assert_eq!(position_bounds_error(source, at(2, 0)), None);
    assert!(position_bounds_error(source, at(3, 0)).is_some());
    // CRLF: the `\r` is part of the break, not a column.
    assert!(position_bounds_error("ab\r\ncd", at(0, 3)).is_some());
    assert_eq!(position_bounds_error("ab\r\ncd", at(0, 2)), None);
}

#[tokio::test(flavor = "current_thread")]
async fn context_windows_come_from_one_cached_read_per_file() {
    let (root, paths) = temp_workspace("context");
    let file = root.join("a.ts");
    std::fs::write(&file, "l1\nl2\nl3\nl4\nl5\n").expect("a.ts");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    let location = |line: u64| {
        serde_json::json!({
            "uri": uri,
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 2}}
        })
    };
    let mut sources = SourceCache::new(&paths);
    let mut first = location(0);
    let mut last = location(4);
    apply_context_lines(&mut first, 1, &mut sources).await;
    apply_context_lines(&mut last, 1, &mut sources).await;
    assert_eq!(sources.reads(), 1);
    assert_eq!(first["content"], "l1\nl2\n");
    assert_eq!(
        first["displayRange"],
        serde_json::json!({"startLine": 1, "endLine": 2})
    );
    assert_eq!(last["content"], "l4\nl5\n");
    assert_eq!(
        last["displayRange"],
        serde_json::json!({"startLine": 4, "endLine": 5})
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn symbol_anchors_resolve_on_the_synchronized_text_not_the_disk() {
    // The path does not exist: resolution must use the supplied content.
    let q = query(serde_json::json!({
        "operation": "definition", "goal": "test", "reasoning": "test",
        "uri": "/nonexistent/lib.rs",
        "symbolName": "greet",
        "lineHint": 2
    }));
    let source = "// é😀\nfn é😀() {} fn greet() {}\n";
    let anchor = resolve_anchor(
        &q,
        "/nonexistent/lib.rs",
        "file:///nonexistent/lib.rs",
        Some(source),
    )
    .expect("anchor from content");
    // `fn é😀() {} fn ` is 15 UTF-16 units (é = 1, 😀 = 2).
    assert_eq!((anchor.line, anchor.character), (1, 15));
    let resolved = anchor.resolved_symbol.expect("receipt");
    assert_eq!(resolved["foundAtLine"], 2);
    assert_eq!(resolved["foundAtCharacter"], 16);
    assert!(resolve_anchor(&q, "/nonexistent/lib.rs", "file:///x", None).is_err());
}

#[test]
fn rust_context_overlays_the_engine_headless_defaults() {
    let mut config = octocode_engine::lsp::types::JsLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: None,
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: Some(
            octocode_engine::lsp::config::rust_analyzer_headless_options(),
        ),
        env: None,
        max_memory_mb: None,
    };
    let q = query(serde_json::json!({
        "operation": "definition", "goal": "test", "reasoning": "test",
        "uri": "/repo/src/lib.rs",
        "position": {"line": 0, "character": 0},
        "rustContext": {"features": ["x"]}
    }));
    apply_rust_context(&mut config, &q).expect("rust context");
    let options = config.initialization_options.expect("options");
    assert_eq!(options["cargo"]["features"], serde_json::json!(["x"]));
    assert_eq!(options["cargo"]["buildScripts"]["enable"], false);
    assert_eq!(options["cargo"]["targetDir"], true);
    assert_eq!(options["procMacro"]["enable"], false);
    assert_eq!(options["checkOnSave"], false);
    assert_eq!(
        options["cachePriming"]["enable"], false,
        "engine defaults the overlay does not set are kept"
    );
}

fn engine_error(message: &str) -> octocode_engine::error::Error {
    octocode_engine::error::Error::new(octocode_engine::error::Status::GenericFailure, message)
}

#[test]
fn failed_hierarchy_expansion_is_an_error_or_a_marked_partial_row() {
    // Nothing expanded and the provider failed: surface the error.
    assert!(expansion_outcome(Vec::new(), vec![engine_error("boom")]).is_err());
    // Nothing expanded and nothing failed: a genuine empty result.
    let (items, failures) = expansion_outcome(Vec::new(), Vec::new()).expect("empty");
    assert!(items.is_empty() && failures.is_empty());

    let q = query(serde_json::json!({
        "operation": "callers", "goal": "test", "reasoning": "test",
        "uri": "file:///repo/a.ts",
        "position": {"line": 0, "character": 0},
        "depth": 3
    }));
    let (items, failures) = expansion_outcome(
        vec![serde_json::json!({"from": {"name": "caller"}})],
        vec![engine_error("LSP error: deeper level")],
    )
    .expect("partial expansion keeps what was found");
    let mut row = items_payload(&q, "callers", serde_json::json!(items));
    mark_partial_expansion(&mut row, &q, &failures);
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["callHierarchyExpansionFailed"])
    );
    assert!(
        row["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("deeper level"))
    );
    assert_eq!(row["next"]["retry"]["tool"], "lspSearch");
    // The runtime envelope copies the caller's reasoning into continuations.
    row["next"]["retry"]["query"]["reasoning"] = serde_json::json!("retry");
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":row}]}),
    )
    .expect("partial hierarchy row satisfies the output contract");
}

/// A fake call graph: `callers[name]` lists the names calling `name`. Each
/// node's selection line is derived from its name, so nodes differ by key.
struct FakeGraph {
    callers: std::collections::HashMap<String, Vec<String>>,
    uri: Option<String>,
    requests: std::cell::Cell<usize>,
}

fn name_line(name: &str) -> u64 {
    name.bytes().fold(1469598103u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
    }) % 100_000
}

fn node_at(name: &str, uri: Option<&str>) -> serde_json::Value {
    let line = name_line(name);
    let mut node = serde_json::json!({
        "name": name,
        "kind": 12,
        "range": {"start": {"line": line, "character": 0}, "end": {"line": line + 3, "character": 1}},
        "selectionRange": {"start": {"line": line, "character": 9}, "end": {"line": line, "character": 12}}
    });
    if let Some(uri) = uri {
        node["uri"] = serde_json::json!(uri);
    }
    node
}

fn node(name: &str) -> serde_json::Value {
    node_at(name, None)
}

impl HierarchySource for FakeGraph {
    async fn expand(
        &self,
        _: Expansion,
        item: serde_json::Value,
    ) -> Result<serde_json::Value, octocode_engine::error::Error> {
        self.requests.set(self.requests.get() + 1);
        let name = item["name"].as_str().unwrap_or_default();
        if name == "broken" {
            return Err(engine_error("expansion failed"));
        }
        // Yield once so a level's requests genuinely interleave.
        tokio::task::yield_now().await;
        Ok(serde_json::json!(
            self.callers
                .get(name)
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, caller)| serde_json::json!({
                    "from": node_at(caller, self.uri.as_deref()),
                    // The same edge read as an outgoing call, so one graph
                    // serves both call directions.
                    "to": node_at(caller, self.uri.as_deref()),
                    "fromRanges": [{"start": {"line": 2 + index, "character": 4}, "end": {"line": 2 + index, "character": 7}}]
                }))
                .collect::<Vec<_>>()
        ))
    }
}

fn graph(edges: &[(&str, &[&str])]) -> FakeGraph {
    FakeGraph {
        callers: edges
            .iter()
            .map(|(callee, callers)| {
                (
                    (*callee).to_owned(),
                    callers.iter().map(|caller| (*caller).to_owned()).collect(),
                )
            })
            .collect(),
        uri: None,
        requests: std::cell::Cell::new(0),
    }
}

async fn walk_from(
    graph: &FakeGraph,
    root: serde_json::Value,
    depth: u32,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::tools::cancel::CancellationCheck,
) -> Result<HierarchyWalk, LspFailure> {
    walk_hierarchy(
        graph,
        &[root],
        Expansion::IncomingCalls,
        depth,
        paths,
        cancel,
    )
    .await
}

async fn walk(graph: &FakeGraph, depth: u32) -> HierarchyWalk {
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");
    walk_from(
        graph,
        node("root"),
        depth,
        &paths,
        &crate::tools::cancel::NeverCancel,
    )
    .await
    .expect("walk")
}

fn edge_summary(walk: &HierarchyWalk) -> Vec<(String, u64, Option<String>)> {
    walk.edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge))
        .map(|edge| {
            (
                edge["from"]["name"].as_str().unwrap_or_default().to_owned(),
                edge["level"].as_u64().unwrap_or(0),
                edge["via"]["name"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_is_breadth_first_and_labels_every_edge_with_level_and_via() {
    // `a` calls root directly AND through `b`; breadth-first expansion must
    // expand `a` once, at level 1, so its caller `c` is reached at level 2.
    let graph = graph(&[("root", &["b", "a"]), ("b", &["a"]), ("a", &["c"])]);
    let walk = walk(&graph, 2).await;
    assert_eq!(
        edge_summary(&walk),
        vec![
            ("b".into(), 1, None),
            ("a".into(), 1, None),
            ("a".into(), 2, Some("b".into())),
            ("c".into(), 2, Some("a".into())),
        ]
    );
    // root, b, a expanded once each; level-2 nodes are not expanded at depth 2.
    assert_eq!(graph.requests.get(), 3);
    let edges = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge))
        .collect::<Vec<_>>();
    // Public coordinates are one-based lines and UTF-16 columns, like
    // `public_location`.
    let b = name_line("b");
    assert_eq!(
        edges[0]["from"]["displayRange"],
        serde_json::json!({"startLine": b + 1, "startCharacter": 10, "endLine": b + 4})
    );
    assert_eq!(edges[0]["from"]["kind"], "function");
    assert_eq!(
        edges[0]["fromRanges"],
        serde_json::json!([{"startLine": 3, "startCharacter": 5, "endLine": 3}])
    );
    assert_eq!(edges[2]["via"]["line"], b + 1);
    assert_eq!(edges[2]["via"]["character"], 10);
    assert!(edges[0].get("via").is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_keeps_cycle_edges_without_re_expanding() {
    let graph = graph(&[("root", &["a"]), ("a", &["root"])]);
    let walk = walk(&graph, 5).await;
    assert_eq!(
        edge_summary(&walk),
        vec![("a".into(), 1, None), ("root".into(), 2, Some("a".into()))]
    );
    assert_eq!(graph.requests.get(), 2, "root and a expand once each");
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_expands_a_diamond_node_once() {
    let graph = graph(&[
        ("root", &["a", "b"]),
        ("a", &["c"]),
        ("b", &["c"]),
        ("c", &["d"]),
    ]);
    let walk = walk(&graph, 3).await;
    assert_eq!(
        edge_summary(&walk),
        vec![
            ("a".into(), 1, None),
            ("b".into(), 1, None),
            ("c".into(), 2, Some("a".into())),
            ("c".into(), 2, Some("b".into())),
            ("d".into(), 3, Some("c".into())),
        ]
    );
    assert_eq!(graph.requests.get(), 4, "c is expanded once");
}

#[tokio::test(flavor = "current_thread")]
async fn repeated_parent_node_pairs_merge_into_one_edge_with_all_sites() {
    let graph = graph(&[("root", &["a", "a"])]);
    let walk = walk(&graph, 1).await;
    assert_eq!(walk.edges.len(), 1);
    let edge = public_edge(Expansion::IncomingCalls, &walk.edges[0]);
    assert_eq!(
        edge["fromRanges"],
        serde_json::json!([
            {"startLine": 3, "startCharacter": 5, "endLine": 3},
            {"startLine": 4, "startCharacter": 5, "endLine": 4}
        ])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_depth_is_clamped_in_code() {
    // A 30-long chain walked with an out-of-schema depth stops at 20 levels.
    let names = (0..30).map(|index| format!("n{index}")).collect::<Vec<_>>();
    let mut graph = graph(&[]);
    graph.callers.insert("root".into(), vec![names[0].clone()]);
    for pair in names.windows(2) {
        graph.callers.insert(pair[0].clone(), vec![pair[1].clone()]);
    }
    let walk = walk(&graph, 1_000).await;
    assert_eq!(walk.edges.len(), MAX_HIERARCHY_DEPTH as usize);
    assert_eq!(
        walk.edges.last().map(|edge| edge.level),
        Some(MAX_HIERARCHY_DEPTH)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_fan_out_cap_truncates_with_a_terminal_limit() {
    let callers = (0..MAX_HIERARCHY_FAN_OUT + 10)
        .map(|index| format!("caller{index}"))
        .collect::<Vec<_>>();
    let mut graph = graph(&[]);
    graph.callers.insert("root".into(), callers);
    let walk = walk(&graph, 1).await;
    assert_eq!(walk.edges.len(), MAX_HIERARCHY_FAN_OUT);
    assert_eq!(walk.fan_out_capped, vec!["root".to_owned()]);

    let q = query(serde_json::json!({
        "operation": "callers", "goal": "test", "reasoning": "test",
        "uri": "file:///repo/a.ts",
        "position": {"line": 0, "character": 0}
    }));
    let items = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge))
        .collect::<Vec<_>>();
    let mut row = items_payload(&q, "callers", serde_json::json!(items));
    mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, &walk)]);
    assert_eq!(row["payload"]["truncated"], true);
    assert_eq!(row["isPartial"], true);
    assert_eq!(row["terminalLimit"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["hierarchyFanOutLimit"])
    );
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":row}]}),
    )
    .expect("fan-out-limited row satisfies the output contract");
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_node_cap_truncates_with_an_executable_continuation() {
    let (root, paths) = temp_workspace("walk-cap");
    let file = root.join("graph.ts");
    std::fs::write(&file, "// graph\n").expect("graph.ts");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    // 40 direct callers, each with 40 callers of its own: 1 + 40 + 1600
    // nodes within depth 2, far past the node cap.
    let mut graph = graph(&[]);
    graph.uri = Some(uri.clone());
    let level1 = (0..40).map(|index| format!("p{index}")).collect::<Vec<_>>();
    for parent in &level1 {
        graph.callers.insert(
            parent.clone(),
            (0..40).map(|index| format!("{parent}c{index}")).collect(),
        );
    }
    graph.callers.insert("root".into(), level1);
    let walk = walk_from(
        &graph,
        node_at("root", Some(&uri)),
        2,
        &paths,
        &crate::tools::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    assert_eq!(
        walk.edges.len(),
        MAX_HIERARCHY_NODES - 1,
        "root + edges fill the cap"
    );
    assert!(walk.dropped_edges > 0);
    assert_eq!(graph.requests.get(), 41, "each node is expanded once");
    let resume = walk.resumes.first().expect("resume point");
    assert_eq!(resume.depth, 1);
    // Every parent whose children were dropped is recorded once.
    let names = walk
        .resumes
        .iter()
        .map(|resume| resume.node["name"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert!(names.len() > 1, "{names:?}");
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        names.len(),
        "no duplicate parents"
    );

    let q = query(serde_json::json!({
        "operation": "callers", "goal": "test", "reasoning": "test",
        "uri": file.to_string_lossy(),
        "symbolName": "root",
        "lineHint": 1,
        "depth": 2
    }));
    let items = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge))
        .collect::<Vec<_>>();
    let mut row = items_payload(&q, "callers", serde_json::json!(items));
    mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, &walk)]);
    assert_eq!(row["payload"]["truncated"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["hierarchyNodeLimit"])
    );
    assert!(row.get("terminalLimit").is_none(), "{}", row["next"]);
    let resume_query = &row["next"]["continueWalk"]["query"];
    assert_eq!(resume_query["depth"], 1);
    assert_eq!(resume_query["uri"], file.to_string_lossy().as_ref());
    assert!(resume_query.get("symbolName").is_none());
    assert_eq!(
        resume_query["position"]["line"],
        resume.node["selectionRange"]["start"]["line"]
    );
    let parents = row["payload"]["unexpandedParents"]
        .as_array()
        .expect("unexpanded parents");
    assert_eq!(parents.len(), walk.resumes.len());
    assert_eq!(parents[0]["remainingDepth"], 1);
    let resumed = (2..=walk.resumes.len())
        .map(|index| format!("continueWalk{index}"))
        .collect::<Vec<_>>();
    for key in &resumed {
        assert_eq!(row["next"][key]["tool"], "lspSearch", "{key}");
    }
    assert!(
        row["next"]
            .get(format!("continueWalk{}", walk.resumes.len() + 1))
            .is_none()
    );
    let mut row = with_next(&q, row);
    for continuation in ["continueWalk", "nextPage"] {
        if row["next"].get(continuation).is_some() {
            row["next"][continuation]["query"]["reasoning"] = serde_json::json!("continue");
        }
    }
    for key in &resumed {
        row["next"][key]["query"]["reasoning"] = serde_json::json!("continue");
    }
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":row}]}),
    )
    .expect("node-capped row satisfies the output contract");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_collects_per_node_failures_and_skips_out_of_policy_nodes() {
    let mut graph = graph(&[("root", &["broken", "fine"]), ("fine", &["deep"])]);
    let walk_all = walk(&graph, 2).await;
    assert_eq!(walk_all.failures.len(), 1, "broken failed at level 2");
    assert_eq!(edge_summary(&walk_all).len(), 3);

    // With a uri outside the (empty) policy, every result is skipped.
    graph.uri = Some("file:///etc/hosts".into());
    let walk = walk(&graph, 2).await;
    assert!(walk.edges.is_empty());
    assert_eq!(walk.out_of_policy, 2);
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_checks_cancellation_between_requests() {
    struct Cancelled;
    impl crate::tools::cancel::CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");
    let graph = graph(&[("root", &["a"])]);
    let error = walk_from(&graph, node("root"), 2, &paths, &Cancelled)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, "lsp.cancelled");
    assert_eq!(graph.requests.get(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn long_awaits_observe_cancellation() {
    struct CancelAfter(std::sync::atomic::AtomicUsize);
    impl crate::tools::cancel::CancellationCheck for CancelAfter {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 2 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
    }
    let cancel = CancelAfter(std::sync::atomic::AtomicUsize::new(0));
    let started = std::time::Instant::now();
    let result = cancellable(&cancel, std::future::pending::<()>()).await;
    assert_eq!(result.expect_err("cancelled").code, "lsp.cancelled");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(
        cancellable(&crate::tools::cancel::NeverCancel, async { 7 })
            .await
            .expect("completes"),
        7
    );
}

#[test]
fn explicit_positions_past_the_document_are_anchor_errors() {
    let position = |line, character| (line, character);
    let source = "ab\ncd\n";
    assert_eq!(position_bounds_error(source, position(0, 2)), None);
    // The empty line after the final newline is addressable.
    assert_eq!(position_bounds_error(source, position(2, 0)), None);
    assert!(
        position_bounds_error(source, position(99999, 0))
            .is_some_and(|error| error.contains("past the end of the document"))
    );
    assert!(
        position_bounds_error(source, position(1, 3))
            .is_some_and(|error| error.contains("past the end of 0-based line 1"))
    );
    let q = query(serde_json::json!({
        "operation": "hover", "goal": "test", "reasoning": "test",
        "uri": "file:///repo/a.ts",
        "position": {"line": 99999, "character": 0}
    }));
    assert!(resolve_anchor(&q, "/unused", "file:///repo/a.ts", Some(source)).is_err());
}

#[test]
fn workspace_symbols_are_flat_named_and_one_based() {
    let symbol = serde_json::json!({
        "name": "run",
        "kind": 12,
        "containerName": "",
        "location": {
            "uri": "file:///repo/a.ts",
            "range": {"start": {"line": 9, "character": 2}, "end": {"line": 12, "character": 1}}
        }
    });
    assert_eq!(
        public_workspace_symbol(&symbol),
        serde_json::json!({
            "name": "run",
            "kind": "function",
            "uri": "file:///repo/a.ts",
            "displayRange": {"startLine": 10, "startCharacter": 3, "endLine": 13}
        })
    );
}

#[test]
fn empty_diagnostics_carry_no_read_recovery_and_symbol_reads_are_matched() {
    let q = query(
        serde_json::json!({"operation": "diagnostic", "goal": "test", "reasoning": "test", "uri": "/repo/a.ts"}),
    );
    let row = with_next(&q, items_payload(&q, "diagnostics", serde_json::json!([])));
    assert_eq!(row["payload"]["category"], "noDiagnostics");
    assert!(row.get("next").is_none(), "{row}");

    let q = query(serde_json::json!({
        "operation": "definition", "goal": "test", "reasoning": "test",
        "uri": "/repo/a.ts",
        "symbolName": "run",
        "lineHint": 3
    }));
    let row = failure(&q, "file:///repo/a.ts", "lsp.anchorUnresolved", "x", true);
    let read = &row["next"]["readFile"]["query"];
    assert_eq!(read["path"], "/repo/a.ts");
    assert_eq!(read["matchString"], "run");
}

#[test]
fn path_policy_denials_are_file_access_failures_not_missing_servers() {
    let denied = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::OutsideAllowedRoots,
        "Path '/etc/hosts' is outside allowed directories",
    ));
    assert_eq!(denied.code, "pathOutsideAllowedRoots");
    assert!(!denied.retryable);
    assert!(denied.hint.contains("ALLOWED_PATHS"));
    let missing = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::NotFound,
        "missing",
    ));
    assert_eq!(missing.code, "fileAccessFailed");
}

#[test]
fn engine_errors_map_to_typed_failures_not_one_unavailable_code() {
    use octocode_engine::error::{Error, RpcError};
    let rpc = |code: i64, data: serde_json::Value| {
        LspFailure::from(Error::rpc(RpcError::from_value(serde_json::json!({
            "code": code,
            "message": "server said no",
            "data": data
        }))))
    };
    let timeout = LspFailure::from(Error::timeout("request timed out"));
    assert_eq!((timeout.code, timeout.retryable), ("lsp.timeout", true));
    let closed = LspFailure::from(Error::connection_closed("LSP connection closed"));
    assert_eq!((closed.code, closed.retryable), ("lsp.serverCrashed", true));
    let missing = rpc(-32601, serde_json::Value::Null);
    assert_eq!(
        (missing.code, missing.retryable),
        ("lsp.capabilityUnavailable", false)
    );
    for code in [-32801, -32802] {
        let stale = rpc(code, serde_json::Value::Null);
        assert_eq!((stale.code, stale.retryable), ("lsp.requestFailed", true));
    }
    let invalid = rpc(-32602, serde_json::Value::Null);
    assert_eq!(
        (invalid.code, invalid.retryable),
        ("lsp.requestFailed", false)
    );
    let other = LspFailure::from(engine_error("boom"));
    assert_eq!((other.code, other.retryable), ("lsp.requestFailed", true));
    assert!(timeout.message.contains("timed out"));
}

/// A definition reached through a symlink and through its target is
/// one definition (canonical-path identity, like the hierarchy walk).
#[cfg(unix)]
#[test]
fn definition_identity_treats_a_symlink_and_its_target_as_one() {
    let (root, _) = temp_workspace("identity-symlink");
    let target = root.join("impl.ts");
    std::fs::write(&target, "export function f() {}\n").expect("target");
    let link = root.join("link.ts");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let snippet = |path: &std::path::Path| octocode_engine::lsp::types::JsCodeSnippet {
        uri: octocode_engine::lsp::uri::path_to_uri(&path.to_string_lossy()).expect("uri"),
        range: octocode_engine::lsp::types::JsRange {
            start: octocode_engine::lsp::types::JsExactPosition {
                line: 0,
                character: 16,
            },
            end: octocode_engine::lsp::types::JsExactPosition {
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
    let _ = std::fs::remove_dir_all(root);
}

/// `callHierarchy` walks both directions; when both hit the node
/// cap, the unexpanded parents and continuations of both are kept (one
/// combined, direction-tagged list), not overwritten by the second walk.
#[tokio::test(flavor = "current_thread")]
async fn call_hierarchy_keeps_the_truncation_of_both_directions() {
    let (root, paths) = temp_workspace("walk-both");
    let file = root.join("graph.ts");
    std::fs::write(&file, "// graph\n").expect("graph.ts");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    let mut graph = graph(&[]);
    graph.uri = Some(uri.clone());
    let level1 = (0..40).map(|index| format!("p{index}")).collect::<Vec<_>>();
    for parent in &level1 {
        graph.callers.insert(
            parent.clone(),
            (0..40).map(|index| format!("{parent}c{index}")).collect(),
        );
    }
    graph.callers.insert("root".into(), level1);
    let mut walks = Vec::new();
    for expansion in [Expansion::IncomingCalls, Expansion::OutgoingCalls] {
        let walk = walk_hierarchy(
            &graph,
            &[node_at("root", Some(&uri))],
            expansion,
            2,
            &paths,
            &crate::tools::cancel::NeverCancel,
        )
        .await
        .expect("walk");
        assert!(!walk.resumes.is_empty(), "{expansion:?} hits the cap");
        walks.push((expansion, walk));
    }
    let q = query(serde_json::json!({
        "operation": "callHierarchy", "goal": "test", "reasoning": "test",
        "uri": file.to_string_lossy(),
        "position": {"line": 0, "character": 9},
        "depth": 2
    }));
    let mut row = items_payload(&q, "callHierarchy", serde_json::json!([]));
    let marked = walks
        .iter()
        .map(|(expansion, walk)| (*expansion, walk))
        .collect::<Vec<_>>();
    mark_truncation(&mut row, &q, &marked);
    let incoming = walks[0].1.resumes.len();
    let outgoing = walks[1].1.resumes.len();
    let parents = row["payload"]["unexpandedParents"]
        .as_array()
        .expect("unexpanded parents");
    assert_eq!(parents.len(), incoming + outgoing, "both directions listed");
    assert!(
        parents[..incoming]
            .iter()
            .all(|parent| parent["direction"] == "incoming")
    );
    assert!(
        parents[incoming..]
            .iter()
            .all(|parent| parent["direction"] == "outgoing")
    );
    let key = |index: usize| match index {
        0 => "continueWalk".to_owned(),
        _ => format!("continueWalk{}", index + 1),
    };
    for index in 0..incoming + outgoing {
        let operation = &row["next"][key(index)]["query"]["operation"];
        let expected = if index < incoming {
            "callers"
        } else {
            "callees"
        };
        assert_eq!(operation, expected, "{}", key(index));
    }
    assert!(row["next"].get(key(incoming + outgoing)).is_none());
    let mut row = with_next(&q, row);
    if let Some(next) = row["next"].as_object_mut() {
        for continuation in next.values_mut() {
            continuation["query"]["goal"] = serde_json::json!("walk");
            continuation["query"]["reasoning"] = serde_json::json!("continue");
        }
    }
    crate::contracts::validate_output(
        "lspSearch",
        &serde_json::json!({"results":[{"index":0,"data":row}]}),
    )
    .expect("two-direction truncation satisfies the output contract");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn anchor_file_uri_is_stated_once_per_row() {
    let canonical = "file:///repo/src/util.ts";
    let mut symbol = serde_json::json!({"name":"greet","uri":canonical,"foundAtLine":1});
    super::receipt::drop_same_uri(&mut symbol, canonical);
    assert!(symbol.get("uri").is_none(), "{symbol}");
    // A payload whose locations all sit in ANOTHER file keeps its uri.
    let mut other = serde_json::json!({"kind":"references","uri":"file:///repo/src/index.ts"});
    super::receipt::drop_same_uri(&mut other, canonical);
    assert_eq!(other["uri"], "file:///repo/src/index.ts");
    // Percent-encoding differences still name the same file.
    let mut encoded = serde_json::json!({"uri":"file:///repo/src/a%20b.ts"});
    super::receipt::drop_same_uri(&mut encoded, "file:///repo/src/a b.ts");
    assert!(encoded.get("uri").is_none(), "{encoded}");
}

#[test]
fn page_two_query_hashes_like_page_one() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "uri":"/tmp/a.rs","symbolName":"is_alive","lineHint":1,"operation":"references",
        "pageSize":1,"goal": "test", "reasoning":"r"
    }))
    .expect("page 1");
    // A continuation lists the same fields in another order.
    let second: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "pageSize":1,"page":2,"snapshot":"lsp-v1:abc","operation":"references",
        "lineHint":1,"goal": "test", "reasoning":"r","symbolName":"is_alive","uri":"/tmp/a.rs"
    }))
    .expect("page 2");
    let items = [serde_json::json!({"uri":"file:///tmp/a.rs"})];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&second, "references", &items),
        "first={} second={}",
        first.to_row(),
        second.to_row()
    );
}

#[test]
fn unresolved_symbol_anchor_reads_the_hinted_lines_and_suggests_near_names() {
    let q = query(serde_json::json!({
        "operation": "references", "goal": "test", "reasoning": "test",
        "uri": "/repo/lib.rs",
        "symbolName": "is_invalid_input_codeX",
        "lineHint": 12
    }));
    let source = (1..=20)
        .map(|line| match line {
            12 => "pub fn is_invalid_input_code(code: &str) -> bool {".to_owned(),
            13 => "    is_input(code)".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(&q, "file:///repo/lib.rs", "lsp.anchorUnresolved", "x", true);
    anchor_recovery(&mut row, &q, Some(&source));
    let read = &row["next"]["readFile"];
    assert_eq!(read["query"]["path"], "/repo/lib.rs", "{row}");
    assert_eq!(read["query"]["startLine"], 7, "{row}");
    assert_eq!(read["query"]["endLine"], 17, "{row}");
    assert!(read["query"].get("matchString").is_none(), "{row}");
    assert!(
        !read["why"]
            .as_str()
            .unwrap_or_default()
            .contains("unavailable"),
        "{row}"
    );
    let retry = &row["next"]["didYouMean"];
    assert_eq!(retry["tool"], "lspSearch", "{row}");
    assert_eq!(
        retry["query"]["symbolName"], "is_invalid_input_code",
        "{row}"
    );
    assert_eq!(retry["query"]["lineHint"], 12, "{row}");
    assert!(
        row["hints"]
            .as_array()
            .is_some_and(|hints| hints.iter().any(|h| h
                .as_str()
                .is_some_and(|h| h.contains("is_invalid_input_code")))),
        "{row}"
    );
}

#[test]
fn call_sites_that_differ_only_in_end_column_publish_once() {
    let edge = HierarchyEdge {
        node: node("callee"),
        parent: None,
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 870, "character": 10}, "end": {"line": 870, "character": 22}}),
            serde_json::json!({"start": {"line": 870, "character": 10}, "end": {"line": 870, "character": 17}}),
            serde_json::json!({"start": {"line": 871, "character": 2}, "end": {"line": 871, "character": 5}}),
        ],
    };
    let public = public_edge(Expansion::OutgoingCalls, &edge);
    assert_eq!(
        public["fromRanges"],
        serde_json::json!([
            {"startLine": 871, "startCharacter": 11, "endLine": 871},
            {"startLine": 872, "startCharacter": 3, "endLine": 872}
        ])
    );
}

#[test]
fn typescript_builtin_lib_declarations_are_recognized_only_under_typescript_lib() {
    let at = |uri: &str| serde_json::json!({"uri": uri});
    assert!(is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/lib.es5.d.ts"
    )));
    assert!(is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/lib.dom.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/@types/node/lib.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/src/lib.utils.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&at(
        "file:///repo/node_modules/typescript/lib/typescript.d.ts"
    )));
    assert!(!is_builtin_lib_declaration(&serde_json::json!({})));
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_walk_omits_builtin_lib_items_and_discloses_the_count() {
    let mut graph = graph(&[("root", &["toUpperCase", "map"])]);
    graph.uri = Some("file:///repo/node_modules/typescript/lib/lib.es5.d.ts".into());
    let walk = walk(&graph, 1).await;
    assert!(walk.edges.is_empty());
    assert_eq!(walk.builtin_lib, 2);
    assert_eq!(walk.out_of_policy, 0, "built-ins are not policy failures");
    let q = query(serde_json::json!({
        "operation": "callees", "goal": "test", "reasoning": "test",
        "uri": "file:///repo/a.ts",
        "position": {"line": 0, "character": 0}
    }));
    let mut row =
        serde_json::json!({"status": "hasResults", "payload": {"kind": "callees", "items": []}});
    mark_truncation(&mut row, &q, &[(Expansion::OutgoingCalls, &walk)]);
    assert_eq!(
        row["warnings"],
        serde_json::json!(["2 TypeScript built-in library items (lib.*.d.ts) were omitted."])
    );
}

#[test]
fn diagnostic_severity_is_published_by_name() {
    let (items, _) = diagnostic_items(Some(serde_json::json!([
        {"severity": 1, "message": "e", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}},
        {"severity": 2, "message": "w"},
        {"severity": 3, "message": "i"},
        {"severity": 4, "message": "h"},
        {"severity": 9, "message": "unknown"},
        {"message": "none"}
    ])));
    let severities = items
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item.get("severity").cloned())
        .collect::<Vec<_>>();
    assert_eq!(
        severities,
        vec![
            Some(serde_json::json!("error")),
            Some(serde_json::json!("warning")),
            Some(serde_json::json!("information")),
            Some(serde_json::json!("hint")),
            Some(serde_json::json!(9)),
            None,
        ]
    );
    assert_eq!(
        items[0]["displayRange"],
        serde_json::json!({"startLine": 1, "startCharacter": 1, "endLine": 1})
    );
}

#[test]
fn long_declaration_content_is_capped_with_a_marker_naming_the_omitted_lines() {
    let body = (0..200)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut location = serde_json::json!({
        "uri": "file:///repo/a.ts",
        "content": body,
        "displayRange": {"startLine": 426, "endLine": 625}
    });
    cap_declaration_content(&mut location);
    let content = location["content"].as_str().expect("content");
    let lines = content.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), MAX_DECLARATION_CONTENT_LINES + 1);
    assert_eq!(lines[0], "line 0");
    assert_eq!(lines[MAX_DECLARATION_CONTENT_LINES - 1], "line 59");
    assert_eq!(
        lines[MAX_DECLARATION_CONTENT_LINES],
        "… 140 more lines omitted (source lines 486-625); read them with localFetch startLine/endLine."
    );
    let short = "a\nb\nc";
    let mut small =
        serde_json::json!({"content": short, "displayRange": {"startLine": 1, "endLine": 3}});
    cap_declaration_content(&mut small);
    assert_eq!(small["content"], short, "short bodies are untouched");
}
