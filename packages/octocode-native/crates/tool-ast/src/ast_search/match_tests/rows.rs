//! Match row shape: completeness, withheld captures, block headers and the
//! anchors the next hop takes.

use super::*;
use octocode_test_support::json_fixture;

/// A result with every match delivered and no required call omits the
/// coverage note. Structural expansion can retain a complete note; a file
/// page left or a scope gap must remain visible.
#[test]
fn match_rows_without_unread_pages_are_complete() {
    let root = Fixture::new();
    let long = format!("call({});\n", "y".repeat(3000));
    root.write("a.ts", &long);
    root.write("b.ts", "call(1);\n");
    let base = json!({"operation":"match","path":root.0,"rule":{"pattern":"call($$$A)"},"parser":"typescript"});
    let out = run(&root.0, base.clone()).expect("match");
    assert!(
        out.get("coverage").is_none(),
        "a complete result without required calls needs no coverage note: {out}"
    );
    assert_eq!(out["pagination"]["hasMore"], false, "{out}");
    assert_eq!(out["files"].as_array().expect("files").len(), 2);
    assert_eq!(
        out["files"][0]["matches"][0]["value"],
        long.trim_end_matches(['\n', ';'])
    );
    assert_eq!(out["files"][1]["matches"][0]["value"], "call(1)");
    assert!(
        out["next"].get("expandCaptures").is_none(),
        "the complete value already includes captures: {out}"
    );
    let row = octocode_runtime_base::response::rows::result_row(0, out.clone(), None);
    assert!(row.pointer("/data/isPartial").is_none(), "{row}");
    let _window = crate::ast_search::scan_window::test_window(1);
    let gap = run(&root.0, base).expect("capped scan");
    assert!(gap.get("coverage").is_none(), "{gap}");
}

/// Files without a grammar (here of no known type, which the inferred
/// language could hold) are named once, as a static notice that says
/// absence there is unproven; a caller's own `limit` that pages the set
/// draws no advice to narrow it.
#[test]
fn census_notices_are_static_and_a_caller_limit_draws_no_narrowing_advice() {
    let root = Fixture::new();
    root.write("notes.ts.in", "call(1)\n");
    // One file holds both matches, so one window holds the whole set and
    // its count is final: only the caller's limit could draw the advice.
    root.write("a.ts", "call(1);\ncall(2);\n");
    root.write("b.ts", "other(2);\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"call($$$A)"},"limit":1}),
    )
    .expect("match");
    let notices = out["notices"].to_string();
    assert!(notices.contains("absence there is unproven"), "{out}");
    assert!(!notices.contains("to read fewer"), "{out}");
    assert_eq!(
        (&out["pagination"]["length"], &out["pagination"]["total"]),
        (&json!(1), &json!(2)),
        "{out}"
    );
    assert!(
        !out["warnings"].to_string().contains("no AST grammar"),
        "the census is a notice, said once by the response stage: {out}"
    );
}

