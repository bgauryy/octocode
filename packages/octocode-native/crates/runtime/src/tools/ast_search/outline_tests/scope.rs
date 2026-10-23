//! What an outline accepts and covers: kinds and source limits, cached
//! pages, default-excluded directories and the empty hint.

use super::*;

#[test]
fn unknown_symbol_kinds_are_rejected_and_source_limits_are_errors() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(&source, "pub fn visible() {}\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"kinds":["functoin"]}),
    )
    .expect_err("unknown kind");
    // An `.input.invalid` code is an input rejection: CLI exit 2 and a
    // "correct the field" hint, never "broaden the query".
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert_eq!(
        crate::id::error_codes::class(error.code),
        crate::id::error_codes::ErrorClass::InvalidInput
    );
    assert!(
        error.message.contains("function, impl"),
        "{}",
        error.message
    );
    let ok = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"kinds":["function"]}),
    )
    .expect("known kind");
    assert_eq!(outline_name(&ok["symbols"][0]), "visible");

    let large = root.0.join("large.rs");
    std::fs::write(
        &large,
        "// x\n".repeat(crate::tools::ast_search::source::MAX_PARSE_SOURCE_BYTES / 5 + 1),
    )
    .expect("large");
    for query in [
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":large}),
        json!({"operation":"syntaxTree","mainGoal":"test","reasoning":"test","path":large}),
    ] {
        let out = run(&root.0, query).expect("limit row");
        assert_eq!(out["errorCode"], "fileTooLarge", "{out}");
        assert_eq!(out["status"], "error", "{out}");
        // No page reaches the rest: a terminal limit naming its cut.
        assert_eq!(out["terminalLimit"], true, "{out}");
        assert_eq!(out["partialReasons"], json!(["fileTooLarge"]), "{out}");
        // No literal searched: the typed error is the whole answer.
        assert!(
            out.get("next").is_none() && out.get("hints").is_none(),
            "{out}"
        );
    }
    // A searched name leads to a text search of it in the same file.
    let named = run(
        &root.0,
        json!({"operation":"symbols","path":large,"symbolName":"visible"}),
    )
    .expect("limit row");
    let lead = &named["next"]["findText"];
    assert_eq!(lead["tool"], "localSearch", "{named}");
    assert_eq!(
        lead["query"]["queries"][0]["matchString"], "visible",
        "{named}"
    );
    assert!(
        lead["query"]["queries"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("large.rs")),
        "{named}"
    );
    let pattern = run(
        &root.0,
        json!({"operation":"match","path":large,"pattern":"console.log"}),
    );
    let error = pattern.expect_err("pattern over the parse limit");
    assert_eq!(error.code, ErrorCode::FileTooLarge);
    let next = error.next.expect("findText lead");
    assert_eq!(next["findText"]["query"]["queries"][0]["regex"], "literal");
    use crate::response::output::ToolOutput;
    assert_eq!(
        crate::tools::ast_search::output::Output
            .error_hint(ErrorCode::FileTooLarge, &serde_json::Value::Null),
        Some(crate::response::output::NO_HINT)
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
    let first = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":1}),
    )
    .expect("first");
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    assert_eq!(
        outline_name(&run(&root.0, next.clone()).expect("second")["symbols"][0]),
        "two"
    );
    // Same byte length: cache invalidation cannot rely only on file size.
    std::fs::write(
        &source,
        "export function one() {}\nexport function six() {}\n",
    )
    .expect("changed");
    let stale = run(&root.0, next.clone()).expect("stale");
    assert_eq!(stale["errorCode"], "staleSnapshot");
    let restart = &stale["next"]["restart"];
    assert_eq!(restart["tool"], "astSearch");
    assert_eq!(restart["query"]["queries"][0]["operation"], "symbols");
    assert_eq!(restart["query"]["queries"][0]["pageSize"], 1);
    // The first page (the default), without any cursor.
    for cursor in ["page", "snapshot", "scanOffset"] {
        assert!(
            restart["query"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {stale}"
        );
    }
    let restarted =
        run(&root.0, restart["query"].clone()).expect("execute returned restart unchanged");
    assert_eq!(outline_name(&restarted["symbols"][0]), "one");
    assert_ne!(restarted["snapshot"], first["snapshot"]);
    let second = run(
        &root.0,
        restarted["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("new snapshot continuation");
    assert_eq!(outline_name(&second["symbols"][0]), "six");
    let fresh = run(
        &root.0,
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":source,"pageSize":2}),
    )
    .expect("fresh");
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
        json!({"operation":"symbols","path":root.0,"symbolName":"missing_name"}),
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
    let lead = &out["next"]["findText"];
    assert_eq!(lead["tool"], "localSearch", "{out}");
    assert_eq!(
        lead["query"]["queries"][0]["matchString"], "missing_name",
        "{out}"
    );
    // Without a name, the hint counts the files and names no text lead.
    let outline = run(
        &root.0,
        json!({"operation":"symbols","path":root.0,"kinds":["class"]}),
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
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"symbolName":"hiddenHelper"}),
    )
    .expect("symbols");
    let names = skipped_names(&out, "default-excluded");
    assert_eq!(names, ["node_modules"], "{out}");
    let lead = &out["next"]["includeSkipped"];
    assert_eq!(lead["tool"], "astSearch", "{out}");
    let query = &lead["query"]["queries"][0];
    assert_eq!(query["defaultExcludes"], false, "{out}");
    assert_eq!(query["symbolName"], "hiddenHelper", "{out}");
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
