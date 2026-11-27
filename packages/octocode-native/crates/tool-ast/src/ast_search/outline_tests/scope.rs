//! What an outline accepts and covers: kinds and source limits, cached
//! pages, default-excluded directories and the empty hint.

use super::*;
use octocode_test_support::json_fixture;

/// `include` narrows a directory outline to the files `match` reads under
/// the same globs: a name at any depth, a path from `path`, a default-
/// excluded directory the glob names.
#[test]
fn include_narrows_a_directory_outline_to_the_files_match_reads() {
    let root = Fixture::new();
    for file in [
        "run.ts",
        "other.ts",
        "sub/run.ts",
        "sub/deep/more.ts",
        "node_modules/pkg/run.ts",
    ] {
        root.write(file, "export function declared() { return 1; }\n");
    }
    let listed = |row: serde_json::Value| {
        let out = run(&root.0, row).expect("a walk");
        let mut paths = octocode_runtime_base::test_support::checks::paths_in(&out["files"]);
        paths.sort();
        paths
    };
    let outline = |include: Option<&[&str]>| {
        let mut row = json!({"operation":"symbols","path":root.0});
        if let Some(include) = include {
            row["include"] = json!(include);
        }
        listed(row)
    };
    let everything = outline(None);
    for include in [
        &["run.ts"][..],
        &["sub/*.ts"],
        &["sub/**"],
        &["**/more.ts", "other.ts"],
        &["node_modules/**/run.ts"],
        &["absent.ts"],
    ] {
        let outlined = outline(Some(include));
        let matched = listed(json!({
            "operation":"match","path":root.0,"parser":"typescript",
            "rule":{"kind":"function_declaration"},"view":"files","include":include,
        }));
        assert_eq!(outlined, matched, "{include:?}");
        assert_ne!(outlined, everything, "{include:?}");
    }
    assert_eq!(outline(Some(&["run.ts"])), ["run.ts", "sub/run.ts"]);
}

#[test]
fn unknown_symbol_kinds_are_rejected_and_source_limits_are_errors() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "pub fn visible() {}\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"symbols","path":source,"symbolKinds":["functoin"]}),
    )
    .expect_err("unknown kind");
    // An `.input.invalid` code is an input rejection: CLI exit 2 and a
    // "correct the field" hint, never "broaden the query".
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert_eq!(
        octocode_runtime_base::id::error_codes::class(error.code),
        octocode_runtime_base::id::error_codes::ErrorClass::CorrectInput
    );
    assert!(
        error.message.contains("function, impl"),
        "{}",
        error.message
    );
    let ok = run(
        &root.0,
        json!({"operation":"symbols","path":source,"symbolKinds":["function"]}),
    )
    .expect("known kind");
    assert_eq!(outline_name(&ok["symbols"][0]), "visible");

    let large = root.0.join("large.rs");
    std::fs::write(
        &large,
        "// x\n".repeat(octocode_runtime_base::security::SourceLimits::REAL.whole_source / 5 + 1),
    )
    .expect("large");
    for query in [
        json!({"operation":"symbols","path":large}),
        json!({"operation":"syntaxTree","path":large}),
    ] {
        let out = run(&root.0, query).expect("limit row");
        assert_eq!(out["errorCode"], "fileTooLarge", "{out}");
        assert_eq!(out["status"], "error", "{out}");
        // No page reaches the rest: a terminal limit naming its cut.
        assert_eq!(out["terminalLimit"], true, "{out}");
        assert_eq!(
            out["partialReasons"],
            json_fixture(r#"["fileTooLarge"]"#),
            "{out}"
        );
        // No literal searched: the shared large-source recovery, and a read
        // of the file's windows (localFetch serves it).
        assert_eq!(
            out["hints"],
            json!([octocode_runtime_base::response::rows::LARGE_SOURCE_HINT]),
            "{out}"
        );
        assert_eq!(out["related"][0]["tool"], "localFetch", "{out}");
    }
    // A searched name leads to a text search of it in the same file.
    let named = run(
        &root.0,
        json!({"operation":"symbols","path":large,"symbolFilter":{"terms":["visible"]}}),
    )
    .expect("limit row");
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&named, "localSearch", |_| true);
    assert_eq!(lead["tool"], "localSearch", "{named}");
    assert_eq!(
        lead["input"]["queries"][0]["matchString"],
        json_fixture(r#"["visible"]"#),
        "{named}"
    );
    assert!(
        lead["input"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("large.rs")),
        "{named}"
    );
    let pattern = run(
        &root.0,
        json!({"operation":"match","path":large,"rule":{"pattern":"console.log"}}),
    );
    let error = pattern.expect_err("pattern over the parse limit");
    assert_eq!(error.code, ErrorCode::FileTooLarge);
    let next = error.calls.expect("findText lead");
    let search = octocode_runtime_base::contracts::validate_query(
        "localSearch",
        next["findText"]["input"]["queries"][0].clone(),
    )
    .expect("valid text fallback");
    assert_eq!(search["regexEngine"], "literal");
    use octocode_runtime_base::response::output::ToolOutput;
    assert_eq!(
        crate::ast_search::output::Output
            .error_hint(ErrorCode::FileTooLarge, &serde_json::Value::Null),
        Some(octocode_runtime_base::response::output::NO_HINT)
    );
}