#[test]
fn match_rows_withhold_captures_by_default_and_state_every_end_with_captures() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\noldCall(\n  two\n);\n").expect("source");
    let query = json!({"operation":"match","path":source,"rule":{"pattern":"oldCall($A)"}});
    let out = run(&root.0, query.clone()).expect("match");
    let matches = out["files"][0]["matches"].as_array().expect("matches");
    // Lean rows (objects): the line (a span adds its end line) and exact
    // source text; no column, captures or provenance.
    let first = json_fixture(r#"{"line": 1, "value": "oldCall(one)"}"#);
    let second = json_fixture(r#"{"line": 2, "endLine": 4, "value": "oldCall(\n  two\n)"}"#);
    assert_eq!(*matches, [first.clone(), second.clone()]);
    assert!(out.get("sourceEvidence").is_none(), "{out}");
    // `includeProvenance` adds each row's exact source correspondence.
    let mut proven = query.clone();
    proven["includeProvenance"] = json!(true);
    let traced = run(&root.0, proven).expect("match");
    let rows = &traced["files"][0]["matches"];
    assert_copied_row(&traced, &rows[0], first, 0..12, None);
    assert_copied_row(&traced, &rows[1], second, 14..30, None);
    // Equal per-file counts repeat `matches.len()`.
    assert!(out["files"][0].get("totalMatchRows").is_none(), "{out}");
    // Both $A captures show in the values: nothing to expand.
    assert!(
        out.get("next")
            .is_none_or(|next| next.get("expandCaptures").is_none()),
        "{out}"
    );
    let mut expanded = query;
    expanded["includeCaptures"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let single = &out["files"][0]["matches"][0];
    assert_eq!(single["line"], 1, "{single}");
    assert_eq!(single["column"], 1, "{single}");
    assert!(single.get("endLine").is_none(), "{single}");
    assert_eq!(single["endColumn"], 13, "single-line end: {single}");
    let span = &out["files"][0]["matches"][1];
    assert_eq!(span["endLine"], 4, "{span}");
    assert!(span["endColumn"].is_u64(), "{span}");
    assert!(
        single.get("metavars").is_none(),
        "duplicate capture map: {single}"
    );
    // The capture lies inside `value`: its range is the text, stated once.
    let capture = &single["metavarRanges"]["A"][0];
    assert!(capture.get("text").is_none(), "{single}");
    let value = single["value"].as_str().expect("value");
    let column = usize::try_from(capture["column"].as_u64().expect("column")).expect("column");
    let end = usize::try_from(capture["endColumn"].as_u64().expect("end")).expect("end");
    let origin = usize::try_from(single["column"].as_u64().expect("column")).expect("column");
    assert_eq!(&value[column - origin..end - origin], "one", "{single}");
    assert!(
        single["metavarRanges"]["A"][0].get("endLine").is_none(),
        "{single}"
    );
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
}

/// The `range` a match row names: its own `line`/`column` to its
/// `endLine` (else `line`)/`endColumn`.
fn row_range(row: &serde_json::Value) -> serde_json::Value {
    json!({
        "start": {"line": row["line"], "column": row["column"]},
        "end": {
            "line": row.get("endLine").unwrap_or(&row["line"]),
            "column": row["endColumn"],
        },
    })
}

/// Every match row `includeCaptures` prints can be sent back as `rule.range`
/// unchanged and returns exactly that row. The emoji before the second call
/// on its line counts two UTF-16 units; the line above it holds one too.
#[test]
fn a_match_row_sent_back_as_range_returns_exactly_that_match() {
    let root = Fixture::new();
    let source = root.0.join("calls.rs");
    std::fs::write(
        &source,
        "// 😀 header\nfn f() {\n    let s = \"😀\"; compile_check(a, None, b);\n    compile_check(\n        c,\n    );\n}\n",
    )
    .expect("source");
    let pattern = "compile_check($$$A)";
    let query = json!({"operation":"match","path":source,"includeCaptures":true,"rule":{"pattern":pattern}});
    let out = run(&root.0, query.clone()).expect("match");
    let rows = out["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(rows.len(), 2, "{out}");
    assert_eq!(
        (&rows[0]["line"], &rows[0]["column"], &rows[0]["endColumn"]),
        (&json!(3), &json!(19), &json!(44)),
        "UTF-16 columns: {out}"
    );
    for row in rows {
        let mut ranged = query.clone();
        ranged["rule"]["range"] = row_range(row);
        let back = run(&root.0, ranged).expect("ranged match");
        assert_eq!(back["files"][0]["matches"], json!([row]), "{row} -> {back}");
    }
}

/// RFC Validation 7: the `compile_check($$$A)` row at column 9 states
/// `endColumn:55`, and that row as `range` returns exactly that match.
#[test]
fn a_single_line_capture_row_states_its_end_column_and_round_trips() {
    let root = Fixture::new();
    let source = root.0.join("gaps.rs");
    std::fs::write(
        &source,
        "fn grammar() {\n    {\n        compile_check(extensions, None, rule.as_ref())\n    }\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","path":source,"includeCaptures":true,"rule":{"pattern":"compile_check($$$A)"}});
    let out = run(&root.0, query.clone()).expect("match");
    let row = &out["files"][0]["matches"][0];
    assert_eq!(
        (&row["line"], &row["column"], &row["endColumn"]),
        (&json!(3), &json!(9), &json!(55)),
        "{out}"
    );
    assert!(row.get("endLine").is_none(), "{row}");
    let mut ranged = query;
    ranged["rule"]["range"] = row_range(row);
    let back = run(&root.0, ranged).expect("ranged match");
    assert_eq!(back["files"][0]["matches"], json!([row]), "{back}");
}

#[test]
fn list_captures_are_withheld_by_default_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("body.rs");
    std::fs::write(
        &source,
        "fn a() -> u8 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n",
    )
    .expect("source");
    let query =
        json!({"operation":"match","path":source,"rule":{"pattern":"fn $N() -> u8 { $$$B }"}});
    let out = run(&root.0, query.clone()).expect("match");
    // A lean row: no capture dump, the header stands in for the body.
    let (line, end, value) = lean_row(&out["files"][0]["matches"][0]);
    assert_eq!((line, end, value), (1, Some(5), "fn a() -> u8 …"), "{out}");
    // The lean page carries no call: `includeCaptures` is the input that
    // returns the whole text.
    assert!(out.get("related").is_none(), "{out}");

    let mut expanded = query;
    expanded["includeCaptures"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let m = &out["files"][0]["matches"][0];
    let body = m["metavarRanges"]["B"].as_array().expect("B ranges");
    assert_eq!(body.len(), 3, "{out}");
    assert!(body[0].get("text").is_none(), "inside value: {out}");
    assert_eq!(body[0]["line"], 2, "{out}");
    assert!(m["metavarRanges"]["N"][0].get("text").is_none(), "{m}");
}

/// A single-file match parses the file as a directory scan does: raw.
/// Redacting first shifted columns and could erase a matched row; values
/// are still redacted in the output.
#[test]
fn single_file_match_keeps_redacted_rows_and_directory_positions() {
    let root = Fixture::new();
    let source = root.0.join("k.ts");
    std::fs::write(
        &source,
        "export const A = \"plain\"; hit(1);\nexport const GITHUB_TOKEN = \"ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789\"; hit(2);\n",
    )
    .expect("source");
    let rows = |path: &std::path::Path| {
        let out = run(
            &root.0,
            json!({"operation":"match","path":path,"parser":"typescript","rule":{"pattern":"export const $N = $V"}}),
        )
        .expect("match");
        let file = out["files"][0].clone();
        assert!(file.is_object(), "{out}");
        file["matches"].as_array().expect("matches").clone()
    };
    let single = rows(&source);
    let directory = rows(&root.0);
    assert_eq!(single.len(), 2, "{single:?}");
    assert_eq!(single, directory);
}

/// A multi-line match with a body block shows its header (text before the
/// body) with line/endLine anchors, not the flattened body; captureText:true
/// (the related capture expansion) returns the whole match text.
#[test]
fn block_matches_default_to_their_header_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn load(\n    path: &str,\n) -> Result<u8, String> {\n    let raw = read(path)?;\n    parse(raw)\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","path":source,"rule":{"kind":"function_item"}});
    let out = run(&root.0, query.clone()).expect("match");
    let header = json_fixture(
        r#"{
            "line": 1,
            "endLine": 6,
            "value": "pub fn load(\n    path: &str,\n) -> Result<u8, String> …"
        }"#,
    );
    assert_eq!(out["files"][0]["matches"][0], header);
    // The lean page carries no call: `includeCaptures` is the input that
    // returns the whole text.
    assert!(out.get("related").is_none(), "{out}");
    let mut proven = query.clone();
    proven["includeProvenance"] = json!(true);
    let traced = run(&root.0, proven).expect("match");
    assert_copied_row(
        &traced,
        &traced["files"][0]["matches"][0],
        header,
        0..98,
        Some(0..52),
    );

    let mut expanded = query;
    expanded["includeCaptures"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let value = out["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert_eq!(
        value,
        "pub fn load(\n    path: &str,\n) -> Result<u8, String> {\n    let raw = read(path)?;\n    parse(raw)\n}"
    );
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
}

/// `next.expandScan` resumes after the files its window evaluated, so
/// the chain of pages and widened scans lists every match and declaration
/// exactly once, under each sort.
#[test]
fn expanded_scans_reach_every_file_exactly_once() {
    let root = Fixture::new();
    for dir in ["a", "a/deep", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..5 {
            let body = "pub fn source() {}\n".repeat(n + 1);
            root.write(&format!("{dir}/f{n}.rs"), body);
        }
    }
    let queries = [
        serde_json::json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"}}),
        serde_json::json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"},"sort":{"by":"matchCount","order":"asc"}}),
        serde_json::json!({"operation":"symbols","path":root.0}),
    ];
    for query in queries {
        let whole = ast_rows(&ast_chain(&root.0, query.clone()));
        assert_eq!(whole.len(), 45, "{query}");
        let pages = {
            let _window = crate::ast_search::scan_window::test_window(2);
            ast_chain(&root.0, query.clone())
        };
        assert!(pages.len() > 1, "{query}");
        assert!(
            pages.iter().any(|page| {
                page["next"]["expandScan"]["input"]["queries"][0]["scanOffset"]
                    .as_u64()
                    .is_some_and(|offset| offset > 0)
            }),
            "{query}"
        );
        let mut walked = ast_rows(&pages);
        let mut expected = whole.clone();
        walked.sort();
        expected.sort();
        assert_eq!(walked, expected, "{query}: no duplicate, no gap");
    }
}

/// The happy-path rows carry exactly what the next hop takes: a file
/// outline names its file once and each declaration as an object
/// `{symbolName, kind, line, endLine}`; a directory outline groups under each
/// file's `path` without restating the queried directory; a match row is
/// `{line, endLine?, value}` (no `endLine` on a single-line match).
#[test]
fn happy_path_rows_carry_the_next_hop_anchors() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    let file = root.0.join("src/lib.rs");
    std::fs::write(
        &file,
        "pub fn alpha() {\n    beta(1);\n}\n\nfn beta(x: u8) {}\n",
    )
    .expect("lib");

    let outline = run(
        &root.0,
        json!({"operation":"symbols","path":file,"symbolFilter":{"terms":["alpha"]}}),
    )
    .expect("file outline");
    assert_eq!(outline["path"], json!("src/lib.rs"), "{outline}");
    let row = &outline["symbols"][0];
    assert_eq!(
        (
            &row["symbolName"],
            &row["line"],
            &row["endLine"],
            &row["kind"]
        ),
        (&json!("alpha"), &json!(1), &json!(3), &json!("function")),
        "{outline}"
    );

    let directory = run(
        &root.0,
        json!({"operation":"symbols","path":root.0.join("src")}),
    )
    .expect("directory outline");
    assert!(directory.get("path").is_none(), "{directory}");
    assert_eq!(
        directory["files"][0]["path"],
        json!("src/lib.rs"),
        "{directory}"
    );
    assert!(directory["files"][0]["symbols"].is_array(), "{directory}");

    let matched = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"any":[
            {"pattern":"fn $N($$$P) { $$$B }"},
            {"pattern":"pub fn $N($$$P) { $$$B }"}
        ]}}),
    )
    .expect("match");
    let rows = matched["files"][0]["matches"].as_array().expect("rows");
    let multi = rows.iter().find(|row| row["line"] == 1).expect("alpha");
    assert_eq!(multi["endLine"], 3, "{matched}");
    // The in-band marker states the clip; no flag repeats it.
    assert!(multi.get("clipped").is_none(), "{matched}");
    assert!(
        multi["value"]
            .as_str()
            .is_some_and(|text| text.ends_with(" …")),
        "{matched}"
    );
    let single = rows.iter().find(|row| row["line"] == 5).expect("beta");
    assert!(single.get("endLine").is_none(), "{matched}");
    assert!(single.get("clipped").is_none(), "{matched}");
    for row in rows {
        let keys = row.as_object().expect("row").keys().collect::<Vec<_>>();
        assert!(
            keys.iter().all(|key| [
                "line",
                "endLine",
                "value",
                "column",
                "clipped",
                "sourceRange",
                "sourceOrigin",
                "sourceDigest",
                "headerSourceRange",
                "headerOrigin"
            ]
            .contains(&key.as_str())),
            "{row}"
        );
    }
}

