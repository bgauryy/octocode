//! Byte-level pins of the compact declaration rows `outline_file` produces:
//! key order feeds the `snapshot` digest, so it must not drift.
use crate::execution::cancel::NeverCancel;
use crate::tools::ast_search::symbols::outline::outline_file;
use crate::tools::test_support::Fixture;
use octocode_contract_types::AstSearchQuerySymbols;
use serde_json::{Value, json};

fn rows(name: &str, source: &str, provenance: bool) -> Vec<String> {
    let root = Fixture::new();
    let path = root.write(name, source).canonicalize().expect("canonical");
    let mut query = json!({"operation":"symbols","path":path});
    if provenance {
        query["includeProvenance"] = json!(true);
    }
    let q: AstSearchQuerySymbols = serde_json::from_value(query).expect("query");
    let outline = outline_file(&q, (&path, name), None, &NeverCancel)
        .expect("outline")
        .unwrap_or_else(|row| panic!("terminal row {row}"));
    let rendered = outline.files[0]
        .rows
        .iter()
        .map(|row| serde_json::to_string(row).expect("row"))
        .collect::<Vec<_>>();
    // The digests and ids depend on the fixture's path; the shape does not.
    let root = path.parent().unwrap().to_string_lossy().into_owned();
    rendered
        .into_iter()
        .map(|row| row.replace(&root, "ROOT"))
        .collect()
}

const RUST: &str = "/// Doc.\n#[derive(Debug)]\npub struct Foo;\nimpl std::fmt::Display for Foo {\n    fn fmt(&self) {}\n}\nimpl Foo {\n    fn a(&self) {}\n}\nimpl Foo {\n    fn a(&self) {}\n}\nfn dup() {} fn dup() {}\n";
const TS: &str = "export class Box {\n  size(): number { return 1 }\n}\nfunction local() {}\nexport { local as shown };\nexport default function main() {}\n";
const PY: &str = "@decorator\ndef run():\n    pass\n\nclass K:\n    def m(self):\n        pass\n";

fn expect(actual: Vec<String>, expected: &[&str]) {
    assert_eq!(actual, expected, "{}", actual.join("\n"));
}

/// A row with its path-dependent 64-hex digests and ids replaced by `H`.
fn masked(row: &str) -> String {
    let mut value: Value = serde_json::from_str(row).expect("row");
    fn walk(value: &mut Value) {
        match value {
            Value::String(text)
                if text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                *text = "H".into();
            }
            Value::Array(items) => items.iter_mut().for_each(walk),
            Value::Object(fields) => fields.values_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut value);
    serde_json::to_string(&value).expect("masked")
}

fn keys_of(row: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(row).expect("row");
    value.as_object().expect("object").keys().cloned().collect()
}

#[test]
fn rust_rows_keep_key_order_and_bytes() {
    expect(
        rows("a.rs", RUST, false),
        &[
            r#"{"name":"Foo","kind":"struct","line":3,"docStartLine":1,"exported":true}"#,
            r#"{"name":"std::fmt::Display for Foo","kind":"impl","line":4,"typeName":"Foo","endLine":6}"#,
            r#"{"name":"fmt","kind":"method","line":5,"parent":"std::fmt::Display for Foo"}"#,
            r#"{"name":"Foo","kind":"impl","line":7,"endLine":9}"#,
            r#"{"name":"a","kind":"method","line":8,"parent":"Foo","parentLine":7}"#,
            r#"{"name":"Foo","kind":"impl","line":10,"endLine":12}"#,
            r#"{"name":"a","kind":"method","line":11,"parent":"Foo","parentLine":10}"#,
            r#"{"name":"dup","kind":"function","line":13,"character":4}"#,
            r#"{"name":"dup","kind":"function","line":13,"character":16}"#,
        ],
    );
}

#[test]
fn rust_rows_with_provenance_keep_key_order_and_bytes() {
    let rows = rows("a.rs", RUST, true);
    assert_eq!(
        masked(&rows[0]),
        r#"{"name":"Foo","kind":"struct","line":3,"occurrenceId":"H","sourceRange":{"endByte":41,"startByte":26},"selectionByteRange":{"endByte":40,"startByte":37},"parseAssessment":{"status":"parsed","scope":"declaration","range":{"start":{"line":2,"character":0},"end":{"line":2,"character":15}}},"docStartLine":1,"exported":true,"sourceOrigin":{"analysisUnitId":"H","segments":[{"analysis":"included","branches":[{"kind":"copied","origin":{"rawRange":{"endByte":41,"startByte":26},"sourceId":"H"}}],"sourceRange":{"endByte":41,"startByte":26}}]},"sourceDigest":"H","selectionOrigin":{"analysisUnitId":"H","segments":[{"analysis":"included","branches":[{"kind":"copied","origin":{"rawRange":{"endByte":40,"startByte":37},"sourceId":"H"}}],"sourceRange":{"endByte":40,"startByte":37}}]}}"#
    );
    let keys: Vec<Vec<String>> = rows.iter().map(|row| keys_of(row)).collect();
    let expected: &[&[&str]] = &[
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "docStartLine",
            "exported",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "typeName",
            "endLine",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "parent",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "endLine",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "parent",
            "parentLine",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "endLine",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "parent",
            "parentLine",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "character",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "character",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
    ];
    for (got, want) in keys.iter().zip(expected) {
        assert_eq!(got, want);
    }
    assert_eq!(keys.len(), expected.len());
}

