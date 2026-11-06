//! Compaction must not change code (`minify::verify`), on one fixture per
//! shape that went wrong before. The corpus sweep is
//! `examples/compact_sweep.rs`.

use std::path::Path;

use super::verify::verify;
use crate::signatures::languages;

#[test]
fn fixtures_keep_their_tokens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/minify/fixtures");
    let mut checked = 0;
    for entry in std::fs::read_dir(&root).expect("fixtures").flatten() {
        let path = entry.path().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).expect("fixture");
        if let Err(change) = verify(&path, &source) {
            panic!("{path}: {change}");
        }
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn empty_macro_replacements_keep_their_preprocessing_tokens() {
    for ending in ["\n", "\r\n"] {
        let source = [
            "#ifndef FIXTURE_H",
            "#define FIXTURE_H",
            "#define Ignore(value, message) ",
            "#define Trace(value) ",
            "#define TraceMore(value) ",
            "#define LIMIT 29",
            "#endif",
            "",
        ]
        .join(ending);
        verify("fixture.h", &source).unwrap_or_else(|error| panic!("{error}: {source:?}"));
    }
}

#[test]
fn shapes_that_changed_code_before_keep_their_tokens() {
    for (path, source) in [
        ("m.c", "#define A \\\n  b \\\n\n\n#define C 1\n"),
        ("m.c", "#  define A 1\n#\tif A\n#  endif\n"),
        (
            "m.c",
            "#define DO() { \\\n  /* c */ \\\n  a = 1; \\\n}\nint x;\n",
        ),
        ("m.mjs", "#!/usr/bin/env node\r\nf();\r\n"),
        ("m.js", "let a = b/**/+/**/+c;\n"),
        (
            "m.rs",
            "fn f<'a>(s: &'a str) -> &'a str { r#\"raw // x\"# ; s } // c\n",
        ),
        ("m.go", "var a = `C:\\path\\` // c\nvar b = 1\n"),
        (
            "m.py",
            "def f():\n    \"\"\"doc\"\"\"\n    # c\n    return 1\n",
        ),
        (
            "m.css",
            ".c { d: e; } /* c */\n.a { b: url('<s v=\"0 0 \\\n    4 5\"><p d=\"M6\"/>'); }\n.f { g: h; }\n",
        ),
        (
            "m.scss",
            "@use \"q\" as *;\n\n/////\n// H\n/////\n\n%h1 {\n  b: c;\n}\n",
        ),
        ("m.css", "// not a comment\n* { box-sizing: border-box; }\n"),
    ] {
        if let Err(change) = verify(path, source) {
            panic!("{path} {source:?}: {change}");
        }
    }
}

/// Malformed input whose error recovery is not linear compacts as whitespace
/// only, under the parse's work bound, with no change of code.
#[test]
fn runaway_recovery_falls_back_to_whitespace_only() {
    let source = "a=/[ ".repeat(64 * 1024 / 5);
    let view = super::apply::apply_content_view_minification(&source, "m.js");
    assert_eq!(
        view.whitespace_only,
        Some(super::apply::WhitespaceOnly::SyntaxErrors)
    );
}

#[test]
fn every_grammar_names_a_comment_the_one_rule_finds() {
    for extension in languages::supported_extensions() {
        let entry = languages::find_entry(extension).expect("a grammar");
        let names: Vec<&str> = (0..entry.language.node_kind_count())
            .filter_map(|id| entry.language.node_kind_for_id(id as u16))
            .collect();
        assert!(
            names.iter().any(|name| name.contains("comment")),
            "{}: names no comment",
            entry.name
        );
    }
}