/// The census names a few of the files no AST grammar parses; every file it
/// counts is its own `unparsed` entry (no count), so paging reaches each
/// one. A file of a known type the scan's grammar does not read (JSON) is a
/// skipped-grammar count, never an entry.
#[test]
fn every_file_the_unparsed_census_counts_is_a_skipped_path_entry() {
    let root = Fixture::new();
    for index in 0..7 {
        root.write(&format!("notes{index}.ts.in"), "call(1)\n");
    }
    root.write("data.json", "{}\n");
    root.write("a.ts", "call(1);\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"call($$$A)"}}),
    )
    .expect("match");
    let notice = out["notices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .find(|notice| notice.contains("no AST grammar"))
        .unwrap_or_else(|| panic!("the census names the skipped files: {out}"));
    let counted: usize = notice
        .split_whitespace()
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("the census counts its files: {notice}"));
    let entries = out["skippedPaths"]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("the counted files are entries: {out}"))
        .iter()
        .filter(|entry| entry["cause"] == "unparsed")
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), counted, "{out}");
    assert_eq!(counted, 7, "{notice}");
    for entry in &entries {
        assert!(entry.get("count").is_none(), "{entry}");
        assert!(entry.get("reason").is_none(), "untyped: {entry}");
        assert!(
            entry["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(".ts.in")),
            "{entry}"
        );
    }
    let distinct = entries
        .iter()
        .map(|entry| entry["path"].clone())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(distinct.len(), counted, "each file once: {out}");
    assert!(
        !out["skippedPaths"].to_string().contains("data.json"),
        "{out}"
    );
}

