//! astSearch scope: path policy, walk disclosure, cancellation and
//! grammar selection.
use super::test_rows::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{
    AfterRoot, Fixture, checks, sensitive_fixture, simple_policy, skipped_reasons,
};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[test]
fn capped_incompatible_grammar_preserves_partial_coverage_and_reaches_later_grammar() {
    let root = Fixture::new();
    root.write("a.py", "def ordinary():\n    return 1\n");
    root.write("z.ts", "export function target() { return 2; }\n");
    for selector in [json!({}), json!({"include":["*.py", "*.ts"]})] {
        let mut query = json!({"operation":"match","path":root.0,
            "rule":{"kind":"function_declaration"},"maxFiles":1,"captureText":true});
        query
            .as_object_mut()
            .expect("query")
            .extend(selector.as_object().expect("selector").clone());
        let first = run(&root.0, query).expect("bounded grammar coverage");
        // The window is partial because of its cut; the python file the
        // rule does not compile in is outside the rule's scope, named in a
        // warning, not a compile failure.
        assert_eq!(first["isPartial"], true, "{first}");
        let reasons = first["partialReasons"].as_array().expect("cuts");
        assert!(!reasons.contains(&json!("compileFailed")), "{first}");
        assert!(reasons.contains(&json!("maxFiles")), "{first}");
        assert!(
            !first["diagnostics"].to_string().contains("a.py"),
            "{first}"
        );
        let warning = first["warnings"][0].as_str().expect("warning");
        assert!(
            warning.starts_with("The query does not apply to python (1 file: "),
            "{first}"
        );
        let next = first["next"]["expandScan"]["query"]["queries"][0].clone();
        assert!(next.is_object(), "{first}");
        assert!(next.get("language").is_none(), "{next}");
        let after = run(&root.0, next).expect("later compatible grammar");
        assert_eq!(after["matchCount"], 1, "{after}");
        assert!(
            after["files"][0]["matches"][0]["value"]
                .as_str()
                .expect("value")
                .contains("function target"),
            "{after}"
        );
        assert!(after["next"].get("expandScan").is_none(), "{after}");
    }
    for selector in [
        json!({"pattern":"function $F() { $$$BODY }"}),
        json!({"rule":{"kind":"function_declaration"}, "include":["*.ts"]}),
        json!({"rule":{"kind":"function_declaration"}, "exclude":["*.py"]}),
    ] {
        let mut query = json!({"operation":"match","path":root.0,"maxFiles":1,"captureText":true});
        query
            .as_object_mut()
            .expect("query")
            .extend(selector.as_object().expect("selector").clone());
        let mut out = run(&root.0, query).expect("valid selector beyond incompatible grammar");
        let mut matches = Vec::new();
        for _ in 0..3 {
            matches.extend(
                out["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|file| {
                        file["matches"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|found| found["value"].clone())
                    }),
            );
            let Some(next) = out["next"].get("expandScan") else {
                break;
            };
            out = run(&root.0, next["query"]["queries"][0].clone()).expect("copied pattern scan");
        }
        assert_eq!(matches.len(), 1, "{matches:?}: {out}");
        assert!(
            matches[0]
                .as_str()
                .expect("match")
                .contains("function target"),
            "{matches:?}"
        );
        assert!(out["next"].get("expandScan").is_none(), "{out}");
    }
    for selector in [
        json!({"rule":{"kind":"not_a_node"}}),
        json!({"language":"python", "rule":{"kind":"function_declaration"}}),
    ] {
        let mut query = json!({"operation":"match","path":root.0,"maxFiles":1});
        query
            .as_object_mut()
            .expect("query")
            .extend(selector.as_object().expect("selector").clone());
        let error = run(&root.0, query).expect_err("invalid selector");
        assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    }
    let query = json!({"operation":"match","path":root.0,"maxFiles":1,"pattern":"target("});
    let first = run(&root.0, query).expect("bounded pattern coverage");
    assert_eq!(first["isPartial"], true, "{first}");
    let next = first["next"]["expandScan"]["query"]["queries"][0].clone();
    assert!(next.is_object(), "{first}");
    let last = run(&root.0, next).expect("remaining incompatible grammar is partial coverage");
    assert_eq!(last["isPartial"], true, "{last}");
    assert!(
        last["partialReasons"]
            .as_array()
            .expect("cuts")
            .contains(&json!("compileFailed")),
        "{last}"
    );
    assert!(last["next"].get("expandScan").is_none(), "{last}");
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,
        "maxFiles":10,"pattern":"target("}),
    )
    .expect_err("whole-scope invalid pattern");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,
        "pattern":"missing("}),
    )
    .expect_err("prefilter cannot hide invalid whole-scope pattern");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
}

