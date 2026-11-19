//! Hit lines: rg-style rows, numbering, clipping and grouped locations.
use super::*;
use octocode_test_support::json_fixture;

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
    let tsv = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [{"path": "t", "matches": [{"line": 3, "value": "3\tfoo"}]}]
                    }
                }
            ]
        }"#,
    );
    assert_eq!(
        render(ToolId::LocalSearch, &tsv, &json!({}), None)
            .expect("lines")
            .out,
        "t:3:3\tfoo"
    );
    // With context, `matchedLines` marks every hit.
    let merged = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a",
                                "matches": [
                                    {
                                        "line": 2,
                                        "matchedLines": [2, 3],
                                        "value": "1\tx\n2\tfoo\n3\tfoo"
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    assert_eq!(
        render(ToolId::LocalSearch, &merged, &context(), None)
            .expect("lines")
            .out,
        "a\n1-x\n2:foo\n3:foo"
    );
    // A clipped snippet that is not numbered prints in the generic layout.
    let clipped = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a",
                                "matches": [{"line": 6, "value": "xxx…", "clipped": true}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &clipped, &context(), None).expect("text");
    assert_eq!(
        lines.out,
        "files:\n  - path: a\n    matches[]: line value clipped\n      6 xxx… true"
    );
}

#[test]
fn empty_values_crlf_and_clipped_structural_matches() {
    let blank = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a",
                                "matches": [{"line": 4, "value": ""}, {"line": 5, "value": "x\r"}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    assert_eq!(
        render(ToolId::LocalSearch, &blank, &json!({}), None)
            .expect("lines")
            .out,
        "a\n4:\n5:x\r"
    );
    let ast = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "m.ts",
                                "matches": [
                                    {
                                        "line": 1,
                                        "endLine": 3,
                                        "column": 1,
                                        "value": "function foo(a) …",
                                        "clipped": true
                                    }
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    assert_eq!(
        render(ToolId::AstSearch, &ast, &json!({}), None)
            .expect("lines")
            .out,
        "m.ts:1:function foo(a) …  [endLine 3 column 1 clipped true]"
    );
}

/// Several hits name their file once, and a declaration only when it changes:
/// a long path on every line made the text larger than the JSON.
#[test]
fn several_hits_print_their_file_and_each_declaration_once() {
    let result = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a.rs",
                                "declarations": [
                                    {
                                        "symbolName": "first",
                                        "kind": "function",
                                        "line": 1,
                                        "endLine": 4
                                    },
                                    {
                                        "symbolName": "second",
                                        "kind": "function",
                                        "line": 5,
                                        "endLine": 7
                                    }
                                ],
                                "matches": [
                                    {"line": 2, "value": "needle();", "enclosing": 0},
                                    {"line": 3, "value": "needle();", "enclosing": 0},
                                    {"line": 6, "value": "needle();", "enclosing": 1}
                                ]
                            },
                            {
                                "path": "b.rs",
                                "matches": [{"line": 9, "value": "needle();"}]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &result, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n2:needle();  [function first 1-4]\n3:needle();\n6:needle();  [function second 5-7]\nb.rs:9:needle();"
    );
}

/// A merged window holds hits of two declarations: its `enclosing` is one
/// index per `matchedLines` entry, and each hit line states its own
/// declaration. A window inside one declaration, with one index, states it once.
#[test]
fn a_merged_window_states_the_declaration_of_each_hit_it_holds() {
    let result = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "a.rs",
                                "declarations": [
                                    {"symbolName": "first", "kind": "function", "line": 1, "endLine": 4},
                                    {"symbolName": "second", "kind": "function", "line": 5, "endLine": 7}
                                ],
                                "matches": [
                                    {
                                        "line": 3,
                                        "endLine": 6,
                                        "value": "needle();\n}\nfn second() {\nneedle();",
                                        "matchedLines": [3, 6],
                                        "enclosing": [0, 1]
                                    }
                                ]
                            },
                            {
                                "path": "b.rs",
                                "declarations": [
                                    {"symbolName": "only", "kind": "function", "line": 1, "endLine": 9}
                                ],
                                "matches": [
                                    {"line": 2, "endLine": 3, "value": "needle();\nneedle();", "matchedLines": [2, 3], "enclosing": 0}
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LocalSearch, &result, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "a.rs\n3:needle();  [function first 1-4]\n4:}\n5:fn second() {\n6:needle();  [function second 5-7]\n\nb.rs\n2:needle();  [function only 1-9]\n3:needle();"
    );
}

#[test]
fn a_single_multiline_hit_prints_its_file_once_and_keeps_every_line() {
    let path = "src/é/long/path/multiline.rs";
    let result = json!({"results":[{"index":0,"data":{"files":[{
        "path":path,
        "matches":[{"line":2,"endLine":4,"column":3,
            "value":"fn café() {\n    call();\r\n}"}]
    }]}}]});
    for tool in [ToolId::LocalSearch, ToolId::AstSearch] {
        let lines = render(tool, &result, &json!({}), None).expect("lines");
        assert_eq!(
            lines.out,
            format!("{path}\n2:fn café() {{  [column 3]\n3:    call();\r\n4:}}")
        );
    }
}

/// A location answer groups its rows by file; each still prints as a hit.
#[test]
fn grouped_location_rows_print_as_hits() {
    let result = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "operation": "references",
                        "files": [
                            {
                                "path": "a.rs",
                                "matches": [
                                    {"line": 2, "content": "2\tneedle();\n", "role": "call"},
                                    {"line": 6, "content": "6\tneedle();\n", "role": "call"}
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#,
    );
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
    let asked = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "references",
                    "path": "b.ts",
                    "symbolName": "needle",
                    "line": 1
                }
            ]
        }"#,
    );
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
        json_fixture(
            r#"{
                "queries": [{"path": "s", "matchString": ["x"], "contextLines": 1}]
            }"#,
        ),
        json_fixture(
            r#"{
                "queries": [{"path": "s", "matchString": "x", "contextLines": 1}]
            }"#,
        ),
        json_fixture(r#"{"path": "s", "matchString": "x", "contextLines": 1}"#),
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

/// Counts by file print as one table whatever count field the files share
/// (`countMatches` gives `matchCount`, `countLines` `matchedLineCount`): the
/// head names the field once, each file is `N path`, and no file restates
/// the field in `[ … ]`.
#[test]
fn count_views_print_one_table_whatever_their_count() {
    for (view, field) in [
        ("countMatches", "matchCount"),
        ("countLines", "matchedLineCount"),
    ] {
        let mut files = Vec::new();
        for (path, count) in [("hits.txt", 60), ("longline.txt", 1)] {
            let mut file = json!({ "path": path });
            file[field] = count.into();
            files.push(file);
        }
        let mut data = json!({"matchedFiles": 2, "filesScanned": 4, "files": files});
        data[field] = 61.into();
        let result = json!({"results": [{"index": 0, "data": data}]});
        let asked = json!({"queries": [{"path": "data", "matchString": ["beacon"], "view": view}]});
        let out = render(ToolId::LocalSearch, &result, &asked, None)
            .expect("text")
            .out;
        let heads: Vec<&str> = out.lines().filter(|line| line.contains("[]:")).collect();
        assert_eq!(heads, [format!("files[]: {field} path")], "{view}:\n{out}");
        for (count, path) in [(60, "hits.txt"), (1, "longline.txt")] {
            assert!(
                out.lines()
                    .any(|line| line.trim() == format!("{count} {path}")),
                "{view} {path}:\n{out}"
            );
        }
        assert!(!out.contains(&format!("[{field}")), "{view}:\n{out}");
    }
}

/// TC7: ghSearchCode states each hit as one numbered line (`"N\ttext"`);
/// the text view prints it as the `{line, value}` hit it stands for.
#[test]
fn a_line_first_hit_prints_as_the_hit_it_stands_for() {
    let row = |matches: Value| {
        json!({"results": [{"index": 0, "data": {
            "files": [{"path": "src/a.js", "matches": matches}], "commitSha": "abc"}}]})
    };
    let query = json!({"queries": [{"owner": "o", "repo": "r", "keywords": ["fn"]}]});
    let objects = render(
        ToolId::GhSearchCode,
        &row(json!([{"line": 3, "value": "fn a()"}, {"line": 9, "value": "fn b(x)\tok"}])),
        &query,
        None,
    )
    .expect("lines");
    let strings = render(
        ToolId::GhSearchCode,
        &row(json!(["3\tfn a()", "9\tfn b(x)\tok"])),
        &query,
        None,
    )
    .expect("lines");
    assert_eq!(strings.out, objects.out);
    assert_eq!(strings.notes, objects.notes);
    assert!(strings.out.contains("fn b(x)"), "{}", strings.out);
}

/// A localSearch span equal to the query's one term leaves out its `value`
/// (the request states it): it prints as the same span holding the term
/// does, beside a span spelled otherwise that keeps its own. Without one
/// term to read (several terms), the spans print as the fields they hold.
#[test]
fn a_span_without_its_value_prints_the_one_term() {
    let page = |first: serde_json::Value, last: serde_json::Value| {
        json!({"results": [{"index": 0, "data": {"files": [
            {"path": "src/lock.rs", "matches": [
                first,
                {"line": 7, "column": 2, "value": "Needle"},
                last
            ]},
            {"path": "src/other.rs", "matches": [{"line": 1, "column": 1}]}
        ]}}]})
    };
    let omitted = page(
        json!({"line": 3, "column": 9}),
        json!({"line": 12, "column": 5}),
    );
    let mut stated = page(
        json!({"line": 3, "column": 9, "value": "needle"}),
        json!({"line": 12, "column": 5, "value": "needle"}),
    );
    stated["results"][0]["data"]["files"][1]["matches"][0]["value"] = json!("needle");
    let query = |terms: serde_json::Value| json!({"queries": [{"path": "src", "matchString": terms, "view": "matchOnly"}]});
    let one = query(json!(["needle"]));
    let printed = render(ToolId::LocalSearch, &omitted, &one, None).expect("lines");
    let expected = render(ToolId::LocalSearch, &stated, &one, None).expect("lines");
    assert_eq!(printed.out, expected.out);
    assert_eq!(printed.notes, expected.notes);
    assert!(printed.out.contains("3:needle"), "{}", printed.out);
    assert!(printed.out.contains("7:Needle"), "{}", printed.out);
    let several = query(json!(["needle", "pin"]));
    let printed = render(ToolId::LocalSearch, &omitted, &several, None).expect("lines");
    assert!(!printed.out.contains(":needle"), "{}", printed.out);
}