/// The warnings of a raw page that say its rows nest.
fn nesting_warnings(out: &serde_json::Value) -> Vec<String> {
    out["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|warning| warning.contains("inside another match"))
        .map(str::to_owned)
        .collect()
}

/// A rule that names no node (here `regex` alone) matches every node whose
/// text holds the hit, so the hit's enclosing nodes come back as rows
/// around it: the page warns, counting the rows inside another and naming
/// the innermost kind, the node a `kind` keeps. A rule that names its kind
/// keeps nested rows of that kind without a warning, and so does a regex
/// rule whose rows do not nest.
#[test]
fn a_rule_naming_no_node_warns_when_its_rows_nest() {
    let root = Fixture::new();
    let file = root.write(
        "lib.rs",
        "fn outer() {\n    let a = || { load(); };\n}\n\nfn other() {}\n",
    );
    let out = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"regex":"load"}}),
    )
    .expect("match");
    let rows = out["files"][0]["matches"].as_array().expect("rows").len();
    assert!(rows > 2, "the hit and the nodes around it: {out}");
    let warnings = nesting_warnings(&out);
    let [warning] = warnings.as_slice() else {
        panic!("one nesting warning: {out}");
    };
    assert!(
        warning.starts_with(&format!("{} matches", rows - 1)),
        "every row but the outermost lies inside another: {warning}"
    );
    assert!(warning.contains("kind"), "{warning}");
    assert!(
        warning.contains("identifier"),
        "the innermost kind: {warning}"
    );
    for named in [
        json_fixture(r#"{"kind": "block", "regex": "load"}"#),
        json_fixture(r#"{"any": [{"kind": "block"}, {"pattern": "load()"}], "regex": "load"}"#),
        json_fixture(r#"{"regex": "^fn other"}"#),
    ] {
        let out = run(
            &root.0,
            json!({"operation":"match","path":file,"rule":named}),
        )
        .expect("match");
        assert!(out["files"][0]["matches"].is_array(), "{named}: {out}");
        assert!(nesting_warnings(&out).is_empty(), "{named}: {out}");
    }
}