#[test]
fn a_final_window_of_an_inapplicable_grammar_is_whole_and_names_it() {
    let root = Fixture::new();
    root.write("a.ts", "export function target() { return 2; }\n");
    root.write("z.py", "def ordinary():\n    return 1\n");
    let first = run(
        &root.0,
        json!({"operation":"match","path":root.0,
        "rule":{"kind":"function_declaration"},"maxFiles":1}),
    )
    .expect("first compatible grammar");
    assert_eq!(first["matchCount"], 1, "{first}");
    let next = first["next"]["expandScan"]["query"]["queries"][0].clone();
    assert!(next.is_object(), "{first}");
    let last = run(&root.0, next).expect("last grammar cannot invalidate earlier hits");
    assert_eq!(last["matchCount"], 0, "{last}");
    // The walk met a grammar that compiles the rule (the earlier window's),
    // so the python file is outside what the rule applies to: the final
    // window is whole and names it.
    assert!(last.get("isPartial").is_none(), "{last}");
    assert!(last.get("terminalLimit").is_none(), "{last}");
    assert!(last.get("diagnostics").is_none(), "{last}");
    let warning = last["warnings"][0].as_str().expect("warning");
    assert!(
        warning.starts_with("The query does not apply to python (1 file: "),
        "{last}"
    );
    assert!(last["next"].get("expandScan").is_none(), "{last}");
}

#[test]
fn directory_scan_windows_infer_each_files_grammar_without_losing_other_languages() {
    let root = Fixture::new();
    for (name, source) in [
        ("a.ts", "hit(1);\nhit(2);\n"),
        ("b.js", "hit(3);\nhit(4);\n"),
        ("c.rs", "fn f() { hit(5);\nhit(6); }\n"),
        ("d.py", "hit(7)\nhit(8)\n"),
    ] {
        root.write(name, source);
    }
    let start = json!({
        "operation":"match","path":root.0,"pattern":"hit($A)",
        "maxFiles":1,"pageSize":1,"matchPageSize":1
    });
    let mut values = Vec::new();
    let walk = checks::walk("astSearch", json!({ "queries": [start] }), |call| {
        let rows = call["queries"].as_array().expect("rows");
        assert_eq!(rows.len(), 1, "one row per page lead: {call}");
        let query = &rows[0];
        assert!(query.get("language").is_none(), "{query}");
        let out = run(&root.0, query.clone()).expect("per-file grammar scan");
        for file in out["files"].as_array().into_iter().flatten() {
            for found in file["matches"].as_array().into_iter().flatten() {
                values.push(found["value"].as_str().expect("match").to_owned());
            }
        }
        checks::Page {
            output: crate::tools::test_support::published(
                crate::id::ToolId::AstSearch,
                query,
                &out,
            ),
            rows: ast_rows(std::slice::from_ref(&out)),
        }
    });
    assert!(walk.queries.len() < 30, "continuations terminate");
    values.sort();
    assert_eq!(
        values,
        (1..=8).map(|i| format!("hit({i})")).collect::<Vec<_>>()
    );
}

