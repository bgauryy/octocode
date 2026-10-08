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

/// A tool row as the public response carries it: leads and prose tips in
/// `hints`, pages in `next`.
fn public_row(data: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::json!({"results":[{"index":0,"data":data}]});
    crate::response::continuations::finalize(
        &mut out,
        crate::tools::id::ToolId::LspSearch,
        &crate::response::continuations::Sources::Rows(&[]),
        &crate::response::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    out
}

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
        "operation": "documentSymbols", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
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
        query = serde_json::from_value(continuation["query"]["queries"][0].clone())
            .expect("executable continuation query");
        assert_eq!(query.snapshot(), Some(snapshot.as_str()));
    }

    assert_eq!(actual, expected);
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
            "operation": operation, "mainGoal": "test", "reasoning": "test",
            "path": "/repo/src/lib.rs",
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
        "operation": "documentSymbols", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
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
    let changed = snapshot_changed(&query);
    assert_eq!(changed["errorCode"], "staleSnapshot");
    assert_eq!(changed["next"]["restart"]["query"]["queries"][0]["page"], 1);
    assert!(
        changed["next"]["restart"]["query"]["queries"][0]
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
    assert_eq!(
        continued["next"]["nextPage"]["query"]["queries"][0]["page"],
        3
    );
    assert!(continued["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string());
}

#[test]
fn semantic_snapshot_ignores_paging_and_workflow_metadata() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "mainGoal": "test", "reasoning": "Find every reference.",
        "debug": false,
        "path": "/repo/src/lib.rs",
        "symbolName": "run",
        "lineHint": 4,
        "pageSize": 1
    }))
    .expect("first page query");
    let continued: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "mainGoal": "test", "reasoning": "Continuation metadata may be normalized.",
        "path": "/repo/src/lib.rs",
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
fn document_symbols_are_compact_outline_rows() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": "file:///repo/src/lib.rs"
    }))
    .expect("document symbols query");
    let range = |start: u64, start_char: u64, end: u64| {
        serde_json::json!({
            "start": {"line": start, "character": start_char},
            "end": {"line": end, "character": 1}
        })
    };
    let symbol = |name: &str, kind: u64, full: (u64, u64), anchor: (u64, u64)| {
        serde_json::json!({
            "name": name,
            "kind": kind,
            "range": range(full.0, 0, full.1),
            "selectionRange": range(anchor.0, anchor.1, anchor.0)
        })
    };
    let mut greeter = symbol("Greeter", 5, (2, 8), (2, 7));
    greeter["children"] = serde_json::json!([symbol("greet", 6, (3, 5), (3, 5))]);
    // Rust `impl` blocks are `object` symbols; their members are listed.
    let mut implementation = symbol("impl Greeter", 19, (16, 20), (16, 5));
    implementation["children"] = serde_json::json!([symbol("new", 12, (17, 19), (17, 7))]);
    // A function's locals are listed under it like any member.
    let mut outer = symbol("outer", 12, (22, 30), (22, 3));
    outer["children"] = serde_json::json!([symbol("x", 13, (23, 23), (23, 8))]);
    let raw = serde_json::json!([
        greeter,
        // Doc comment / attribute lines are part of `range`; the row's line
        // is the name line (`selectionRange`), a usable lineHint.
        symbol("documented", 12, (10, 14), (12, 7)),
        implementation,
        outer,
        // Two symbols on one line carry their 1-based column.
        symbol("a", 14, (32, 32), (32, 6)),
        symbol("b", 14, (32, 32), (32, 13)),
    ]);

    let envelope = items_payload(&query, "documentSymbols", raw);
    assert_eq!(envelope["lsp"]["source"], "lsp");
    assert!(envelope.get("summary").is_none(), "{envelope}");
    // Outline rows (P1), checked through their text outline.
    let mut payload = envelope["payload"].clone();
    payload["symbols"] = serde_json::json!(crate::tools::symbol_outline::outline_rows(
        &crate::tools::symbol_outline::flatten_members(
            payload["symbols"].as_array().expect("rows")
        )
    ));
    assert_eq!(
        payload,
        serde_json::json!({
            "kind": "documentSymbols",
            "symbols": [
                "3-9 class Greeter",
                "  4-6 method greet",
                "13-15 function documented",
                "17-21 object impl Greeter",
                "  18-20 function new",
                "23-31 function outer",
                "  24 variable x",
                "33 constant a col 7; 33 b col 14"
            ]
        })
    );
    crate::contracts::validate_output("lspSearch", &public_row(&envelope))
        .expect("outline rows satisfy the output contract");
    // CHAIN: an outline leads to localFetch: the first top-level
    // multi-line declaration, name line to last line.
    let read = &envelope["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{envelope}");
    assert_eq!(
        read["query"]["queries"][0],
        serde_json::json!({"path": "file:///repo/src/lib.rs", "ranges": ["3-9"]}),
        "{envelope}"
    );
}

