//! A single file's grammar: its extension, or an explicit `language` when
//! the extension selects no grammar (an extensionless script, a `.txt`).
use super::{AstSearchQuery, execute_ast};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::response::output::ToolOutput as _;
use octocode_runtime_base::response::result::ToolError;
use octocode_runtime_base::test_support::{Fixture, simple_policy};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn run(root: &std::path::Path, query: Value) -> Result<Value, ToolError> {
    let (paths, security) = simple_policy(root);
    let query: AstSearchQuery = serde_json::from_value(query).expect("typed astSearch row");
    execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &NeverCancel,
        None,
    )
}

const SCRIPT: &str = "export function hello(a: number) { return a }\n";

/// `language` parses a file whose extension maps to no grammar, in every
/// operation: symbols, match and syntaxTree.
#[test]
fn an_explicit_language_parses_a_file_whose_extension_selects_no_grammar() {
    let root = Fixture::new();
    for name in ["script", "notes.txt"] {
        let file = root.0.join(name);
        std::fs::write(&file, SCRIPT).expect("source");
        let symbols = run(
            &root.0,
            json!({"operation":"symbols","path":file,"parser":"typescript"}),
        )
        .expect("symbols");
        assert_eq!(symbols["symbols"][0]["symbolName"], "hello", "{symbols}");
        assert_eq!(symbols["symbols"][0]["kind"], "function", "{symbols}");
        let matched = run(
            &root.0,
            json!({"operation":"match","path":file,"parser":"typescript","rule":{"kind":"function_declaration"}}),
        )
        .expect("match");
        assert_eq!(matched["matchCount"], 1, "{matched}");
        assert!(matched.get("isPartial").is_none(), "{matched}");
        let tree = run(
            &root.0,
            json!({"operation":"syntaxTree","path":file,"parser":"typescript"}),
        )
        .expect("tree");
        assert_eq!(tree["isPartial"], false, "{tree}");
        assert_eq!(tree["nodes"][1], "1 export_statement 1:1-1:46 ^0", "{tree}");
    }
}

/// An extension of another grammar is still a mismatch, and the message
/// names that extension and its grammar.
#[test]
fn a_file_of_another_grammar_is_a_mismatch_that_names_its_extension() {
    let root = Fixture::new();
    let file = root.0.join("b.py");
    std::fs::write(&file, "def f():\n    pass\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"symbols","path":file,"parser":"typescript"}),
    )
    .expect_err("python is not typescript");
    assert_eq!(error.code, ErrorCode::LanguageMismatch);
    assert!(error.message.contains(".py"), "{}", error.message);
    assert!(error.message.contains("python"), "{}", error.message);
    // The message states the repair; the generic hint would contradict it.
    assert_eq!(
        crate::ast_search::Output.error_hint(ErrorCode::LanguageMismatch, &serde_json::Value::Null),
        Some(octocode_runtime_base::response::output::NO_HINT)
    );
}

/// Without `language`, a file with no extension is unsupported, and the
/// diagnostic says it has no extension (never a `.txt` it does not have).
#[test]
fn an_extensionless_file_without_language_names_no_extension() {
    let root = Fixture::new();
    let file = root.0.join("noext");
    std::fs::write(&file, "foo(1)\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"pattern":"foo($A)"}}),
    )
    .expect("row");
    let message = out["diagnostics"][0]["message"]
        .as_str()
        .expect("diagnostic");
    assert!(message.contains("no extension"), "{out}");
    assert!(!message.contains(".txt"), "{out}");
    assert_eq!(out["isPartial"], true, "{out}");
}

/// One file per way a name can select a grammar: an own extension, none, a
/// custom suffix after a grammar's, and a suffix no grammar claims.
const NAMED_FILES: [&str; 4] = ["a.ts", "noext", "d.tsx.tmpl", "e.typescript"];

fn matched_paths(out: &Value) -> Vec<&str> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["path"].as_str())
        .collect()
}

