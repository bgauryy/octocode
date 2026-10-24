//! Importer windows without sites continue the scan instead of answering empty.

use super::*;

/// An importer window with no verified site is a step of a continuing scan,
/// not an empty answer: the request goes on to the next window, so a later
/// page always lists sites or ends the scan, and the sites past the silent
/// windows are one `next.nextImporterPage` away.
#[test]
fn importer_windows_without_sites_are_skipped_not_returned_empty() {
    if !require_node() {
        return;
    }
    for operation in ["references", "callers"] {
        let (_dir, pages) = tokio::runtime::Runtime::new()
            .expect("rt")
            .block_on(walk_across_silent_windows(operation, 48));
        // Windows: f000-f023 | n000-n023 | n024-n047 | z000.
        assert_eq!(
            pages.len(),
            2,
            "{operation}: the silent windows cost no page"
        );
        let landed = &pages[1].row;
        assert!(landed.get("status").is_none(), "{operation}: {landed}");
        assert!(
            lists_path(landed, "z000.ts"),
            "{operation}: the importer behind the silent windows is listed: {landed}"
        );
        assert!(landed.get("isPartial").is_none(), "{operation}: {landed}");
    }
}

/// One request that verifies several importer windows opens more files than
/// the client keeps open (64): each window re-synchronizes the anchor first,
/// so the identity checks of the last window still run against the
/// anchor's program. The fake server resolves a candidate's occurrence to
/// the declaration only while `a.ts` is open, as tsserver drops the
/// program a closed anchor held.
#[test]
fn importer_windows_past_the_open_document_cap_keep_the_anchor_open() {
    if !require_node() {
        return;
    }
    let tracking = IMPORTER_SERVER
        .replace(
            "function handle(msg) {\n  if (msg.method === 'exit') process.exit(0);",
            "const open = new Set();\nfunction handle(msg) {\n  if (msg.method === 'exit') process.exit(0);\n  if (msg.method === 'textDocument/didOpen') open.add(msg.params.textDocument.uri);\n  if (msg.method === 'textDocument/didClose') open.delete(msg.params.textDocument.uri);",
        )
        .replace(
            "      return [decl];\n    }\n    case 'textDocument/references'",
            "      return open.has(declUri) ? [decl] : [];\n    }\n    case 'textDocument/references'",
        );
    assert_eq!(
        tracking.matches("open.has(declUri)").count() + tracking.matches("open.add").count(),
        2,
        "both edits applied"
    );
    // Windows: f000-f023 (page 1) | n000-n023 | n024-n047 | n048-n071 |
    // z000: page 2 opens 73 candidates in one request.
    let (_dir, root, execution) =
        importer_fixture_serving(&[("f", 24), ("n", 72), ("z", 1)], &tracking);
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(walk_pages(&pool, &root, &execution, start, None));
    let last = &pages.last().expect("pages").row;
    assert!(
        lists_path(last, "z000.ts"),
        "the importer after 72 silent candidates is verified: {last}"
    );
    assert!(last.get("isPartial").is_none(), "{last}");
}

/// The first page is no exception: when its windows, up to the request cap,
/// held no site and the scan goes on, it is a page of a continuing scan
/// (exit 0 with the importer lead), never an `empty` result.
#[test]
fn a_first_page_of_silent_windows_with_more_to_scan_is_not_empty() {
    if !require_node() {
        return;
    }
    let silent = 24 * (crate::tools::lsp_search::importers::MAX_WINDOWS_PER_REQUEST + 2);
    // The operations walk separate workspaces and pools, so they run side by
    // side: each window of a silent server waits out its own settle.
    let walk = |operation: &'static str| async move {
        let (_dir, root, execution) = importer_fixture_of(&[("n", silent), ("z", 1)]);
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": operation, "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
        });
        let (pages, _) = walk_pages(&pool, &root, &execution, start, None).await;
        let first = &pages[0].row;
        assert_ne!(
            first.get("status").and_then(serde_json::Value::as_str),
            Some("empty"),
            "{operation}: {first}"
        );
        assert!(
            first["next"]["nextImporterPage"].is_object(),
            "{operation}: {first}"
        );
        assert!(
            pages
                .last()
                .is_some_and(|page| lists_path(&page.row, "z000.ts")),
            "{operation}: the walk reaches the importer past the silent windows"
        );
    };
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async { tokio::join!(walk("references"), walk("callers")) });
}

