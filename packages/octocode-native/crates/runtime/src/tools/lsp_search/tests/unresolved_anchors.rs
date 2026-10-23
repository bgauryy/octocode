//! Unresolved anchors: near names, real symbol lines and leads that
//! never point back at the failing query.

use super::*;

#[test]
fn unresolved_symbol_anchor_suggests_near_names_or_locates_the_name_elsewhere() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/lib.rs",
        "symbolName": "is_invalid_input_codeX",
        "line": 12
    }));
    let source = (1..=20)
        .map(|line| match line {
            12 => "pub fn is_invalid_input_code(code: &str) -> bool {".to_owned(),
            13 => "    is_input(code)".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(
        &q,
        "file:///repo/lib.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(&source));
    // A resolved candidate is the recovery: no read beside it, and no
    // static prose.
    assert!(row["next"].get("read").is_none(), "{row}");
    // The lead names the one near name; no sentence repeats it.
    assert!(row.get("hints").is_none(), "{row}");
    // A name the file never spells is declared elsewhere: the recovery
    // locates it in the workspace (whose results carry the line to retry
    // with), not a read of lines that cannot hold it, wherever `line` was.
    let unrelated = (1..=20)
        .map(|line| format!("// line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let past = requery(&q, serde_json::json!({"line": 99_999_999}));
    for asked in [&q, &past] {
        let mut blank = failure(
            asked,
            "file:///repo/lib.rs",
            ErrorCode::AnchorUnresolved,
            "x",
            true,
        );
        anchor_recovery(&mut blank, asked, Some(&unrelated));
        assert!(blank["next"].get("read").is_none(), "{blank}");
        let locate = &blank["next"]["findSimilar"];
        assert_eq!(locate["tool"], "lspSearch", "{blank}");
        let search = &locate["query"]["queries"][0];
        assert_eq!(search["operation"], "workspaceSymbol", "{blank}");
        assert_eq!(search["symbolName"], "is_invalid_input_codeX", "{blank}");
        assert_eq!(search["path"], "/repo/lib.rs", "{blank}");
        assert!(
            blank
                .get("hints")
                .is_none_or(|hints| hints == &serde_json::json!([])),
            "{blank}"
        );
    }
    let retry = &row["next"]["findSimilar"];
    assert_eq!(retry["tool"], "lspSearch", "{row}");
    assert_eq!(
        retry["query"]["queries"][0]["symbolName"], "is_invalid_input_code",
        "{row}"
    );
    assert_eq!(retry["query"]["queries"][0]["line"], 12, "{row}");
}

#[test]
fn unresolved_anchor_leads_to_real_symbol_lines() {
    let q = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts",
        "symbolName": "render", "line": 300
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
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(&source));
    let lead = |key: &str| row["next"][key]["query"]["queries"][0].clone();
    // The declaration line first, then the nearest uses.
    assert_eq!(lead("findSimilar")["symbolName"], "render", "{row}");
    assert_eq!(lead("findSimilar")["line"], 312, "{row}");
    assert_eq!(lead("findSimilar2")["line"], 350, "{row}");
    // Every candidate fits the tool's batch limit and gets a runnable suggestion.
    assert_eq!(lead("findSimilar3")["line"], 40, "{row}");
    assert!(
        row["next"]["findSimilar"].get("confidence").is_none(),
        "{row}"
    );
    let hints = row["hints"].to_string();
    assert!(!hints.contains("`render` appears on line 40."), "{row}");
    assert!(!hints.contains("312") && !hints.contains("350"), "{row}");
}

/// Lines past the leads' batch are stated as further occurrences ("also"),
/// never as where the symbol is.
#[test]
fn lines_beyond_the_leads_are_stated_as_also() {
    let q = query(serde_json::json!({
        "operation": "references", "path": "/repo/a.ts",
        "symbolName": "render", "line": 1
    }));
    let source = (1..=40)
        .map(|line| {
            if line % 5 == 0 {
                "  render();".to_owned()
            } else {
                format!("// line {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(&source));
    let leads = row["next"]
        .as_object()
        .expect("next")
        .keys()
        .filter(|key| key.starts_with("findSimilar"))
        .count();
    assert!(leads < 8, "{row}");
    let note = row["hints"]
        .as_array()
        .expect("hints")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .find(|hint| hint.contains("`render`"))
        .expect("a note on the uncovered lines");
    assert!(note.starts_with("`render` also appears on line"), "{note}");
}

/// An orderHint past its line's occurrences leads to the last one on the
/// same line, with no listing of the name's other lines.
#[test]
fn an_order_hint_past_the_line_leads_to_its_last_occurrence() {
    let q = query(serde_json::json!({
        "operation": "definition", "path": "/repo/lib.rs",
        "symbolName": "config", "line": 3, "orderHint": 2
    }));
    let source = "struct L { config: u8 }\nfn a(config: u8) {}\nfn f() -> L { L { config: target.config } }\nfn b(config: u8) {}\n";
    let mut row = failure(
        &q,
        "file:///repo/lib.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(source));
    let lead = &row["next"]["findSimilar"]["query"]["queries"][0];
    assert_eq!(lead["orderHint"], 1, "{row}");
    assert_eq!(lead["line"], 3, "{row}");
    assert!(row["next"].get("findSimilar2").is_none(), "{row}");
    assert!(
        !row["hints"].to_string().contains("appears on line"),
        "{row}"
    );
}

/// A near-miss spelling that only a comment holds is no anchor.
#[test]
fn near_names_skip_comment_words() {
    let q = query(serde_json::json!({
        "operation": "hover", "path": "/repo/anchor.rs",
        "symbolName": "Anchor", "line": 1
    }));
    let source = "//! The anchor of a request.\nfn other() {}\n";
    let mut row = failure(
        &q,
        "file:///repo/anchor.rs",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    anchor_recovery(&mut row, &q, Some(source));
    // No anchor here: the one lead locates the name in the workspace.
    assert_eq!(
        row["next"]["findSimilar"]["query"]["queries"][0]["operation"], "workspaceSymbol",
        "{row}"
    );
    assert!(row["next"].get("findSimilar2").is_none(), "{row}");
    assert!(!row["hints"].to_string().contains("Closest names"), "{row}");
}

/// A name the file holds only in a comment, or only on the failing line,
/// never leads back to the failing query: a code line elsewhere is the
/// lead; without one, a workspace search for the name.
#[test]
fn unresolved_anchor_never_leads_back_to_itself() {
    let lead_for = |source: &str| {
        let q = query(serde_json::json!({
            "operation": "hover", "path": "/repo/anchor.rs",
            "symbolName": "Request", "line": 1
        }));
        let mut row = failure(
            &q,
            "file:///repo/anchor.rs",
            ErrorCode::AnchorUnresolved,
            "x",
            true,
        );
        anchor_recovery(&mut row, &q, Some(source));
        assert!(row["next"].get("findSimilar2").is_none(), "{row}");
        assert!(row["next"].get("read").is_none(), "{row}");
        row["next"]["findSimilar"]["query"]["queries"][0].clone()
    };
    let search = lead_for("//! Request handling.\nfn other() {}\n");
    assert_eq!(
        search,
        serde_json::json!({
            "operation": "workspaceSymbol", "path": "/repo/anchor.rs",
            "symbolName": "Request"
        })
    );
    let quoted = lead_for("//! Request handling.\nfn other() { let s = \"Request\"; }\n");
    assert_eq!(quoted["operation"], "workspaceSymbol", "{quoted}");
    let later = lead_for("//! Request handling.\nfn other() {}\nstruct Request;\n");
    assert_eq!(later["operation"], "hover", "{later}");
    assert_eq!(later["line"], 3, "{later}");
}

/// Leads that cover every candidate line leave no sentence beside them.
#[test]
fn covered_anchor_candidates_add_no_text() {
    let q = query(serde_json::json!({
        "operation": "callers", "path": "/repo/a.ts",
        "symbolName": "render", "line": 300
    }));
    let source = (1..=400)
        .map(|line| match line {
            312 => "export function render(scene: Scene) {".to_owned(),
            _ => format!("// line {line}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    let before = row["hints"].clone();
    anchor_recovery(&mut row, &q, Some(&source));
    assert_eq!(
        row["next"]["findSimilar"]["query"]["queries"][0]["line"],
        312
    );
    assert_eq!(row["hints"], before, "{row}");
}

/// A directory named where a file anchors the request is `notAFile`, with
/// a lead into the directory: a text search for a named symbol, else its
/// file listing.
#[test]
fn a_directory_anchor_is_not_a_file_and_leads_into_it() {
    let root = tempfile::tempdir().expect("root");
    let root = root.path().canonicalize().expect("canonical root");
    std::fs::create_dir_all(root.join("src/tools")).expect("dir");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let named = query(serde_json::json!({
        "operation": "references", "path": "src/tools", "symbolName": "run", "line": 1
    }));
    let row = directory_anchor(&named, &paths).expect("a directory row");
    assert_eq!(row["status"], "error", "{row}");
    assert_eq!(row["errorCode"], "notAFile", "{row}");
    assert_eq!(
        row["error"], "src/tools is a directory, not a file.",
        "{row}"
    );
    assert!(row.get("hints").is_none(), "{row}");
    let lead = &row["next"]["findText"];
    assert_eq!(lead["tool"], "localSearch", "{row}");
    assert_eq!(
        lead["query"]["queries"][0],
        serde_json::json!({"path": "src/tools", "matchString": "\\brun\\b", "regex": "rust",
            "caseMode": "sensitive"}),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("directory row satisfies the output contract");

    let unnamed = query(serde_json::json!({"operation": "documentSymbols", "path": "src/tools"}));
    let row = directory_anchor(&unnamed, &paths).expect("a directory row");
    let lead = &row["next"]["viewTree"];
    assert_eq!(lead["tool"], "structureSearch", "{row}");
    assert_eq!(
        lead["query"]["queries"][0],
        serde_json::json!({"operation": "files", "path": "src/tools"}),
        "{row}"
    );
    crate::tools::test_support::assert_row_contract(
        crate::id::ToolId::LspSearch,
        &lsp_search_query::to_row(&unnamed),
        &row,
    );
    // A file, or a path that does not exist, is not this failure.
    std::fs::write(root.join("src/a.rs"), "fn run() {}\n").expect("file");
    let file = query(serde_json::json!({"operation": "documentSymbols", "path": "src/a.rs"}));
    assert!(directory_anchor(&file, &paths).is_none());
    let missing = query(serde_json::json!({"operation": "documentSymbols", "path": "src/nope"}));
    assert!(directory_anchor(&missing, &paths).is_none());
    // A missing path names its error and the nearest listing, no prose.
    let failure = paths
        .validate_read(root.join("src/nope"))
        .map_err(LspFailure::path_denied)
        .expect_err("missing");
    let row = path_failure(&missing, &paths, failure).expect("a missing-path row");
    assert_eq!(row["errorCode"], "pathNotFound", "{row}");
    assert!(row.get("hints").is_none(), "{row}");
    assert_eq!(row["next"]["viewTree"]["tool"], "structureSearch", "{row}");
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("missing-path row satisfies the output contract");
}