#[test]
fn type_aliases_are_named_from_their_declaring_line() {
    let range = |line: u64| serde_json::json!({"start": {"line": line, "character": 0}, "end": {"line": line, "character": 1}});
    let symbol = |name: &str, line: u64| serde_json::json!({"name": name, "kind": 13, "range": range(line), "selectionRange": range(line)});
    let mut outer = symbol("holder", 3);
    outer["children"] = serde_json::json!([symbol("Inner", 4)]);
    let mut symbols = serde_json::json!([
        symbol("Alias", 0),
        symbol("Exported", 1),
        symbol("value", 2),
        outer,
        symbol("Typed", 5),
    ]);
    let content = "type Alias = string;\nexport declare type Exported<T> = T;\nconst value = 1;\nconst holder = () => {\n  type Inner = number;\ntype TypedLonger = 1;\n";
    super::render::name_type_aliases(&mut symbols, content);
    let kinds: Vec<_> = [
        &symbols[0],
        &symbols[1],
        &symbols[2],
        &symbols[3],
        &symbols[3]["children"][0],
        &symbols[4],
    ]
    .iter()
    .map(|symbol| symbol["kind"].clone())
    .collect();
    assert_eq!(
        kinds,
        [
            serde_json::json!("type"),
            serde_json::json!("type"),
            serde_json::json!(13),
            serde_json::json!(13),
            serde_json::json!("type"),
            serde_json::json!(13)
        ]
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
    assert_eq!(public["content"], "6\tpub fn greet() {}\n", "{public}");
}

#[test]
fn widened_content_reports_its_first_line_and_single_file_pages_hoist_the_path() {
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
    // The gutter states where widened content begins.
    assert_eq!(widened["content"], "5\ta\n6\tb\n7\tc\n", "{widened}");
    assert!(widened.get("contentStartLine").is_none(), "{widened}");

    let same = [widened.clone(), widened.clone()];
    assert_eq!(
        shared_location_path(&same).as_deref(),
        Some("/repo/src/lib.rs")
    );
    let mut other = widened.clone();
    other["path"] = serde_json::json!("/repo/src/main.rs");
    assert!(shared_location_path(&[widened.clone(), other]).is_none());
    assert!(
        shared_location_path(&[widened]).is_none(),
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
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
        "symbolName": "execute",
        "lineHint": 10
    }));
    let empty = with_next(&query, empty(&query, "noLocations", "none", true));
    assert_eq!(empty["status"], "empty");
    assert_eq!(empty["next"]["read"]["confidence"], "exact");
    assert!(
        empty["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("Verify the symbol and anchor")),
        "{empty}"
    );
    let down = failure(
        &query,
        "file:///repo/src/lib.rs",
        "serverUnavailable",
        "missing",
        false,
    );
    assert_eq!(down["status"], "error");
    assert_eq!(down["errorCode"], "serverUnavailable");
    assert_eq!(down["next"]["read"]["tool"], "localFetch");
    assert!(
        down["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("astSearch symbols/match")),
        "{down}"
    );

    let timeout = failure(
        &query,
        "file:///repo/src/lib.rs",
        "timeout",
        "timed out",
        false,
    );
    assert_eq!(timeout["next"]["retry"]["tool"], "lspSearch");
    assert_eq!(timeout["next"]["read"]["tool"], "localFetch");

    let explicitly_scoped = requery(&query, serde_json::json!({"workspaceRoot": "/repo"}));
    let scoped = failure(
        &explicitly_scoped,
        "file:///repo/src/lib.rs",
        "capabilityUnavailable",
        "unsupported",
        true,
    );
    assert_eq!(scoped["next"]["read"]["tool"], "localFetch");
}

#[test]
fn workspace_root_failures_emit_a_string_path_without_directory_read_recovery() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "workspaceSymbol", "mainGoal": "test", "reasoning": "test",
        "workspaceRoot": "/repo",
        "symbolName": "execute"
    }))
    .expect("workspace-symbol query");

    let down = failure(
        &query,
        "file:///repo",
        "serverUnavailable",
        "missing",
        false,
    );

    assert!(down["path"].is_string(), "{down}");
    assert!(down.get("next").is_none(), "{down}");
    assert!(
        down["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("Provide path")),
        "{down}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&down))
        .expect("workspace-root failure must satisfy the internal output contract");
}

#[test]
fn workspace_root_empty_results_do_not_emit_a_pathless_read_recovery() {
    let q = query(serde_json::json!({
        "operation": "workspaceSymbol", "mainGoal": "test", "reasoning": "test",
        "workspaceRoot": "/repo",
        "symbolName": "nothing"
    }));
    let mut row = with_next(&q, items_payload(&q, "symbols", serde_json::json!([])));
    assert_eq!(row["status"], "empty");
    assert!(row.get("next").is_none(), "{row}");
    // execute() stamps the canonical root URI on every row.
    row["uri"] = serde_json::json!("file:///repo");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
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
        serde_json::json!({"operation": "diagnostic", "mainGoal": "test", "reasoning": "test", "path": "file:///repo/a.ts"}),
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
    let listed = envelope["payload"]["matches"].as_array().expect("matches");
    assert_eq!(listed.len(), 2, "{envelope}");
    assert_eq!(listed[0]["message"], "first");
    assert_eq!(envelope["pagination"]["totalItems"], 2);

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
            serde_json::json!({"path": absolute("src/a.ts"), "matchCount": 2, "lines": [5, 10]}),
            serde_json::json!({"path": absolute("src/b.ts"), "matchCount": 1, "lines": [1]}),
        ]
    );
    // Through the envelope, base + path names the real file.
    let envelope = crate::response::rows::envelope(vec![serde_json::json!({
        "index": 0,
        "data": {"path": absolute("src/deep/anchor.ts"), "payload": {"files": summaries}}
    })]);
    let base = envelope["root"].as_str().expect("root");
    for file in envelope["results"][0]["data"]["payload"]["files"]
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
    let root = std::env::temp_dir().join(format!("octocode-lsp-grouped-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    std::fs::write(root.join("a.ts"), "foo\nfoo\n").expect("a.ts");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": uri,
        "symbolName": "foo", "lineHint": 1,
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
    assert!(result["payload"].get("matches").is_none(), "{result}");
    assert_eq!(
        result["payload"]["files"],
        serde_json::json!([{"path": root.join("a.ts").to_string_lossy(), "matchCount": 2, "lines": [1, 2]}])
    );
    assert_eq!(result["payload"]["matchCount"], 2);
    crate::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("grouped references satisfy the output contract");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_capped_alias_scan_is_disclosed_with_a_text_search() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/a.ts", "symbolName": "foo", "lineHint": 1
    }));
    let mut row = serde_json::json!({
        "type": "references",
        "payload": {"kind": "references", "coverage": {"scope": "languageServer", "exhaustive": false}}
    });
    let scope = super::scope::Scope::new("/repo".into(), vec!["*.ts".into()]);
    super::recovery::disclose_alias_cap(&mut row, &q, &scope);
    assert_eq!(row["payload"]["coverage"]["aliasScan"], "capped", "{row}");
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!([super::recovery::ALIAS_SCAN_CAPPED_REASON])
    );
    // The alias cap is fixed per request: a terminal limit, with the
    // lexical lead as the only way past it.
    assert_eq!(row["terminalLimit"], true, "{row}");
    assert_eq!(row["next"]["textSearch"]["tool"], "localSearch", "{row}");
    assert_eq!(
        row["next"]["textSearch"]["query"]["queries"][0]["path"],
        "/repo"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn recovered_alias_references_are_labeled_in_output() {
    let (root, paths) = temp_workspace("alias-label");
    std::fs::write(root.join("a.ts"), "foo\nbar\n").expect("a.ts");
    let uri =
        octocode_engine::lsp::uri::path_to_uri(&root.join("a.ts").to_string_lossy()).expect("uri");
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": uri,
        "symbolName": "foo", "lineHint": 1
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
    let files = result["payload"]["files"].as_array().expect("files");
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
    assert_eq!(result["payload"]["recoveredAliasReferences"], 1);
    crate::contracts::validate_output("lspSearch", &public_row(&result))
        .expect("labeled references satisfy the output contract");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn references_default_to_compact_rows_grouped_by_file() {
    let (root, paths) = temp_workspace("compact-refs");
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
            "path": uri_a, "symbolName": "foo", "lineHint": 5
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
    )
    .await;
    let payload = &result["payload"];
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
    assert_eq!(payload["totalFiles"], 2);
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
        )
        .await;
        assert!(result["payload"]["matches"].is_array(), "{extra}: {result}");
    }
    // A short reference list is compact too.
    let result = locations(
        &compact(serde_json::json!({})),
        &mut SourceCache::new(&paths),
        "references",
        "referencesProvider",
        snippets[..2].to_vec(),
    )
    .await;
    assert!(result["payload"].get("matches").is_none(), "{result}");
    assert_eq!(
        result["payload"]["files"],
        serde_json::json!([{"path": a.to_string_lossy(), "matches": [
            {"line": 5, "column": 14, "endLine": 29, "value": "export const foo = ("},
            {"line": 11, "column": 3, "value": "foo(bar);"}
        ]}]),
        "{result}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn paged_rows_state_page_facts_once_in_next() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts", "symbolName": "foo", "lineHint": 1
    }));
    let page = |payload: serde_json::Value| {
        serde_json::json!({
            "payload": payload,
            "pagination": {"currentPage": 1, "hasMore": true, "nextPage": 2, "pageSize": 3, "snapshot": "lsp-v1:abc"}
        })
    };
    let compact = with_next(
        &q,
        page(
            serde_json::json!({"kind": "references", "files": [{"path": "/repo/a.ts", "matches": ["1:1 foo"]}]}),
        ),
    );
    assert_eq!(
        compact["next"]["nextPage"]["query"]["queries"][0]["snapshot"],
        "lsp-v1:abc"
    );
    assert!(compact["pagination"].get("snapshot").is_none(), "{compact}");
    let rows = with_next(
        &q,
        page(serde_json::json!({"kind": "references", "matches": [{"path": "/repo/a.ts"}]})),
    );
    // The continuation carries the page and the snapshot; the row's
    // pagination keeps the same page facts on every page (B12), its size
    // included.
    for row in [&compact, &rows] {
        assert_eq!(row["next"]["nextPage"]["query"]["queries"][0]["page"], 2);
        assert_eq!(
            row["pagination"],
            serde_json::json!({"currentPage": 1, "hasMore": true, "pageSize": 3}),
            "{row}"
        );
    }
}

#[test]
fn local_search_identity_continuations_validate_as_anchored_queries() {
    // The shape localSearch emits as next.references / next.callers for a
    // declaration hit: uri + symbolName + lineHint, nothing else.
    for operation in ["references", "callers"] {
        let continuation = serde_json::json!({
            "mainGoal": "Who uses run_and_clear_commit_hooks?",
            "reasoning": "Declaration hit from localSearch.",
            "operation": operation,
            "path": "/repo/django/db/backends/base/base.py",
            "symbolName": "run_and_clear_commit_hooks",
            "lineHint": 749
        });
        let validated = crate::contracts::validate_query("lspSearch", continuation)
            .unwrap_or_else(|error| panic!("{operation}: {error:?}"));
        let parsed: LspSearchQuery = serde_json::from_value(validated).expect("typed query");
        assert!(matches!(parsed, LspSearchQuery::Anchored(_)), "{operation}");
        assert_eq!(parsed.operation(), operation);
        assert_eq!(parsed.symbol_name(), Some("run_and_clear_commit_hooks"));
    }
}

