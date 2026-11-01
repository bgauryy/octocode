//! Directory scans over mixed grammars: windows, inference and the
//! grammars a pattern does not apply to.

use super::*;

#[test]
fn capped_incompatible_grammar_preserves_partial_coverage_and_reaches_later_grammar() {
    let root = Fixture::new();
    let window = crate::tools::ast_search::scan_window::test_window(1);
    root.write("a.py", "def ordinary():\n    return 1\n");
    root.write("z.ts", "export function target() { return 2; }\n");
    for selector in [json!({}), json!({"include":["*.py", "*.ts"]})] {
        let mut query = json!({"operation":"match","path":root.0,"rule":{"kind":"function_declaration"},"includeCaptures":true});
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
        assert!(reasons.contains(&json!("scanBudget")), "{first}");
        assert!(
            !first["diagnostics"].to_string().contains("a.py"),
            "{first}"
        );
        assert_eq!(first["skippedGrammars"], json!({"python":1}), "{first}");
        let next = first["next"]["expandScan"]["input"]["queries"][0].clone();
        assert!(next.is_object(), "{first}");
        assert!(next.get("parser").is_none(), "{next}");
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
        json!({"rule":{"pattern":"function $F() { $$$BODY }"}}),
        json!({"rule":{"kind":"function_declaration"},"include":["*.ts"]}),
        json!({"rule":{"kind":"function_declaration"},"exclude":["*.py"]}),
    ] {
        let mut query = json!({"operation":"match","path":root.0,"includeCaptures":true});
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
            out = run(&root.0, next["input"]["queries"][0].clone()).expect("copied pattern scan");
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
        json!({"parser":"python","rule":{"kind":"function_declaration"}}),
    ] {
        let mut query = json!({"operation":"match","path":root.0});
        query
            .as_object_mut()
            .expect("query")
            .extend(selector.as_object().expect("selector").clone());
        let error = run(&root.0, query).expect_err("invalid selector");
        assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    }
    let query = json!({"operation":"match","path":root.0,"rule":{"pattern":"target("}});
    let first = run(&root.0, query).expect("bounded pattern coverage");
    assert_eq!(first["isPartial"], true, "{first}");
    let next = first["next"]["expandScan"]["input"]["queries"][0].clone();
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
    drop(window);
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"target("}}),
    )
    .expect_err("whole-scope invalid pattern");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"missing("}}),
    )
    .expect_err("prefilter cannot hide invalid whole-scope pattern");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
}

#[test]
fn a_final_window_of_an_inapplicable_grammar_is_whole_and_names_it() {
    let root = Fixture::new();
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    root.write("a.ts", "export function target() { return 2; }\n");
    root.write("z.py", "def ordinary():\n    return 1\n");
    let first = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"kind":"function_declaration"}}),
    )
    .expect("first compatible grammar");
    assert_eq!(first["matchCount"], 1, "{first}");
    let next = first["next"]["expandScan"]["input"]["queries"][0].clone();
    assert!(next.is_object(), "{first}");
    let last = run(&root.0, next).expect("last grammar cannot invalidate earlier hits");
    assert_eq!(last["matchCount"], 0, "{last}");
    // The walk met a grammar that compiles the rule (the earlier window's),
    // so the python file is outside what the rule applies to: the final
    // window is whole and names it.
    assert!(last.get("isPartial").is_none(), "{last}");
    assert!(last.get("terminalLimit").is_none(), "{last}");
    assert!(last.get("diagnostics").is_none(), "{last}");
    assert_eq!(last["skippedGrammars"], json!({"python":1}), "{last}");
    assert!(last["next"].get("expandScan").is_none(), "{last}");
}

#[test]
fn directory_scan_windows_infer_each_files_grammar_without_losing_other_languages() {
    let root = Fixture::new();
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    for (name, source) in [
        ("a.ts", "hit(1);\nhit(2);\n"),
        ("b.js", "hit(3);\nhit(4);\n"),
        ("c.rs", "fn f() { hit(5);\nhit(6); }\n"),
        ("d.py", "hit(7)\nhit(8)\n"),
    ] {
        root.write(name, source);
    }
    let start = json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"}});
    let mut values = Vec::new();
    let walk = checks::walker::walk("astSearch", json!({"queries":[start]}), |call| {
        let input = crate::tools::test_support::opened_input(crate::id::ToolId::AstSearch, call);
        let rows = input["queries"].as_array().expect("rows");
        assert_eq!(rows.len(), 1, "one row per page lead: {call}");
        let query = &rows[0];
        assert!(query.get("parser").is_none(), "{query}");
        let out = run(&root.0, query.clone()).expect("per-file grammar scan");
        for file in out["files"].as_array().into_iter().flatten() {
            for found in file["matches"].as_array().into_iter().flatten() {
                values.push(found["value"].as_str().expect("match").to_owned());
            }
        }
        checks::Page {
            output: crate::tools::test_support::published(crate::id::ToolId::AstSearch, &out),
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
        json!({"operation":"match","path":root.0,"rule":{"pattern":"b()"}}),
    )
    .expect("match");
    assert_eq!(out["inferredLanguage"], "rust", "{out}");
    assert_eq!(crate::tools::ast_rule::present_walks(), before, "one walk");
    let empty = Fixture::new();
    empty.write("notes.txt", "b()\n");
    let error = run(
        &empty.0,
        json!({"operation":"match","path":empty.0,"rule":{"pattern":"b()"}}),
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
    let out = crate::tools::ast_search::test_rows::run_window(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"}}),
        1,
    )
    .expect("language inferred from the .rs files");
    assert_eq!(out["matchCount"], 2, "{out}");
    assert_eq!(out["inferredLanguage"], "rust", "{out}");
    let next = out["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next.get("parser").is_none(), "{out}");
    let continued = crate::tools::ast_search::test_rows::run_window(&root.0, next, 1)
        .expect("same inferred scope");
    assert_eq!(continued["snapshot"], out["snapshot"], "{continued}");
    assert!(continued["next"].get("nextPage").is_none(), "{continued}");
    let explicit = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"},"parser":"rust"}),
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
        json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"}}),
    )
    .expect("each file selects its grammar");
    assert_eq!(out["matchCount"], 5, "{out}");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(5), "{out}");
    assert!(out.get("inferredLanguage").is_none(), "{out}");
    let explicit = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"},"parser":"rust"}),
    )
    .expect("explicit selector");
    assert_eq!(explicit["matchCount"], 1, "{explicit}");
    assert_eq!(
        explicit["files"].as_array().map(Vec::len),
        Some(1),
        "{explicit}"
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
        json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"}}),
    )
    .expect("one family");
    assert_eq!(out["inferredLanguage"], "typescript", "{out}");
    assert_eq!(out["matchCount"], 2, "{out}");
}