/// Each astSearch walk (the outline scan of `symbols`, the structural scan
/// of `match`; neither is structureSearch's walk) applies the path policy
/// before it counts or reads a descendant: with `includeHidden`, the
/// credential directory, the sensitive file names and `.env.ts` are neither
/// scanned nor named.
#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = sensitive_fixture();
    root.write(".aws/hidden.ts", "oldCall(secret);\n");
    root.write(".env.ts", "oldCall(ignored);\n");
    root.write("visible.ts", "oldCall(visible);\n");
    let (paths, security) = simple_policy(&root.0);
    let root_name = root.0.file_name().expect("root name").to_string_lossy();
    for (row, count, expected) in [
        (
            json!({"operation":"symbols","path":root.0,"includeHidden":true}),
            "filesScanned",
            2,
        ),
        (
            json!({"operation":"match","path":root.0,"language":"typescript","pattern":"oldCall($A)","includeHidden":true}),
            "matchCount",
            1,
        ),
    ] {
        let out = execute_row(row.clone(), &paths, &security, &NeverCancel).expect("walk");
        assert_eq!(out[count], expected, "{row}: {out}");
        let listed = checks::paths_in(&out["files"]);
        assert!(!listed.is_empty(), "{row}: {out}");
        assert!(
            listed
                .iter()
                .all(|path| path.starts_with(&format!("{root_name}/visible."))),
            "{row}: {out}"
        );
        for withheld in ["hidden.ts", ".env.ts", "credentials", "tfstate"] {
            checks::assert_absent(&out, withheld);
        }
    }
}

/// A directory scan whose path policy withheld candidate entries says so,
/// as localSearch and structureSearch do: an empty or "complete" result
/// over a credential-store directory is not proof of absence. A source
/// folder merely named `secrets/` or `private/` is scanned, not withheld.
#[test]
fn directory_scans_disclose_policy_withheld_entries() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".aws")).expect("aws dir");
    root.write(".aws/store.ts", "register(secret);\n");
    // The repository's own `.git` is a dot entry out of the walk's
    // view, so it is not disclosed on every scan.
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(
        "visible.ts",
        "register(visible);\nexport function shown() {}\n",
    );
    let disclosed = |out: &serde_json::Value| {
        let reasons = skipped_reasons(out, "withheld");
        reasons.len() == 1 && reasons[0].0 == "security-policy dir .aws/"
    };
    let matches = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"typescript","pattern":"register($A)","includeHidden":true}),
    )
    .expect("match");
    assert_eq!(matches["matchCount"], 1, "{matches}");
    assert!(disclosed(&matches), "{matches}");
    assert!(matches.get("coverage").is_none(), "{matches}");
    let empty = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"typescript","pattern":"register(secret)","includeHidden":true}),
    )
    .expect("empty match");
    assert!(disclosed(&empty), "{empty}");
    let symbols = run(&root.0, json!({"operation":"symbols","path":root.0})).expect("symbols");
    // Dot directories are out of a symbols walk's view, disclosed or not.
    assert_eq!(symbols["filesScanned"], 1, "{symbols}");
    checks::assert_absent(&symbols, "store.ts");
}

/// A link inside the workspace is never followed by a directory scan: a
/// scope whose sources sit only behind links says so, with localSearch's
/// wording, instead of an empty outline or a `languageRequired` that blames
/// the grammar.
#[cfg(unix)]
#[test]
fn directory_scans_disclose_in_workspace_links_they_do_not_follow() {
    let root = Fixture::new();
    root.write(
        "real/skill/run.ts",
        "register(linked);\nexport function shown() {}\n",
    );
    std::fs::create_dir_all(root.0.join("links")).expect("links dir");
    std::os::unix::fs::symlink(root.0.join("real/skill"), root.0.join("links/skill"))
        .expect("dir link");
    std::os::unix::fs::symlink(
        root.0.join("real/skill/run.ts"),
        root.0.join("links/run.ts"),
    )
    .expect("file link");
    let notice = [("symlink not followed".into(), 2)];
    let scope = root.0.join("links");
    let matched = run(
        &root.0,
        json!({"operation":"match","path":scope,"pattern":"register($A)"}),
    )
    .expect("no languageRequired behind links");
    assert_eq!(skipped_reasons(&matched, "withheld"), notice, "{matched}");
    let typed = run(
        &root.0,
        json!({"operation":"match","path":scope,"language":"typescript","pattern":"register($A)"}),
    )
    .expect("typed match");
    assert_eq!(skipped_reasons(&typed, "withheld"), notice, "{typed}");
    let symbols = run(&root.0, json!({"operation":"symbols","path":scope})).expect("symbols");
    assert_eq!(skipped_reasons(&symbols, "withheld"), notice, "{symbols}");
    // A scope with no file at all names no grammar to set either.
    std::fs::create_dir_all(root.0.join("empty")).expect("empty dir");
    let empty = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("empty"),"pattern":"register($A)"}),
    )
    .expect("an empty scope is no languageRequired");
    assert_eq!(empty["matchCount"], 0, "{empty}");
}