#[test]
fn typescript_rows_keep_key_order_and_bytes() {
    expect(
        rows("a.ts", TS, false),
        &[
            r#"{"name":"Box","kind":"class","line":1,"endLine":3,"exported":true}"#,
            r#"{"name":"size","kind":"method","line":2,"parent":"Box"}"#,
            r#"{"name":"local","kind":"function","line":4,"exported":true,"exportedAs":["shown"]}"#,
            r#"{"name":"main","kind":"function","line":6,"exported":true,"exportedAs":["default"]}"#,
        ],
    );
}

#[test]
fn typescript_rows_with_provenance_keep_key_order_and_bytes() {
    let rows = rows("a.ts", TS, true);
    assert_eq!(
        masked(&rows[0]),
        r#"{"name":"Box","kind":"class","line":1,"occurrenceId":"H","sourceRange":{"endByte":50,"startByte":0},"selectionByteRange":{"endByte":16,"startByte":13},"parseAssessment":{"status":"parsed","scope":"declaration","range":{"start":{"line":0,"character":0},"end":{"line":2,"character":1}}},"endLine":3,"exported":true,"sourceOrigin":{"analysisUnitId":"H","segments":[{"analysis":"included","branches":[{"kind":"copied","origin":{"rawRange":{"endByte":50,"startByte":0},"sourceId":"H"}}],"sourceRange":{"endByte":50,"startByte":0}}]},"sourceDigest":"H","selectionOrigin":{"analysisUnitId":"H","segments":[{"analysis":"included","branches":[{"kind":"copied","origin":{"rawRange":{"endByte":16,"startByte":13},"sourceId":"H"}}],"sourceRange":{"endByte":16,"startByte":13}}]}}"#
    );
    let keys: Vec<Vec<String>> = rows.iter().map(|row| keys_of(row)).collect();
    let expected: &[&[&str]] = &[
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "endLine",
            "exported",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "parent",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "exported",
            "exportedAs",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "exported",
            "exportedAs",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
    ];
    for (got, want) in keys.iter().zip(expected) {
        assert_eq!(got, want);
    }
    assert_eq!(keys.len(), expected.len());
}

#[test]
fn python_rows_keep_key_order_and_bytes() {
    expect(
        rows("a.py", PY, false),
        &[
            r#"{"name":"run","kind":"function","line":2,"startLine":1,"endLine":3,"exported":true}"#,
            r#"{"name":"K","kind":"class","line":5,"endLine":7,"exported":true}"#,
            r#"{"name":"m","kind":"method","line":6,"endLine":7,"parent":"K"}"#,
        ],
    );
}

#[test]
fn python_rows_with_provenance_keep_key_order_and_bytes() {
    let rows = rows("a.py", PY, true);
    assert_eq!(
        masked(&rows[0]),
        r#"{"name":"run","kind":"function","line":2,"occurrenceId":"H","sourceRange":{"endByte":30,"startByte":0},"selectionByteRange":{"endByte":18,"startByte":15},"parseAssessment":{"status":"parsed","scope":"declaration","range":{"start":{"line":0,"character":0},"end":{"line":2,"character":8}}},"startLine":1,"endLine":3,"exported":true,"sourceOrigin":{"analysisUnitId":"H","segments":[{"analysis":"included","branches":[{"kind":"copied","origin":{"rawRange":{"endByte":30,"startByte":0},"sourceId":"H"}}],"sourceRange":{"endByte":30,"startByte":0}}]},"sourceDigest":"H","selectionOrigin":{"analysisUnitId":"H","segments":[{"analysis":"included","branches":[{"kind":"copied","origin":{"rawRange":{"endByte":18,"startByte":15},"sourceId":"H"}}],"sourceRange":{"endByte":18,"startByte":15}}]}}"#
    );
    let keys: Vec<Vec<String>> = rows.iter().map(|row| keys_of(row)).collect();
    let expected: &[&[&str]] = &[
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "startLine",
            "endLine",
            "exported",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "endLine",
            "exported",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
        &[
            "name",
            "kind",
            "line",
            "occurrenceId",
            "sourceRange",
            "selectionByteRange",
            "parseAssessment",
            "endLine",
            "parent",
            "sourceOrigin",
            "sourceDigest",
            "selectionOrigin",
        ],
    ];
    for (got, want) in keys.iter().zip(expected) {
        assert_eq!(got, want);
    }
    assert_eq!(keys.len(), expected.len());
}