/// A `graph.ts` placeholder under `root` and its file URI.
fn graph_ts(root: &std::path::Path) -> (std::path::PathBuf, String) {
    let file = root.join("graph.ts");
    std::fs::write(&file, "// graph\n").expect("graph.ts");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    (file, uri)
}

fn temp_workspace(tag: &str) -> (std::path::PathBuf, crate::policy::path::PathPolicy) {
    let root = std::env::temp_dir().join(format!("octocode-lsp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let paths = crate::tools::test_support::workspace_policy(&root);
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
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": large.to_string_lossy(),
        "symbolName": "foo", "lineHint": 1,
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
    assert_eq!(result["pagination"]["totalItems"], 2, "{result}");
    let rows = result["payload"]["matches"].as_array().expect("rows");
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
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/nonexistent/lib.rs",
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
    // Found on its lineHint: the receipt adds the column, not echoes.
    assert!(resolved.get("foundAtLine").is_none(), "{resolved}");
    assert!(resolved.get("name").is_none(), "{resolved}");
    assert_eq!(resolved["foundAtCharacter"], 16);
    // Found off its hint: the line and the deviation are new facts.
    let off = query(serde_json::json!({
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/nonexistent/lib.rs",
        "symbolName": "greet",
        "lineHint": 1
    }));
    let moved = resolve_anchor(
        &off,
        "/nonexistent/lib.rs",
        "file:///nonexistent/lib.rs",
        Some(source),
    )
    .expect("anchor near the hint")
    .resolved_symbol
    .expect("receipt");
    assert_eq!(moved["foundAtLine"], 2, "{moved}");
    assert_eq!(moved["lineDeviation"], 1, "{moved}");
    assert!(resolve_anchor(&q, "/nonexistent/lib.rs", "file:///x", None).is_err());
}

#[test]
fn member_qualified_symbol_names_anchor_on_the_last_member() {
    let anchor_for = |path: &str, source: &str, name: &str, line: u32| {
        let q = query(serde_json::json!({
            "operation": "definition", "mainGoal": "test", "reasoning": "test",
            "path": path, "symbolName": name, "lineHint": line
        }));
        let anchor = resolve_anchor(&q, path, "file:///repo/x", Some(source)).expect(name);
        (anchor.line, anchor.character)
    };
    let js = "var ns = 1;\nthis.ns = function(fn) { var ns = {}; return ns; };\n";
    // `this.ns` anchors on `ns` (column 5), not on `this`.
    assert_eq!(anchor_for("/repo/a.js", js, "this.ns", 2), (1, 5));
    assert_eq!(anchor_for("/repo/a.js", js, "ns", 2), (1, 5));
    // A member repeated earlier on the line resolves inside the qualified
    // expression.
    let chain = "b.x = a.b.x = function () {};\n";
    assert_eq!(anchor_for("/repo/b.js", chain, "a.b.x", 1), (0, 10));
    let rust = "impl Foo {\n    fn bar() {}\n}\nfn main() { Foo::bar(); }\n";
    assert_eq!(anchor_for("/repo/lib.rs", rust, "Foo::bar", 4), (3, 17));
    // Not spelled contiguously: the bare member is resolved near the hint.
    let split = "this\n  .ns = function () {};\n";
    assert_eq!(anchor_for("/repo/c.js", split, "this.ns", 2), (1, 3));
    // Unqualified and non-identifier names resolve unchanged.
    assert_eq!(anchor_for("/repo/a.js", js, "this", 2), (1, 0));
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
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
        "symbolName": "foo", "lineHint": 1,
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
    octocode_engine::error::Error::new(message)
}

#[test]
fn failed_hierarchy_expansion_is_an_error_or_a_marked_partial_row() {
    // Nothing expanded and the provider failed: surface the error.
    assert!(expansion_outcome(Vec::new(), vec![engine_error("boom")]).is_err());
    // Nothing expanded and nothing failed: a genuine empty result.
    let (items, failures) = expansion_outcome(Vec::new(), Vec::new()).expect("empty");
    assert!(items.is_empty() && failures.is_empty());

    let q = query(serde_json::json!({
        "operation": "callers", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts",
        "symbolName": "foo", "lineHint": 1,
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
    row["next"]["retry"]["query"]["queries"][0]["reasoning"] = serde_json::json!("retry");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("partial hierarchy row satisfies the output contract");

    // The disclosure is one count plus next.retry; every distinct failure
    // is listed once, with its count, in the verbose `expansionFailures`.
    let mut failures = (0..6)
        .map(|n| engine_error(&format!("LSP error: node {n}")))
        .collect::<Vec<_>>();
    failures.extend((0..4).map(|_| engine_error("LSP error: timeout")));
    let mut row = items_payload(&q, "callers", serde_json::json!(items));
    mark_partial_expansion(&mut row, &q, &failures);
    let warnings = row["warnings"].as_array().cloned().unwrap_or_default();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].as_str().is_some_and(|w| w.contains("10 items")
            && w.contains("7 distinct")
            && w.contains("4×")
            && w.contains("timeout")),
        "{warnings:?}"
    );
    let listed = row["expansionFailures"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(listed.len(), 7, "{listed:?}");
    assert!(
        listed
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains("node 5"))),
        "{listed:?}"
    );
    assert!(
        listed.iter().any(|w| w
            .as_str()
            .is_some_and(|w| w.starts_with("4×") && w.contains("timeout"))),
        "{listed:?}"
    );
    assert!(
        crate::tools::id::ToolId::LspSearch
            .verbose_paths()
            .contains(&"results[].data.expansionFailures")
    );
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
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
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
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
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
    let edge = public_edge(Expansion::IncomingCalls, &walk.edges[0], None);
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
async fn the_anchor_keeps_every_direct_result_and_capped_parents_resume() {
    let (root, paths) = temp_workspace("walk-fan-out");
    let (file, uri) = graph_ts(&root);
    let wide = |prefix: &str| {
        (0..MAX_HIERARCHY_FAN_OUT + 10)
            .map(|index| format!("{prefix}{index}"))
            .collect::<Vec<_>>()
    };
    // The anchor's own results are paged, never cut.
    let mut flat_graph = graph(&[]);
    flat_graph.uri = Some(uri.clone());
    flat_graph.callers.insert("root".into(), wide("caller"));
    let flat = walk_from(
        &flat_graph,
        node_at("root", Some(&uri)),
        1,
        &paths,
        &crate::tools::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    assert_eq!(flat.edges.len(), MAX_HIERARCHY_FAN_OUT + 10);
    assert!(flat.fan_out_capped.is_empty() && flat.resumes.is_empty());

    // A wide node below the anchor keeps the first results and resumes from
    // itself, where it is the anchor and lists every result.
    let mut deep_graph = graph(&[("root", &["hub"])]);
    deep_graph.uri = Some(uri.clone());
    deep_graph.callers.insert("hub".into(), wide("caller"));
    let deep = walk_from(
        &deep_graph,
        node_at("root", Some(&uri)),
        2,
        &paths,
        &crate::tools::cancel::NeverCancel,
    )
    .await
    .expect("walk");
    assert_eq!(deep.edges.len(), 1 + MAX_HIERARCHY_FAN_OUT);
    assert_eq!(deep.fan_out_capped, vec!["hub".to_owned()]);
    let q = query(serde_json::json!({
        "operation": "callers", "mainGoal": "test", "reasoning": "test",
        "path": file.to_string_lossy(),
        "symbolName": "root",
        "lineHint": 1,
        "depth": 2
    }));
    let items = deep
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
        .collect::<Vec<_>>();
    let mut row = items_payload(&q, "callers", serde_json::json!(items));
    mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, &deep)]);
    assert_eq!(row["payload"]["truncated"], true);
    assert_eq!(row["isPartial"], true);
    assert!(row.get("terminalLimit").is_none(), "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["hierarchyFanOutLimit"])
    );
    assert_eq!(row["payload"]["unexpandedParents"][0]["name"], "hub");
    let resume = &row["next"]["continueWalk"]["query"]["queries"][0];
    assert_eq!(resume["depth"], 1, "{resume}");
    // D2: re-anchored by name and 1-based line, never a 0-based position.
    assert!(resume.get("position").is_none(), "{resume}");
    assert_eq!(resume["symbolName"], "hub", "{resume}");
    assert_eq!(
        resume["lineHint"].as_u64(),
        node_at("hub", Some(&uri))["selectionRange"]["start"]["line"]
            .as_u64()
            .map(|line| line + 1),
        "{resume}"
    );
    let mut row = with_next(&q, row);
    row["next"]["continueWalk"]["query"]["queries"][0]["reasoning"] = serde_json::json!("continue");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("fan-out-limited row satisfies the output contract");
    let resumed = walk_from(
        &deep_graph,
        node_at("hub", Some(&uri)),
        1,
        &paths,
        &crate::tools::cancel::NeverCancel,
    )
    .await
    .expect("resumed walk");
    assert_eq!(resumed.edges.len(), MAX_HIERARCHY_FAN_OUT + 10);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn hierarchy_node_cap_truncates_with_an_executable_continuation() {
    let (root, paths) = temp_workspace("walk-cap");
    let (file, uri) = graph_ts(&root);
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
        "operation": "callers", "mainGoal": "test", "reasoning": "test",
        "path": file.to_string_lossy(),
        "symbolName": "root",
        "lineHint": 1,
        "depth": 2
    }));
    let items = walk
        .edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
        .collect::<Vec<_>>();
    let mut row = items_payload(&q, "callers", serde_json::json!(items));
    mark_truncation(&mut row, &q, &[(Expansion::IncomingCalls, &walk)]);
    assert_eq!(row["payload"]["truncated"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["hierarchyNodeLimit"])
    );
    assert!(row.get("terminalLimit").is_none(), "{}", row["next"]);
    let resume_query = &row["next"]["continueWalk"]["query"]["queries"][0];
    assert_eq!(resume_query["depth"], 1);
    assert_eq!(resume_query["path"], file.to_string_lossy().as_ref());
    // D2: walk continuations re-anchor by symbol and 1-based line.
    assert!(resume_query.get("position").is_none(), "{resume_query}");
    assert_eq!(resume_query["symbolName"], resume.node["name"]);
    assert_eq!(
        resume_query["lineHint"].as_u64(),
        resume.node["selectionRange"]["start"]["line"]
            .as_u64()
            .map(|line| line + 1)
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
            row["next"][continuation]["query"]["queries"][0]["reasoning"] =
                serde_json::json!("continue");
        }
    }
    for key in &resumed {
        row["next"][key]["query"]["queries"][0]["reasoning"] = serde_json::json!("continue");
    }
    crate::contracts::validate_output("lspSearch", &public_row(&row))
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
    assert_eq!(error.code, "cancelled");
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
    assert_eq!(result.expect_err("cancelled").code, "cancelled");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(
        cancellable(&crate::tools::cancel::NeverCancel, async { 7 })
            .await
            .expect("completes"),
        7
    );
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
            "path": "/repo/a.ts",
            "displayRange": {"startLine": 10, "startCharacter": 3, "endLine": 13}
        })
    );
}

