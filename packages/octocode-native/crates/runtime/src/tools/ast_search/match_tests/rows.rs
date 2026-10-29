//! Match row shape: completeness, withheld captures, block headers and the
//! anchors the next hop takes.

use super::*;

/// A match row with every file and match page shown and no scope gap is
/// `coverage.state: "complete"`: its expansions (`expandCaptures`, `expandValues`) and the
/// hits read are drill-downs, so the response stage does not mark it
/// partial. A file page left, or a scope gap, keeps it open.
#[test]
fn match_rows_without_unread_pages_are_complete() {
    let root = Fixture::new();
    let long = format!("call({});\n", "y".repeat(3000));
    root.write("a.ts", &long);
    root.write("b.ts", "call(1);\n");
    let base = json!({"operation":"match","path":root.0,"rule":{"pattern":"call($$$A)"},"parser":"typescript"});
    let out = run(&root.0, base.clone()).expect("match");
    assert_eq!(out["coverage"]["state"], "complete", "{out}");
    assert!(out["next"].get("expandCaptures").is_some(), "{out}");
    let row = crate::response::rows::result_row(0, out.clone(), None);
    assert!(row.pointer("/data/isPartial").is_none(), "{row}");
    let mut capped = base;
    capped["maxFiles"] = json!(1);
    let gap = run(&root.0, capped).expect("capped scan");
    assert!(gap.get("coverage").is_none(), "{gap}");
}

#[test]
fn match_rows_withhold_captures_by_default_and_omit_single_line_ends() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\noldCall(\n  two\n);\n").expect("source");
    let query = json!({"operation":"match","path":source,"rule":{"pattern":"oldCall($A)"}});
    let out = run(&root.0, query.clone()).expect("match");
    let matches = out["files"][0]["matches"].as_array().expect("matches");
    // Lean rows (objects): the line and 1-based column (a span adds its
    // end line) and exact source text; no captures, no provenance.
    let first = json!({"line":1,"column":1,"value":"oldCall(one)"});
    let second = json!({"line":2,"column":1,"endLine":4,"value":"oldCall(\n  two\n)"});
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
    assert!(
        single.get("endColumn").is_none(),
        "single-line end: {single}"
    );
    let span = &out["files"][0]["matches"][1];
    assert_eq!(span["endLine"], 4, "{span}");
    assert!(span["endColumn"].is_u64(), "{span}");
    assert!(
        single.get("metavars").is_none(),
        "duplicate capture map: {single}"
    );
    assert_eq!(single["metavarRanges"]["A"][0]["text"], "one", "{single}");
    assert!(
        single["metavarRanges"]["A"][0].get("endLine").is_none(),
        "{single}"
    );
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
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
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");

    let mut expanded = query;
    expanded["includeCaptures"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let m = &out["files"][0]["matches"][0];
    let body = m["metavarRanges"]["B"].as_array().expect("B ranges");
    assert_eq!(body.len(), 3, "{out}");
    assert_eq!(body[0]["text"], "let x = 1;", "{out}");
    assert_eq!(body[0]["line"], 2, "{out}");
    assert_eq!(m["metavarRanges"]["N"][0]["text"], "a", "{m}");
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
/// (next.expandCaptures) returns the whole match text.
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
    let header = json!({"line":1,"column":1,"endLine":6,"value":"pub fn load(\n    path: &str,\n) -> Result<u8, String> …","clipped":true});
    assert_eq!(out["files"][0]["matches"][0], header);
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");
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
        let mut cut = query.clone();
        cut["maxFiles"] = serde_json::json!(2);
        let pages = ast_chain(&root.0, cut);
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
    assert_eq!(multi["clipped"], true, "{matched}");
    assert!(multi["value"].is_string(), "{matched}");
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
