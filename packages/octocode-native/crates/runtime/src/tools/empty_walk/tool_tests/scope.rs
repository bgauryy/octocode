//! What every walking tool's scope reaches and discloses alike.
use octocode_runtime_base::execution::cancel::NeverCancel;
use serde_json::json;

use super::*;
use octocode_contract_types::AstSearchQuery;
use octocode_test_support::json_fixture;

/// An `include` glob that names a default-excluded directory (`build/*.ts`)
/// asks for its files: every tool with `include` walks into it, with no
/// lead needed.
#[test]
fn an_include_naming_a_default_excluded_directory_reaches_it_in_every_tool() {
    let scenario = &SCENARIOS[2];
    let root = Fixture::new();
    std::fs::write(root.0.join("visible.md"), "nothing here\n").expect("visible");
    std::fs::create_dir_all(root.0.join("build")).expect("build");
    std::fs::write(root.0.join("build/needle.ts"), "function needle() {}\n").expect("needle");
    for tool in tools()
        .into_iter()
        .filter(|tool| tool.name != "astSearch symbols")
    {
        let label = describe(&tool, scenario);
        let mut query = (tool.query)(&root.0);
        query["include"] = json_fixture(r#"["build/*.ts"]"#);
        let out = (tool.run)(&root.0, &query);
        assert!(
            (tool.found)(&out),
            "{label}: include did not lift the prune: {out}"
        );
    }
}

/// A generated file (`*.min.js`) is default-excluded by every searching
/// tool alike: none reads it by default, each names it and leads to it with
/// `defaultExcludes:false`. structureSearch lists it: a listing is an
/// inventory of what exists.
#[test]
fn a_generated_file_is_default_excluded_by_every_searching_tool() {
    let root = Fixture::new();
    std::fs::write(root.0.join("visible.md"), "nothing here\n").expect("visible");
    std::fs::write(root.0.join("app.min.js"), "function needle() {}\n").expect("generated");
    let searches = [
        (
            local_search(),
            json_fixture(r#"{"matchString": ["needle"]}"#),
        ),
        (
            ast_match(),
            json_fixture(
                r#"{
                    "operation": "match",
                    "parser": "javascript",
                    "rule": {"pattern": "function needle() {}"}
                }"#,
            ),
        ),
        (
            ast_symbols(),
            json_fixture(
                r#"{
                    "operation": "symbols",
                    "symbolFilter": {"terms": ["needle"], "mode": "exact"}
                }"#,
            ),
        ),
    ];
    for (tool, fields) in searches {
        let out = (tool.run)(&root.0, &row(&root.0, fields));
        assert!(
            !(tool.found)(&out),
            "{}: read a generated file: {out}",
            tool.name
        );
        let lead = skipped_query(&out).unwrap_or_else(|| panic!("{}: no lead: {out}", tool.name));
        assert_eq!(
            lead["defaultExcludes"],
            json!(false),
            "{}: {lead}",
            tool.name
        );
        assert!(
            prose(&out).contains("*.min.js")
                || out["skippedPaths"].to_string().contains("*.min.js"),
            "{}: names the pattern: {out}",
            tool.name
        );
        let found = (tool.run)(&root.0, &lead);
        assert!(
            (tool.found)(&found),
            "{}: lead found nothing: {found}",
            tool.name
        );
    }
    let listing = structure("files", &FLAGGED);
    let out = (listing.run)(
        &root.0,
        &row(&root.0, json_fixture(r#"{"operation": "files"}"#)),
    );
    assert!(out.to_string().contains("app.min.js"), "inventory: {out}");
}

/// An `include` that spells a generated file out (`app.min.js`) asks for it:
/// the default exclusion lifts for that file, but a wildcard that merely
/// reaches it (`*.js`) does not.
#[test]
fn an_include_naming_a_generated_file_exactly_reads_it() {
    let root = Fixture::new();
    std::fs::write(root.0.join("visible.md"), "nothing here\n").expect("visible");
    std::fs::write(root.0.join("app.min.js"), "function needle() {}\n").expect("generated");
    for tool in [local_search(), ast_match()] {
        let mut query = (tool.query)(&root.0);
        if tool.name == "astSearch match" {
            query["parser"] = json!("javascript");
        }
        query["include"] = json_fixture(r#"["app.min.js"]"#);
        let out = (tool.run)(&root.0, &query);
        assert!(
            (tool.found)(&out),
            "{}: exact include did not lift the default: {out}",
            tool.name
        );
        query["include"] = json_fixture(r#"["*.js"]"#);
        let out = (tool.run)(&root.0, &query);
        assert!(
            !(tool.found)(&out),
            "{}: a wildcard include read a generated file: {out}",
            tool.name
        );
    }
}

/// The file-type table is built only for a page that passed a file over: an
/// astSearch match over files every grammar reads builds nothing, one beside
/// a `.toml` builds it once.
#[test]
fn an_ast_match_builds_the_file_type_table_only_for_a_passed_over_file() {
    use octocode_tools_shared::leads::unparsed::TABLE_BUILDS;
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "fn needle() {}\n").expect("source");
    let query = row(
        &root.0,
        json_fixture(
            r#"{
                "operation": "match",
                "parser": "rust",
                "rule": {"pattern": "fn needle() {}"}
            }"#,
        ),
    );
    TABLE_BUILDS.with(|builds| builds.set(0));
    let out = run_ast(&root.0, &query);
    assert!(out["matchCount"].as_u64().is_some_and(|n| n > 0), "{out}");
    assert_eq!(TABLE_BUILDS.with(std::cell::Cell::get), 0, "{out}");
    std::fs::write(root.0.join("conf.toml"), "needle = 1\n").expect("passed over");
    run_ast(&root.0, &query);
    assert_eq!(TABLE_BUILDS.with(std::cell::Cell::get), 1);
}

/// A glob with a `..` segment never matches under `path`: every tool with
/// globs warns of it in the same words.
#[test]
fn a_parent_glob_is_warned_of_in_every_tool() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.ts"), "function needle() {}\n").expect("source");
    let expected = octocode_runtime_base::policy::include::parent_glob_warning(
        &["../src/*.ts".to_owned()],
        &[],
    )
    .expect("warning");
    for tool in tools() {
        let mut query = (tool.query)(&root.0);
        let field = if tool.name == "astSearch symbols" {
            "exclude"
        } else {
            "include"
        };
        query[field] = json_fixture(r#"["../src/*.ts"]"#);
        let out = (tool.run)(&root.0, &query);
        let expected = if field == "exclude" {
            octocode_runtime_base::policy::include::parent_glob_warning(
                &[],
                &["../src/*.ts".to_owned()],
            )
            .expect("warning")
        } else {
            expected.clone()
        };
        assert!(prose(&out).contains(&expected), "{}: {out}", tool.name);
    }
}

/// astSearch admits a source by the one text rule in both modes: a file
/// named as `path` parses exactly as the directory scan parses it. A
/// BOM-marked UTF-16 file decodes (as localFetch reads it) and matches; a
/// Latin-1 file matches with the decode warning; a file with a NUL is binary
/// in both.
#[test]
fn ast_search_admits_a_source_the_same_as_a_file_and_in_a_directory() {
    let root = Fixture::new();
    let utf16 = std::iter::once(0xFEFF_u16)
        .chain("function needle() {}\n".encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    std::fs::write(root.0.join("wide.js"), utf16).expect("utf16");
    std::fs::write(root.0.join("blob.js"), b"function needle() {}\n\0\x01\x02").expect("blob");
    let matched = |path: &Path| {
        let out = run_ast(
            &root.0,
            &row(
                path,
                json_fixture(
                    r#"{
                        "operation": "match",
                        "parser": "javascript",
                        "rule": {"pattern": "function needle() {}"}
                    }"#,
                ),
            ),
        );
        out["matchCount"].as_u64().unwrap_or(0)
    };
    assert_eq!(matched(&root.0.join("wide.js")), 1, "UTF-16 file");
    assert_eq!(
        matched(&root.0),
        1,
        "UTF-16 in the directory scan, the NUL file refused"
    );
    let query: AstSearchQuery = serde_json::from_value(row(
        &root.0.join("blob.js"),
        json_fixture(
            r#"{
                "operation": "match",
                "parser": "javascript",
                "rule": {"pattern": "function needle() {}"}
            }"#,
        ),
    ))
    .expect("query");
    let (paths, security) = simple_policy(&root.0);
    let error = octocode_tool_ast::ast_search::execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &NeverCancel,
        None,
    )
    .expect_err("binary");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::BinaryFileUnsupported
    );
    // Latin-1 is admitted as a read shows it, in both modes: the match is
    // found and the row states the shared decode warning.
    std::fs::create_dir(root.0.join("legacy")).expect("legacy");
    std::fs::write(
        root.0.join("legacy/latin.js"),
        b"function needle() {} // caf\xe9\n",
    )
    .expect("latin");
    for path in [root.0.join("legacy"), root.0.join("legacy/latin.js")] {
        let out = run_ast(
            &root.0,
            &row(
                &path,
                json_fixture(
                    r#"{
                        "operation": "match",
                        "parser": "javascript",
                        "rule": {"pattern": "function needle() {}"}
                    }"#,
                ),
            ),
        );
        assert_eq!(out["matchCount"], 1, "Latin-1 at {path:?}: {out}");
        assert!(
            out.to_string()
                .contains(octocode_engine::text::LATIN1_WARNING),
            "{path:?}: {out}"
        );
    }
}