#[test]
fn empty_diagnostics_carry_no_read_recovery_and_symbol_reads_are_matched() {
    let q = query(
        serde_json::json!({"operation": "diagnostic", "mainGoal": "test", "reasoning": "test", "path": "/repo/a.ts"}),
    );
    let row = with_next(&q, items_payload(&q, "diagnostics", serde_json::json!([])));
    assert_eq!(row["payload"]["category"], "noDiagnostics");
    assert!(row.get("next").is_none(), "{row}");

    let q = query(serde_json::json!({
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/a.ts",
        "symbolName": "run",
        "lineHint": 3
    }));
    let row = failure(&q, "file:///repo/a.ts", "anchorUnresolved", "x", true);
    let read = &row["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["path"], "/repo/a.ts");
    assert_eq!(read["matchString"], "run");
}

#[test]
fn path_policy_denials_are_file_access_failures_not_missing_servers() {
    let denied = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::OutsideAllowedRoots,
        "Path '/etc/hosts' is outside allowed directories",
    ));
    assert_eq!(denied.code, "outsideAllowedRoots");
    assert!(!denied.retryable);
    assert!(denied.hint.contains("ALLOWED_PATHS"));
    let missing = LspFailure::path_denied(crate::policy::PolicyError::new(
        crate::policy::PolicyErrorCode::NotFound,
        "missing",
    ));
    assert_eq!(missing.code, "pathNotFound");
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
    assert_eq!((timeout.code, timeout.retryable), ("timeout", true));
    let closed = LspFailure::from(Error::connection_closed("LSP connection closed"));
    assert_eq!((closed.code, closed.retryable), ("serverCrashed", true));
    let missing = rpc(-32601, serde_json::Value::Null);
    assert_eq!(
        (missing.code, missing.retryable),
        ("capabilityUnavailable", false)
    );
    for code in [-32801, -32802] {
        let stale = rpc(code, serde_json::Value::Null);
        assert_eq!((stale.code, stale.retryable), ("requestFailed", true));
    }
    let invalid = rpc(-32602, serde_json::Value::Null);
    assert_eq!((invalid.code, invalid.retryable), ("requestFailed", false));
    let other = LspFailure::from(engine_error("boom"));
    assert_eq!((other.code, other.retryable), ("requestFailed", true));
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
        "path":"/tmp/a.rs","symbolName":"is_alive","lineHint":1,"operation":"references",
        "pageSize":1,"mainGoal": "test", "reasoning":"r"
    }))
    .expect("page 1");
    // A continuation lists the same fields in another order.
    let second: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "pageSize":1,"page":2,"snapshot":"lsp-v1:abc","operation":"references",
        "lineHint":1,"mainGoal": "test", "reasoning":"r","symbolName":"is_alive","path":"/tmp/a.rs"
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
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/lib.rs",
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
    let mut row = failure(&q, "file:///repo/lib.rs", "anchorUnresolved", "x", true);
    anchor_recovery(&mut row, &q, Some(&source));
    let read = &row["next"]["read"];
    assert_eq!(read["query"]["queries"][0]["path"], "/repo/lib.rs", "{row}");
    assert_eq!(
        read["query"]["queries"][0]["ranges"],
        serde_json::json!(["7-17"]),
        "{row}"
    );
    assert!(
        read["query"]["queries"][0].get("startLine").is_none(),
        "{row}"
    );
    assert!(
        read["query"]["queries"][0].get("matchString").is_none(),
        "{row}"
    );
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
        retry["query"]["queries"][0]["symbolName"], "is_invalid_input_code",
        "{row}"
    );
    assert_eq!(retry["query"]["queries"][0]["lineHint"], 12, "{row}");
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
    let public = public_edge(Expansion::OutgoingCalls, &edge, None);
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
async fn builtin_lib_callees_are_listed_not_counted() {
    let mut graph = graph(&[("root", &["toUpperCase", "map", "map"])]);
    graph.uri = Some("file:///repo/node_modules/typescript/lib/lib.es5.d.ts".into());
    let walk = walk(&graph, 1).await;
    assert!(walk.edges.is_empty());
    assert_eq!(
        walk.builtin_lib,
        vec!["toUpperCase".to_owned(), "map".to_owned()]
    );
    assert_eq!(walk.out_of_policy, 0, "built-ins are not policy failures");
    let q = query(serde_json::json!({
        "operation": "callees", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts",
        "symbolName": "foo", "lineHint": 1
    }));
    let mut row =
        serde_json::json!({"status": "hasResults", "payload": {"kind": "callees", "items": []}});
    mark_truncation(&mut row, &q, &[(Expansion::OutgoingCalls, &walk)]);
    assert!(row.get("warnings").is_none(), "{row}");
    assert_eq!(
        row["payload"]["builtinLib"],
        serde_json::json!(["toUpperCase", "map"])
    );
    // A container (`detail`) qualifies the name.
    let mut node = node_at("toUpperCase", Some("file:///x/typescript/lib/lib.es5.d.ts"));
    node["detail"] = serde_json::json!("String");
    assert_eq!(builtin_name(&node), "String.toUpperCase");
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
    let rest = cap_declaration_content(&mut location).expect("a read of the omitted lines");
    let content = location["content"].as_str().expect("content");
    let lines = content.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), MAX_DECLARATION_CONTENT_LINES + 1);
    assert_eq!(lines[0], "line 0");
    assert_eq!(lines[MAX_DECLARATION_CONTENT_LINES - 1], "line 59");
    assert_eq!(
        lines[MAX_DECLARATION_CONTENT_LINES],
        "… 140 more lines omitted (source lines 486-625); next.readDeclaration reads them."
    );
    // The omitted lines are one executable read, each line once.
    assert_eq!(rest["tool"], "localFetch");
    assert_eq!(
        rest["query"]["queries"][0],
        serde_json::json!({"path": "/repo/a.ts", "ranges": ["486-625"]})
    );
    let short = "a\nb\nc";
    let mut small =
        serde_json::json!({"content": short, "displayRange": {"startLine": 1, "endLine": 3}});
    assert!(cap_declaration_content(&mut small).is_none());
    assert_eq!(small["content"], short, "short bodies are untouched");
}

