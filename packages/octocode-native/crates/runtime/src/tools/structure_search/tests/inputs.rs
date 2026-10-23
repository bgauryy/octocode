//! Refused inputs and roots: invalid filters, missing and file roots, symlink
//! loops, retired astSearch shapes, and no parser reached.

use super::*;

#[test]
fn invalid_time_filters_are_rejected_instead_of_skipped() {
    // Admission rejects malformed duration strings before generated typed
    // parsing or the walk; a typed executor test cannot construct this query.
    for field in ["modifiedWithin", "modifiedBefore", "accessedWithin"] {
        let error = crate::contracts::validate_query(
            "structureSearch",
            json!({"operation":"files","path":".","time":{field:"banana"}}),
        )
        .expect_err("invalid filter must not broaden the result");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path == ["time", field]),
            "{field}: {error:?}"
        );
        let diagnostic = crate::contracts::format_input_error("structureSearch", &error);
        assert!(diagnostic.to_string().contains(field), "{diagnostic}");
    }
}

/// A duration the contract's pattern admits but no walk can apply (it
/// overflows) fails as invalid input instead of listing unfiltered.
#[test]
fn an_overflowing_time_filter_is_invalid_input_not_an_unfiltered_listing() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() {}\n");
    for field in ["modifiedWithin", "modifiedBefore", "accessedWithin"] {
        let error = run(
            &root.0,
            json!({"operation":"files","path":root.0,"time":{field:"99999999999999999999d"}}),
        )
        .expect_err("an overflowing duration must not broaden the result");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{field}: {error:?}");
        assert!(
            error.message.contains(&format!("time.{field}=")),
            "{error:?}"
        );
    }
}

#[test]
fn invalid_size_filter_is_invalid_input_not_an_execution_failure() {
    // The contract's `size` pattern rejects an unreadable bound before the
    // walk runs, and names the field.
    for (size, field) in [
        (json!({"greater":"10zz"}), "size.greater"),
        (json!({"greater":"1.5k","less":"k"}), "size.less"),
    ] {
        let error = crate::contracts::validate_query(
            "structureSearch",
            json!({"operation":"files","path":".","size":size}),
        )
        .expect_err("an unparsable size is invalid input");
        let paths: Vec<String> = error
            .issues
            .iter()
            .map(|issue| issue.path.join("."))
            .collect();
        assert!(
            paths.iter().any(|path| path.ends_with(field)),
            "{field}: {paths:?}"
        );
    }
    for valid in ["10k", "1.5m", "200", "3gb"] {
        crate::contracts::validate_query(
            "structureSearch",
            json!({"operation":"files","path":".","size":{"greater":valid}}),
        )
        .unwrap_or_else(|error| panic!("{valid}: {error:?}"));
    }
}

#[test]
fn missing_path_is_not_found() {
    let root = Fixture::new();
    let error = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0.join("nope")}),
    )
    .expect_err("missing path");
    assert_eq!(error.code, ErrorCode::PathNotFound, "{error:?}");
}

/// A file passed as a tree root is named for what it is, workspace-relative
/// (no absolute path), with a lead that reads its outline instead.
#[test]
fn a_file_tree_root_leads_to_reading_the_file() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "fn a() {}\n").expect("file");
    {
        let error = run(
            &root.0,
            json!({"operation":"tree","path":root.0.join("src/lib.rs")}),
        )
        .expect_err("a file is not a directory");
        assert_eq!(error.code, ErrorCode::NotADirectory, "{error:?}");
        assert!(
            !error.message.contains(&*root.0.to_string_lossy()),
            "absolute path leaked: {error:?}"
        );
        let lead = &error.next.as_ref().expect("lead")["read"];
        assert_eq!(lead["tool"], "localFetch", "{error:?}");
        let row = &lead["query"]["queries"][0];
        assert_eq!(row["path"], "src/lib.rs", "{error:?}");
        assert_eq!(row["minify"], "symbols", "{error:?}");
    }
}

/// A path refusal with no shared code (here a symlink loop) uses the local
/// tools' one code, never a structureSearch-only dotted code.
#[cfg(unix)]
#[test]
fn a_symlink_loop_root_is_a_path_validation_failure() {
    let root = Fixture::new();
    std::os::unix::fs::symlink(root.0.join("b"), root.0.join("a")).expect("a");
    std::os::unix::fs::symlink(root.0.join("a"), root.0.join("b")).expect("b");
    let error = run(&root.0, json!({"operation":"tree","path":root.0.join("a")}))
        .expect_err("a looping root");
    assert_eq!(error.code, ErrorCode::PathValidationFailed, "{error:?}");
}

/// A missing path leads to a tree of its nearest existing parent, so the
/// agent sees what is there instead of a dead end.
#[test]
fn a_missing_path_leads_to_its_nearest_existing_parent() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    let error = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src/gone/deeper")}),
    )
    .expect_err("missing path");
    assert_eq!(error.code, ErrorCode::PathNotFound, "{error:?}");
    let lead = &error.next.as_ref().expect("lead")["viewTree"];
    assert_eq!(lead["tool"], "structureSearch", "{lead}");
    let query = lead["query"]["queries"][0].clone();
    assert_eq!(query["operation"], "tree", "{lead}");
    assert_eq!(query["path"], "src", "{lead}");
    run(&root.0, query).expect("the parent lists");
}

#[test]
fn old_ast_shapes_are_not_structure_queries() {
    let root = Fixture::new();
    for retired in [
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","treeKind":"filesystem","path":root.0}),
        json!({"operation":"syntaxTree","mainGoal": "test", "reasoning":"test","path":root.0}),
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"language":"rust"}),
    ] {
        assert!(
            serde_json::from_value::<StructureSearchQuery>(retired).is_err(),
            "not a structureSearch query"
        );
    }
}

/// Layout questions must never pay for (or depend on) a parser: the module
/// walks the filesystem only.
#[test]
fn structure_search_never_reaches_a_parser() {
    for (name, source) in [
        ("mod.rs", include_str!("../mod.rs")),
        ("files.rs", include_str!("../files.rs")),
        ("tree.rs", include_str!("../tree.rs")),
    ] {
        for forbidden in [
            "structural::",
            "ast_search",
            "syntax_tree",
            "tree_sitter",
            "ast_grep",
            "language_extensions",
        ] {
            assert!(!source.contains(forbidden), "{name} references {forbidden}");
        }
    }
}
