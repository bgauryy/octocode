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
use octocode_contract_types::ErrorCode;

mod fixtures;
mod hierarchy;
mod rows;
mod sources;

use fixtures::*;

/// A tool row as the public response carries it: leads and prose tips in
/// `hints`, pages in `next`.
/// A raw lspSearch row as the response layer publishes it: `status` and
/// other row-level fields lifted out of `data` (see `response::rows`).
fn public_row(data: &serde_json::Value) -> serde_json::Value {
    let row = crate::response::rows::result_row(
        crate::id::ToolId::LspSearch,
        0,
        &serde_json::json!({}),
        data.clone(),
        data.get("status").and_then(serde_json::Value::as_str),
    );
    let mut out = serde_json::json!({ "results": [row] });
    crate::response::continuations::finalize(
        &mut out,
        crate::id::ToolId::LspSearch,
        &crate::response::continuations::Sources::Rows(&[]),
        &crate::response::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    crate::response::channels::publish_hints(&mut out, crate::id::ToolId::LspSearch);
    out
}

/// Client options for fake servers, which answer at once: no settle wait.
pub(super) const FAKE_SERVER_CLIENT: octocode_engine::lsp::client::LspClientOptions =
    octocode_engine::lsp::client::LspClientOptions {
        settle_cap: Some(std::time::Duration::ZERO),
    };

/// A pool whose clients take [`FAKE_SERVER_CLIENT`].
pub(super) fn fake_server_pool() -> octocode_engine::lsp::pool::LspClientPool {
    octocode_engine::lsp::pool::LspClientPool::new(octocode_engine::lsp::pool::LspPoolOptions {
        client: FAKE_SERVER_CLIENT,
        ..octocode_engine::lsp::pool::LspPoolOptions::default()
    })
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

/// A count-cut page of `items`: the cut an explicit `pageSize` asks for.
fn counted_page(items: &[Value], page: u32, size: u32) -> (Vec<Value>, Value, Option<NextPage>) {
    paginate(
        items,
        page,
        &PageCut {
            page_size: Some(size),
            budget: TEST_BUDGET,
            sizes: &plain_sizes(items),
            snapshot: "lsp-v3:test",
            resume: None,
        },
    )
}

/// A page budget large enough that only an explicit count cuts a test page.
const TEST_BUDGET: usize = 16_000;

#[test]
fn pagination_omits_nullable_next_page_at_the_terminal_page() {
    let items = vec![serde_json::json!({"name": "one"})];
    let (_, terminal, next) = counted_page(&items, 1, 20);
    assert!(next.is_none());
    assert_eq!(terminal["hasMore"], false);
    assert!(terminal.get("nextPage").is_none());

    let items = vec![
        serde_json::json!({"name": "one"}),
        serde_json::json!({"name": "two"}),
    ];
    let (_, partial, next) = counted_page(&items, 1, 1);
    // The cursor rides the next page's lead, never the pagination block.
    assert!(partial.get("nextPage").is_none() && partial.get("snapshot").is_none());
    let next = next.expect("a page follows");
    assert_eq!((next.page, next.snapshot.as_str()), (2, "lsp-v3:test"));
}

#[test]
fn semantic_pagination_continuations_cover_the_full_result_fixture() {
    let expected = (0..7)
        .map(|index| {
            serde_json::json!({"symbolName": format!("symbol-{index}"), "path": "/repo/src/lib.rs"})
        })
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

    let walk = crate::tools::test_support::checks::walk(
        "lspSearch",
        serde_json::json!({"queries": [lsp_search_query::to_row(&query)]}),
        |call| {
            let query: LspSearchQuery = serde_json::from_value(call["queries"][0].clone())
                .expect("executable continuation query");
            assert_eq!(lsp_search_query::snapshot(&query), Some(snapshot.as_str()));
            let (page, pagination, next_page) = counted_page(
                &expected,
                lsp_search_query::page(&query).unwrap_or(1),
                lsp_search_query::page_size(&query).expect("explicit pageSize"),
            );
            let mut response = serde_json::json!({"matches": page, "pagination": pagination});
            // The fixture's pages are cut under its own digest.
            let next_page = next_page.map(|next| NextPage {
                snapshot: snapshot.clone(),
                ..next
            });
            offer_next_page(&query, &mut response, next_page);
            if response["pagination"]["hasMore"] != true {
                assert!(response.get("next").is_none());
            } else {
                assert_eq!(response["next"]["nextPage"]["tool"], "lspSearch");
            }
            let rows = page
                .iter()
                .map(|item| item["symbolName"].as_str().expect("name").to_owned())
                .collect();
            actual.extend(page);
            let row = response;
            crate::tools::test_support::checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::LspSearch,
                    &lsp_search_query::to_row(&query),
                    &row,
                ),
                rows,
            }
        },
    );

    assert_eq!(walk.queries.len(), 3);
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
fn later_pages_check_a_pinned_snapshot_and_carry_it_forward() {
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
    // A page without a snapshot pins nothing: it pages the current answer.
    assert!(!snapshot_mismatch(&query, &snapshot));
    let pinned = requery(&query, serde_json::json!({"snapshot": "lsp-v3:stale"}));
    assert!(snapshot_mismatch(&pinned, &snapshot));
    let changed = snapshot_changed(&query);
    assert_eq!(changed["errorCode"], "staleSnapshot");
    let restart = &changed["next"]["restart"]["query"]["queries"][0];
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(restart.get(cursor).is_none(), "{cursor}: {changed}");
    }

    let continued_query = requery(&query, serde_json::json!({"snapshot": snapshot}));
    let mut continued = serde_json::json!({"pagination": {"hasMore": true}});
    offer_next_page(
        &continued_query,
        &mut continued,
        Some(NextPage {
            page: 3,
            snapshot: snapshot.clone(),
        }),
    );
    assert_eq!(
        continued["next"]["nextPage"]["query"]["queries"][0]["page"],
        3
    );
    assert!(continued["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string());

    // The last page a continuation may name (the contract `page` maximum)
    // keeps its rows and states the terminal limit instead of a lead past it.
    let mut last = serde_json::json!({
        "locations": [{"uri": "file:///repo/a.ts"}],
        "pagination": {"currentPage": 1000, "hasMore": true}
    });
    offer_next_page(
        &requery(
            &query,
            serde_json::json!({"snapshot": snapshot, "page": 1000}),
        ),
        &mut last,
        Some(NextPage {
            page: 1001,
            snapshot: snapshot.clone(),
        }),
    );
    assert!(last.pointer("/next/nextPage").is_none(), "{last}");
    assert_eq!(last["terminalLimit"], true, "{last}");
    assert_eq!(
        last["partialReasons"],
        serde_json::json!([crate::response::paging::CURSOR_MAXIMUM])
    );
    assert!(last["locations"].is_array(), "{last}");
}

/// A later page is addressed by its snapshot or a fixed pageSize, never by
/// a page number alone (whose rows would depend on the batch's window).
#[test]
fn a_later_page_needs_its_snapshot_or_a_page_size() {
    let bare = query(serde_json::json!({
        "operation": "references", "path": "/repo/a.rs", "symbolName": "f", "line": 1, "page": 2
    }));
    assert!(super::unpinned_later_page(&bare));
    assert!(!super::unpinned_later_page(&requery(
        &bare,
        serde_json::json!({"snapshot": "lsp-v3:x"})
    )));
    assert!(!super::unpinned_later_page(&requery(
        &bare,
        serde_json::json!({"pageSize": 10})
    )));
    assert!(!super::unpinned_later_page(&requery(
        &bare,
        serde_json::json!({"page": 1})
    )));
}

/// The refused later page leads back to the walk's first page, whose
/// `next.nextPage` carries the snapshot: no server starts, and the lead is
/// the same query without any cursor.
#[tokio::test(flavor = "current_thread")]
async fn an_unpinned_later_page_leads_to_the_restart() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.rs"), "fn f() {}\n").expect("file");
    let row = serde_json::json!({
        "operation": "references", "path": root.join("a.rs").to_string_lossy(),
        "symbolName": "f", "line": 1, "page": 2
    });
    let result = super::execute(
        query(row.clone()),
        &crate::execution::cancel::NeverCancel,
        &fake_server_pool(),
        &paths,
        &super::LspExecutionConfig::default(),
        None,
    )
    .await
    .expect("row");
    assert_eq!(result["errorCode"], "invalidInput", "{result}");
    let restart = &result["next"]["restart"];
    assert_eq!(restart["tool"], "lspSearch", "{result}");
    assert!(
        restart["query"]["queries"][0].get("page").is_none(),
        "{result}"
    );
    let mut expected = row;
    expected.as_object_mut().expect("row").remove("page");
    let restarted = query(restart["query"]["queries"][0].clone());
    assert_eq!(
        lsp_search_query::to_row(&restarted),
        lsp_search_query::to_row(&query(expected)),
        "{result}"
    );
    crate::tools::test_support::assert_row_contract(
        crate::id::ToolId::LspSearch,
        &serde_json::json!({
            "operation": "references", "path": root.join("a.rs").to_string_lossy(),
            "symbolName": "f", "line": 1, "page": 2
        }),
        &result,
    );
}

/// A user server for `extension` launched by `command`, through a config
/// file written under `dir`.
#[cfg(unix)]
fn user_server(dir: &std::path::Path, extension: &str, command: &str) -> super::LspExecutionConfig {
    let config = dir.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {extension: {
            "command": command, "args": [], "languageId": "plaintext"
        }}})
        .to_string(),
    )
    .expect("config");
    super::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..super::LspExecutionConfig::default()
    }
}

