//! `syntaxTree` rows: unsupported files, line anchors, repairs and error rows.

use super::*;

/// A syntax tree of a file with no registered grammar is a failed row
/// (`status: "error"`, a declared code, the engine's message), not a partial
/// payload that exits 0.
#[test]
fn syntax_tree_of_an_unsupported_file_is_an_error_row() {
    let root = Fixture::new();
    let file = root.0.join("data.json");
    std::fs::write(&file, "{\"a\":1}\n").expect("file");
    let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("row");
    assert_eq!(out["status"], "error", "{out}");
    assert_eq!(out["errorCode"], "languageUnsupported", "{out}");
    assert!(
        out["error"]
            .as_str()
            .is_some_and(|message| message.contains(".json")),
        "{out}"
    );
    assert!(out.get("isPartial").is_none(), "{out}");
}

#[test]
fn unsupported_syntax_trees_offer_document_outlines_or_text_reads() {
    let root = Fixture::new();
    use octocode_contract_types::ReadView;
    for (name, view) in [
        ("instructions.md", ReadView::Symbols),
        ("data.json", ReadView::Symbols),
        ("notes.txt", ReadView::Content),
        ("README", ReadView::Content),
    ] {
        let file = root.write(name, "# Instructions\nRead this text.\n");
        let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("row");
        assert_eq!(out["errorCode"], "languageUnsupported", "{out}");
        let read = &out["related"][0];
        assert_eq!(read["kind"], "related", "{out}");
        assert_eq!(read["tool"], "localFetch", "{out}");
        let query = &read["input"]["queries"][0];
        assert_eq!(query["path"], file.to_string_lossy().as_ref(), "{out}");
        assert_eq!(
            crate::response::result::read_view(crate::id::ToolId::LocalFetch, query),
            view,
            "{out}"
        );
        crate::contracts::validate_built_query("localFetch", query.clone()).expect("valid read");
        assert!(out.get("next").is_none(), "{out}");
    }
}

/// `line` roots the tree at the smallest node covering that line: the
/// rows start at the statement, not at the file's program node.
#[test]
fn syntax_tree_starts_at_the_node_covering_a_line() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "const a = 1;\nfunction f() {\n  return a;\n}\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"line":3}),
    )
    .expect("row");
    let first = out["nodes"][0]["node"].as_str().expect("node row");
    assert!(first.contains("return_statement 3:"), "{out}");
    assert!(!out.to_string().contains("program"), "{out}");
}

/// A `line` past the last line is invalid input that names the line
/// count, with the whole-file tree as its repair; never a failed execution
/// that suggests broadening.
#[test]
fn syntax_tree_line_past_the_end_is_invalid_input_with_a_repair() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(&file, "const a = 1;\nfunction f() {\n  return a;\n}\n").expect("file");
    let error = run(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"line":99}),
    )
    .expect_err("past the end");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert!(error.message.contains("(4 lines)"), "{error:?}");
    let repair = &error.calls.as_deref().expect("repair")["repair"];
    let row = &repair["input"]["queries"][0];
    assert_eq!(row["operation"], "syntaxTree", "{repair}");
    assert!(row.get("line").is_none(), "{repair}");
    crate::contracts::validate_query("astSearch", row.clone()).expect("a valid repair row");
}

/// A blank `line` roots the tree at the next item and says so once, on the
/// first page only.
#[test]
fn syntax_tree_blank_line_snaps_to_the_next_item_and_notes_it_once() {
    let root = Fixture::new();
    let file = root.0.join("a.ts");
    std::fs::write(
        &file,
        "const a = 1;\n\nfunction f() {\n  const b = a;\n  const c = b;\n  return c;\n}\n",
    )
    .expect("file");
    let first = crate::tools::ast_search::test_rows::run_window(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"line":2}),
        1,
    )
    .expect("row");
    let node = first["nodes"][0]["node"].as_str().expect("node row");
    assert!(node.contains("function_declaration 3:"), "{first}");
    assert_eq!(
        first["diagnostics"][0]["code"], "syntaxTree.options.lineBlank",
        "{first}"
    );
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert_eq!(next["line"], 2, "{first}");
    let second = crate::tools::ast_search::test_rows::run_window(&root.0, next, 1).expect("page 2");
    assert!(
        second["nodes"].as_array().is_some_and(|n| !n.is_empty()),
        "{second}"
    );
    assert!(second.get("diagnostics").is_none(), "{second}");
}

