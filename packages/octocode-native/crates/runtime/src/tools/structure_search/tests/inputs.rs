use super::*;

#[test]
fn typed_filter_bounds_reject_invalid_input_before_execution() {
    for fields in [
        json!({"sizeBytes": {}}),
        json!({"sizeBytes":{"min":-1}}),
        json!({"sizeBytes":{"min":2,"max":1}}),
        json!({"sizeBytes":{"min":"1k"}}),
        json!({"modifiedAt":{}}),
        json!({"modifiedAt":{"from":"yesterday"}}),
        json!({"accessedAt":{"from":"2026-10-24T10:00:00Z","to":"2026-10-23T10:00:00Z"}}),
        json!({"minDepth":3,"maxDepth":2}),
        json!({"pageSize":1}),
        json!({"extras":["details"]}),
        json!({"entryType":"f"}),
        json!({"time":{"modifiedWithin":"1d"}}),
        json!({"size":{"greater":"1k"}}),
        json!({"sort":"name"}),
    ] {
        let mut row = json!({"operation":"files","path":"."});
        row.as_object_mut()
            .expect("row")
            .extend(fields.as_object().expect("fields").clone());
        assert!(
            crate::contracts::validate_query("structureSearch", row.clone()).is_err(),
            "{row}"
        );
    }
    for fields in [
        json!({"sizeBytes":{"min":0,"max":2}}),
        json!({"modifiedAt":{"from":"2026-10-24T10:00:00Z","to":"2026-10-24T10:00:00Z"}}),
        json!({"accessedAt":{"to":"2026-10-24T10:00:00Z"}}),
    ] {
        let mut row = json!({"operation":"files","path":"."});
        row.as_object_mut()
            .expect("row")
            .extend(fields.as_object().expect("fields").clone());
        crate::contracts::validate_query("structureSearch", row.clone())
            .unwrap_or_else(|error| panic!("{row}: {error:?}"));
    }
}

#[test]
fn missing_path_is_not_found() {
    let root = Fixture::new();
    let error = run(
        &root.0,
        json!({"operation":"tree","path":root.0.join("nope")}),
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
        let lead = &error.calls.as_ref().expect("lead")["read"];
        assert_eq!(lead["tool"], "localFetch", "{error:?}");
        let row = &lead["input"]["queries"][0];
        assert_eq!(row["path"], "src/lib.rs", "{error:?}");
        assert_eq!(row["view"], "symbols", "{error:?}");
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
    let lead = &error.calls.as_ref().expect("lead")["viewTree"];
    assert_eq!(lead["tool"], "structureSearch", "{lead}");
    let query = lead["input"]["queries"][0].clone();
    assert_eq!(query["operation"], "tree", "{lead}");
    assert_eq!(query["path"], "src", "{lead}");
    run(&root.0, query).expect("the parent lists");
}

#[test]
fn old_ast_shapes_are_not_structure_queries() {
    let root = Fixture::new();
    for retired in [
        json!({"operation":"tree","treeKind":"filesystem","path":root.0}),
        json!({"operation":"syntaxTree","path":root.0}),
        json!({"operation":"tree","path":root.0,"fileType":"rust"}),
    ] {
        assert!(
            serde_json::from_value::<StructureSearchQuery>(retired).is_err(),
            "not a structureSearch query"
        );
    }
}