#[test]
fn directory_prefilter_skips_are_aggregated_once() {
    let root = Fixture::new();
    for name in ["one.ts", "two.ts", "three.ts"] {
        std::fs::write(root.0.join(name), "const value = 1;\n").expect("source");
    }
    let (paths, security) = simple_policy(&root.0);
    let result = execute_row(
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test",
            "path":root.0,
            "language":"typescript",
            "pattern":"missingCall($A);"
        }),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("structural zero");
    let prefilter = result["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "structural.prefilter.skipped")
        .collect::<Vec<_>>();
    assert_eq!(prefilter.len(), 1, "{result}");
    assert!(
        prefilter[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("3 file(s)")),
        "{result}"
    );
    assert_eq!(result["status"], "empty", "{result}");
}

/// A cancellation during either astSearch directory walk stops it with
/// `cancelled`.
#[test]
fn cancellation_interrupts_descendant_traversal() {
    let root = Fixture::new();
    root.write("source.rs", "source");
    let (paths, security) = simple_policy(&root.0);
    for row in [
        json!({"operation":"symbols","path":root.0}),
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"source($A)"}),
    ] {
        let error = execute_row(
            row.clone(),
            &paths,
            &security,
            &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
        )
        .expect_err("cancel the walk");
        assert_eq!(error.code, ErrorCode::Cancelled, "{row}");
    }
}

#[test]
fn lang_type_is_validated_and_intersected_with_include() {
    let root = Fixture::new();
    root.write("a.ts", "oldCall(x);\n");
    root.write("b.py", "oldCall(x)\n");
    let unknown = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"klingon","pattern":"oldCall($A)"}),
    )
    .expect_err("unknown language");
    assert_eq!(unknown.code, ErrorCode::LanguageUnsupported);

    let out = run(
        &root.0,
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript",
            "include":["*.ts","*.py"],"pattern":"oldCall($A)"
        }),
    )
    .expect("intersected include");
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert!(
        files[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("a.ts")),
        "{out}"
    );

    let mismatch = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0.join("b.py"),"language":"typescript","pattern":"oldCall($A)"}),
    )
    .expect_err("single file outside language");
    assert_eq!(mismatch.code, ErrorCode::LanguageMismatch);

    root.write("notes.txt", "oldCall(x)\n");
    let unsupported = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0.join("notes.txt"),"pattern":"oldCall($A)"}),
    )
    .expect("unsupported single file");
    assert_eq!(unsupported["isPartial"], true, "{unsupported}");
    assert_ne!(unsupported["status"], "empty", "{unsupported}");
}

#[test]
fn dot_prefixed_lang_type_selects_only_that_extension() {
    let root = Fixture::new();
    root.write("a.ts", "oldCall(x);\n");
    root.write("b.mts", "oldCall(y);\n");

    let exact = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":".ts","pattern":"oldCall($A)"}),
    )
    .expect("exact extension");
    let files = exact["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{exact}");
    assert!(
        files[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("a.ts"))
    );

    let family = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"typescript","pattern":"oldCall($A)"}),
    )
    .expect("language family");
    assert_eq!(
        family["files"].as_array().expect("files").len(),
        2,
        "{family}"
    );
}