/// A failed inspection states its error once: the row carries no empty
/// page, no snapshot, no single-page block and no diagnostics that repeat
/// `error`, and no recovery text about paging.
#[test]
fn syntax_tree_error_row_states_the_error_once() {
    let root = Fixture::new();
    let file = root.0.join("data.json");
    std::fs::write(&file, "{\"a\":1}\n").expect("file");
    let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("row");
    let mut keys: Vec<&str> = out
        .as_object()
        .expect("row")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "error",
            "errorCode",
            "operation",
            "path",
            "related",
            "status"
        ],
        "{out}"
    );
    checks::assert_absent(&out, "next page");
}

/// A syntax tree whose parse recovered from errors is the whole recovered
/// tree: the walk finished and no limit cut it, so the row is partial for
/// `parseRecovered` and claims no terminal limit. The reason is the one
/// statement of the recovery: no diagnostic row repeats it.
#[test]
fn a_recovered_syntax_tree_is_partial_without_a_terminal_limit() {
    let root = Fixture::new();
    let file = root.write("broken.ts", "function f( {\n");
    let out = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("tree");
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
    assert_eq!(out.get("terminalLimit"), None, "{out}");
    assert_eq!(out.get("diagnostics"), None, "{out}");
}

/// D1, D4 (bin16 retest): a tree's coordinates and byte ranges count the
/// text a read shows, as every view's do: a node after a hidden value sits
/// where the read shows it, and no range or length tells the value's.
#[test]
fn syntax_tree_coordinates_count_the_shown_text_without_exposing_text() {
    let root = Fixture::new();
    let token = format!("ghp_{}", "a".repeat(36));
    let function = "function after() { return 7; }";
    let source = format!("const value = \"{token}\"; {function}\n");
    let file = root.write("coordinates.ts", &source);
    let security = crate::security::ContentSecurity::new();
    assert_ne!(security.sanitize_text(&source, Some(&file)).content, source);
    let plain = run(&root.0, json!({"operation":"syntaxTree","path":file})).expect("plain tree");
    assert!(
        !plain.to_string().contains(&token),
        "source text must not enter node rows"
    );
    let out = run(
        &root.0,
        json!({"operation":"syntaxTree","path":file,"includeProvenance":true}),
    )
    .expect("tree");
    let node = out["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|row| {
            row["node"]
                .as_str()
                .is_some_and(|node| node.contains("function_declaration "))
        })
        .expect("function node");
    let shown = security
        .sanitize_text(&source, Some(&file))
        .content
        .into_owned();
    assert!(shown.contains("[REDACTED-"), "{shown}");
    let start = shown.find(function).expect("shown function");
    assert_ne!(start, source.find(function).expect("source function"));
    assert_eq!(
        node["sourceRange"],
        json!({"startByte":start,"endByte":start+function.len()})
    );
    assert_eq!(
        out["nodes"][0]["sourceRange"],
        json!({"startByte":0,"endByte":shown.len()})
    );
    // The node's columns are the read's too.
    let column = start + 1;
    let node_text = node["node"].as_str().expect("node row");
    assert!(node_text.contains(&format!("1:{column}-")), "{node_text}");
    assert!(
        !out.to_string().contains(&token),
        "source text must not enter node rows"
    );
    assert_eq!(out["isPartial"], false);
}

#[test]
fn syntax_tree_snapshot_detects_edits_hidden_by_redaction() {
    let root = Fixture::new();
    let source = |letter: &str| {
        format!(
            "const value = \"ghp_{}\"; function after() {{ return 7; }}\n",
            letter.repeat(36)
        )
    };
    let file = root.write("snapshot.ts", source("a"));
    let first = crate::tools::ast_search::test_rows::run_window(
        &root.0,
        json!({"operation":"syntaxTree","path":file}),
        1,
    )
    .expect("first page");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next.is_object(), "{first}");
    std::fs::write(&file, source("b")).expect("edit synthetic token");
    let resumed =
        crate::tools::ast_search::test_rows::run_window(&root.0, next, 1).expect("stale row");
    assert_eq!(resumed["errorCode"], "staleSnapshot", "{resumed}");
}