#[test]
fn hover_range_is_published_as_a_one_based_display_range() {
    let hover = public_hover(serde_json::json!({
        "contents": {"kind": "markdown", "value": "```ts\nconst a: number\n```"},
        "range": {"start": {"line": 3, "character": 8}, "end": {"line": 4, "character": 2}}
    }));
    assert!(hover.get("range").is_none(), "{hover}");
    assert_eq!(
        hover["displayRange"],
        serde_json::json!({"startLine": 4, "startCharacter": 9, "endLine": 5}),
        "{hover}"
    );
    assert_eq!(hover["contents"]["kind"], "markdown", "{hover}");
    let bare = public_hover(serde_json::json!({"contents": "text"}));
    assert_eq!(bare, serde_json::json!({"contents": "text"}));
    crate::contracts::validate_output(
        "lspSearch",
        &public_row(&serde_json::json!({
            "type": "hover",
            "uri": "file:///repo/a.ts",
            "lsp": {"serverAvailable": true, "provider": "hoverProvider"},
            "payload": {"kind": "hover", "hover": hover}
        })),
    )
    .expect("published hover satisfies the internal output contract");
}

#[test]
fn call_edges_compact_to_per_file_call_rows() {
    let caller = |path: &str,
                  name: &str,
                  kind: &str,
                  start: u64,
                  end: u64,
                  sites: &[(u64, u64)]| {
        serde_json::json!({
            "from": {"name": name, "kind": kind, "uri": format!("file://{path}"),
                "displayRange": {"startLine": start, "startCharacter": 7, "endLine": end}},
            "fromRanges": sites.iter().map(|(line, column)| serde_json::json!({"startLine": line, "startCharacter": column, "endLine": line})).collect::<Vec<_>>(),
            "level": 1
        })
    };
    let mut method = caller(
        "/repo/App.tsx",
        "renderEmbeddables",
        "method",
        1833,
        2120,
        &[(1981, 32)],
    );
    method["from"]["detail"] = serde_json::json!("App");
    let mut recovered = caller("/repo/lib.ts", "load", "function", 718, 718, &[(729, 24)]);
    recovered["source"] = serde_json::json!("recoveredFromReferences");
    // A detail that only restates the name (`fn other()`) is dropped; a
    // signature is kept only to tell apart listed nodes sharing a name; a
    // container name (`App`) is kept.
    let mut render = caller(
        "/repo/svg.ts",
        "render",
        "function",
        97,
        848,
        &[(359, 38), (402, 5)],
    );
    render["from"]["detail"] = serde_json::json!("pub fn render(\n    ctx: &Ctx,\n) -> Svg");
    let mut other = caller("/repo/svg.ts", "other", "function", 900, 950, &[(910, 3)]);
    other["from"]["detail"] = serde_json::json!("pub(crate) async fn other()");
    let mut row = serde_json::json!({"payload": {"kind": "callers", "matches": [
        render,
        method,
        recovered,
        other,
    ]}});
    compact_calls(&mut row, &[], &repo_policy());
    assert!(row["payload"].get("matches").is_none(), "{row}");
    assert_eq!(
        row["payload"]["files"],
        serde_json::json!([
            {"path": "/repo/svg.ts", "matches": [
                {"symbolName": "render", "kind": "function", "line": 97, "endLine": 848,
                    "sites": [{"line": 359, "column": 38}, {"line": 402, "column": 5}]},
                {"symbolName": "other", "kind": "function", "line": 900, "endLine": 950,
                    "sites": [{"line": 910, "column": 3}]}
            ]},
            {"path": "/repo/App.tsx", "matches": [
                {"symbolName": "renderEmbeddables", "kind": "method", "line": 1833, "endLine": 2120,
                    "sites": [{"line": 1981, "column": 32}], "detail": "App"}
            ]},
            {"path": "/repo/lib.ts", "matches": [
                {"symbolName": "load", "kind": "function", "line": 718,
                    "sites": [{"line": 729, "column": 24}], "source": "recoveredFromReferences"}
            ]}
        ]),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("compact callers satisfy the output contract");
    // Deeper and outgoing edges use the same rows: `to` names a callee (its
    // call sites are in the caller), and `via <name>@<line>` names the parent
    // node an edge below level 1 connects to.
    let via = |name: &str, path: &str, line: u64| serde_json::json!({"name": name, "uri": format!("file://{path}"), "line": line, "character": 7});
    let mut level2 = caller("/repo/b.ts", "b", "function", 5, 9, &[(6, 3)]);
    level2["level"] = serde_json::json!(2);
    level2["via"] = via("a", "/repo/a.ts", 1);
    let mut callee = caller("/repo/c.ts", "c", "function", 10, 12, &[(2, 5)]);
    callee["to"] = callee["from"].take();
    callee.as_object_mut().expect("item").remove("from");
    // Two listed nodes share a name and line: the via names its file,
    // relative to the workspace like every row path.
    let mut ambiguous = caller("/repo/sub/z.ts", "z", "function", 40, 44, &[(41, 2)]);
    ambiguous["level"] = serde_json::json!(2);
    ambiguous["via"] = via("walk", "/repo/y.ts", 281);
    let items = serde_json::json!([
        caller("/repo/a.ts", "a", "function", 1, 3, &[(2, 1)]),
        caller("/repo/x.ts", "walk", "function", 281, 290, &[(285, 1)]),
        caller("/repo/y.ts", "walk", "function", 281, 299, &[(290, 1)]),
        level2,
        ambiguous,
        callee
    ]);
    let mut deep = serde_json::json!({"payload": {"kind": "callers", "matches": items.clone()}});
    compact_calls(&mut deep, items.as_array().expect("items"), &repo_policy());
    assert_eq!(
        deep["payload"]["files"],
        serde_json::json!([
            {"path": "/repo/a.ts", "matches": [{"symbolName": "a", "kind": "function", "line": 1,
                "endLine": 3, "sites": [{"line": 2, "column": 1}]}]},
            {"path": "/repo/x.ts", "matches": [{"symbolName": "walk", "kind": "function", "line": 281,
                "endLine": 290, "sites": [{"line": 285, "column": 1}]}]},
            {"path": "/repo/y.ts", "matches": [{"symbolName": "walk", "kind": "function", "line": 281,
                "endLine": 299, "sites": [{"line": 290, "column": 1}]}]},
            {"path": "/repo/b.ts", "matches": [{"symbolName": "b", "kind": "function", "line": 5,
                "endLine": 9, "sites": [{"line": 6, "column": 3}], "via": {"symbolName": "a", "line": 1}}]},
            {"path": "/repo/sub/z.ts", "matches": [{"symbolName": "z", "kind": "function", "line": 40,
                "endLine": 44, "sites": [{"line": 41, "column": 2}],
                "via": {"symbolName": "walk", "line": 281, "path": "y.ts"}}]},
            {"path": "/repo/c.ts", "matches": [{"symbolName": "c", "kind": "function", "line": 10,
                "endLine": 12, "sites": [{"line": 2, "column": 5}]}]}
        ]),
        "{deep}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&deep))
        .expect("compact hierarchy rows satisfy the output contract");
    // Type-hierarchy items are nodes, not calls: they stay items.
    let mut types = serde_json::json!({"payload": {"kind": "supertypes", "matches": [
        {"name": "Base", "kind": "class", "uri": "file:///repo/a.ts", "level": 1}
    ]}});
    let before = types.clone();
    compact_calls(&mut types, &[], &repo_policy());
    assert_eq!(types, before);
}

/// hover reads one position: walk controls are rejected before any server
/// starts, with the operations they apply to.
#[test]
fn hover_rejects_walk_controls_with_the_operations_that_take_them() {
    for (field, value, applies) in [
        ("depth", serde_json::json!(2), "callers"),
        ("groupByFile", serde_json::json!(true), "references"),
    ] {
        let mut row = serde_json::json!({"operation":"hover","path":"/tmp/a.rs","symbolName":"main","lineHint":1});
        row[field] = value;
        let error = crate::contracts::validate("lspSearch", serde_json::json!({"queries":[row]}))
            .expect_err("hover with a walk control");
        let message = error
            .issues
            .iter()
            .map(|issue| format!("{issue:?}"))
            .collect::<String>();
        assert!(
            message.contains(&format!("{field} only applies to"))
                && message.contains(applies)
                && message.contains("remove it from hover"),
            "{message}"
        );
    }
}

/// A capped declaration body numbers its source lines; the trailing
/// omission marker is not a source line and stays unnumbered.
#[test]
fn capped_declaration_content_numbers_lines_but_not_the_marker() {
    let body = (1..=70).map(|n| format!("line {n}\n")).collect::<String>();
    let mut location = serde_json::json!({
        "uri": "file:///repo/src/lib.rs",
        "range": {"start": {"line": 9, "character": 0}, "end": {"line": 78, "character": 1}},
        "content": body,
        "displayRange": {"startLine": 10, "endLine": 79}
    });
    super::locations::cap_declaration_content(&mut location).expect("read of the rest");
    let public = public_location(location);
    let content = public["content"].as_str().expect("content");
    assert!(content.starts_with("10\tline 1\n"), "{content}");
    assert!(content.contains("\n69\tline 60\n… "), "{content}");
    assert!(
        content
            .lines()
            .last()
            .is_some_and(|marker| marker.starts_with("… ")),
        "{content}"
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
    let base = |uri: String, workspace: &str| serde_json::json!({"operation":"documentSymbols","mainGoal":"t","reasoning":"t","path":uri,"workspaceRoot":workspace,"page":2,"pageSize":1});
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
        assert_eq!(query.resolve_paths(&paths).expect("valid"), canonical);
    }
    assert_eq!(absolute.to_row(), relative.to_row());
    assert_eq!(absolute.to_row(), uri.to_row());
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
        "operation": "references", "path": uri, "symbolName": "foo", "lineHint": 3
    }));
    let column = serde_json::json!({"uri": uri, "foundAtCharacter": 5, "orderHint": 1});
    let moved = serde_json::json!({"uri": uri, "foundAtCharacter": 5, "foundAtLine": 4, "lineDeviation": 1});
    assert_eq!(public_resolved_symbol(&named, Some(column), uri), None);
    assert_eq!(
        public_resolved_symbol(&named, Some(moved), uri),
        Some(serde_json::json!({"foundAtCharacter": 5, "foundAtLine": 4, "lineDeviation": 1}))
    );
}

