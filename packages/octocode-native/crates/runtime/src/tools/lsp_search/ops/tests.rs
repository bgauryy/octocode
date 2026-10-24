use super::{binding_rank, language_name, match_tier, rank_workspace_symbols};
use octocode_contract_types::LspSearchQuery;

fn snippet(path: &str, start: u32, end: u32) -> octocode_engine::lsp::types::LspCodeSnippet {
    use octocode_engine::lsp::types::{LspExactPosition, LspRange};
    let at = |line| LspExactPosition { line, character: 0 };
    octocode_engine::lsp::types::LspCodeSnippet {
        uri: format!("file://{path}"),
        range: LspRange {
            start: at(start),
            end: at(end),
        },
        content: String::new(),
        symbol_kind: None,
        display_range: None,
    }
}

/// The heritage scan beside a server answer adds only declarations the
/// answer does not cover: a server location spanning the scan row's
/// line (at its name, or over its whole declaration) is the same row.
#[test]
fn the_scan_adds_only_the_declarations_the_server_left_out() {
    let found = [snippet("/r/a.ts", 3, 3), snippet("/r/b.ts", 10, 20)];
    let scan = super::heritage::HeritageScan {
        base: "Base".into(),
        files: 3,
        found: vec![
            snippet("/r/a.ts", 3, 3),
            snippet("/r/b.ts", 12, 12),
            snippet("/r/b.ts", 25, 25),
            snippet("/r/c.ts", 3, 3),
        ],
        unread: Vec::new(),
    };
    let (extra, base) = super::scan_only(&found, scan);
    let left = extra
        .iter()
        .map(|row| {
            (
                row.uri.rsplit('/').next().unwrap_or_default(),
                row.range.start.line,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(left, [("b.ts", 25), ("c.ts", 3)]);
    assert_eq!(base, "Base");
}

/// Every project-marker family a workspace-root search names takes its
/// language id from the grammar registry.
#[test]
fn root_marker_families_are_named_by_language_id() {
    let named = [".ts", ".rs", ".go", ".py"].map(language_name);
    assert_eq!(named, ["typescript", "rust", "go", "python"]);
    assert_eq!(language_name(".unknown-ext"), "unknown");
}

/// A heritage answer missing files it could not read says which,
/// stays partial for them, and leads to a text search over exactly them.
#[test]
fn a_scan_answer_names_its_unread_files_and_leads_to_them() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "path": "/r/a.ts", "symbolName": "Base", "line": 1, "operation": "subtypes"
    }))
    .expect("query");
    let scope = super::Scope::new("/r".into(), Vec::new());
    let paths = crate::tools::test_support::workspace_policy(std::path::Path::new("/r"));
    let unread = vec!["/r/big.ts".to_owned()];
    let mut row = serde_json::json!({});
    super::mark_scan_answer(
        &mut row,
        &query,
        &scope,
        &paths,
        "scan note".into(),
        &unread,
    );
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["serverUnanswered", "heritageUnread"])
    );
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("big.ts") && !warnings.contains("/r/"),
        "{warnings}"
    );
    let lead = &crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(lead["tool"], "localSearch", "{row}");
    // The warning and the lead name the file the same way.
    assert_eq!(lead["input"]["queries"][0]["path"], "big.ts", "{row}");

    let mut complete = serde_json::json!({});
    super::mark_scan_answer(
        &mut complete,
        &query,
        &scope,
        &paths,
        "scan note".into(),
        &[],
    );
    assert_eq!(
        complete["partialReasons"],
        serde_json::json!(["serverUnanswered"])
    );
    assert!(
        !complete["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "localSearch")
    );
}

/// The published rows (`symbolName`, not the server's `name`) rank
/// the exact name ahead of rust-analyzer's alphabetical fuzzy matches.
#[test]
fn public_workspace_symbols_rank_the_exact_name_first() {
    let raw = |name: &str| {
        serde_json::json!({"name": name, "kind": 12,
            "location": {"uri": "file:///repo/a.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}}})
    };
    let mut rows = ["a_keeps_alive", "b_alive_pool", "is_alive_now", "is_alive"]
        .map(|name| super::super::locations::public_workspace_symbol(&raw(name)));
    rank_workspace_symbols(&mut rows, "is_alive");
    assert_eq!(rows[0]["symbolName"], "is_alive", "{rows:?}");
    assert_eq!(rows[1]["symbolName"], "is_alive_now", "{rows:?}");
}

#[test]
fn workspace_symbols_rank_declarations_before_same_name_bindings() {
    let mut rows = [
        ("Scene", "variable"),
        ("scene", "property"),
        ("Scene", "class"),
    ];
    rows.sort_by_key(|(name, kind)| (match_tier(name, "Scene"), binding_rank(kind)));
    assert_eq!(rows[0], ("Scene", "class"));
    assert_eq!(rows[1], ("Scene", "variable"));
}

#[test]
fn workspace_symbols_rank_exact_before_fuzzy() {
    let mut names = vec![
        "a_keeps_alive",
        "process_is_alive",
        "is_alive_now",
        "IS_ALIVE",
        "is_alive",
    ];
    names.sort_by_key(|name| match_tier(name, "is_alive"));
    assert_eq!(
        names,
        vec![
            "is_alive",
            "IS_ALIVE",
            "is_alive_now",
            "process_is_alive",
            "a_keeps_alive"
        ]
    );
}
