//! A single file's grammar: its extension, or an explicit `language` when
//! the extension selects no grammar (an extensionless script, a `.txt`).
use super::{AstSearchQuery, execute_ast};
use crate::execution::cancel::NeverCancel;
use crate::id::ToolId;
use crate::response::result::ToolError;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};

fn run(root: &std::path::Path, query: Value) -> Result<Value, ToolError> {
    let (paths, security) = simple_policy(root);
    let query: AstSearchQuery = serde_json::from_value(query).expect("typed astSearch row");
    execute_ast(&query, &paths, &security, &NeverCancel, None)
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
            json!({"operation":"symbols","path":file,"language":"typescript"}),
        )
        .expect("symbols");
        assert_eq!(symbols["symbols"][0]["symbolName"], "hello", "{symbols}");
        assert_eq!(symbols["symbols"][0]["kind"], "function", "{symbols}");
        let matched = run(
            &root.0,
            json!({"operation":"match","path":file,"language":"typescript","rule":"kind: function_declaration"}),
        )
        .expect("match");
        assert_eq!(matched["matchCount"], 1, "{matched}");
        assert!(matched.get("isPartial").is_none(), "{matched}");
        let tree = run(
            &root.0,
            json!({"operation":"syntaxTree","path":file,"language":"typescript"}),
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
        json!({"operation":"symbols","path":file,"language":"typescript"}),
    )
    .expect_err("python is not typescript");
    assert_eq!(error.code, "languageMismatch");
    assert!(error.message.contains(".py"), "{}", error.message);
    assert!(error.message.contains("python"), "{}", error.message);
    // The message states the repair; the generic hint would contradict it.
    assert_eq!(
        ToolId::AstSearch.output().error_hint("languageMismatch"),
        Some(crate::response::output::NO_HINT)
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
        json!({"operation":"match","path":file,"pattern":"foo($A)"}),
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
    let globs = json!({"typescript":["noext","*.tmpl","*.typescript"]});
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":"kind: function_declaration","languageGlobs":globs}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 4, "{out}");
    let paths = matched_paths(&out);
    for name in NAMED_FILES {
        assert!(
            paths.iter().any(|path| path.ends_with(&format!("/{name}"))),
            "{name}: {out}"
        );
    }
    assert!(out.get("warnings").is_none(), "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");

    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("noext"),"rule":"kind: function_declaration","languageGlobs":globs}),
    )
    .expect_err("a single file takes language");
    assert_eq!(error.code, "languageDirectoryRequired", "{}", error.message);
}

/// Without `languageGlobs`, the files no grammar parses are named in one
/// warning by their workspace paths, with the field that maps them to a
/// grammar; the parsed scope stays whole.
#[test]
fn match_reports_files_skipped_without_a_grammar() {
    let root = Fixture::new();
    for name in NAMED_FILES {
        root.write(name, SCRIPT);
    }
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":"kind: function_declaration"}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    let warnings = out["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{out}");
    let warning = warnings[0].as_str().expect("warning text");
    assert!(warning.starts_with("3 files skipped"), "{warning}");
    // Named as row paths read: workspace-relative, so each can be read as is.
    assert!(
        warning.contains("(d.tsx.tmpl, e.typescript, noext)"),
        "{warning}"
    );
    assert!(warning.contains("languageGlobs maps"), "{warning}");
    assert!(out.get("isPartial").is_none(), "{out}");
}