#[test]
fn cpp_header_can_use_explicit_cpp_grammar_without_changing_h_default() {
    let root = Fixture::new();
    let header = root.0.join("widget.h");
    std::fs::write(
        &header,
        "template <typename T> class Widget { public: T value; };\n",
    )
    .expect("header");

    let selected = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":header,"language":"cpp","rule":{"kind":"class_specifier"}}),
    )
    .expect("explicit C++ match");
    assert_eq!(selected["matchCount"], 1, "{selected}");

    let selected_directory = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"cpp","include":["*.h"],"rule":{"kind":"class_specifier"}}),
    )
    .expect("explicit C++ directory match");
    assert_eq!(selected_directory["matchCount"], 1, "{selected_directory}");

    let selected_directory_default = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"cpp","rule":{"kind":"class_specifier"}}),
    )
    .expect("C++ directory includes ambiguous headers");
    assert_eq!(
        selected_directory_default["matchCount"], 1,
        "{selected_directory_default}"
    );

    let tree = run(
        &root.0,
        json!({"operation":"syntaxTree","mainGoal":"test","reasoning":"test","path":header,"language":"cpp"}),
    )
    .expect("explicit C++ tree");
    assert_eq!(tree["isPartial"], false, "{tree}");
    let default_tree = run(
        &root.0,
        json!({"operation":"syntaxTree","mainGoal":"test","reasoning":"test","path":header}),
    )
    .expect(".h defaults to C");
    assert_eq!(default_tree["isPartial"], true, "{default_tree}");
    assert!(
        tree["nodes"].as_array().is_some_and(|nodes| nodes
            .iter()
            .any(|node| node.as_str().and_then(|row| row.split(' ').nth(1))
                == Some("class_specifier"))),
        "{tree}"
    );

    let symbols = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":header,"language":"cpp"}),
    )
    .expect("explicit C++ symbols");
    assert!(
        outline_names(&symbols["symbols"]).contains(&"Widget"),
        "{symbols}"
    );

    let wrong_file = root.0.join("wrong.c");
    std::fs::write(&wrong_file, "int value;\n").expect("C file");
    let mismatch = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":wrong_file,"language":"cpp","rule":{"kind":"declaration"}}),
    )
    .expect_err("C source is not an ambiguous header");
    assert_eq!(mismatch.code, ErrorCode::LanguageMismatch);
}

#[test]
fn directory_symbols_use_path_scoped_header_parser_and_preserve_it_in_next_page() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("include")).expect("include directory");
    std::fs::create_dir_all(root.0.join("legacy")).expect("legacy directory");
    root.write("include/widget.h", "namespace Space { class Widget {}; }\n");
    root.write("legacy/plain.h", "struct Plain { int x; };\n");
    let result = run(&root.0, json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"languageGlobs":{"cpp":["include/**/*.h"]},"pageSize":1})).expect("directory symbols");
    assert_eq!(result["filesScanned"], 2, "{result}");
    assert_eq!(
        result["next"]["nextPage"]["query"]["queries"][0]["languageGlobs"]["cpp"][0],
        "include/**/*.h"
    );
    let next_query = result["next"]["nextPage"]["query"]["queries"][0].clone();
    let next = run(&root.0, next_query).expect("next page");
    let names = [
        outline_name(&result["files"][0]["symbols"][0]),
        outline_name(&next["files"][0]["symbols"][0]),
    ];
    assert!(names.contains(&"Widget"), "{result} {next}");
    assert!(names.contains(&"Plain"), "{result} {next}");
}

/// A directory match without language uses the one grammar whose files the
/// scope holds; continuations retain per-file inference rather than narrowing it.
/// A directory match without `language` infers its grammar from the census
/// of the scan's own walk: no second walk, the same `inferredLanguage`, and
/// a scope without any supported file is still `languageRequired`.
#[test]
fn the_inferred_grammar_comes_from_the_scan_walk() {
    let root = Fixture::new();
    root.write("src/a.rs", "fn a() { b(); }\n");
    root.write("src/b.rs", "fn b() {}\n");
    root.write("notes.txt", "b()\n");
    let before = crate::tools::ast_rule::present_walks();
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"b()"}),
    )
    .expect("match");
    assert_eq!(out["inferredLanguage"], "rust", "{out}");
    assert_eq!(crate::tools::ast_rule::present_walks(), before, "one walk");
    let empty = Fixture::new();
    empty.write("notes.txt", "b()\n");
    let error = run(
        &empty.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":empty.0,"pattern":"b()"}),
    )
    .expect_err("no grammar");
    assert_eq!(error.code, ErrorCode::LanguageRequired, "{error:?}");
}