/// `languageGlobs` names the parser of directory files whose names select
/// none: a directory `match` parses every one of them, and only a single
/// file takes `language` instead.
#[test]
fn match_parses_files_named_by_language_globs() {
    let root = Fixture::new();
    for name in NAMED_FILES {
        root.write(name, SCRIPT);
    }
    let globs = json_fixture(r#"{"typescript": ["noext", "*.tmpl", "*.typescript"]}"#);
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"kind":"function_declaration"},"parserGlobs":globs}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 4, "{out}");
    let paths = matched_paths(&out);
    for name in NAMED_FILES {
        assert!(
            paths.iter().any(|path| std::path::Path::new(path)
                .file_name()
                .is_some_and(|file| file == name)),
            "{name}: {out}"
        );
    }
    assert!(out.get("warnings").is_none(), "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");

    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("noext"),"rule":{"kind":"function_declaration"},"parserGlobs":globs}),
    )
    .expect_err("a single file takes language");
    assert_eq!(
        error.code,
        ErrorCode::LanguageDirectoryRequired,
        "{}",
        error.message
    );
}

/// Without `languageGlobs`, the files no grammar parses are counted in one
/// warning, with the cause and the field that maps them to a grammar, and
/// each is named once, by its workspace path, in the page's `skippedPaths`
/// (cause `unparsed`): the warning names none of them again. The parsed
/// scope stays whole. A file of binary content (an image) is source of no
/// language: never counted or named.
#[test]
fn match_reports_files_skipped_without_a_grammar() {
    let root = Fixture::new();
    for name in NAMED_FILES {
        root.write(name, SCRIPT);
    }
    root.write("logo.png", "\u{89}PNG\r\n\u{1a}\n\u{0}\u{0}\u{0}\rIHDR");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"kind":"function_declaration"}}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    let warnings = out["notices"].as_array().expect("notices");
    assert_eq!(warnings.len(), 1, "{out}");
    let warning = warnings[0].as_str().expect("warning text");
    let unparsed: Vec<&str> = out["skippedPaths"]["entries"]
        .as_array()
        .expect("skipped entries")
        .iter()
        .filter(|entry| entry["cause"] == "unparsed")
        .map(|entry| entry["path"].as_str().expect("a path"))
        .collect();
    // Each a file of the searched root (the response stage spells row paths).
    let named: Vec<&str> = unparsed
        .iter()
        .filter_map(|path| path.rsplit('/').next())
        .collect();
    assert_eq!(named, ["d.tsx.tmpl", "e.typescript", "noext"], "{out}");
    assert!(
        warning.starts_with(&format!("{} files skipped", unparsed.len())),
        "{warning}"
    );
    for name in named {
        assert!(!warning.contains(name), "{name} is named twice: {warning}");
    }
    assert!(warning.contains("parserGlobs maps"), "{warning}");
    assert!(out.get("isPartial").is_none(), "{out}");
}

