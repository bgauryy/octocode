//! Hit lines: rg-style rows, numbering, clipping and grouped locations.
use super::*;

#[test]
fn hits_print_like_rg_and_lose_no_field() {
    let lines = render(ToolId::LocalSearch, &sample(), &context(), None).expect("lines");
    assert_eq!(
        lines.out,
        [
            "a.rs",
            "25-use a;",
            "26:use SearcherBuilder;",
            "27-use b;",
            "--",
            "996-) {",
            "997:  let sb = SearcherBuilder::new();  [function build 992-1016]",
            "# rowPart: part 1 of 2",
            "# responsePagination: hasMore",
            "# warnings: incomplete",
            "# responsePagination.next: octocode localSearch '{\"cursor\":\"issued-response-page\"}'",
            "# next.nextPage: octocode localSearch '{\"cursor\":\"issued-tool-page\"}'",
        ]
        .join("\n")
    );
    assert_eq!(
        lines.notes,
        "# matchCount=matchedLineCount 2\n# root: /nowhere (paths are relative to it)"
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
    // A clipped snippet that is not numbered prints in the generic layout.
    let clipped = json!({"results":[{"index":0,"data":{"files":[{"path":"a","matches":[
            {"line":6,"value":"xxx…","clipped":true}]}]}}]});
    let lines = render(ToolId::LocalSearch, &clipped, &context(), None).expect("text");
    assert_eq!(
        lines.out,
        "files:\n  - path: a\n    matches[]: line value clipped\n      6 xxx… true"
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
            {"line":2,"content":"2\tneedle();\n","role":"call"},
            {"line":6,"content":"6\tneedle();\n","role":"call"}
        ]
    }]}}]});
    let lines = render(ToolId::LspSearch, &result, &json!({}), None).expect("lines");
    assert!(lines.out.starts_with("a.rs\n2:needle();"), "{}", lines.out);
    assert!(lines.out.contains("\n6:needle();"), "{}", lines.out);
}

/// A text field every hit of a file holds states once per run of equal
/// values; a column the line spells the symbol at, and nowhere else, is left
/// to the text; hits on one line print it once with each column.
#[test]
fn location_labels_state_once_per_run_and_columns_only_where_needed() {
    let hit = |line: u64, text: &str, column: u64, role: &str| {
        json!({"line":line,"column":column,
            "content":format!("{line}\t{text}\n"),"role":role})
    };
    let result = json!({"results":[{"index":0,"data":{"operation":"references","files":[{
        "path":"a.ts",
        "matches":[
            hit(1, "import { needle } from './b';", 10, "import"),
            hit(4, "needle(needle);", 1, "call"),
            hit(4, "needle(needle);", 8, "call"),
            hit(6, "  needle();", 3, "call"),
            hit(8, "  /* needle */ needle();", 16, "call"),
            hit(9, "export { needle };", 10, "reference")
        ]
    }]}}]});
    let asked = json!({"queries":[{"operation":"references","path":"b.ts","symbolName":"needle","line":1}]});
    let lines = render(ToolId::LspSearch, &result, &asked, None).expect("lines");
    assert_eq!(
        lines.out,
        [
            "a.ts",
            "1:import { needle } from './b';  [role import]",
            "4:needle(needle);  [column 1,8 role call]",
            "6:  needle();",
            "8:  /* needle */ needle();  [column 16]",
            "9:export { needle };  [role reference]",
        ]
        .join("\n")
    );
    // A field only some hits hold prints on each of them.
    let mut sourced = result.clone();
    sourced["results"][0]["data"]["files"][0]["matches"][0]["source"] = "recovered".into();
    sourced["results"][0]["data"]["files"][0]["matches"][3]["source"] = "recovered".into();
    let out = render(ToolId::LspSearch, &sourced, &asked, None)
        .expect("lines")
        .out;
    assert!(out.contains("[role import source recovered]"), "{out}");
    assert!(out.contains("6:  needle();  [source recovered]"), "{out}");
    // Without the symbol's name nothing says where it is: every column stays.
    let lines = render(ToolId::LspSearch, &result, &json!({}), None).expect("lines");
    assert!(
        lines.out.contains("6:  needle();  [column 3]"),
        "{}",
        lines.out
    );
}

/// `contextLines: N` is shorthand the runtime repairs to `{before:N,after:N}`
/// before the call runs: its text is the object form's, one file line number
/// on each line (`N:` a hit, `N-` context), never the value's own `N\t`
/// prefix printed after the hit's line.
#[test]
fn shorthand_context_lines_print_as_the_object_form() {
    let object = render(ToolId::LocalSearch, &sample(), &context(), None)
        .expect("lines")
        .out;
    for shorthand in [
        json!({"queries":[{"path":"s","matchString":["x"],"contextLines":1}]}),
        json!({"queries":[{"path":"s","matchString":"x","contextLines":1}]}),
        json!({"path":"s","matchString":"x","contextLines":1}),
    ] {
        let lines = render(ToolId::LocalSearch, &sample(), &shorthand, None).expect("lines");
        assert_eq!(lines.out, object, "{shorthand}");
    }
    assert!(object.contains("\n26:use SearcherBuilder;"), "{object}");
    for line in object
        .lines()
        .filter(|line| line.starts_with(char::is_numeric))
    {
        let text = line.trim_start_matches(char::is_numeric);
        let text = text
            .strip_prefix(':')
            .or_else(|| text.strip_prefix('-'))
            .expect("N: or N- before the text");
        let second = text.trim_start_matches(char::is_numeric);
        assert!(
            second.len() == text.len() || !second.starts_with(['\t', ':']),
            "a second line number: {line}"
        );
    }
}
