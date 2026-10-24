//! Match values preserve parser-selected source; preview bounds do not rewrite it.
use super::test_rows::*;
use crate::tools::test_support::Fixture;
use serde_json::json;

#[test]
fn capture_text_preserves_literal_whitespace_line_endings_and_indentation() {
    for (file, source, selected, rule) in [
        (
            "quoted.ts",
            "sink(\"a  b\t c\");",
            "sink(\"a  b\t c\")",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "escaped.js",
            r"sink('a  b \' c');",
            r"sink('a  b \' c')",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "template.ts",
            "sink(`line  one\n  ${value}\n😀  three`);",
            "sink(`line  one\n  ${value}\n😀  three`)",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "regex.js",
            "sink(/a  b[\t ]+/gu);",
            "sink(/a  b[\t ]+/gu)",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "raw.rs",
            "fn main() { sink(r##\"line  one\n    \"quoted\"\n😀  end\"##); }",
            "sink(r##\"line  one\n    \"quoted\"\n😀  end\"##)",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "triple.py",
            "sink(\"\"\"line  one\n    line two\n\"\"\")",
            "sink(\"\"\"line  one\n    line two\n\"\"\")",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "block.py",
            "def choose(value):\r\n    if value:\r\n        return \"a  b\"\r\n    return \"c\t d\"\r\n",
            "def choose(value):\r\n    if value:\r\n        return \"a  b\"\r\n    return \"c\t d\"",
            json!({"kind":"function_definition"}),
        ),
        (
            "crlf.ts",
            "sink(\r\n  \"a\u{a0} b😀\"\r\n);",
            "sink(\r\n  \"a\u{a0} b😀\"\r\n)",
            json!({"pattern":"sink($A)"}),
        ),
        (
            "comments.ts",
            "function choose() {\n  // preserve  comment\n  return `a  b`;\n}",
            "function choose() {\n  // preserve  comment\n  return `a  b`;\n}",
            json!({"kind":"function_declaration"}),
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