/// An executable stand-in server: never started by these tests.
#[cfg(unix)]
fn executable(path: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, "#!/bin/sh\n").expect("server");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
async fn run_row(
    row: serde_json::Value,
    paths: &crate::policy::path::PathPolicy,
    execution: &super::LspExecutionConfig,
) -> serde_json::Value {
    super::execute(
        query(row),
        &crate::execution::cancel::NeverCancel,
        &fake_server_pool(),
        paths,
        execution,
        None,
    )
    .await
    .expect("row")
}

/// An anchor that resolves nowhere starts no server; the row still says
/// whether the configured server would launch.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn an_unresolved_anchor_reports_whether_its_server_would_launch() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "const a = 1;\n").expect("file");
    let installed = executable(&root.join("server"));
    let missing = root.join("missing-server").to_string_lossy().into_owned();
    for (command, available) in [(installed, true), (missing, false)] {
        let row = serde_json::json!({
            "operation": "hover", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "absent", "line": 1
        });
        let result = run_row(row, &paths, &user_server(&root, ".ts", &command)).await;
        assert_eq!(result["errorCode"], "anchorUnresolved", "{result}");
        assert_eq!(
            result["lsp"]["serverAvailable"], available,
            "{command}: {result}"
        );
    }
}

/// From a member crate, the Cargo workspace inferred for a file lies above
/// the allowed roots. With no workspaceRoot asked for, the server runs at
/// the allowed root and the row says so; an explicit root outside the
/// allowed roots still fails.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn an_inferred_root_above_the_allowed_roots_falls_back_with_a_warning() {
    let (_dir, root, _) = temp_workspace();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/a\"]\n",
    )
    .expect("workspace");
    let krate = root.join("crates/a");
    std::fs::create_dir_all(krate.join("src")).expect("crate");
    std::fs::write(krate.join("Cargo.toml"), "[package]\nname = \"a\"\n").expect("package");
    let file = krate.join("src/lib.rs");
    std::fs::write(&file, "fn f() {}\n").expect("source");
    let crates = root.join("crates");
    let paths = crate::tools::test_support::workspace_policy(&crates);
    let execution = user_server(&crates, ".rs", &executable(&crates.join("server")));
    let row = serde_json::json!({
        "operation": "hover", "path": file.to_string_lossy(),
        "symbolName": "absent", "line": 1
    });
    let inferred = run_row(row.clone(), &paths, &execution).await;
    assert_eq!(inferred["errorCode"], "anchorUnresolved", "{inferred}");
    let warning = inferred["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.contains("outside the allowed roots") && warning.contains("runs at ."),
        "{inferred}"
    );

    let mut explicit = row;
    explicit["workspaceRoot"] = serde_json::json!(root.to_string_lossy());
    let refused = run_row(explicit, &paths, &execution).await;
    assert_eq!(refused["errorCode"], "workspaceRootInvalid", "{refused}");
}

