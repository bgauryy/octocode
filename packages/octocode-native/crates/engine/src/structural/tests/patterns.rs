//! Pattern matching per grammar: captures, expandos, immunity and extensions.

use super::*;

fn run_pattern(src: &str, ext: &str, pattern: &str) -> Vec<StructuralMatch> {
    search(src, ext, Some(pattern), None).expect("pattern search should succeed")
}

#[test]
fn finds_call_and_captures_single_metavar() {
    let src = "const a = foo(bar);\nconst b = foo(baz);\n";
    let matches = run_pattern(src, "ts", "foo($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start_line, 1);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["bar".to_string()][..])
    );
    assert_eq!(
        matches[1].metavars.get("X").map(Vec::as_slice),
        Some(&["baz".to_string()][..])
    );
}

/// Trailing punctuation
/// the pattern omits (a trailing argument comma, a statement terminator) is
/// not required of the candidate, as in ast-grep.
#[test]
fn pattern_search_preserves_optional_trailing_punctuation() {
    let src = "throw new Error(\"one\");\nthrow new Error(\n  `two ${x}`,\n);\nexport const answer = 42;\n";
    let errors = run_pattern(src, "ts", "new Error($MSG)");
    assert_eq!(
        errors.iter().map(|m| m.start_line).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        errors[1].metavars.get("MSG").map(Vec::as_slice),
        Some(&["`two ${x}`".to_owned()][..])
    );
    let exports = run_pattern(src, "ts", "export const $N = $V");
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].start_line, 5);
    assert_eq!(
        exports[0].metavars.get("N").map(Vec::as_slice),
        Some(&["answer".to_owned()][..])
    );
}

#[test]
fn captures_multi_metavar_as_list() {
    let src = "log(1, 2, 3);\n";
    let matches = run_pattern(src, "js", "log($$$ARGS)");
    assert_eq!(matches.len(), 1);
    // Multi-captures preserve punctuation so callers can reconstruct
    // argument lists without guessing where separators were.
    assert_eq!(
        matches[0].metavars.get("ARGS").map(Vec::as_slice),
        Some(
            &[
                "1".to_string(),
                ",".to_string(),
                "2".to_string(),
                ",".to_string(),
                "3".to_string()
            ][..]
        )
    );
}

#[test]
fn document_probe_matches_root_without_ellipsis_panic() {
    for ext in ["ts", "py"] {
        let matches = run_pattern("foo(a)\nbar(b)\n", ext, "$$$");
        assert_eq!(matches.len(), 1, "{ext} should return the document root");
        assert_eq!(matches[0].start_line, 1);
        assert!(!matches[0].text.is_empty());
        assert!(matches[0].metavars.is_empty());
    }
}

#[test]
fn comment_and_string_immunity() {
    // KPI #1: a literal `eval(x)` inside a comment and inside a string must
    // NOT match — only the real call site (line 3) does.
    let src = "// eval(evil)\nconst s = \"eval(evil)\";\neval(real);\n";
    let matches = run_pattern(src, "js", "eval($X)");
    assert_eq!(matches.len(), 1, "only the real call site matches");
    assert_eq!(matches[0].start_line, 3);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["real".to_string()][..])
    );
}

#[test]
fn python_pattern_with_expando_char() {
    // Python's expando char is µ, not $ — exercises pre_process_pattern.
    let src = "print(hello)\nprint(world)\n";
    let matches = run_pattern(src, "py", "print($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["hello".to_string()][..])
    );
}

#[test]
fn rust_pattern_with_expando_char() {
    let src = "fn main() {\n    println(a);\n    println(b);\n}\n";
    let matches = run_pattern(src, "rs", "println($X)");
    assert_eq!(matches.len(), 2);
}

#[cfg(feature = "tree-sitter-cuda")]
#[test]
fn cuda_pattern_matches_kernel_launch_and_captures_configuration() {
    let src = "void launch(int *values) { kernel<<<1, 32>>>(values); }\n";
    let matches = run_pattern(
        src,
        "cu",
        "void launch(int *$ARG) { kernel<<<$GRID, $BLOCK>>>($ARG); }",
    );
    assert_eq!(matches.len(), 1);
    for (name, expected) in [("GRID", "1"), ("BLOCK", "32"), ("ARG", "values")] {
        assert_eq!(
            matches[0].metavars.get(name).map(Vec::as_slice),
            Some(&[expected.to_owned()][..]),
            "{name}"
        );
    }
}