/// Under one language, `parser` or the one the walk infers, a file of no
/// grammar whose name is of a known other type (`CMakeLists.txt`) is never
/// a warning: an empty answer under an inferred grammar counts it quietly
/// in `skippedFormats`, and an answer with matches states neither the
/// count nor the grammar it inferred. Only the files of unknown type, which
/// could be the language's source, are named as unproven absence.
#[test]
fn match_with_language_does_not_report_files_outside_its_scope() {
    let root = Fixture::new();
    for name in NAMED_FILES {
        root.write(name, SCRIPT);
    }
    root.write("CMakeLists.txt", "project(x)\n");
    let query = |kind: &str, extra: Value| {
        let mut row = json!({"operation":"match","path":root.0,"rule":{"kind":kind}});
        row.as_object_mut()
            .expect("row")
            .extend(extra.as_object().expect("extra").clone());
        run(&root.0, row).expect("match")
    };
    let parser = || json_fixture(r#"{"parser": "typescript"}"#);
    let restricted = query("function_declaration", parser());
    assert_eq!(restricted["matchCount"], 1, "{restricted}");
    assert!(restricted.get("warnings").is_none(), "{restricted}");
    assert!(restricted.get("isPartial").is_none(), "{restricted}");
    let unrestricted = query("function_declaration", json!({}));
    assert_eq!(unrestricted["matchCount"], 1, "{unrestricted}");
    // The same scope with no node of the kind: the empty answers.
    let restricted_empty = query("class_declaration", parser());
    let unrestricted_empty = query("class_declaration", json!({}));
    for empty in [&restricted_empty, &unrestricted_empty] {
        assert_eq!(empty["status"], "empty", "{empty}");
    }
    assert_eq!(
        unrestricted_empty["inferredLanguage"], "typescript",
        "{unrestricted_empty}"
    );
    // An empty answer under an inferred grammar counts the other format
    // quietly; `parser` asked for one grammar's files, so it states no
    // count of the others; an answer with matches states neither.
    for (out, counts) in [
        (&restricted, None),
        (&unrestricted, None),
        (&restricted_empty, None),
        (&unrestricted_empty, Some(json_fixture(r#"{"cmake": 1}"#))),
    ] {
        let warning = out["notices"][0].as_str().expect("notice");
        assert!(warning.starts_with("3 files skipped"), "{warning}");
        assert!(!warning.contains("CMakeLists.txt"), "{warning}");
        assert_eq!(out.get("skippedFormats"), counts.as_ref(), "{out}");
        assert!(out.get("skippedGrammars").is_none(), "{out}");
    }
    for matched in [&restricted, &unrestricted] {
        super::test_rows::assert_unqualified(matched);
    }
}

/// A templated declaration's `startLine` is its `template <…>` header (C++
/// and CUDA), a decorated Python one's first decorator; `line` stays on the
/// name and `endLine` at the end.
#[test]
fn symbols_start_at_the_template_or_decorator_header() {
    let root = Fixture::new();
    let cases = [
        (
            "k.cuh",
            "// doc\ntemplate<typename T>\n__global__ void\nkernel(T x) {\n}\n",
            json_fixture(
                r#"{
                    "symbolName": "kernel",
                    "kind": "function",
                    "line": 4,
                    "endLine": 5,
                    "startLine": 2,
                    "docStartLine": 1
                }"#,
            ),
        ),
        (
            "k.cpp",
            "template <typename T>\nstruct S {\n};\n",
            json_fixture(
                r#"{
                    "symbolName": "S",
                    "kind": "struct",
                    "line": 2,
                    "endLine": 3,
                    "startLine": 1
                }"#,
            ),
        ),
        (
            "k.py",
            "@dec\n@dec2(1)\ndef foo():\n    pass\n",
            json_fixture(
                r#"{
                    "symbolName": "foo",
                    "kind": "function",
                    "line": 3,
                    "endLine": 4,
                    "startLine": 1
                }"#,
            ),
        ),
    ];
    for (name, source, expected) in cases {
        let file = root.0.join(name);
        std::fs::write(&file, source).expect("source");
        let out = run(&root.0, json!({"operation":"symbols","path":file})).expect("symbols");
        let row = &out["symbols"][0];
        for (field, value) in expected.as_object().expect("expected") {
            assert_eq!(&row[field], value, "{name}.{field}: {out}");
        }
    }
}

/// Symbols of a file no extractor reads is `symbolsUnsupported` whether it
/// has an unknown extension or none, never an empty outline; the hint leads
/// to `language` / `languageGlobs`.
#[test]
fn symbols_of_a_file_without_a_grammar_are_unsupported_not_empty() {
    let root = Fixture::new();
    for name in ["noext", "x.txt"] {
        let file = root.0.join(name);
        std::fs::write(&file, SCRIPT).expect("source");
        let out = run(&root.0, json!({"operation":"symbols","path":file})).expect("row");
        assert_eq!(out["status"], "error", "{out}");
        assert_eq!(out["errorCode"], "symbolsUnsupported", "{out}");
    }
    let hint = crate::ast_search::Output
        .error_hint(ErrorCode::SymbolsUnsupported, &serde_json::Value::Null)
        .expect("hint");
    assert!(
        hint.contains("parser") && hint.contains("parserGlobs"),
        "{hint}"
    );
    assert!(!hint.contains("Broaden"), "{hint}");
}

/// Symbols of a file whose grammar is known but has no declaration
/// extractor (JSON) is `symbolsUnsupported` for that file type: setting
/// `parser` cannot help, so the row says so and leads to the localFetch
/// outline of the file instead.
#[test]
fn symbols_of_a_grammar_without_an_extractor_lead_to_the_fetch_outline() {
    let root = Fixture::new();
    let file = root.0.join("package.json");
    std::fs::write(
        &file,
        "{\n  \"name\": \"x\",\n  \"scripts\": {\"a\": \"b\"}\n}\n",
    )
    .expect("json");
    let out = run(&root.0, json!({"operation":"symbols","path":file})).expect("row");
    assert_eq!(out["status"], "error", "{out}");
    assert_eq!(out["errorCode"], "symbolsUnsupported", "{out}");
    let text = out.to_string();
    assert!(!text.contains("Set parser"), "{out}");
    assert_eq!(
        out["hints"],
        json_fixture(
            r#"[
                "symbols does not outline .json files; the related[] localFetch request outlines this one, or search it with localSearch."
            ]"#
        ),
        "{out}"
    );
    let read =
        octocode_runtime_base::test_support::checks::related_call(&out, "localFetch", |_| true);
    assert_eq!(read["tool"], "localFetch", "{out}");
    let row = read.pointer("/input/queries/0").unwrap_or(&read["input"]);
    assert_eq!(row["view"], "symbols", "{out}");
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("package.json")),
        "{out}"
    );
}