#[test]
fn semantic_snapshot_ignores_paging_and_workflow_metadata() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "mainGoal": "test", "reasoning": "Find every reference.",
        "extras": ["receipts"],
        "path": "/repo/src/lib.rs",
        "symbolName": "run",
        "line": 4,
        "pageSize": 1
    }))
    .expect("first page query");
    let continued: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "mainGoal": "test", "reasoning": "Continuation metadata may be normalized.",
        "path": "/repo/src/lib.rs",
        "symbolName": "run",
        "line": 4,
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

/// A fallback names the capability it actually used: the receipt keeps it
/// instead of the operation's own provider, which never answered.
#[test]
fn the_receipt_keeps_the_provider_a_fallback_answered_with() {
    let mut fallback = serde_json::json!({"provider": "implementationProvider"})
        .as_object()
        .cloned()
        .expect("object");
    settle_provider(&mut fallback, "subtypes");
    assert_eq!(fallback["provider"], "implementationProvider");
    let mut bare = serde_json::Map::new();
    settle_provider(&mut bare, "subtypes");
    assert_eq!(bare["provider"], "typeHierarchyProvider");
}

/// A resolved anchor with no subtypes says so, instead of asking the caller
/// to verify an anchor that was found.
#[test]
fn an_empty_hierarchy_row_says_the_relation_is_empty() {
    for (operation, wording) in [
        ("subtypes", "extends or implements"),
        ("supertypes", "base"),
    ] {
        let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
            "operation": operation, "path": "src/a.ts", "symbolName": "A", "line": 1
        }))
        .expect("hierarchy query");
        let row = empty(&query, "noLocations", "no results", true);
        let hint = row["hints"][0].as_str().expect("hint");
        assert!(hint.contains(wording), "{operation}: {hint}");
        assert!(!hint.contains("Verify the symbol"), "{hint}");
        assert!(hint.chars().count() <= 120, "{hint}");
    }
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": "src/a.ts", "symbolName": "A", "line": 1
    }))
    .expect("references query");
    let row = empty(&query, "noLocations", "no results", true);
    assert!(
        row["hints"][0]
            .as_str()
            .expect("hint")
            .contains("Verify the symbol")
    );
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
        // is the name line (`selectionRange`), a usable line.
        symbol("documented", 12, (10, 14), (12, 7)),
        implementation,
        outer,
        // Two symbols on one line carry their 1-based column.
        symbol("a", 14, (32, 32), (32, 6)),
        symbol("b", 14, (32, 32), (32, 13)),
    ]);

    let envelope = items_payload(&query, "documentSymbols", raw.clone(), TEST_BUDGET);
    assert_eq!(envelope["lsp"]["source"], "lsp");
    assert!(envelope.get("summary").is_none(), "{envelope}");
    // Outline rows, checked through their text outline.
    let mut payload = serde_json::json!({"kind": envelope["kind"], "symbols": envelope["symbols"]});
    payload["symbols"] = serde_json::json!(crate::response::symbol_outline::outline_rows(
        &crate::response::symbol_outline::flatten_members(
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
    // Every row is an object, and pages count the six top-level rows: a
    // one-row page keeps a container's members with it.
    assert!(
        envelope["symbols"]
            .as_array()
            .expect("rows")
            .iter()
            .all(|row| row.is_object()
                && row["members"]
                    .as_array()
                    .is_none_or(|members| members.iter().all(serde_json::Value::is_object))),
        "{envelope}"
    );
    assert_eq!(envelope["pagination"]["totalItems"], 6, "{envelope}");
    let paged: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": "file:///repo/src/lib.rs", "pageSize": 1
    }))
    .expect("paged query");
    let first = items_payload(&paged, "documentSymbols", raw.clone(), TEST_BUDGET);
    assert_eq!(first["pagination"]["totalItems"], 6, "{first}");
    assert_eq!(first["pagination"]["totalPages"], 6, "{first}");
    assert_eq!(
        first["symbols"],
        serde_json::json!([envelope["symbols"][0].clone()]),
        "{first}"
    );
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
}

