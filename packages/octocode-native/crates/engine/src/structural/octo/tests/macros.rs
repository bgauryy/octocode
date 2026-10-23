//! Matching inside Rust macro token trees and C-family #define bodies.

use super::*;

const RUST_MACRO_SOURCE: &str = "fn t() {\n    assert!(status.unwrap().success());\n    assert_eq!(&read.unwrap(), b\"x\");\n    let v = vec![a.unwrap(); 2];\n    outer!(inner!(deep.unwrap()));\n    plain.unwrap();\n}\n";

#[test]
fn rust_patterns_match_inside_macro_token_trees() {
    let found = run_pattern(RUST_MACRO_SOURCE, "rs", "$X.unwrap()");
    let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "status.unwrap()",
            "read.unwrap()",
            "a.unwrap()",
            "deep.unwrap()",
            "plain.unwrap()"
        ]
    );
    // Positions stay in file coordinates.
    assert_eq!((found[0].start_line, found[0].start_col), (2, 12));
    assert_eq!(found[0].metavar_ranges["X"][0].line, 2);
    let rule = run_rule(RUST_MACRO_SOURCE, "rs", json!({"pattern":"$X.unwrap()"}));
    assert_eq!(rule.len(), 5);
}

/// A DSL macro body re-parsed as code never matches inside its strings:
/// `json!({"pattern":"pub fn source() {}"})` holds no function.
#[test]
fn rust_macro_bodies_never_match_inside_string_literals() {
    let source = "fn t() {\n    let q = json!({\n        \"operation\":\"match\",\"path\":root,\n        \"pattern\":\"pub fn source() {}\",\"maxFiles\":1\n    });\n}\n";
    let found = run_pattern(source, "rs", "fn $NAME() {}");
    assert!(
        found
            .iter()
            .all(|m| m.text != "fn source() {}" && m.text != "pub fn source() {}"),
        "{:?}",
        found.iter().map(|m| &m.text).collect::<Vec<_>>()
    );
}

#[test]
fn rust_macro_bodies_keep_enclosing_ancestors_and_skip_the_delimiter_wrapper() {
    let inside = run_rule(
        RUST_MACRO_SOURCE,
        "rs",
        json!({"pattern":"$X.unwrap()","inside":{"kind":"function_item","stopBy":"end"}}),
    );
    assert_eq!(inside.len(), 5);
    let not_inside = run_rule(
        RUST_MACRO_SOURCE,
        "rs",
        json!({"pattern":"$X.unwrap()","not":{"inside":{"kind":"function_item","stopBy":"end"}}}),
    );
    assert!(not_inside.is_empty());
    // `assert!(x)`'s own parentheses are delimiters, not a parenthesized expression.
    let tuples = run_rule(
        "fn t() { assert_eq!(a, b); f((c, d)); }\n",
        "rs",
        json!({"kind":"tuple_expression"}),
    );
    assert_eq!(tuples.len(), 1);
    assert_eq!(tuples[0].text, "(c, d)");
    // The kind-only fast path expands macro bodies too.
    assert_eq!(
        run_rule(RUST_MACRO_SOURCE, "rs", json!({"kind":"call_expression"})).len(),
        6
    );
}

/// A `macro_rules!` template body is searched like an invocation's
/// arguments (re-parsed in place), never silently skipped.
#[test]
fn rust_macro_rules_template_bodies_are_searched() {
    let source = "macro_rules! get {\n    ($e:expr) => {\n        config.value.unwrap()\n    };\n}\nfn main() {\n    other.unwrap();\n}\n";
    let found = run_pattern(source, "rs", "$X.unwrap()");
    let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts, ["config.value.unwrap()", "other.unwrap()"]);
    assert_eq!((found[0].start_line, found[0].start_col), (3, 8));
    // The matcher pattern half (`($e:expr)`) is no source and stays flat.
    assert!(run_rule(source, "rs", json!({"kind":"parenthesized_expression"})).is_empty());
}

