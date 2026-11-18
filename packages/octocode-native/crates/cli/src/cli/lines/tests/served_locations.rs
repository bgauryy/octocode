//! lspSearch location rows as the runtime serves them: a one-line `content`
//! at the row's own `line` carries no number, and a multi-line one is
//! numbered. Both print as hit lines under their file.
use super::*;
use octocode_test_support::json_fixture;

/// A served `references` answer: one-line contents, unnumbered, and the
/// column only where two rows share a line.
fn references() -> Value {
    json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "files": [
                            {
                                "path": "src/lib.ts",
                                "matches": [
                                    {"role": "declaration", "line": 1, "content": "export function helperFn(a: number): number {"},
                                    {"role": "call", "line": 4, "column": 37, "content": "export const twice = (x: number) => helperFn(helperFn(x));"},
                                    {"role": "call", "line": 4, "column": 46, "content": "export const twice = (x: number) => helperFn(helperFn(x));"}
                                ]
                            },
                            {
                                "path": "src/use.ts",
                                "matches": [
                                    {"role": "import", "line": 1, "content": "import { helperFn } from \"./lib\";"},
                                    {"role": "call", "line": 3, "content": "  return helperFn(2);"}
                                ]
                            }
                        ],
                        "matchCount": 5,
                        "matchedFiles": 2,
                        "workspaceRoot": "."
                    }
                }
            ]
        }"#,
    )
}

fn asked() -> Value {
    json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "references",
                    "path": "src/lib.ts",
                    "symbolName": "helperFn",
                    "line": 1
                }
            ]
        }"#,
    )
}

#[test]
fn served_references_print_as_hit_lines_under_each_file() {
    let lines = render(ToolId::LspSearch, &references(), &asked(), None).expect("lines");
    assert_eq!(
        lines.out,
        [
            "src/lib.ts",
            "1:export function helperFn(a: number): number {  [role declaration]",
            "4:export const twice = (x: number) => helperFn(helperFn(x));  [column 37,46 role call]",
            "",
            "src/use.ts",
            "1:import { helperFn } from \"./lib\";  [role import]",
            "3:  return helperFn(2);  [role call]",
        ]
        .join("\n")
    );
    // Every other field is still stated, beside the lines.
    for fact in ["matchCount", "matchedFiles", "workspaceRoot"] {
        assert!(
            lines.notes.contains(fact) || lines.out.contains(fact),
            "{fact}: {}\n{}",
            lines.out,
            lines.notes
        );
    }
}

/// A numbered body and an unnumbered one-line row in one answer: the body's
/// lines keep their numbers and the one-line row prints at its own line.
#[test]
fn an_unnumbered_row_beside_a_numbered_body_prints_at_its_line() {
    let definition = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "matches": [
                            {"path": "w.rs", "line": 73, "content": "72\t/// doc\n73\tpub fn f(\n74\t) {}"},
                            {"path": "w.rs", "line": 90, "content": "f();"}
                        ]
                    }
                }
            ]
        }"#,
    );
    let lines = render(ToolId::LspSearch, &definition, &json!({}), None).expect("lines");
    assert_eq!(
        lines.out,
        "w.rs\n72-/// doc\n73:pub fn f(\n74-) {}\n--\n90:f();"
    );
}