/// Skipped paths remain disclosed even when a bounded probe cannot prove
/// whether they contain a match. The related search lifts that scope.
#[test]
fn every_tool_discloses_unchecked_scope_and_offers_an_executable_related_search() {
    let scenario = &SCENARIOS[2];
    for tool in tools() {
        let label = describe(&tool, scenario);
        let root = repo(scenario, "needle.ts", "function needle() {}\n");
        let query = (tool.query)(&root.0);
        let out = (tool.run)(&root.0, &query);
        assert!(
            out["skippedPaths"].to_string().contains("node_modules")
                || prose(&out).contains("node_modules"),
            "{label}: skipped directory undisclosed: {out}"
        );
        let out = octocode_tools_shared::empty_walk::with_probe_limits(
            octocode_tools_shared::empty_walk::CUT_AT_ONCE,
            || (tool.run)(&root.0, &query),
        );
        assert!(
            out.get("partialReasons").is_none() && out["isPartial"] != json!(true),
            "{label}: the requested scope was walked whole: {out}"
        );
        assert!(prose(&out).contains("were not searched"), "{label}: {out}");
        let lead = skipped_query(&out).unwrap_or_else(|| panic!("{label}: {out}"));
        assert_eq!(lead["defaultExcludes"], json!(false), "{label}: {lead}");
    }
}

/// One prune semantic in every walking tool: inside a git work tree a
/// default-excluded name git does not ignore (a repository's own `build/`)
/// is walked, and outside one it is pruned by its name alone.
#[test]
fn a_git_tracked_build_dir_is_walked_by_every_tool_alike() {
    for git in [true, false] {
        let root = Fixture::new();
        if git {
            std::fs::create_dir(root.0.join(".git")).expect("repository marker");
        }
        std::fs::write(root.0.join("visible.md"), "nothing here\n").expect("visible");
        std::fs::create_dir_all(root.0.join("build")).expect("build");
        std::fs::write(root.0.join("build/needle.ts"), "function needle() {}\n").expect("needle");
        for tool in tools() {
            let out = (tool.run)(&root.0, &(tool.query)(&root.0));
            assert_eq!(
                (tool.found)(&out),
                git,
                "{} (git work tree: {git}): {out}",
                tool.name
            );
        }
    }
}