#[test]
fn provider_receipt_exposes_effective_invocation_and_omits_empty_optional_fields() {
    let config = octocode_engine::lsp::types::LspLanguageServerConfig {
        command: "rust-analyzer".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: "/repo".into(),
        language_id: Some("rust".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
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

    let config_without_args = octocode_engine::lsp::types::LspLanguageServerConfig {
        args: Some(Vec::new()),
        ..config
    };
    let client = octocode_engine::lsp::client::NativeLspClient::new(config_without_args.clone());
    let receipt = resolved_server_receipt(&config_without_args, &client);
    assert!(receipt.get("argv").is_none());

    // The source is discovery's, not a guess from the argv: a user entry
    // whose program sits in node_modules is still the user's, and a
    // bundled server whose argv names no node_modules is still bundled.
    for (source, argv, expected) in [
        (
            octocode_engine::lsp::types::ServerSource::User,
            "/x/node_modules/typescript-language-server/lib/cli.mjs",
            "user",
        ),
        (
            octocode_engine::lsp::types::ServerSource::Bundled,
            "--lsp",
            "bundled",
        ),
    ] {
        let config = octocode_engine::lsp::types::LspLanguageServerConfig {
            args: Some(vec![argv.into()]),
            source,
            ..config_without_args.clone()
        };
        let client = octocode_engine::lsp::client::NativeLspClient::new(config.clone());
        assert_eq!(
            resolved_server_receipt(&config, &client)["source"],
            expected
        );
    }
}

#[test]
fn empty_and_unavailable_rows_expose_status_and_recovery_next() {
    let query = query(serde_json::json!({
        "operation": "definition", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
        "symbolName": "execute",
        "line": 10
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
    // A package that includes no unloaded source: no tip.
    assert_eq!(empty["hints"].as_array().map(Vec::len), Some(1), "{empty}");
    // A package that includes build-script output: a name may be generated
    // by it (off by default).
    let package = tempfile::tempdir().expect("package");
    std::fs::create_dir_all(package.path().join("src")).expect("src");
    std::fs::write(
        package.path().join("Cargo.toml"),
        "[package]\nname = \"p\"\n",
    )
    .expect("manifest");
    std::fs::write(package.path().join("build.rs"), "fn main() {}\n").expect("build.rs");
    let unused_script = super::failure::empty(
        &requery(
            &query,
            serde_json::json!({"path": package.path().join("src/lib.rs").to_string_lossy()}),
        ),
        "noLocations",
        "none",
        true,
    );
    assert_eq!(
        unused_script["hints"].as_array().map(Vec::len),
        Some(1),
        "a build script whose output nothing includes: {unused_script}"
    );
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\n",
    )
    .expect("lib");
    let scripted = requery(
        &query,
        serde_json::json!({"path": package.path().join("src/lib.rs").to_string_lossy()}),
    );
    let tipped = super::failure::empty(&scripted, "noLocations", "none", true);
    assert!(
        tipped["hints"][1]
            .as_str()
            .is_some_and(|hint| hint.contains("rustContext {buildScripts:true}")),
        "{tipped}"
    );
    let built = requery(
        &scripted,
        serde_json::json!({"rustContext": {"buildScripts": true}}),
    );
    let built = super::failure::empty(&built, "noLocations", "none", true);
    assert_eq!(built["hints"].as_array().map(Vec::len), Some(1), "{built}");
    // Source included from outside the package never loads: its own tip,
    // and no rerun.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let outside = super::failure::empty(&scripted, "noLocations", "none", true);
    assert!(
        outside["hints"][1]
            .as_str()
            .is_some_and(|hint| hint.contains("outside its directory")),
        "{outside}"
    );
    assert!(
        outside["next"].get("includeBuildScripts").is_none(),
        "{outside}"
    );
    // Both: both tips, and no ~2 GB rerun the outside include may defeat.
    std::fs::write(
        package.path().join("src/lib.rs"),
        "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"../../elsewhere/types.rs\");\n",
    )
    .expect("lib");
    let both = super::failure::empty(&scripted, "noLocations", "none", true);
    assert_eq!(both["hints"].as_array().map(Vec::len), Some(3), "{both}");
    assert!(both["next"].get("includeBuildScripts").is_none(), "{both}");
    let down = failure(
        &query,
        "file:///repo/src/lib.rs",
        ErrorCode::ServerUnavailable,
        "missing",
        false,
    );
    assert_eq!(down["status"], "error");
    assert_eq!(down["errorCode"], "serverUnavailable");
    assert_eq!(down["next"]["read"]["tool"], "localFetch");
    // The fallback is an executable lexical lead, not a static tip.
    let text = &down["next"]["findText"];
    assert_eq!(text["tool"], "localSearch", "{down}");
    let search = &text["query"]["queries"][0];
    assert_eq!(search["path"], "/repo/src", "{down}");
    assert_eq!(search["matchString"], "\\bexecute\\b", "{down}");
    assert_eq!(search["regex"], "rust", "{down}");
    assert_eq!(search["caseMode"], "sensitive", "{down}");
    assert!(
        down["hints"].as_array().is_some_and(Vec::is_empty),
        "{down}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&down))
        .expect("unavailable row satisfies the output contract");

    let timeout = failure(
        &query,
        "file:///repo/src/lib.rs",
        ErrorCode::Timeout,
        "timed out",
        false,
    );
    assert_eq!(timeout["next"]["retry"]["tool"], "lspSearch");
    assert_eq!(timeout["next"]["read"]["tool"], "localFetch");

    let explicitly_scoped = requery(&query, serde_json::json!({"workspaceRoot": "/repo"}));
    let scoped = failure(
        &explicitly_scoped,
        "file:///repo/src/lib.rs",
        ErrorCode::CapabilityUnavailable,
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
        ErrorCode::ServerUnavailable,
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
    let row = with_next(
        &q,
        items_payload(&q, "symbols", serde_json::json!([]), TEST_BUDGET),
    );
    assert_eq!(row["status"], "empty");
    assert!(row.get("next").is_none(), "{row}");
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
    let mut row = lsp_search_query::to_row(query);
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
    let envelope = items_payload(&q, "diagnostics", items, TEST_BUDGET);
    let listed = envelope["matches"].as_array().expect("matches");
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
        let envelope = with_next(&q, items_payload(&q, "diagnostics", items, TEST_BUDGET));
        assert_eq!(envelope["status"], "empty", "{envelope}");
        assert_eq!(envelope["category"], "noDiagnostics");
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
    let (page, pagination, _) = counted_page(&items, 5, 1);
    assert!(page.is_empty(), "must not clamp to the last page");
    assert_eq!(pagination["currentPage"], 5);
    assert_eq!(pagination["totalPages"], 2);
    assert_eq!(pagination["hasMore"], false);
    assert_eq!(pagination["outOfRange"], true);
    assert!(pagination.get("nextPage").is_none());

    let (page, pagination, _) = counted_page(&items, 2, 1);
    assert_eq!(page.len(), 1);
    assert!(pagination.get("outOfRange").is_none());
}

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
    let root_dir = crate::tools::test_support::Fixture::new();
    let root = root_dir.0.canonicalize().expect("canonical root");
    std::fs::write(root.join("a.ts"), "foo\nfoo\n").expect("a.ts");
    let paths = crate::tools::test_support::workspace_policy(&root);
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

/// A server that pushed nothing inside the wait has not said "no
/// diagnostics": the empty row is partial, with a retry, never a clean result.
#[test]
fn unpublished_diagnostics_are_partial_not_clean() {
    let q = query(serde_json::json!({
        "operation": "diagnostic", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts"
    }));
    let mut row = items_payload(&q, "diagnostics", serde_json::json!([]), TEST_BUDGET);
    mark_diagnostics_unpublished(&mut row, &q, 5_000);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["diagnosticsNotPublished"]),
        "{row}"
    );
    assert_eq!(row["next"]["retry"]["tool"], "lspSearch", "{row}");
    assert!(
        row["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("5000 ms")),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("an unpublished-diagnostics row satisfies the output contract");
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
    let scope = super::scope::Scope::new("/repo".into(), vec!["*.ts".into()]);
    super::recovery::disclose_alias_cap(&mut row, &q, &scope);
    assert_eq!(row["coverage"]["aliasScan"], "capped", "{row}");
    assert_eq!(row["isPartial"], true);
    assert_eq!(
        row["partialReasons"],
        serde_json::json!([super::recovery::ALIAS_SCAN_CAPPED_REASON])
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

#[test]
fn paged_rows_state_page_facts_once_in_next() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts", "symbolName": "foo", "line": 1, "pageSize": 3
    }));
    let items = (0..7)
        .map(|index| serde_json::json!({"name": format!("s{index}")}))
        .collect::<Vec<_>>();
    let row = items_payload(&q, "symbols", serde_json::json!(items), TEST_BUDGET);
    // The continuation carries the page and the snapshot; the row's
    // pagination keeps the page facts on every page, its size included, and
    // no cursor.
    let call = &row["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(call["page"], 2, "{row}");
    assert!(call["snapshot"].is_string(), "{row}");
    assert_eq!(
        row["pagination"],
        serde_json::json!({"currentPage": 1, "totalPages": 3, "totalItems": 7, "hasMore": true, "pageSize": 3}),
        "{row}"
    );
}

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
        let mixed = walk(&paths, &query, &found, &[12_000, 5_000, window]).await;
        assert!(
            mixed.len() >= 3,
            "grow/shrink must preserve the pinned row cursor"
        );
        assert_eq!(identities(&mixed), expected);
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