#[test]
fn cached_symbol_pages_still_check_content_and_path_policy() {
    let root = Fixture::new();
    let source = root.0.join("pages.ts");
    std::fs::write(
        &source,
        "export function one() {}\nexport function two() {}\n",
    )
    .expect("source");
    let first =
        run_window(&root.0, json!({"operation":"symbols","path":source}), 1).expect("first");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert_eq!(
        outline_name(&run_window(&root.0, next.clone(), 1).expect("second")["symbols"][0]),
        "two"
    );
    // Same byte length: cache invalidation cannot rely only on file size.
    std::fs::write(
        &source,
        "export function one() {}\nexport function six() {}\n",
    )
    .expect("changed");
    let stale = run_window(&root.0, next.clone(), 1).expect("stale");
    assert_eq!(stale["errorCode"], "staleSnapshot");
    let restart =
        octocode_runtime_base::test_support::checks::related_call(&stale, "astSearch", |_| true);
    assert_eq!(restart["tool"], "astSearch");
    assert_eq!(restart["input"]["queries"][0]["operation"], "symbols");
    // The first page (the default), without any cursor.
    for cursor in ["page", "snapshot", "scanOffset"] {
        assert!(
            restart["input"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {stale}"
        );
    }
    let restarted = run_window(&root.0, restart["input"].clone(), 1)
        .expect("execute returned restart unchanged");
    assert_eq!(outline_name(&restarted["symbols"][0]), "one");
    assert_ne!(
        octocode_runtime_base::test_support::pinned_snapshot(&restarted),
        octocode_runtime_base::test_support::pinned_snapshot(&first)
    );
    let second = run_window(
        &root.0,
        restarted["next"]["nextPage"]["input"]["queries"][0].clone(),
        1,
    )
    .expect("new snapshot continuation");
    assert_eq!(outline_name(&second["symbols"][0]), "six");
    let fresh = run(&root.0, json!({"operation":"symbols","path":source})).expect("fresh");
    assert_eq!(outline_names(&fresh["symbols"])[1], "six");
    let forbidden = Fixture::new();
    assert!(
        run(&forbidden.0, next).is_err(),
        "warm extraction must not bypass root authorization"
    );
}

/// An empty symbols search says what it searched (the name and the
/// file count) and leads to the name's text.
#[test]
fn empty_symbols_hint_names_file_count_and_text_lead() {
    let root = Fixture::new();
    root.write("a.rs", "fn alpha() {}\n");
    root.write("b.rs", "fn beta() { missing_name(); }\n");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["missing_name"]}}),
    )
    .expect("symbols");
    assert_eq!(out["status"], "empty", "{out}");
    let hints = out["hints"].as_array().expect("hints");
    assert!(
        hints.iter().any(|hint| hint
            .as_str()
            .is_some_and(|text| text.contains("0 declarations named missing_name in 2 files"))),
        "{out}"
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&out, "localSearch", |_| true);
    assert_eq!(lead["tool"], "localSearch", "{out}");
    assert_eq!(
        lead["input"]["queries"][0]["matchString"],
        json_fixture(r#"["missing_name"]"#),
        "{out}"
    );
    // Without a name, the hint counts the files and names no text lead.
    let outline = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolKinds":["class"]}),
    )
    .expect("outline");
    assert_eq!(outline["status"], "empty", "{outline}");
    assert!(
        outline["hints"][0]
            .as_str()
            .is_some_and(|text| text.contains("0 declarations in 2 files")),
        "{outline}"
    );
    assert!(outline["next"].get("findText").is_none(), "{outline}");
}

/// A directory outline discloses the default-excluded directories it
/// pruned, and an empty one leads to the same query over them.
#[test]
fn symbols_disclose_default_excluded_directories() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("node_modules/pkg")).expect("dirs");
    root.write(
        "node_modules/pkg/index.ts",
        "export function hiddenHelper() {}\n",
    );
    root.write("main.ts", "export const x = 1;\n");
    let out = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"symbolFilter":{"terms":["hiddenHelper"]}}),
    )
    .expect("symbols");
    let names = skipped_names(&out, "default-excluded");
    assert_eq!(names, ["node_modules"], "{out}");
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&out, "astSearch", |input| {
            input["queries"][0]["defaultExcludes"] == false
        });
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let query = &lead["input"]["queries"][0];
    assert_eq!(query["defaultExcludes"], false, "{out}");
    assert_eq!(
        query["symbolFilter"]["terms"],
        json_fixture(r#"["hiddenHelper"]"#),
        "{out}"
    );
    assert!(query.get("snapshot").is_none(), "{out}");
    let all = run(&root.0, query.clone()).expect("lead runs");
    assert!(
        all["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| file["symbols"].as_array().into_iter().flatten())
            .any(|symbol| symbol["symbolName"] == "hiddenHelper"),
        "{all}"
    );
}
