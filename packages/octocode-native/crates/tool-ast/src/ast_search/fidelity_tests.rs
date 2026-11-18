//! Match values preserve parser-selected source; preview bounds do not rewrite it.
use super::test_rows::*;
use octocode_runtime_base::test_support::Fixture;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn capture_text_preserves_literal_whitespace_line_endings_and_indentation() {
    for (file, source, selected, rule) in [
        (
            "quoted.ts",
            "sink(\"a  b\t c\");",
            "sink(\"a  b\t c\")",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "escaped.js",
            r"sink('a  b \' c');",
            r"sink('a  b \' c')",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "template.ts",
            "sink(`line  one\n  ${value}\n😀  three`);",
            "sink(`line  one\n  ${value}\n😀  three`)",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "regex.js",
            "sink(/a  b[\t ]+/gu);",
            "sink(/a  b[\t ]+/gu)",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "raw.rs",
            "fn main() { sink(r##\"line  one\n    \"quoted\"\n😀  end\"##); }",
            "sink(r##\"line  one\n    \"quoted\"\n😀  end\"##)",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "triple.py",
            "sink(\"\"\"line  one\n    line two\n\"\"\")",
            "sink(\"\"\"line  one\n    line two\n\"\"\")",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "block.py",
            "def choose(value):\r\n    if value:\r\n        return \"a  b\"\r\n    return \"c\t d\"\r\n",
            "def choose(value):\r\n    if value:\r\n        return \"a  b\"\r\n    return \"c\t d\"",
            json_fixture(r#"{"kind": "function_definition"}"#),
        ),
        (
            "crlf.ts",
            "sink(\r\n  \"a\u{a0} b😀\"\r\n);",
            "sink(\r\n  \"a\u{a0} b😀\"\r\n)",
            json_fixture(r#"{"pattern": "sink($A)"}"#),
        ),
        (
            "comments.ts",
            "function choose() {\n  // preserve  comment\n  return `a  b`;\n}",
            "function choose() {\n  // preserve  comment\n  return `a  b`;\n}",
            json_fixture(r#"{"kind": "function_declaration"}"#),
        ),
    ] {
        let root = Fixture::new();
        root.write(file, source);
        let out = run(
            &root.0,
            json!({"operation":"match","path":root.0.join(file),"rule":rule,"includeCaptures":true}),
        )
        .expect("source match");
        let rows = out["files"][0]["matches"].as_array().expect("matches");
        assert_eq!(rows.len(), 1, "{file}: {out}");
        assert_eq!(rows[0]["value"], selected, "{file}: {out}");
        assert!(rows[0].get("clipped").is_none(), "{file}: {out}");
    }
}

#[test]
fn complete_captures_preserve_multiline_and_unicode_source() {
    let root = Fixture::new();
    let source = "function choose(\n  value = \"a  b\",\n) {\n  return value;\n}";
    root.write("header.ts", source);
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("header.ts"),
        "rule":{"kind":"function_declaration"},"includeCaptures":true}),
    )
    .expect("complete match");
    assert_eq!(out["files"][0]["matches"][0]["value"], source);
    assert!(out["next"].get("expandCaptures").is_none());

    let prefix = "const icon = \"😀\"; ";
    let selected = "sink(\"😀  x\")";
    root.write("unicode.ts", format!("{prefix}{selected};\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("unicode.ts"),
        "rule":{"pattern":"sink($A)"},"includeCaptures":true}),
    )
    .expect("unicode capture");
    let row = &out["files"][0]["matches"][0];
    assert_eq!(row["value"], selected);
    assert_eq!(row["column"], prefix.encode_utf16().count() + 1);
}

#[test]
fn a_column_is_kept_only_where_two_matches_repeat_a_line_and_header() {
    let root = Fixture::new();
    root.write(
        "pair.ts",
        "export const both = [a.unwrap(), a.unwrap()];\nexport const one = b.unwrap();\n",
    );
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("pair.ts"),"rule":{"pattern":"$X.unwrap()"}}),
    )
    .expect("same-line matches");
    let rows = out["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(rows.len(), 3, "{out}");
    assert_ne!(rows[0], rows[1], "two hits on one line must differ: {out}");
    assert!(rows[0]["column"].is_number() && rows[1]["column"].is_number());
    assert_ne!(rows[0]["column"], rows[1]["column"]);
    assert!(rows[2].get("column").is_none(), "{out}");
}

/// Whether a row keeps its column is read over the whole result: a pair a
/// page boundary splits keeps its columns on both served pages, and a row
/// alone on its line has none on its page.
#[test]
fn a_pair_a_page_boundary_splits_keeps_its_columns_on_both_pages() {
    let root = Fixture::new();
    root.write(
        "same_line.rs",
        "fn pair(x: Option<u8>) -> (u8, u8) {\n    (x.unwrap(), x.unwrap())\n}\nfn one(y: Option<u8>) -> u8 { y.unwrap() }\n",
    );
    let pages = ast_chain(
        &root.0,
        json!({"operation":"match","path":root.0.join("same_line.rs"),
            "rule":{"pattern":"$X.unwrap()"},"limit":1}),
    );
    let rows = pages
        .iter()
        .flat_map(|page| {
            let served = octocode_runtime_base::test_support::published(
                octocode_runtime_base::id::ToolId::AstSearch,
                page,
            );
            served["results"][0]["data"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|file| file["matches"].as_array().cloned().unwrap_or_default())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 3, "one match per page: {rows:?}");
    assert_eq!(
        (rows[0]["line"].as_u64(), rows[1]["line"].as_u64()),
        (Some(2), Some(2)),
        "{rows:?}"
    );
    assert!(
        rows[0]["column"].is_number() && rows[1]["column"].is_number(),
        "{rows:?}"
    );
    assert_ne!(rows[0]["column"], rows[1]["column"], "{rows:?}");
    assert!(rows[2].get("column").is_none(), "{rows:?}");
}