/// `language` restricts the scope to that language's files: a file of no
/// grammar (or of another one) was never in it, so it is not a skipped file
/// and no warning names it, while the same walk without `language` does.
#[test]
fn match_with_language_does_not_report_files_outside_its_scope() {
    let root = Fixture::new();
    for name in NAMED_FILES {
        root.write(name, SCRIPT);
    }
    root.write("CMakeLists.txt", "project(x)\n");
    let query = |extra: Value| {
        let mut row =
            json!({"operation":"match","path":root.0,"rule":"kind: function_declaration"});
        row.as_object_mut()
            .expect("row")
            .extend(extra.as_object().expect("extra").clone());
        run(&root.0, row).expect("match")
    };
    let restricted = query(json!({"language":"typescript"}));
    assert_eq!(restricted["matchCount"], 1, "{restricted}");
    assert!(restricted.get("warnings").is_none(), "{restricted}");
    assert!(restricted.get("isPartial").is_none(), "{restricted}");
    let unrestricted = query(json!({}));
    let warning = unrestricted["warnings"][0].as_str().expect("warning");
    assert!(warning.starts_with("4 files skipped"), "{warning}");
    assert!(warning.contains("CMakeLists.txt"), "{warning}");
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
            json!({"symbolName":"kernel","kind":"function","line":4,"endLine":5,"startLine":2,"docStartLine":1}),
        ),
        (
            "k.cpp",
            "template <typename T>\nstruct S {\n};\n",
            json!({"symbolName":"S","kind":"struct","line":2,"endLine":3,"startLine":1}),
        ),
        (
            "k.py",
            "@dec\n@dec2(1)\ndef foo():\n    pass\n",
            json!({"symbolName":"foo","kind":"function","line":3,"endLine":4,"startLine":1}),
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
    let hint = ToolId::AstSearch
        .output()
        .error_hint("symbolsUnsupported")
        .expect("hint");
    assert!(
        hint.contains("language") && hint.contains("languageGlobs"),
        "{hint}"
    );
    assert!(!hint.contains("Broaden"), "{hint}");
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
                        .is_some_and(|text| text.contains("language:\"cpp\""))
            })
    };
    let symbols = run(&root.0, json!({"operation":"symbols","path":header})).expect("symbols");
    assert!(suggests(&symbols), "{symbols}");
    let matched = run(
        &root.0,
        json!({"operation":"match","path":header,"rule":"kind: identifier"}),
    )
    .expect("match");
    assert!(suggests(&matched), "{matched}");
    let cpp = run(
        &root.0,
        json!({"operation":"symbols","path":header,"language":"cpp"}),
    )
    .expect("cpp symbols");
    assert!(!suggests(&cpp), "{cpp}");
    // A plain C header parses cleanly as C: no suggestion.
    std::fs::write(&header, "int foo(int a);\nstruct S { int y; };\n").expect("C header");
    let plain = run(&root.0, json!({"operation":"symbols","path":header})).expect("symbols");
    assert!(!suggests(&plain), "{plain}");
}

/// An outline parsed under an explicit `language` reuses its cached parse:
/// the entry is keyed by the parser extension actually used, so a second
/// identical call hits it and another override never reads it.
#[test]
fn an_overridden_outline_is_cached_under_the_parser_it_used() {
    let root = Fixture::new();
    let file = root.0.join("cached-script");
    std::fs::write(&file, SCRIPT).expect("source");
    let row = json!({"operation":"symbols","path":file,"language":"typescript"});
    let first = run(&root.0, row.clone()).expect("first symbols");
    let canonical = file.canonicalize().expect("canonical");
    let canonical = canonical.to_string_lossy();
    let typescript =
        crate::tools::ast_rule::primary_extension("typescript").expect("typescript extension");
    let cached =
        crate::tools::declarations_cache::document(SCRIPT, &canonical, Some(&typescript), || {
            panic!("the typescript outline is cached")
        });
    assert!(cached.is_some(), "the overridden parse is cached");
    let second = run(&root.0, row).expect("second symbols");
    assert_eq!(second["symbols"], first["symbols"], "{second}");
    assert_eq!(second["symbols"][0]["symbolName"], "hello", "{second}");
    let javascript =
        crate::tools::ast_rule::primary_extension("javascript").expect("javascript extension");
    assert_ne!(javascript, typescript);
    let other =
        crate::tools::declarations_cache::document(SCRIPT, &canonical, Some(&javascript), || None);
    assert!(other.is_none(), "another override never reuses the entry");
}