#[test]
fn directory_match_infers_the_only_grammar_present() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() { hit(1); }\n");
    std::fs::create_dir(root.0.join("nested")).expect("nested");
    root.write("nested/b.rs", "fn b() { hit(2); }\n");
    root.write("README.md", "hit(3)\n");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"hit($A)","pageSize":1}),
    )
    .expect("language inferred from the .rs files");
    assert_eq!(out["matchCount"], 2, "{out}");
    assert_eq!(out["inferredLanguage"], "rust", "{out}");
    let next = out["next"]["nextPage"]["query"]["queries"][0].clone();
    assert!(next.get("language").is_none(), "{out}");
    let continued = run(&root.0, next).expect("same inferred scope");
    assert_eq!(continued["snapshot"], out["snapshot"], "{continued}");
    assert!(continued["next"].get("nextPage").is_none(), "{continued}");
    let explicit = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"pattern":"hit($A)","pageSize":1,"language":"rust"}),
    )
    .expect("explicit language");
    assert_eq!(explicit["matchCount"], out["matchCount"], "{explicit}");
    assert!(explicit.get("inferredLanguage").is_none(), "{explicit}");
}

#[test]
fn directory_match_covers_each_parsing_grammar_and_explicit_language_still_filters() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() { hit(1); }\n");
    root.write("b.ts", "hit(2);\n");
    root.write("c.ts", "hit(3);\n");
    root.write("d.js", "hit(4);\n");
    root.write("e.py", "hit(5)\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"hit($A)"}),
    )
    .expect("each file selects its grammar");
    assert_eq!(out["matchCount"], 5, "{out}");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(5), "{out}");
    assert!(out.get("inferredLanguage").is_none(), "{out}");
    let explicit = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"hit($A)","language":"rust"}),
    )
    .expect("explicit selector");
    assert_eq!(explicit["matchCount"], 1, "{explicit}");
    assert_eq!(
        explicit["files"].as_array().map(Vec::len),
        Some(1),
        "{explicit}"
    );
}

#[test]
fn a_bare_include_word_matches_file_names_containing_it() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("nested")).expect("dir");
    root.write("nested/handler_test.rs", "fn a() { hit(1); }\n");
    root.write("other.rs", "fn b() { hit(2); }\n");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,
            "language":"rust","pattern":"hit($A)","include":["test"]}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert!(
        out["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("handler_test.rs")))),
        "{out}"
    );
}

/// `.ts` and `.tsx` are one family: `typescript` scans `.tsx` with the TSX
/// parser, so a mixed directory needs no language.
#[test]
fn directory_match_over_a_grammar_family_infers_the_family() {
    let root = Fixture::new();
    root.write("a.ts", "hit(1);\n");
    root.write("b.tsx", "const v = <div>{hit(2)}</div>;\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"hit($A)"}),
    )
    .expect("one family");
    assert_eq!(out["inferredLanguage"], "typescript", "{out}");
    assert_eq!(out["matchCount"], 2, "{out}");
}

/// A directory whose files the walk skipped as ignored or hidden parsed
/// nothing: the empty match leads to the same query with those walks on,
/// and does not offer a repair that drops `language`.
#[test]
fn an_empty_match_over_ignored_and_hidden_files_leads_to_them() {
    for (dir, flag) in [("gen", "includeIgnored"), (".cfg", "includeHidden")] {
        let root = Fixture::new();
        std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
        root.write(".gitignore", "gen/\n");
        let pkg = root.0.join("pkg");
        std::fs::create_dir_all(pkg.join(dir)).expect("dir");
        std::fs::write(pkg.join(dir).join("a.py"), "print(1)\n").expect("source");
        let request = json!({
            "operation":"match","mainGoal":"test","reasoning":"test",
            "path":pkg,"language":"python","pattern":"print($A)"
        });
        let empty = run(&root.0, request.clone()).expect("empty match");
        assert!(empty["files"].is_null(), "{dir}: {empty}");
        assert!(empty["next"].get("repair").is_none(), "{dir}: {empty}");
        let lead = &empty["next"]["includeSkipped"];
        assert_eq!(lead["tool"], "astSearch", "{dir}: {empty}");
        let retry = &lead["query"]["queries"][0];
        assert_eq!(retry[flag], true, "{dir}: {empty}");
        assert!(
            empty["hints"]
                .to_string()
                .contains(if flag == "includeIgnored" {
                    ".gitignore"
                } else {
                    "hidden"
                }),
            "{dir}: {empty}"
        );
        let found = run(&root.0, retry.clone()).expect("lead runs");
        assert_eq!(found["matchCount"], 1, "{dir}: {found}");
    }
}