#[cfg(feature = "tree-sitter-asm")]
#[test]
fn assembly_pattern_uses_an_ascii_expando() {
    let src = "target:\n  mov %rax, %rbx\n  mov %rcx, %rdx\n";
    let matches = run_pattern(src, "asm", "mov $SOURCE, $DESTINATION");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("SOURCE").map(Vec::as_slice),
        Some(&["%rax".to_owned()][..])
    );
}

#[test]
fn relational_rule_inside_function() {
    // KPI: a rule that plain patterns cannot express — `await` calls that
    // are `inside` a for-loop. `stopBy: end` walks all ancestors.
    let src =
        "async function f() {\n  for (const x of xs) {\n    await g(x);\n  }\n  await h();\n}\n";
    let rule = json!({"pattern":"await $C","inside":{"kind":"for_in_statement","stopBy":"end"}});
    let matches = search(src, "ts", None, Some(&rule)).expect("rule search should succeed");
    assert_eq!(
        matches.len(),
        1,
        "only the await inside the for-loop matches"
    );
    assert_eq!(matches[0].start_line, 3);
}

#[test]
fn unsupported_extension_errors() {
    match search("x", "zzz", Some("foo()"), None) {
        Err(e) => assert!(e.contains("does not support")),
        Ok(_) => panic!("expected an unsupported-extension error"),
    }
}

#[test]
fn supported_extensions_are_rust_owned() {
    let exts = languages::supported_extensions();
    assert!(exts.contains(&"ts"));
    assert!(exts.contains(&"rs"));
}

// ── markup / style grammars (HTML/CSS) ────────────────────────────────────

// ── Scala ─────────────────────────────────────────────────────────────────

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_pattern_captures_call_argument() {
    // Expando is µ, so `$X` → `µX`, a valid Scala identifier.
    let src = "object M {\n  def f() = { println(hello); println(world) }\n}\n";
    let matches = run_pattern(src, "scala", "println($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["hello".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_comment_and_string_immunity() {
    // KPI #1: a `println(evil)` in a comment and in a string must NOT match.
    let src = "object M {\n  // println(evil)\n  val s = \"println(evil)\"\n  def go() = println(real)\n}\n";
    let matches = run_pattern(src, "scala", "println($X)");
    assert_eq!(matches.len(), 1, "only the real call site matches");
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["real".to_string()][..])
    );
}

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_extensions_are_supported() {
    let exts = languages::supported_extensions();
    for ext in ["scala", "sc", "sbt"] {
        assert!(exts.contains(&ext), "structural search must support .{ext}");
    }
}

// ── First-class extension aliases ─────────────────────────────────────────

#[test]
fn mts_uses_typescript_grammar_and_dollar_expando() {
    // `.mts` must resolve to the TS entry (expando `$`, not the µ fallback).
    let src = "const a = foo(bar);\nconst b = foo(baz);\n";
    let matches = run_pattern(src, "mts", "foo($X)");
    assert_eq!(matches.len(), 2);
    assert_eq!(
        matches[0].metavars.get("X").map(Vec::as_slice),
        Some(&["bar".to_string()][..])
    );
}

#[test]
fn retained_alias_extensions_are_supported() {
    let exts = languages::supported_extensions();
    for ext in ["mts", "cts", "pyi"] {
        assert!(exts.contains(&ext), "structural search must support .{ext}");
    }
}

/// A pattern whose root the grammar names by a `name` field is a
/// declaration: its kind, and its name unless that is a metavariable.
/// Expressions and calls are not declarations.
#[test]
fn a_named_root_pattern_is_a_declaration() {
    let rust = pattern_declaration("fn $NAME($$$ARGS) -> $R { $$$BODY }", "rs").expect("fn");
    assert_eq!(rust.kind, "function_item");
    assert_eq!(rust.name, None);
    let named = pattern_declaration("fn run() {}", "rs").expect("named fn");
    assert_eq!(
        named.name,
        Some(("identifier".to_owned(), "run".to_owned()))
    );
    let class = pattern_declaration("class $C { $$$B }", "ts").expect("class");
    assert_eq!(class.kind, "class_declaration");
    assert_eq!(pattern_declaration("$X.unwrap()", "rs"), None);
    assert_eq!(pattern_declaration("foo($A)", "ts"), None);
    // A call whose grammar names its method by a `name` field is a call.
    assert_eq!(pattern_declaration("Collections.sort($X)", "java"), None);
    assert_eq!(pattern_declaration("$O.sort($X)", "java"), None);
    // So is a path, though its grammar names its last segment `name`.
    assert_eq!(pattern_declaration("Ordering::SeqCst", "rs"), None);
    assert_eq!(pattern_declaration("std::chrono::seconds", "cpp"), None);
    assert_eq!(pattern_declaration("fn ???", "rs"), None);
}
