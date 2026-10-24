//! Patterns across a mixed-grammar scope, and recovered parses that could
//! hide a match.

use super::*;

/// A pattern that compiles in one grammar of a mixed scope cannot match the
/// files of a grammar it does not compile in: they are outside what it
/// applies to, not unread evidence. The result is whole (not partial, no
/// failure diagnostic, no terminal limit), and one warning names each such
/// language with its file count and the compiler's reason.
#[test]
fn a_pattern_that_does_not_apply_to_a_grammar_is_named_not_failed() {
    let root = Fixture::new();
    root.write("main.py", "def run(a):\n    return a\n");
    // Each spells the pattern's anchor, so the prefilter reads them all.
    for name in ["a.rs", "b.rs", "c.rs"] {
        root.write(name, "// def\nfn x() {}\n");
    }
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"def $F($$$A):\n    $$$B"},"view":"countMatches"}),
    )
    .expect("python matches");
    assert_eq!(out["matchCount"], 1, "{out}");
    for field in [
        "isPartial",
        "partialReasons",
        "terminalLimit",
        "diagnostics",
    ] {
        assert!(out.get(field).is_none(), "{field}: {out}");
    }
    // A pattern is written in one grammar: the files of the others are a
    // quiet count, not a warning.
    assert!(out.get("warnings").is_none(), "{out}");
    assert_eq!(out["skippedGrammars"], json!({"rust":3}), "{out}");
    // The grammar that compiles it stays the inferred scope.
    assert_eq!(out["inferredLanguage"], "python", "{out}");
}

/// A query that compiles in no grammar of the scope is still an error, and a
/// scope whose every grammar compiles it has nothing to disclose.
#[test]
fn a_pattern_that_compiles_nowhere_errors_and_one_that_compiles_everywhere_is_silent() {
    let root = Fixture::new();
    root.write("main.py", "def run(a):\n    return a\n");
    root.write("a.rs", "// def\nfn x() {}\n");
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"run("}}),
    )
    .expect_err("no grammar of the scope compiles it");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"view":"files"}),
    )
    .expect("every grammar compiles it");
    assert!(out.get("warnings").is_none(), "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");
}

/// A macro attribute before a method garbles the method's parse: the call
/// inside its body is no `call_expression`, so a call pattern misses it.
const GARBLED_CPP: &str = "class lexer {\n    JSON_HEDLEY_NON_NULL(2)\n    token_type scan_literal(const char_type* literal_text, const std::size_t length,\n                            token_type return_type)\n    {\n        JSON_ASSERT(char_traits<char_type>::to_char_type(current) == literal_text[0]);\n        return return_type;\n    }\n};\n";

/// The same macro attribute, with a call the parse keeps whole.
const LOCAL_ERROR_CPP: &str = "struct S {\n    JSON_HEDLEY_NON_NULL(2)\n    int scan(const char* a, int b)\n    {\n        JSON_ASSERT(a != nullptr);\n        return b;\n    }\n};\n";

/// A match over files whose recovered parse could hide a match says so, as
/// `symbols` does: partial with `parseRecovered`, and one warning with the
/// count and a few names. The count is the scan's, so every page states it;
/// a file whose error is away from the query's anchor is not counted.
#[test]
fn a_match_discloses_files_whose_recovered_parse_could_hide_a_match() {
    let root = Fixture::new();
    for name in ["a", "b", "c", "d"] {
        root.write(&format!("pkg/lib/{name}.cpp"), GARBLED_CPP);
    }
    root.write(
        "pkg/lib/clean.cpp",
        "int f() { JSON_ASSERT(1); return 2; }\n",
    );
    root.write("pkg/lib/local.cpp", LOCAL_ERROR_CPP);
    let row = json!({"operation":"match","path":root.0.join("pkg/lib"),"parser":"cpp","rule":{"pattern":"JSON_ASSERT($$$A)"}});
    let first = run_window(&root.0, row, 1).expect("first page");
    assert_eq!(first["isPartial"], json!(true), "{first}");
    assert_eq!(
        first["partialReasons"],
        json!(["parseRecovered"]),
        "{first}"
    );
    let warnings = first["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{first}");
    let warning = warnings[0].as_str().expect("warning");
    assert!(
        warning.starts_with("4 files parsed with syntax errors"),
        "{warning}"
    );
    // Paths read as the rows' do after the response stage, from the
    // workspace root, so each passes to localFetch verbatim.
    assert!(
        warning.contains("(pkg/lib/a.cpp, pkg/lib/b.cpp, pkg/lib/c.cpp, …)"),
        "{warning}"
    );
    assert!(!warning.contains("d.cpp"), "{warning}");
    assert!(
        !warning.contains("local.cpp") && !warning.contains("clean.cpp"),
        "{warning}"
    );
    assert!(first.get("coverage").is_none(), "{first}");
    // Every page states the same coverage.
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    let second = run_window(&root.0, next, 1).expect("second page");
    assert_eq!(second["warnings"], first["warnings"], "{second}");
    assert_eq!(
        second["partialReasons"],
        json!(["parseRecovered"]),
        "{second}"
    );
}

/// A recovered parse that cannot hide a match is no gap: errors away from
/// the anchor, and a clean file, leave the answer as complete as before.
#[test]
fn recovered_syntax_away_from_the_match_is_not_disclosed() {
    let root = Fixture::new();
    root.write("local.cpp", LOCAL_ERROR_CPP);
    root.write("clean.cpp", "int f() { JSON_ASSERT(1); return 2; }\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"cpp","rule":{"pattern":"JSON_ASSERT($$$A)"}}),
    )
    .expect("match runs");
    assert_eq!(out["matchCount"], 2, "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert!(out.get("warnings").is_none(), "{out}");
}

/// One file's disclosure names it once; a single-file scan carries it too.
#[test]
fn a_single_file_match_discloses_its_recovered_parse() {
    let root = Fixture::new();
    root.write("src/lexer.cpp", GARBLED_CPP);
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("src/lexer.cpp"),"rule":{"pattern":"JSON_ASSERT($$$A)"}}),
    )
    .expect("match runs");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
    let warning = out["warnings"][0].as_str().expect("warning");
    assert!(
        warning.starts_with("1 file parsed with syntax errors"),
        "{warning}"
    );
    assert!(warning.contains("(src/lexer.cpp)"), "{warning}");
}
