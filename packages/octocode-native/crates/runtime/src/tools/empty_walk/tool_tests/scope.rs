//! What every walking tool's scope reaches and discloses alike.

use super::*;

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
        query["include"] = json!(["build/*.ts"]);
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
        (local_search(), json!({"matchString": "needle"})),
        (
            ast_match(),
            json!({"operation": "match", "language": "javascript", "pattern": "function needle() {}"}),
        ),
        (
            ast_symbols(),
            json!({"operation": "symbols", "symbolName": "needle"}),
        ),
    ];
    for (tool, fields) in searches {
        let out = (tool.run)(&root.0, &row(&root.0, fields));
        assert!(
            !(tool.found)(&out),
            "{}: read a generated file: {out}",
            tool.name
        );
        let lead = lead_query(&out, "includeSkipped")
            .unwrap_or_else(|| panic!("{}: no lead: {out}", tool.name));
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
    let out = (listing.run)(&root.0, &row(&root.0, json!({"operation": "files"})));
    assert!(out.to_string().contains("app.min.js"), "inventory: {out}");
}

/// A glob with a `..` segment never matches under `path`: every tool with
/// globs warns of it in the same words.
#[test]
fn a_parent_glob_is_warned_of_in_every_tool() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.ts"), "function needle() {}\n").expect("source");
    let expected = crate::policy::include::parent_glob_warning(&["../src/*.ts".to_owned()], &[])
        .expect("warning");
    for tool in tools() {
        let mut query = (tool.query)(&root.0);
        let field = if tool.name == "astSearch symbols" {
            "exclude"
        } else {
            "include"
        };
        query[field] = json!(["../src/*.ts"]);
        let out = (tool.run)(&root.0, &query);
        let expected = if field == "exclude" {
            crate::policy::include::parent_glob_warning(&[], &["../src/*.ts".to_owned()])
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
/// file with a NUL is binary in both.
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
                json!({"operation": "match", "language": "javascript", "pattern": "function needle() {}"}),
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
    let query: crate::tools::ast_search::AstSearchQuery = serde_json::from_value(row(
        &root.0.join("blob.js"),
        json!({"operation": "match", "language": "javascript", "pattern": "function needle() {}"}),
    ))
    .expect("query");
    let (paths, security) = simple_policy(&root.0);
    let error =
        crate::tools::ast_search::execute_ast(&query, &paths, &security, &NeverCancel, None)
            .expect_err("binary");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::BinaryFileUnsupported
    );
    // Latin-1 is no Unicode text: the scan skips it unreadable, and the file
    // named alone is refused the same way, never parsed from a guess.
    std::fs::create_dir(root.0.join("legacy")).expect("legacy");
    std::fs::write(
        root.0.join("legacy/latin.js"),
        b"function needle() {} // caf\xe9\n",
    )
    .expect("latin");
    assert_eq!(
        matched(&root.0.join("legacy")),
        0,
        "Latin-1 in the directory scan"
    );
    let query: crate::tools::ast_search::AstSearchQuery = serde_json::from_value(row(
        &root.0.join("legacy/latin.js"),
        json!({"operation": "match", "language": "javascript", "pattern": "function needle() {}"}),
    ))
    .expect("query");
    let error =
        crate::tools::ast_search::execute_ast(&query, &paths, &security, &NeverCancel, None)
            .expect_err("not unicode");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::FileReadFailed,
        "{error:?}"
    );
}

/// Every walking tool states what its skipped paths hold in one shape
/// (`… behind skipped paths (…)`), and a check of them cut before it found
/// anything the same way too: a warning that absence there is unproven, the
/// result partial (`skippedPathsUnchecked`), and the lead over every cause
/// the walk skipped by.
#[test]
fn every_tool_discloses_skipped_paths_and_a_cut_check_in_one_shape() {
    let scenario = &SCENARIOS[2];
    for tool in tools() {
        let label = describe(&tool, scenario);
        let root = repo(scenario, "needle.ts", "function needle() {}\n");
        let query = (tool.query)(&root.0);
        let out = (tool.run)(&root.0, &query);
        assert!(
            prose(&out).contains(" behind skipped paths (node_modules/)"),
            "{label}: {out}"
        );
        let out = crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
            (tool.run)(&root.0, &query)
        });
        assert!(
            prose(&out).contains("were not checked in time, so absence there is unproven."),
            "{label}: {out}"
        );
        assert!(
            out["partialReasons"]
                .as_array()
                .is_some_and(|reasons| reasons.contains(&json!("skippedPathsUnchecked"))),
            "{label}: {out}"
        );
        let lead = lead_query(&out, "includeSkipped").unwrap_or_else(|| panic!("{label}: {out}"));
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
