//! Patterns across a mixed-grammar scope, and recovered parses that could
//! hide a match.

use super::*;
use octocode_test_support::json_fixture;

/// A kind no grammar knows, over a directory, is refused in the grammar the
/// scope holds, with that grammar's nearest kinds to retry with.
#[test]
fn an_unknown_kind_names_the_scopes_nearest_kinds() {
    let root = Fixture::new();
    root.write("src/a.rs", "fn a() {}\n");
    root.write("src/b.rs", "fn b() {}\n");
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"kind":"function_decl"}}),
    )
    .expect_err("unknown kind");
    assert!(error.message.contains("function_item"), "{}", error.message);
}

/// A string written in either quote style matches the other: the import of
/// `"node:fs"` and of `'node:fs'` are one import to a pattern of either.
#[test]
fn quote_style_does_not_decide_a_match() {
    let root = Fixture::new();
    root.write("ts/a.ts", "import { a } from \"node:fs\";\n");
    root.write("ts/b.ts", "import { b } from 'node:fs';\n");
    root.write(
        "py/c.py",
        "p.add_parser('start')\np.add_parser(\"start\")\n",
    );
    for (pattern, dir) in [
        ("import { $$$A } from \"node:fs\"", "ts"),
        ("import { $$$A } from 'node:fs'", "ts"),
        ("$P.add_parser('start')", "py"),
        ("$P.add_parser(\"start\")", "py"),
    ] {
        let out = run(
            &root.0,
            json!({"operation":"match","path":root.0.join(dir),"rule":{"pattern":pattern}}),
        )
        .expect("match");
        assert_eq!(out["matchCount"], json!(2), "{pattern}: {out}");
    }
}

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
    assert_eq!(
        out["skippedGrammars"],
        json_fixture(r#"{"rust": 3}"#),
        "{out}"
    );
    // The grammar that compiles it stays the inferred scope.
    assert_eq!(out["inferredLanguage"], "python", "{out}");
}

