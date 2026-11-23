//! The line a call is cited at: where it names what it calls first, as a
//! language server cites a call site.

use super::*;

fn lines_of(source: &str, path: &str) -> Vec<(String, u32)> {
    let facts = extract_graph_facts_with_metadata(source, path)
        .expect("graph facts")
        .facts;
    let mut calls: Vec<_> = facts
        .calls
        .iter()
        .map(|call| (call.callee.clone(), call.line))
        .collect();
    calls.sort();
    calls
}

#[test]
fn a_method_call_split_over_lines_is_cited_at_its_name_and_a_chain_at_its_head() {
    let source = "fn f(client: &Client) {
    let a = client
        .open(1);
    let b = read(2)
        .map_err(e);
    let c = self::read(3);
}
";
    assert_eq!(
        lines_of(source, "src/lib.rs"),
        [
            ("client.open".to_owned(), 3),
            ("read".to_owned(), 4),
            ("read().map_err".to_owned(), 4),
            ("self::read".to_owned(), 6),
        ]
    );
}

#[test]
fn every_tree_sitter_language_cites_a_split_method_call_at_its_name() {
    for (path, source) in [
        ("main.py", "def f(c):\n    return (c\n        .open(1))\n"),
        (
            "main.go",
            "package m\nfunc f(c C) {\n\tc.\n\t\tOpen(1)\n}\n",
        ),
        (
            "Main.java",
            "class M { void f(C c) {\n    c\n        .open(1);\n} }\n",
        ),
        (
            "main.c",
            "void f(struct C *c) {\n    c\n        ->open(1);\n}\n",
        ),
        ("main.cpp", "void f(C &c) {\n    c\n        .open(1);\n}\n"),
        ("main.cu", "void f(C &c) {\n    c\n        .open(1);\n}\n"),
        (
            "Main.cs",
            "class M { void F(C c) {\n    c\n        .Open(1);\n} }\n",
        ),
        (
            "Main.scala",
            "object M { def f(c: C) = {\n    c\n      .open(1)\n} }\n",
        ),
    ] {
        let lines = lines_of(source, path);
        let open = lines
            .iter()
            .find(|(callee, _)| callee.ends_with("pen"))
            .unwrap_or_else(|| panic!("{path}: {lines:?}"));
        let expected = source
            .lines()
            .position(|line| line.contains("pen("))
            .expect("name line") as u32
            + 1;
        assert_eq!(open.1, expected, "{path}: {lines:?}");
    }
}

/// The calls the JS/TS (oxc) lane records, as (callee, line).
fn oxc_lines(source: &str, path: &str) -> Vec<(String, u64)> {
    let json = crate::signatures::js_oxc::tests::extract_graph_facts(source, path).expect("facts");
    let value: serde_json::Value = serde_json::from_str(&json).expect("json");
    let mut calls: Vec<_> = value["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            (
                call["callee"].as_str().expect("callee").to_owned(),
                call["line"].as_u64().expect("line"),
            )
        })
        .collect();
    calls.sort();
    calls
}

#[test]
fn the_js_and_ts_lane_cites_a_split_call_at_its_name_and_a_chain_at_its_head() {
    let source = "function f(c) {\n    c\n        .open(1);\n    read(2)\n        .then(g);\n    new\n        Box(3);\n}\n";
    for path in ["main.ts", "main.js"] {
        let lines = oxc_lines(source, path);
        let line = |callee: &str| {
            lines
                .iter()
                .find(|(name, _)| name == callee)
                .unwrap_or_else(|| panic!("{path}: {callee} in {lines:?}"))
                .1
        };
        assert_eq!(line("c.open"), 3, "{path}: {lines:?}");
        assert_eq!(line("read"), 4, "{path}: {lines:?}");
        // A chain row witnesses its head call: cited where `read` is.
        assert_eq!(line("read().then"), 4, "{path}: {lines:?}");
        assert_eq!(line("Box"), 7, "{path}: {lines:?}");
    }
}