#[test]
fn rust_macro_bodies_without_the_anchor_keep_flat_tokens() {
    // Flat token-tree identifiers already match; a re-parsed body replaces
    // its flat tokens, so nothing is reported twice.
    let found = run_rule(
        "fn t() { assert!(ready); log!(ready, other(ready)); }\n",
        "rs",
        json!({"kind":"identifier","regex":"^ready$"}),
    );
    assert_eq!(found.len(), 3);
    // A body without the pattern's anchor keeps its flat tokens.
    assert!(run_pattern("fn t() { assert!(ok); }\n", "rs", "$X.unwrap()").is_empty());
}

const C_DEFINE_SOURCE: &str = "#define NODE_FREE(array) zfree((array)->alloc)\n#define LIMIT limit(8)\n#define RESET(p) do { p = zfree(p); } while (0)\n#define WRAPPED(p) \\\n    zfree(p)\nvoid f(void) { zfree(q); }\n";

/// `#define` bodies are opaque `preproc_arg` text to tree-sitter; they are
/// re-parsed in place in the file's own grammar (C, C++ and CUDA alike), so a
/// pattern reaches the calls inside them at file coordinates.
#[test]
fn c_family_patterns_match_inside_define_bodies() {
    for ext in ["c", "cpp", "cu"] {
        let found = run_pattern(C_DEFINE_SOURCE, ext, "zfree($$$A)");
        let texts: Vec<_> = found.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            texts,
            ["zfree((array)->alloc)", "zfree(p)", "zfree(p)", "zfree(q)"],
            ".{ext}"
        );
        assert_eq!((found[0].start_line, found[0].start_col), (1, 25), ".{ext}");
        assert_eq!((found[1].start_line, found[1].start_col), (3, 26), ".{ext}");
        // A body continued past a backslash-newline keeps its own line.
        assert_eq!((found[2].start_line, found[2].start_col), (5, 4), ".{ext}");
        assert_eq!(found[0].metavar_ranges["A"][0].line, 1, ".{ext}");
        let limit = run_pattern(C_DEFINE_SOURCE, ext, "limit($N)");
        assert_eq!(limit.len(), 1, ".{ext}");
        assert_eq!(limit[0].metavars["N"], ["8"], ".{ext}");
    }
}

#[test]
fn c_family_define_bodies_keep_enclosing_ancestors_and_the_kind_fast_path() {
    let inside = run_rule(
        C_DEFINE_SOURCE,
        "c",
        json!({"pattern":"zfree($$$A)","inside":{"kind":"preproc_function_def","stopBy":"end"}}),
    );
    assert_eq!(inside.len(), 3);
    let outside = run_rule(
        C_DEFINE_SOURCE,
        "c",
        json!({"pattern":"zfree($$$A)","not":{"inside":{"kind":"preproc_function_def","stopBy":"end"}}}),
    );
    assert_eq!(outside.len(), 1);
    // The whole body can be the match: no delimiter wrapper hides it.
    assert_eq!(
        run_rule(C_DEFINE_SOURCE, "c", json!({"kind":"call_expression"})).len(),
        5
    );
}

/// A body that is no expression or statement (a bare brace, a type, a token
/// paste, a string, nothing) yields no match and no recovered syntax.
#[test]
fn c_family_define_bodies_that_are_not_code_add_no_noise() {
    let source = "#define OPEN {\n#define TYPE unsigned int\n#define EMPTY\n#define CAT(a, b) a ## b\n#define MSG \"zfree(x)\"\n#define LIST 1, 2, 3\n";
    for ext in ["c", "cpp", "cu"] {
        assert!(run_pattern(source, ext, "zfree($$$A)").is_empty(), ".{ext}");
        assert!(
            run_rule(source, ext, json!({"kind":"ERROR"})).is_empty(),
            ".{ext}"
        );
        assert!(
            run_rule(source, ext, json!({"kind":"call_expression"})).is_empty(),
            ".{ext}"
        );
    }
}