/// An outgoing call site lies in the caller's file, so a callee row is filed
/// under the caller and names the callee's file beside its declaration range
/// when it differs. Incoming and outgoing rows never share a file entry.
#[test]
fn outgoing_call_sites_are_filed_under_the_caller() {
    let root = node_at("main", Some("file:///repo/index.ts"));
    let mut callee = node_at("helper", Some("file:///repo/types.d.ts"));
    callee["kind"] = serde_json::json!(12);
    let outgoing = HierarchyEdge {
        node: callee,
        parent: Some(root.clone()),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 380, "character": 2}, "end": {"line": 380, "character": 8}}),
        ],
    };
    let incoming = HierarchyEdge {
        node: node_at("boot", Some("file:///repo/index.ts")),
        parent: Some(root),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 8}}),
        ],
    };
    let items = vec![
        public_edge(Expansion::IncomingCalls, &incoming, None),
        public_edge(Expansion::OutgoingCalls, &outgoing, None),
    ];
    assert!(
        items.iter().all(|item| item.get("via").is_none()),
        "level-1 edges name no parent: {items:?}"
    );
    let mut row = serde_json::json!({"payload": {"kind": "callers", "matches": items}});
    compact_calls(&mut row, &[], &repo_policy());
    let helper_line = name_line("helper") + 1;
    let boot_line = name_line("boot") + 1;
    assert_eq!(
        row["payload"]["files"],
        serde_json::json!([
            {"path": "/repo/index.ts", "matches": [{"symbolName": "boot", "kind": "function",
                "line": boot_line, "endLine": boot_line + 3, "sites": [{"line": 4, "column": 5}]}]},
            {"path": "/repo/index.ts", "matches": [{"symbolName": "helper", "kind": "function",
                "line": helper_line, "endLine": helper_line + 3, "path": "types.d.ts",
                "sites": [{"line": 381, "column": 3}]}]}
        ]),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("caller-filed callee rows satisfy the output contract");
}

/// A declaration lead runs `callers` for a callable and `references`
/// otherwise, and is withheld when no language server would start for the
/// file.
#[test]
fn verify_query_picks_the_operation_and_needs_a_server() {
    let dir = tempfile::tempdir().expect("fixture");
    let file = dir.path().join("a.ts");
    std::fs::write(&file, "export function a() {}\n").expect("file");
    let path = file.to_string_lossy().into_owned();
    let callers = verify_query(&path, "a", 1, Verify::for_kind("function")).expect("ts server");
    assert_eq!(
        callers,
        serde_json::json!({"path": path, "operation": "callers", "symbolName": "a", "lineHint": 1})
    );
    let references = verify_query(&path, "A", 3, Verify::for_kind("class")).expect("ts server");
    assert_eq!(references["operation"], "references");
    let unknown = dir.path().join("a.unknownext");
    std::fs::write(&unknown, "a\n").expect("file");
    assert_eq!(
        verify_query(&unknown.to_string_lossy(), "a", 1, Verify::Callers),
        None,
        "no server for the extension: no lead"
    );
}

/// A callers or references success offers one read of the listed sites in
/// context: one localFetch row per file (first files first), merged ±6-line
/// windows, at most the batch limit of rows.
#[test]
fn site_rows_offer_one_read_of_the_sites_in_context() {
    let mut row = serde_json::json!({"payload": {"kind": "callers", "files": [
        {"path": "/repo/a.ts", "matches": [{"symbolName": "render", "kind": "function", "line": 97,
            "endLine": 848, "sites": [{"line": 359, "column": 38}, {"line": 362, "column": 5}]}]},
        {"path": "/repo/b.ts", "matches": [
            {"symbolName": "load", "kind": "function", "line": 1, "endLine": 9, "sites": [{"line": 4, "column": 2}]},
            {"symbolName": "go", "kind": "function", "line": 30, "endLine": 50, "sites": [{"line": 40, "column": 1}]}
        ]}
    ]}});
    attach_read_lead(&mut row);
    let read = &row["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{row}");
    assert_eq!(
        read["query"]["queries"],
        serde_json::json!([
            {"path": "/repo/a.ts", "ranges": ["353-368"]},
            {"path": "/repo/b.ts", "ranges": ["1-10", "34-46"]}
        ]),
        "{row}"
    );
    let files = (0..7)
        .map(|n| serde_json::json!({"path": format!("/repo/{n}.ts"), "matches": [{"line": n + 10, "column": 1, "value": "x"}]}))
        .collect::<Vec<_>>();
    let mut wide = serde_json::json!({"payload": {"kind": "references", "files": files}});
    attach_read_lead(&mut wide);
    assert_eq!(
        wide["next"]["read"]["query"]["queries"]
            .as_array()
            .map(Vec::len),
        Some(5)
    );
    let mut empty = serde_json::json!({"status": "empty", "payload": {"kind": "empty"}});
    attach_read_lead(&mut empty);
    assert!(empty.get("next").is_none());
}