/// A `.h` that parses with errors under C but cleanly under C++ says so: the
/// outline under C is unreliable (a class reads as a function).
#[test]
fn a_cpp_header_parsed_as_c_suggests_cpp() {
    let root = Fixture::new();
    let header = root.0.join("a.h");
    std::fs::write(&header, "int foo(int a);\nclass X { int y; };\n").expect("header");
    let suggests = |out: &Value| {
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|diagnostic| {
                diagnostic["code"] == "structural.header.cpp"
                    && diagnostic["message"]
                        .as_str()
                        .is_some_and(|text| text.contains("parser:\"cpp\""))
            })
    };
    let symbols = run(&root.0, json!({"operation":"symbols","path":header})).expect("symbols");
    assert!(suggests(&symbols), "{symbols}");
    let matched = run(
        &root.0,
        json!({"operation":"match","path":header,"rule":{"kind":"identifier"}}),
    )
    .expect("match");
    assert!(suggests(&matched), "{matched}");
    let cpp = run(
        &root.0,
        json!({"operation":"symbols","path":header,"parser":"cpp"}),
    )
    .expect("cpp symbols");
    assert!(!suggests(&cpp), "{cpp}");
    // A plain C header parses cleanly as C: no suggestion.
    std::fs::write(&header, "int foo(int a);\nstruct S { int y; };\n").expect("C header");
    let plain = run(&root.0, json!({"operation":"symbols","path":header})).expect("symbols");
    assert!(!suggests(&plain), "{plain}");
}

/// An explicit parser is applied on every request, independently of another grammar.
#[test]
fn an_overridden_outline_uses_the_requested_parser() {
    let root = Fixture::new();
    let file = root.0.join("script");
    std::fs::write(&file, SCRIPT).expect("source");
    let row = json!({"operation":"symbols","path":file,"parser":"typescript"});
    let first = run(&root.0, row.clone()).expect("first symbols");
    let second = run(&root.0, row).expect("second symbols");
    assert_eq!(second["symbols"], first["symbols"], "{second}");
    assert_eq!(second["symbols"][0]["symbolName"], "hello", "{second}");
    let canonical = file.canonicalize().expect("canonical");
    let canonical = canonical.to_string_lossy();
    for parser in ["typescript", "javascript"] {
        let extension =
            octocode_tools_shared::ast_rule::primary_extension(parser).expect("extension");
        let actual =
            octocode_tools_shared::declarations::document(SCRIPT, &canonical, Some(&extension));
        let expected = octocode_engine::signatures::extract_declarations_document_with_extension(
            SCRIPT, &canonical, &extension,
        );
        assert_eq!(actual.as_deref(), expected.as_ref(), "{parser}");
    }
}

/// A directory outline takes `parser` as `match` does: each file still
/// reads by its own extension (or `parserGlobs`), and the call succeeds.
#[test]
fn a_directory_outline_accepts_parser_and_reads_each_file_by_its_extension() {
    let root = Fixture::new();
    root.write("a.ts", "export function item() {}\n");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"parser":"typescript"}),
    )
    .expect("parser on a directory");
    assert!(out.to_string().contains("item"), "{out}");
}

/// `symbols` over a directory skips a one-line source whose parse is
/// estimated past the memory budget, with the same `fileTooLarge` skip the
/// other operations name, and outlines the rest.
#[test]
fn directory_symbols_skip_a_source_too_dense_to_parse_within_the_budget() {
    let root = Fixture::new();
    let budget = octocode_engine::structural::parse_budget();
    let calls = budget / octocode_engine::structural::BYTES_PER_TOKEN / 5 + 1000;
    std::fs::write(
        root.0.join("dense.js"),
        format!("var a=[{}];\n", "f(1),".repeat(calls)),
    )
    .expect("dense");
    std::fs::write(root.0.join("plain.js"), "function hello() { return 1 }\n").expect("plain");
    let out = run(&root.0, json!({"operation":"symbols","path":root.0})).expect("row");
    let served = out.to_string();
    assert!(served.contains("hello"), "{out}");
    assert!(
        served.contains("dense.js") && served.contains("too") && out["isPartial"] == true,
        "{out}"
    );
}
