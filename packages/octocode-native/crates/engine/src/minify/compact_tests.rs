//! What `compact` must do to the measured fixtures in `minify/fixtures`, each
//! of which an earlier hand-rolled scan got wrong.

use super::apply::{WhitespaceOnly, apply_content_view_minification};
use super::layout::Indent;

fn compact(source: &str, path: &str) -> String {
    let view = apply_content_view_minification(source, path);
    assert_eq!(view.whitespace_only, None, "{path}");
    joined(view)
}

fn joined(view: super::apply::CompactView) -> String {
    view.lines.into_iter().map(|kept| kept.text).collect()
}

fn fixture(name: &str) -> String {
    let path = format!("{}/src/minify/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

#[test]
fn rust_lifetimes_do_not_open_a_string() {
    let view = compact(&fixture("e.rs"), "e.rs");
    for kept in [
        "r#\"raw // not comment \"#",
        "\"it's // not a comment\"",
        "\"tail // keep me\"",
        "'outer: loop",
    ] {
        assert!(view.contains(kept), "lost {kept}: {view}");
    }
    for removed in ["SAFETY", "// field", "// quote char", "still comment"] {
        assert!(!view.contains(removed), "kept {removed}: {view}");
    }
}

#[test]
fn raw_strings_end_where_the_language_ends_them() {
    let go = compact(&fixture("f.go"), "f.go");
    assert!(go.contains("`C:\\path\\`"), "{go}");
    assert!(go.contains("`a // not comment\n  b ${x`"), "{go}");
    assert!(!go.contains("// windows"), "{go}");
    assert!(!go.contains("// trailing"), "{go}");
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_raw_strings_end_where_the_language_ends_them() {
    let cpp = compact(&fixture("i.cpp"), "i.cpp");
    assert!(cpp.contains("R\"(a \" // c)\";"), "{cpp}");
    assert!(cpp.contains("int n = 1;"), "{cpp}");
    assert!(!cpp.contains("keep n"), "{cpp}");
}

#[test]
fn a_space_that_changes_a_selector_stays() {
    assert!(compact(&fixture("j.css"), "j.css").contains(".a :hover"));
    assert!(compact(&fixture("k.scss"), "k.scss").contains("& :hover"));
}

#[test]
fn sass_keeps_its_nesting() {
    let view = joined(apply_content_view_minification(
        &fixture("l.sass"),
        "l.sass",
    ));
    for line in ["  color: red", "  .b", "    margin: 0"] {
        assert!(view.contains(line), "lost {line:?}: {view}");
    }
}

#[test]
fn without_a_grammar_only_whitespace_goes_and_the_view_says_so() {
    for name in [
        "l.sass", "r.sh", "m.html", "aa.vue", "Makefile", "u.php", "ac.sql", "x.hs", "w.scm",
        "y.txt", "z.lock", "ab.xml",
    ] {
        let source = fixture(name);
        let view = apply_content_view_minification(&source, name);
        assert_eq!(
            view.whitespace_only,
            Some(WhitespaceOnly::NoGrammar),
            "{name}"
        );
        let squash =
            |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
        assert_eq!(squash(&joined(view)), squash(&source), "{name}");
    }
}

#[test]
fn markdown_and_data_stay_as_they_are() {
    for name in ["s.md", "t.md", "n.json", "o.jsonc", "p.yaml", "q.toml"] {
        let source = fixture(name);
        let view = apply_content_view_minification(&source, name);
        assert_eq!(view.whitespace_only, None, "{name}");
        assert!(view.as_is, "{name}");
        assert_eq!(joined(view), source, "{name}");
    }
    // Code, with a grammar or without one, is laid out again, never as is.
    for name in ["a.ts", "y.txt"] {
        assert!(
            !apply_content_view_minification(&fixture(name), name).as_is,
            "{name}"
        );
    }
}

#[test]
fn a_source_the_grammar_cannot_read_keeps_its_comments_and_says_so() {
    let view = apply_content_view_minification("fn main( {\n// c\n", "broken.rs");
    assert_eq!(view.whitespace_only, Some(WhitespaceOnly::SyntaxErrors));
    assert_eq!(joined(view), "fn main( {\n// c\n");
}

#[test]
fn a_citation_is_the_line_the_text_is_on() {
    let source = "/* block\n   foo();\n*/\n\nfoo();\n";
    let view = apply_content_view_minification(source, "a.ts");
    assert_eq!(view.lines.len(), 1);
    assert_eq!(view.lines[0].line, 5, "the comment's foo() is on line 2");
}

#[test]
fn indentation_stays_only_in_a_grammar_that_scans_it() {
    let python = "def f():\n    return 1\n";
    assert_eq!(super::apply::indent_of(python, "m.py"), Some(Indent::Keep));
    assert!(compact(python, "m.py").contains("\n    return 1\n"));
    for (source, name) in [("fn f() {\n}\n", "m.rs"), ("func f() {\n}\n", "m.go")] {
        assert_eq!(
            super::apply::indent_of(source, name),
            Some(Indent::Strip),
            "{name}"
        );
    }
    #[cfg(feature = "tree-sitter-scala")]
    assert_eq!(
        super::apply::indent_of("object A:\n  val b = 1\n", "m.scala"),
        Some(Indent::Keep)
    );
}

#[test]
fn a_docstring_stays_a_comment_goes() {
    let source = "def f():\n    \"\"\"doc\"\"\"\n    # gone\n    return 1\n";
    let view = compact(source, "m.py");
    assert!(view.contains("doc"), "{view}");
    assert!(!view.contains("gone"), "{view}");
}

#[test]
fn a_shebang_stays_and_a_comment_never_glues_tokens() {
    assert!(compact("#!/usr/bin/env python3\nx = 1\n", "m.py").starts_with("#!/usr/bin/env"));
    assert!(compact("#!/usr/bin/env node\nf();\n", "m.js").starts_with("#!/usr/bin/env"));
    assert_eq!(
        compact("let a = b/**/+/**/+c;\n", "m.js"),
        "let a = b + +c;\n"
    );
}

#[test]
fn python_strings_that_are_values_stay() {
    let view = compact(&fixture("d.py"), "d.py");
    assert!(view.contains("value string that matters"), "{view}");
    assert!(view.contains("kept after SQL"), "{view}");
    assert!(!view.contains("comment between statements"), "{view}");
}

#[test]
fn jsx_text_is_content() {
    let view = compact(&fixture("c.jsx"), "c.jsx");
    assert!(view.contains("Hello , world ( yes )"), "{view}");
    assert!(view.contains("see // here is text, it's fine"), "{view}");
    assert!(!view.contains("jsx comment"), "{view}");
}

/// No deep-stack worker: the parse, walk and layout keep their own stacks,
/// so a deeply nested source compacts on a small thread stack.
#[test]
fn deep_nesting_compacts_on_a_small_stack() {
    let depth = 20_000;
    let source = format!(
        "let x = {}1{}; // gone\n",
        "[".repeat(depth),
        "]".repeat(depth)
    );
    let view = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || joined(apply_content_view_minification(&source, "deep.js")))
        .expect("thread")
        .join()
        .expect("no stack overflow");
    assert!(view.ends_with("1".to_owned().as_str()) || view.contains("];\n"));
    assert!(!view.contains("gone"));
}

#[test]
fn what_a_grammar_cannot_read_stays_and_the_rest_compacts() {
    let source = "#define DECL(x) x\nDECL(int) a = 1; // gone\nint b = 2; /* gone */\n";
    let view = apply_content_view_minification(source, "m.c");
    let text = joined(view.clone());
    assert!(
        !text.contains("gone") || text.contains("DECL(int)"),
        "{text}"
    );
    assert_ne!(view.whitespace_only, Some(WhitespaceOnly::NoGrammar));
}

#[test]
fn go_build_lines_and_cgo_preambles_stay() {
    let view = compact(&fixture("f.go"), "f.go");
    for kept in [
        "//go:build linux",
        "// +build linux",
        "//go:generate stringer -type=Kind",
        "//go:embed data.txt",
        "#include <stdio.h>",
    ] {
        assert!(view.contains(kept), "lost {kept}: {view}");
    }
    assert!(!view.contains("Package p docs"), "{view}");
}

#[test]
fn a_stopped_parse_is_whitespace_only_and_not_reused() {
    let lexing = super::apply::lex("fn f() {} // c\n", "m.rs", &|| true);
    assert!(!lexing.reusable());
    let view = super::apply::compact_view("fn f() {} // c\n", &lexing);
    assert_eq!(view.whitespace_only, Some(WhitespaceOnly::Interrupted));
    assert!(super::apply::lex("fn f() {}\n", "m.rs", &|| false).reusable());
}

#[test]
fn css_and_scss_compact_by_their_grammars() {
    let css = ".c :hover { color : red ; } /* gone */\n.d{background:url(//cdn.x/a.png)}\n.e { width: calc(100% - 2px); content: \"a  /* kept */  b\"; }\n";
    let view = compact(css, "m.css");
    for kept in [
        ".c :hover",
        "url(//cdn.x/a.png)",
        "calc(100% - 2px)",
        "\"a  /* kept */  b\"",
    ] {
        assert!(view.contains(kept), "lost {kept}: {view}");
    }
    assert!(!view.contains("gone"), "{view}");
    let scss = "// gone\n.a { & :hover { color: red; } &:focus { b: c; } }\n$u: url(//x.y/z);\n";
    let view = compact(scss, "m.scss");
    assert!(
        view.contains("& :hover") && view.contains("&:focus"),
        "{view}"
    );
    assert!(
        view.contains("url(//x.y/z)") && !view.contains("gone"),
        "{view}"
    );
}

#[test]
fn a_token_with_whitespace_inside_stays_whole() {
    let view = compact("#  define A 1\n#\tif A\n#  endif\n", "m.c");
    assert!(
        view.contains("#  define A") && view.contains("#\tif"),
        "{view:?}"
    );
    let crlf = compact("#!/usr/bin/env node\r\nf();\r\n", "m.mjs");
    assert!(crlf.starts_with("#!/usr/bin/env node\r\n"), "{crlf:?}");
}

#[test]
fn a_lone_carriage_return_stays_a_line_break() {
    let view = compact("let a = 1\r// c\rlet b = 2\r\n", "m.js");
    // A line terminator still ends the first statement.
    let between = &view[view.find('1').unwrap()..view.find("let b").unwrap()];
    assert!(
        between.contains('\r') && !between.contains("//"),
        "{view:?}"
    );
    assert!(view.ends_with("let b = 2\n"), "{view:?}");
}

/// CSS reads a backslash-newline in a string as part of the string, where
/// the grammar sees an error: the string's lines stay byte for byte.
#[test]
fn a_css_string_keeps_its_continued_lines() {
    let css = ".c { d: e; } /* gone */\n.a { b: url('<s v=\"0 0 \\\n    4 5\"><p d=\"M6\"/>'); }\n.f { g: h; }\n";
    let view = compact(css, "m.css");
    assert!(
        view.contains("'<s v=\"0 0 \\\n    4 5\"><p d=\"M6\"/>'"),
        "{view:?}"
    );
    assert!(!view.contains("gone"), "{view:?}");
}

/// A `//` line the grammar reads as `/` and a comment is a comment from its
/// first slash: no slash of it is left alone in the view.
#[test]
fn a_scss_comment_line_leaves_no_slash() {
    let scss = "@use \"q\" as *;\n\n/////\n// H\n/////\n\n%h1 {\n  b: c;\n}\n";
    let view = compact(scss, "m.scss");
    assert!(!view.replace("/////", "").contains('/'), "{view:?}");
    assert!(view.contains("%h1 {\nb: c;\n}"), "{view:?}");
}

/// CSS has no `//` comment: the grammar's `js_comment` is text of a rule.
#[test]
fn a_css_double_slash_line_stays_and_a_scss_one_goes() {
    let source = "// not a comment\n* { box-sizing: border-box; }\n";
    let css = compact(source, "m.css");
    assert!(css.contains("// not a comment"), "{css:?}");
    let scss = compact(source, "m.scss");
    assert!(!scss.contains("not a comment"), "{scss:?}");
}