/// Two listed callers share a name: their signatures tell them apart.
#[test]
fn signatures_are_kept_only_for_callers_that_share_a_name() {
    let caller = |path: &str, detail: &str, start: u64, site: u64| {
        serde_json::json!({
            "from": {"name": "render", "kind": "function", "detail": detail, "uri": format!("file://{path}"),
                "displayRange": {"startLine": start, "startCharacter": 7, "endLine": start + 5}},
            "fromRanges": [{"startLine": site, "startCharacter": 3, "endLine": site}],
            "level": 1
        })
    };
    let mut row = serde_json::json!({"payload": {"kind": "callers", "matches": [
        caller("/repo/a.rs", "fn render(ctx: &Ctx) -> Svg", 10, 12),
        caller("/repo/b.rs", "fn render(page: &Page)", 20, 22)
    ]}});
    compact_calls(&mut row, &[], &repo_policy());
    assert_eq!(
        row["payload"]["files"],
        serde_json::json!([
            {"path": "/repo/a.rs", "matches": [{"symbolName": "render", "kind": "function", "line": 10,
                "endLine": 15, "sites": [{"line": 12, "column": 3}], "detail": "fn render(ctx: &Ctx) -> Svg"}]},
            {"path": "/repo/b.rs", "matches": [{"symbolName": "render", "kind": "function", "line": 20,
                "endLine": 25, "sites": [{"line": 22, "column": 3}], "detail": "fn render(page: &Page)"}]}
        ]),
        "{row}"
    );
}

/// Only a file that spells a rename of the symbol (`symbol as x`, or a
/// destructured `symbol: x`) is parsed for aliasing imports.
#[test]
fn only_a_spelled_rename_is_parsed_for_aliases() {
    for renamed in [
        "import { foo as bar } from './a';",
        "import {\n  foo\n    as bar,\n} from './a';",
        "const { foo: bar } = require('./a');",
        "use crate::a::foo as bar;",
        "from a import (foo as bar)",
    ] {
        assert!(super::recovery::may_rename(renamed, "foo"), "{renamed}");
    }
    for plain in [
        "import { foo } from './a';",
        "foo(1); foobar as x; x.foo::y;",
        "const food = { foo };",
    ] {
        assert!(!super::recovery::may_rename(plain, "foo"), "{plain}");
    }
}

/// tsserver reports a call in a constructor with the whole class as caller;
/// the row shows the constructor that holds the call site instead. The
/// class file is read once (policy-checked, bounded) for every class caller
/// it holds.
#[tokio::test(flavor = "current_thread")]
async fn class_callers_narrow_to_the_member_holding_the_call_site() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("canonical root");
    let file = root.join("editor.ts");
    std::fs::write(
        &file,
        "export class Editor {\n  value = 1;\n  constructor() {\n    mutate(this);\n  }\n  other() {}\n}\n",
    )
    .expect("source");
    let uri = format!("file://{}", file.display());
    let class = serde_json::json!({
        "name": "Editor", "kind": 5, "uri": uri,
        "range": {"start": {"line": 0, "character": 0}, "end": {"line": 6, "character": 1}},
        "selectionRange": {"start": {"line": 0, "character": 13}, "end": {"line": 0, "character": 19}}
    });
    let edge = |site: serde_json::Value| super::walk::HierarchyEdge {
        node: class.clone(),
        parent: None,
        level: 1,
        sites: vec![site],
    };
    let policy = crate::tools::test_support::workspace_policy(&root);
    let mut sources = super::source::SourceCache::new(&policy);
    let mut facts = super::walk::ClassFacts::default();
    let site = serde_json::json!({"start": {"line": 3, "character": 4}, "end": {"line": 3, "character": 10}});
    let narrowed = facts
        .narrow(&edge(site), &mut sources)
        .await
        .expect("constructor holds the call");
    assert_eq!(narrowed["kind"], 9, "{narrowed}");
    assert_eq!(narrowed["detail"], "Editor", "{narrowed}");
    assert_eq!(narrowed["range"]["start"]["line"], 2, "{narrowed}");
    assert_eq!(narrowed["range"]["end"]["line"], 4, "{narrowed}");
    // A site outside every member keeps the class.
    let field = serde_json::json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}});
    assert!(facts.narrow(&edge(field), &mut sources).await.is_none());
    assert_eq!(sources.reads(), 1, "one read for both class callers");
    // A class file outside the read policy is never read.
    let outside = tempfile::tempdir().expect("outside");
    let foreign = outside
        .path()
        .canonicalize()
        .expect("canonical")
        .join("x.ts");
    std::fs::write(
        &foreign,
        "export class X {\n  constructor() {\n    f();\n  }\n}\n",
    )
    .expect("foreign");
    let mut foreign_class = class.clone();
    foreign_class["uri"] = serde_json::json!(format!("file://{}", foreign.display()));
    let foreign_edge = super::walk::HierarchyEdge {
        node: foreign_class,
        parent: None,
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": 2, "character": 4}, "end": {"line": 2, "character": 5}}),
        ],
    };
    assert!(facts.narrow(&foreign_edge, &mut sources).await.is_none());
    assert_eq!(sources.reads(), 1, "the refused file was not read");
}

/// The workspace policy of the `/repo` fixtures.
fn repo_policy() -> crate::policy::path::PathPolicy {
    crate::tools::test_support::workspace_policy(std::path::Path::new("/repo"))
}

#[test]
fn declared_in_paths_are_workspace_relative() {
    let root = node_at("main", Some("file:///repo/src/deep/a.ts"));
    let mut callee = node_at("map", Some("file:///repo/node_modules/x/index.d.ts"));
    callee["kind"] = serde_json::json!(12);
    let mut outside = node_at("ext", Some("file:///elsewhere/lib.d.ts"));
    outside["kind"] = serde_json::json!(12);
    let edge = |node: serde_json::Value, line: u64| HierarchyEdge {
        node,
        parent: Some(root.clone()),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": line, "character": 2}, "end": {"line": line, "character": 8}}),
        ],
    };
    let items = vec![
        public_edge(Expansion::OutgoingCalls, &edge(callee, 10), None),
        public_edge(Expansion::OutgoingCalls, &edge(outside, 11), None),
    ];
    let mut row = serde_json::json!({"payload": {"kind": "callees", "matches": items}});
    compact_calls(&mut row, &[], &repo_policy());
    let calls = row["payload"]["files"][0]["matches"]
        .as_array()
        .expect("calls");
    let map_line = name_line("map") + 1;
    let ext_line = name_line("ext") + 1;
    assert_eq!(
        calls,
        &vec![
            serde_json::json!({"symbolName": "map", "kind": "function", "line": map_line,
                "endLine": map_line + 3, "path": "node_modules/x/index.d.ts",
                "sites": [{"line": 11, "column": 3}]}),
            serde_json::json!({"symbolName": "ext", "kind": "function", "line": ext_line,
                "endLine": ext_line + 3, "path": "/elsewhere/lib.d.ts",
                "sites": [{"line": 12, "column": 3}]}),
        ],
        "{row}"
    );
}