/// A request skips at most [`crate::tools::lsp_search::importers::MAX_WINDOWS_PER_REQUEST`] windows. A
/// page that ends there with no site says so plainly: it is a successful
/// page of a scan that continues (never an `empty` result), its note names
/// the candidates verified, and `next.nextImporterPage` reaches the rest.
#[test]
fn a_capped_run_of_silent_windows_is_a_page_of_a_continuing_scan() {
    if !require_node() {
        return;
    }
    let silent = 24 * (crate::tools::lsp_search::importers::MAX_WINDOWS_PER_REQUEST + 2);
    // Separate workspaces and pools: the operations walk side by side.
    let walk = |operation: &'static str| async move {
        let (_dir, pages) = walk_across_silent_windows(operation, silent).await;
        assert_eq!(pages.len(), 3, "{operation}: first, capped, landing");
        let capped = &pages[1].row;
        assert!(
            capped.get("status").is_none(),
            "{operation}: a page of a continuing scan is not an empty result: {capped}"
        );
        assert!(capped["next"]["nextImporterPage"].is_object(), "{capped}");
        assert_eq!(capped["isPartial"], true, "{capped}");
        let said = capped.to_string();
        assert!(
            said.contains("held no reference to this symbol") && said.contains("scan continues"),
            "{operation}: {capped}"
        );
        // One counter: sites are paged by `pagination`, the scan is told in
        // candidates, never as a second "page" count.
        assert!(!said.contains("importer page"), "{operation}: {capped}");
        assert!(
            pages[2].row[if operation == "references" {
                "matches"
            } else {
                "files"
            }]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("z000.ts")))),
            "{operation}"
        );
    };
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async { tokio::join!(walk("references"), walk("callers")) });
}

/// A first window with no verified importer is followed in the same
/// request: the importer in the next window lands on page 1, with no
/// importer lead to walk.
#[test]
fn a_silent_first_window_goes_on_to_the_next_in_one_call() {
    if !require_node() {
        return;
    }
    let walk = |operation: &'static str| async move {
        // Windows: n000-n023 | z000.
        let (_dir, root, execution) = importer_fixture_of(&[("n", 24), ("z", 1)]);
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": operation, "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
        });
        let (pages, _) = walk_pages(&pool, &root, &execution, start, None).await;
        assert_eq!(pages.len(), 1, "{operation}: one call");
        let first = &pages[0].row;
        assert!(lists_path(first, "z000.ts"), "{operation}: {first}");
        assert!(first["next"].get("nextImporterPage").is_none(), "{first}");
        assert!(first.get("isPartial").is_none(), "{operation}: {first}");
    };
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async { tokio::join!(walk("references"), walk("callers")) });
}

/// Candidates that import the name from another module cannot reach the
/// anchor: they are checked by their imports and never opened, so they cut
/// no importer window and count as covered, not as text-only files.
#[test]
fn candidates_importing_another_module_are_checked_without_a_window() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 1)]);
    for index in 0..40 {
        std::fs::write(
            root.join(format!("m{index:03}.ts")),
            "import { target } from './other';\ntarget();\n",
        )
        .expect("lookalike");
    }
    std::fs::write(
        root.join("other.ts"),
        "export function other() {}\nexport function target() {}\n",
    )
    .expect("other");
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(walk_pages(&pool, &root, &execution, start, None));
    assert_eq!(pages.len(), 1);
    let row = &pages[0].row;
    assert!(lists_path(row, "f000.ts"), "{row}");
    assert!(!lists_path(row, "m000.ts"), "{row}");
    assert!(row["next"].get("nextImporterPage").is_none(), "{row}");
    assert!(row.get("isPartial").is_none(), "{row}");
    assert_eq!(row["coverage"]["importerScan"], "complete", "{row}");
    assert!(
        row["coverage"]
            .get("textOnlyFiles")
            .is_none_or(|count| count == 0),
        "{row}"
    );
}
