//! Hit lines: rg-style rows, numbering, clipping and grouped locations.
use super::*;

#[test]
fn hits_print_like_rg_and_lose_no_field() {
    let lines = render(ToolId::LocalSearch, &sample(), &context(), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n25-use a;\n26:use SearcherBuilder;\n27-use b;\n--\n996-) {\n997:  let sb = SearcherBuilder::new();  [function build 992-1016]"
    );
    for token in ["issued-tool-page", "issued-response-page"] {
        assert_eq!(lines.notes.matches(token).count(), 1, "{}", lines.notes);
    }
    assert!(
        lines.notes.contains("# rowPart: part 1 of 2"),
        "{}",
        lines.notes
    );
    assert!(
        lines.notes.contains("# warnings: incomplete"),
        "{}",
        lines.notes
    );
    for evidence in [
        "use a;",
        "use SearcherBuilder;",
        "use b;",
        "let sb = SearcherBuilder::new();",
    ] {
        assert!(lines.out.contains(evidence));
    }
}

#[test]
fn numbering_follows_the_query_and_matched_lines() {
    // Without context, a `N\t` prefix is the line's own text.
    let tsv = json!({"results":[{"index":0,"data":{"files":[{"path":"t","matches":[{"line":3,"value":"3\tfoo"}]}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &tsv, &json!({}), None)
            .expect("lines")
            .out,
        "t:3:3\tfoo"
    );
    // With context, `matchedLines` marks every hit.
    let merged = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":2,"matchedLines":[2,3],"value":"1\tx\n2\tfoo\n3\tfoo"}]}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &merged, &context(), None)
            .expect("lines")
            .out,
        "a\n1-x\n2:foo\n3:foo"
    );
    // A clipped snippet that is not numbered prints as `key: value` text.
    let clipped = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":6,"value":"xxx…","clipped":true}]}]}}]});
    let lines = render(ToolId::LocalSearch, &clipped, &context(), None).expect("text");
    assert_eq!(
        lines.out,
        "files:\n  - path: a\n    matches:\n      - line 6 value xxx… clipped"
    );
}

#[test]
fn empty_values_crlf_and_clipped_structural_matches() {
    let blank = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[{"line":4,"value":""},{"line":5,"value":"x\r"}]}]}}]});
    assert_eq!(
        render(ToolId::LocalSearch, &blank, &json!({}), None)
            .expect("lines")
            .out,
        "a\n4:\n5:x\r"
    );
    let ast = json!({"results":[{"index":0,"data":{"files":[{"path":"m.ts","matches":[
            {"line":1,"endLine":3,"column":1,"value":"function foo(a) …","clipped":true}]}]}}]});
    assert_eq!(
        render(ToolId::AstSearch, &ast, &json!({}), None)
            .expect("lines")
            .out,
        "m.ts:1:function foo(a) …  [endLine 3 column 1 clipped]"
    );
}

/// Several hits name their file once, and a declaration only when it changes:
/// a long path on every line made the text larger than the JSON.
#[test]
fn several_hits_print_their_file_and_each_declaration_once() {
    let result = json!({"results":[{"index":0,"data":{"files":[{
        "path":"a.rs",
        "declarations":[
            {"symbolName":"first","kind":"function","line":1,"endLine":4},
            {"symbolName":"second","kind":"function","line":5,"endLine":7}
        ],
        "matches":[
            {"line":2,"value":"needle();","enclosing":0},
            {"line":3,"value":"needle();","enclosing":0},
            {"line":6,"value":"needle();","enclosing":1}
        ]
    },{
        "path":"b.rs",
        "matches":[{"line":9,"value":"needle();"}]
    }]}}]});
    let lines = render(ToolId::LocalSearch, &result, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n2:needle();  [function first 1-4]\n3:needle();\n6:needle();  [function second 5-7]\nb.rs:9:needle();"
    );
}

/// A location answer groups its rows by file; each still prints as a hit.
#[test]
fn grouped_location_rows_print_as_hits() {
    let result = json!({"results":[{"index":0,"data":{"operation":"references","files":[{
        "path":"a.rs",
        "matches":[
            {"displayRange":{"startLine":2,"endLine":2},"content":"2\tneedle();\n","role":"call"},
            {"displayRange":{"startLine":6,"endLine":6},"content":"6\tneedle();\n","role":"call"}
        ]
    }]}}]});
    let lines = render(ToolId::LspSearch, &result, &json!({}), None).expect("lines");
    assert!(lines.out.starts_with("a.rs\n2:needle();"), "{}", lines.out);
    assert!(lines.out.contains("\n6:needle();"), "{}", lines.out);
}