#[test]
fn file_level_partial_clears_when_text_files_match() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.ts"), "export function foo() {}\n").expect("a");
    std::fs::write(root.join("b.ts"), "import { foo } from './a';\nfoo();\n").expect("b");
    let scope = super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files("foo", &paths, &crate::tools::cancel::NeverCancel))
        .expect("scan");
    let q = query(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "foo", "lineHint": 1
    }));
    let row = || {
        serde_json::json!({"payload": {"kind": "references",
        "coverage": {"scope": "languageServer", "exhaustive": false}}})
    };
    // One file only the text scan sees: flagged, and the lead lists it.
    scope.answer([root.join("a.ts").to_string_lossy()]);
    let mut partial = row();
    flag_partial(&mut partial, &q, "importerScanCapped", "capped", &scope);
    assert_eq!(partial["isPartial"], true, "{partial}");
    // The importer cap is a window, not a terminal limit:
    // `next.nextImporterPage` verifies the unchecked candidates.
    assert!(partial.get("terminalLimit").is_none(), "{partial}");
    assert_eq!(
        partial["payload"]["coverage"]["textOnlyFiles"], 1,
        "{partial}"
    );
    let lead = &partial["next"]["textSearch"]["query"]["queries"];
    assert_eq!(lead.as_array().map(Vec::len), Some(1), "{partial}");
    assert_eq!(
        lead[0]["path"],
        root.join("b.ts").to_string_lossy().as_ref()
    );
    // Every file that spells the name is in the answer: the file-level
    // reason is moot.
    scope.answer([root.join("b.ts").to_string_lossy()]);
    let mut agreed = row();
    flag_partial(&mut agreed, &q, "importerScanCapped", "capped", &scope);
    assert!(agreed.get("isPartial").is_none(), "{agreed}");
    assert!(agreed.get("next").is_none(), "{agreed}");
    assert_eq!(
        agreed["payload"]["coverage"]["textOnlyFiles"], 0,
        "{agreed}"
    );
    assert!(
        agreed["payload"]["coverage"].get("reason").is_none(),
        "{agreed}"
    );
}

#[test]
fn site_level_partial_survives_matching_counts() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::write(root.join("a.py"), "def get():\n    pass\nget()\n").expect("a");
    let scope = super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.py".into()]);
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(scope.text_files("get", &paths, &crate::tools::cancel::NeverCancel))
        .expect("scan");
    scope.answer([root.join("a.py").to_string_lossy()]);
    let q = query(serde_json::json!({
        "operation": "references", "path": root.join("a.py").to_string_lossy(),
        "symbolName": "get", "lineHint": 1
    }));
    let mut row = serde_json::json!({"payload": {"kind": "references"}});
    flag_partial(&mut row, &q, "dynamicDispatch", "dynamic", &scope);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["payload"]["coverage"]["textOnlyFiles"], 0, "{row}");
    assert_eq!(row["payload"]["coverage"]["reason"], "dynamicDispatch");
    // No file to list: the lead searches the whole scope.
    assert_eq!(
        row["next"]["textSearch"]["query"]["queries"][0]["path"],
        root.to_string_lossy().as_ref()
    );
}

#[test]
fn first_pages_of_incoming_walks_reuse_responses_keyed_by_scope_fingerprint() {
    let callers = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts", "symbolName": "f", "lineHint": 1
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
        "operation": "hover", "path": "/repo/a.ts", "symbolName": "f", "lineHint": 1
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

#[test]
fn workspace_symbol_next_page_keeps_relative_root() {
    let mut row = serde_json::json!({"next": {"nextPage": {"tool": "lspSearch", "query": {"queries": [
        {"operation": "workspaceSymbol", "symbolName": "f", "workspaceRoot": "/abs/repo/packages", "page": 2}
    ]}}, "read": {"tool": "localFetch", "query": {"queries": [{"path": "/abs/repo/packages"}]}}}});
    restore_root_spelling(&mut row, Some("/abs/repo/packages"), Some("packages"));
    assert_eq!(
        row["next"]["nextPage"]["query"]["queries"][0]["workspaceRoot"],
        "packages"
    );
    assert_eq!(
        row["next"]["read"]["query"]["queries"][0]["path"],
        "/abs/repo/packages"
    );
}

#[test]
fn workspace_symbol_accepts_directory_path() {
    let (root, paths) = temp_workspace("ws-dir");
    std::fs::create_dir_all(root.join("src")).expect("src");
    let mut q = query(serde_json::json!({
        "operation": "workspaceSymbol", "symbolName": "f",
        "path": root.join("src").to_string_lossy()
    }));
    let resolved = q
        .resolve_paths(&paths)
        .expect("a directory path is the root");
    let src = root.join("src").canonicalize().expect("canonical");
    assert_eq!(resolved, src.to_string_lossy());
    assert_eq!(q.workspace_root(), Some(src.to_string_lossy().as_ref()));
    // Other operations still need a file.
    let mut refs = query(serde_json::json!({
        "operation": "references", "symbolName": "f", "lineHint": 1,
        "path": root.join("src").to_string_lossy()
    }));
    assert!(refs.resolve_paths(&paths).is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn incoming_walk_answers_level_one_before_importer_roots_join() {
    let (root, paths) = temp_workspace("walk-order");
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
    let cancel = crate::tools::cancel::NeverCancel;
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

#[test]
fn unresolved_anchor_leads_to_real_symbol_lines() {
    let q = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts",
        "symbolName": "render", "lineHint": 300
    }));
    let source = (1..=400)
        .map(|line| match line {
            40 => "  render();".to_owned(),
            312 => "export function render(scene: Scene) {".to_owned(),
            350 => "  return render(next);".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(&q, "file:///repo/a.ts", "anchorUnresolved", "x", true);
    anchor_recovery(&mut row, &q, Some(&source));
    let lead = |key: &str| row["next"][key]["query"]["queries"][0].clone();
    // The declaration line first, then the nearest uses.
    assert_eq!(lead("didYouMean")["symbolName"], "render", "{row}");
    assert_eq!(lead("didYouMean")["lineHint"], 312, "{row}");
    assert_eq!(lead("didYouMean2")["lineHint"], 350, "{row}");
    assert_eq!(lead("didYouMean3")["lineHint"], 40, "{row}");
    assert!(row["next"].get("didYouMean4").is_none(), "{row}");
    assert_eq!(row["next"]["didYouMean"]["confidence"], "high", "{row}");
}

#[test]
fn builtin_lib_and_text_only_counts_satisfy_the_output_contract() {
    let row = serde_json::json!({
        "lsp": {"serverAvailable": true},
        "payload": {
            "kind": "callees",
            "files": [{"path": "/repo/a.ts", "matches": [{"symbolName": "f", "kind": "function", "line": 10, "endLine": 12, "sites": [{"line": 2, "column": 3}]}]}],
            "builtinLib": ["String.toUpperCase", "map"],
            "coverage": {"scope": "languageServer", "exhaustive": false, "textOnlyFiles": 3, "reason": "importerScanCapped"}
        }
    });
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("builtinLib and textOnlyFiles are contract-valid");
}

/// tsserver files calls made inside a top-level callback (`describe(() =>
/// …)`, `it(…)`) under the whole module; each site is named by the
/// innermost function symbol around it, as reference-derived callers are.
#[test]
fn module_callers_name_the_innermost_enclosing_callback() {
    let range = |start: u64, end: u64| serde_json::json!({"start": {"line": start, "character": 0}, "end": {"line": end, "character": 2}});
    let module = serde_json::json!({
        "name": "\"/repo/a.test\"", "kind": 2, "uri": "file:///repo/a.test.ts",
        "range": range(0, 40), "selectionRange": range(0, 40)
    });
    let site = |line: u64| serde_json::json!({"start": {"line": line, "character": 4}, "end": {"line": line, "character": 9}});
    let edge = HierarchyEdge {
        node: module,
        parent: None,
        level: 1,
        sites: vec![site(3), site(10), site(30)],
    };
    let symbols = serde_json::json!([{
        "name": "describe('x') callback", "kind": 12, "range": range(1, 12), "selectionRange": range(1, 1),
        "children": [{"name": "it('y')   callback", "kind": 12, "range": range(2, 5), "selectionRange": range(2, 2)}]
    }]);
    let split = split_module_callers(&edge, &symbols);
    let named = split
        .iter()
        .map(|edge| {
            (
                edge.node["name"].as_str().unwrap_or_default().to_owned(),
                edge.sites.len(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        named,
        vec![
            ("it('y') callback".to_owned(), 1),
            ("describe('x') callback".to_owned(), 1),
            ("\"/repo/a.test\"".to_owned(), 1),
        ]
    );
    assert_eq!(split[0].node["uri"], "file:///repo/a.test.ts");
    // A function caller is kept as the server named it.
    let mut function = edge;
    function.node["kind"] = serde_json::json!(12);
    assert_eq!(split_module_callers(&function, &symbols).len(), 1);
}
