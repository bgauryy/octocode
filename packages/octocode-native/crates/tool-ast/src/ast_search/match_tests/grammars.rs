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
/// failure diagnostic, no terminal limit, no warning). Beside matches they
/// changed nothing and are not stated; an empty answer counts them per
/// grammar and names the grammar it inferred.
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
    // A pattern is written in one grammar: the files of the others are
    // no warning, and beside its matches no count either.
    assert!(out.get("warnings").is_none(), "{out}");
    assert_unqualified(&out);
    // The same pattern asking for a second parameter matches nothing: the
    // empty answer counts the files of the other grammar, quietly.
    let none = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"def $F($A, $B):\n    $$$C"},"view":"countMatches"}),
    )
    .expect("python parses it");
    assert_eq!(none["status"], "empty", "{none}");
    for field in ["isPartial", "partialReasons", "terminalLimit", "warnings"] {
        assert!(none.get(field).is_none(), "{field}: {none}");
    }
    assert_eq!(
        none["skippedGrammars"],
        json_fixture(r#"{"rust": 3}"#),
        "{none}"
    );
    // The grammar that compiles it stays the inferred scope.
    assert_eq!(none["inferredLanguage"], "python", "{none}");
}

/// Under a rule language, fixed by `parser` or inferred from the scan, a file
/// no grammar parses whose name is of a known other type (ripgrep's default
/// type table) draws no warning: an empty answer under an inferred grammar
/// counts it quietly in `skippedFormats` (one with matches states neither
/// the count nor the grammar), and `parser`, which narrowed the walk to its
/// own grammar's files, states no count. Only a file of unknown
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
    // The scope holds a function and no struct: an answer with a match
    // and an empty one, each with and without `parser`.
    for (parser, kind) in [
        (None, "function_item"),
        (Some("rust"), "function_item"),
        (None, "struct_item"),
        (Some("rust"), "struct_item"),
    ] {
        let mut query = json!({"operation":"match","path":root.0,"rule":{"kind":kind}});
        if let Some(parser) = parser {
            query["parser"] = json!(parser);
        }
        let out = run(&root.0, query).expect("match");
        let empty = kind == "struct_item";
        if empty {
            assert_eq!(out["status"], "empty", "{parser:?}: {out}");
            assert_eq!(
                out.get("inferredLanguage"),
                parser.is_none().then(|| json!("rust")).as_ref(),
                "{parser:?}: {out}"
            );
        } else {
            assert_eq!(out["matchCount"], 1, "{parser:?}: {out}");
            assert_unqualified(&out);
        }
        let notices = out["notices"].to_string();
        assert!(notices.contains("1 file skipped"), "{parser:?}: {out}");
        // The one file the warning counts is named once, in `skippedPaths`.
        let unparsed: Vec<&str> = out["skippedPaths"]["entries"]
            .as_array()
            .expect("skipped entries")
            .iter()
            .filter(|entry| entry["cause"] == "unparsed")
            .filter_map(|entry| entry["path"].as_str())
            .collect();
        assert!(
            matches!(unparsed.as_slice(), [path] if path.ends_with("src/gen.rs.in")),
            "{parser:?}: {out}"
        );
        assert!(!notices.contains("gen.rs.in"), "{parser:?}: {out}");
        for typed in ["Cargo.toml", "site.css", "page.html"] {
            assert!(!notices.contains(typed), "{parser:?} {typed}: {out}");
            assert!(
                !unparsed.iter().any(|path| path.ends_with(typed)),
                "{parser:?} {typed}: {out}"
            );
        }
        // `parser` asked for one grammar's files: the other formats are
        // outside the request, so only an inferred grammar counts them.
        // A format holds no source: never among the unsearched grammars.
        assert!(out.get("skippedGrammars").is_none(), "{parser:?}: {out}");
        let counts = out.get("skippedFormats");
        if empty && parser.is_none() {
            assert_eq!(
                counts,
                Some(&json_fixture(r#"{"css": 1, "html": 1, "toml": 1}"#)),
                "{out}"
            );
        } else {
            assert!(counts.is_none(), "{parser:?}: {out}");
        }
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
    assert!(!notices.contains("gen.rs.in"), "{mixed}");
    assert!(
        mixed["skippedPaths"].to_string().contains("gen.rs.in"),
        "{mixed}"
    );
    assert_unqualified(&mixed);
    // The same scope asked for a call no file makes: the formats that hold
    // no source of either grammar qualify the empty answer.
    let none = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"zzqneverzz($A)"},"view":"files"}),
    )
    .expect("several grammars");
    assert_eq!(none["status"], "empty", "{none}");
    assert!(none.get("inferredLanguage").is_none(), "{none}");
    assert!(none.get("skippedGrammars").is_none(), "{none}");
    assert_eq!(
        none["skippedFormats"],
        json_fixture(r#"{"css": 1, "html": 1, "toml": 1}"#),
        "{none}"
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

/// The source files a rule did not search (`skippedGrammars`: the grammars
/// it does not compile in) are counted apart from the files that hold no
/// source (`skippedFormats`). A rule that spells no pattern asks by kind,
/// which reads as a question of every grammar: one note adds what the
/// counts cannot, each such grammar's compiler reason (its nearest kinds),
/// and repeats neither the counts nor what the field says. A pattern is
/// code of one grammar: no such note. The counts qualify an empty answer;
/// one with matches states the note alone.
#[test]
fn a_kind_rule_names_the_grammars_it_does_not_compile_in() {
    let root = Fixture::new();
    root.write("a.ts", "function alpha() { return 1; }\n");
    root.write("p.py", "def py():\n    return 1\n");
    root.write("r.rs", "fn r() {}\n");
    root.write("d.json", "{}\n");
    let python_notes = |out: &serde_json::Value| {
        out["notices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|note| note.contains("python: "))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    // `rule` matches one node; `none`, the same rule narrowed to a name no
    // file holds, matches nothing, and its empty answer states the counts.
    let notices = |rule: serde_json::Value, none: serde_json::Value| {
        let matched = run(
            &root.0,
            json!({"operation":"match","path":root.0,"rule":rule}),
        )
        .expect("match");
        assert_eq!(matched["matchCount"], 1, "{matched}");
        assert_unqualified(&matched);
        let out = run(
            &root.0,
            json!({"operation":"match","path":root.0,"rule":none}),
        )
        .expect("empty match");
        assert_eq!(out["status"], "empty", "{out}");
        // The note is the same beside matches and without them.
        assert_eq!(python_notes(&matched), python_notes(&out), "{out}");
        let counts = out["skippedGrammars"].as_object().expect("counts").clone();
        // The format no grammar reads is counted apart, in both cases.
        assert_eq!(out["skippedFormats"], json!({"json": 1}), "{out}");
        let notes = python_notes(&out);
        (counts, notes, out)
    };
    let (counts, notes, out) = notices(
        json!({"kind":"function_declaration"}),
        json!({"kind":"function_declaration","regex":"^zzqneverzz$"}),
    );
    let [note] = notes.as_slice() else {
        panic!("one note names the grammars: {out}");
    };
    // Every counted grammar is named with its reason, and only those are
    // counted; the note states no count and no format.
    assert_eq!(
        counts.keys().collect::<Vec<_>>(),
        ["python", "rust"],
        "{out}"
    );
    for grammar in counts.keys() {
        assert!(note.contains(&format!("{grammar}: ")), "{grammar}: {note}");
    }
    assert!(!note.contains("json"), "{note}");
    assert!(!note.chars().any(|c| c.is_ascii_digit()), "{note}");
    assert!(!note.contains("not searched"), "the field says it: {note}");
    // Each grammar's own kind for the same thing is in its reason.
    for kind in ["function_definition", "function_item"] {
        assert!(note.contains(kind), "{kind}: {note}");
    }
    let (counts, notes, out) = notices(
        json!({"pattern":"function $N() { $$$B }"}),
        json!({"pattern":"function zzqneverzz() { $$$B }"}),
    );
    assert!(counts.contains_key("python"), "{out}");
    assert!(notes.is_empty(), "a pattern is one grammar's code: {out}");
}