/// Without `language` each file selects its grammar, so an empty scan that
/// parsed nothing still probes the skipped walks: the source behind an ignore
/// rule or a hidden directory is named, with the query that lifts it.
#[test]
fn an_empty_match_without_language_leads_to_skipped_files() {
    for (dir, flag) in [("gen", "includeIgnored"), (".cfg", "includeHidden")] {
        let root = Fixture::new();
        std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
        root.write(".gitignore", "gen/\n");
        let pkg = root.0.join("pkg");
        std::fs::create_dir_all(pkg.join(dir)).expect("dir");
        std::fs::write(pkg.join(dir).join("a.py"), "print(1)\n").expect("source");
        let request = json!({
            "operation":"match","mainGoal":"test","reasoning":"test",
            "path":pkg,"pattern":"print($A)"
        });
        let empty = run(&root.0, request).expect("empty match");
        let retry = &empty["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(retry[flag], true, "{dir}: {empty}");
        assert!(retry.get("language").is_none(), "{dir}: {empty}");
        let found = run(&root.0, retry.clone()).expect("lead runs");
        assert_eq!(found["matchCount"], 1, "{dir}: {found}");
    }
}

/// An empty match that did parse files still names the ignored files of its
/// grammar it left out, and leads to the same query with them on: the hint
/// about the pattern's shape is not the only explanation.
#[test]
fn an_empty_match_beside_parsed_files_names_the_ignored_ones() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git dir");
    root.write(".gitignore", "gen/\n");
    std::fs::create_dir_all(root.0.join("gen")).expect("dir");
    root.write("gen/b.ts", "export const y = double(2);\n");
    root.write("a.ts", "export const x = 1;\n");
    let request = json!({
        "operation":"match","mainGoal":"test","reasoning":"test",
        "path":root.0,"language":"typescript","pattern":"double($A)"
    });
    let empty = run(&root.0, request).expect("empty match");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert_eq!(empty["filesScanned"], json!(1), "{empty}");
    let hints = empty["hints"].to_string();
    assert!(hints.contains(".gitignore:1 'gen/'"), "{hints}");
    let retry = &empty["next"]["includeSkipped"]["query"]["queries"][0];
    assert_eq!(retry["includeIgnored"], true, "{empty}");
    let found = run(&root.0, retry.clone()).expect("lead runs");
    assert_eq!(found["matchCount"], 1, "{found}");
}

/// A file the scan skips (over the parse limit, not UTF-8) is one skip:
/// `filesSkipped` alone (never also `filesUnevaluated`), one diagnostic per
/// file, and its recovery names tools that reach it.
#[test]
fn a_skipped_file_is_one_skip_with_one_diagnostic() {
    let root = Fixture::new();
    root.write("a.ts", "foo(1);\n");
    let large = "foo(2);\n".repeat(super::MAX_PARSE_SOURCE_BYTES / 8 + 1);
    root.write("big.ts", large);
    std::fs::write(
        root.0.join("bin.ts"),
        [0xff_u8, b'f', b'o', b'o', b'(', 0xfe],
    )
    .expect("binary");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"typescript","pattern":"foo($A)"}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 1, "{out}");
    assert_eq!(out["partialReasons"], json!(["filesSkipped"]), "{out}");
    let diagnostics = out["diagnostics"].as_array().expect("diagnostics");
    for name in ["big.ts", "bin.ts"] {
        let about = diagnostics
            .iter()
            .filter(|d| d["path"].as_str().is_some_and(|path| path.ends_with(name)))
            .collect::<Vec<_>>();
        assert_eq!(about.len(), 1, "{name}: {out}");
    }
    let large = diagnostics
        .iter()
        .find(|d| d["code"] == "structural.file.tooLarge")
        .expect("too large");
    let recovery = large["recovery"].as_str().expect("recovery");
    assert!(
        recovery.contains("localSearch") && recovery.contains("localFetch"),
        "{recovery}"
    );
    assert!(!recovery.contains("maxFileBytes"), "{recovery}");
}