/// Under a rule language, fixed by `parser` or inferred from the scan, a file
/// no grammar parses whose name is of a known other type (ripgrep's default
/// type table) is a quiet `skippedGrammars` count; only a file of unknown
/// type, which could hold the language's source, says absence there is
/// unproven. Without one language the same holds for every grammar the scan
/// reads: a JSON, CSS or HTML file holds none of their source.
#[test]
fn under_a_rule_language_only_files_of_unknown_type_warn() {
    let root = Fixture::new();
    root.write("src/lib.rs", "fn a() {}\n");
    root.write("src/gen.rs.in", "fn b() {}\n");
    root.write("Cargo.toml", "[package]\nname = \"x\"\n");
    root.write("site.css", "a {}\n");
    root.write("page.html", "<p></p>\n");
    for parser in [None, Some("rust")] {
        let mut query = json!({"operation":"match","path":root.0,"rule":{"kind":"function_item"}});
        if let Some(parser) = parser {
            query["parser"] = json!(parser);
        } else {
            assert_eq!(
                run(&root.0, query.clone()).expect("match")["inferredLanguage"],
                "rust"
            );
        }
        let out = run(&root.0, query).expect("match");
        assert_eq!(out["matchCount"], 1, "{parser:?}: {out}");
        let notices = out["notices"].to_string();
        assert!(notices.contains("1 file skipped"), "{parser:?}: {out}");
        assert!(notices.contains("gen.rs.in"), "{parser:?}: {out}");
        for typed in ["Cargo.toml", "site.css", "page.html"] {
            assert!(!notices.contains(typed), "{parser:?} {typed}: {out}");
        }
        assert_eq!(
            out["skippedGrammars"],
            json_fixture(r#"{"css": 1, "html": 1, "toml": 1}"#),
            "{parser:?}: {out}"
        );
    }
    root.write("ts/a.ts", "run(1);\n");
    let mixed = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"view":"files"}),
    )
    .expect("several grammars");
    assert!(mixed.get("inferredLanguage").is_none(), "{mixed}");
    let notices = mixed["notices"].to_string();
    assert!(notices.contains("1 file skipped"), "{mixed}");
    assert!(notices.contains("gen.rs.in"), "{mixed}");
    assert_eq!(
        mixed["skippedGrammars"],
        json_fixture(r#"{"css": 1, "html": 1, "toml": 1}"#),
        "{mixed}"
    );
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
/// count and a few names. Each scan window states the files it read, so the
/// walk names every such file once; a file whose error is away from the
/// query's anchor is not counted.
#[test]
fn a_match_discloses_files_whose_recovered_parse_could_hide_a_match() {
    let root = Fixture::new();
    for name in ["a", "b", "c", "d"] {
        root.write(&format!("pkg/lib/{name}.cpp"), GARBLED_CPP);
    }
    // Two matches here and one in local.cpp: page 1 holds one, so the walk
    // goes on past the recovered files.
    root.write(
        "pkg/lib/clean.cpp",
        "int f() { JSON_ASSERT(1); JSON_ASSERT(2); return 2; }\n",
    );
    root.write("pkg/lib/local.cpp", LOCAL_ERROR_CPP);
    let row = json!({"operation":"match","path":root.0.join("pkg/lib"),"parser":"cpp","rule":{"pattern":"JSON_ASSERT($$$A)"}});
    let mut pending = vec![row];
    let mut named = Vec::new();
    let mut counted = 0;
    let mut pages = 0;
    while let Some(call) = pending.pop() {
        let page = run_window(&root.0, call, 1).expect("page");
        pages += 1;
        assert!(pages < 20, "nonterminating walk");
        // The recovered-parse notice, once on a window's page; the
        // multi-page set's narrowing notice rides beside it.
        let warnings = page["notices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|notice| notice.contains("parsed with syntax errors"))
            .collect::<Vec<_>>();
        assert!(warnings.len() <= 1, "{page}");
        if let Some(warning) = warnings.first() {
            assert_eq!(page["isPartial"], json!(true), "{page}");
            assert_eq!(
                page["partialReasons"],
                json_fixture(r#"["parseRecovered"]"#),
                "{page}"
            );
            assert!(page.get("coverage").is_none(), "{page}");
            counted += warning
                .split_whitespace()
                .next()
                .and_then(|count| count.parse::<usize>().ok())
                .expect("a count");
            // Paths read as the rows' do after the response stage, from the
            // workspace root, so each passes to localFetch verbatim.
            for name in ["a", "b", "c", "d", "clean", "local"] {
                if warning.contains(&format!("pkg/lib/{name}.cpp")) {
                    named.push(name);
                }
            }
        }
        if let Some(call) = page["next"].get("nextPage") {
            pending.push(call["input"]["queries"][0].clone());
        }
    }
    assert_eq!(counted, 4, "each recovered file once across the walk");
    named.sort_unstable();
    assert_eq!(named, ["a", "b", "c", "d"], "never clean.cpp or local.cpp");
}

/// A coverage cut is a fact of the scan, not of a page: every page names
/// its reason, so no page says `isPartial` beside `hasMore` without one.
#[test]
fn a_scans_partial_reason_is_stated_on_every_page() {
    let root = Fixture::new();
    for name in ["a", "b", "c", "d"] {
        root.write(&format!("lib/{name}.cpp"), GARBLED_CPP);
    }
    for name in ["e", "f", "g"] {
        root.write(
            &format!("lib/{name}.cpp"),
            "int f() { JSON_ASSERT(1); return 2; }\n",
        );
    }
    let mut pending = Some(
        json!({"operation":"match","path":root.0.join("lib"),"parser":"cpp","rule":{"pattern":"JSON_ASSERT($$$A)"},"view":"files"}),
    );
    let mut pages = Vec::new();
    while let Some(call) = pending.take() {
        let page = run_window(&root.0, call, 1).expect("page");
        assert!(pages.len() < 20, "nonterminating walk");
        pending = page["next"]
            .get("nextPage")
            .map(|call| call["input"]["queries"][0].clone());
        pages.push(page);
    }
    assert!(pages.len() > 2, "{pages:#?}");
    for (at, page) in pages.iter().enumerate() {
        assert_eq!(page["isPartial"], json!(true), "page {at}: {page}");
        assert_eq!(
            page["partialReasons"],
            json_fixture(r#"["parseRecovered"]"#),
            "page {at}: {page}"
        );
    }
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
    assert_eq!(
        out["partialReasons"],
        json_fixture(r#"["parseRecovered"]"#),
        "{out}"
    );
    let warning = out["notices"][0].as_str().expect("notice");
    assert!(
        warning.starts_with("1 file parsed with syntax errors"),
        "{warning}"
    );
    assert!(warning.contains("(src/lexer.cpp)"), "{warning}");
}
