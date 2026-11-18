//! Empty-listing leads: past skipped paths, name filters, metadata filters
//! and the depth bound, each built from the query without walking again.
use super::*;
use octocode_test_support::json_fixture;

/// An empty listing whose walk skipped an ignored entry says the skipped
/// paths were not searched and leads to them, without walking them.
#[test]
fn an_empty_listing_names_its_skipped_causes_without_walking_them() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "*.gen.ts\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("sub/inner")).expect("dirs");
    std::fs::write(root.0.join("sub/inner/a.gen.ts"), "x\n").expect("generated");
    for operation in ["files", "tree"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": "sub/inner", "extensions": ["ts"]}),
        )
        .expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        let warning = out["warnings"][0].as_str().unwrap_or_default();
        assert!(
            warning.starts_with("Skipped paths (ignored; default-excluded) were not searched"),
            "{operation}: {out}"
        );
        assert!(
            !out["files"].to_string().contains("a.gen.ts"),
            "{operation}: {out}"
        );
        let lead = &related(&out, "structureSearch")["input"]["queries"][0];
        assert_eq!(lead["includeIgnored"], true, "{operation}: {out}");
    }
}

/// An entry the caller excluded is the caller's own filter: the walk drops
/// it before the path policy sees it, so it is never counted as withheld
/// or hidden.
#[test]
fn excluded_entries_are_not_counted_as_withheld_or_hidden() {
    let root = sensitive_fixture();
    for operation in ["tree", "files"] {
        let plain = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"includeHidden":true}),
        )
        .expect("listing");
        assert!(
            skipped_count(&plain, "withheld") > 0,
            "{operation}: {plain}"
        );
        let excluded = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"includeHidden":true,"exclude":["terraform.tfstate",".aws"]}),
        )
        .expect("excluded listing");
        let count = skipped_count(&excluded, "withheld");
        assert_eq!(count, 0, "{operation}: {excluded}");
        let hidden = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"exclude":[".aws"]}),
        )
        .expect("listing without hidden");
        assert_eq!(skipped_count(&hidden, "hidden"), 0, "{operation}: {hidden}");
    }
}

/// An empty listing its name filters alone emptied (nothing skipped to
/// lift) leads to the same listing without them (`broadenSearch`), which
/// lists what is there; a tree keeps the depth it walked.
#[test]
fn a_name_filter_miss_leads_to_the_listing_without_its_name_filters() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/deep")).expect("dir");
    std::fs::write(root.0.join("src/deep/lib.rs"), "x\n").expect("file");
    for query in [
        json!({"operation":"files","path":root.0,"nameRegex":"^nomatch$","entryType": "file"}),
        json!({"operation":"files","path":root.0,"include":["*.nomatch"]}),
    ] {
        let out = run(&root.0, query.clone()).expect("empty listing");
        assert_eq!(out["status"], "empty", "{out}");
        octocode_runtime_base::test_support::assert_row_contract(
            octocode_runtime_base::id::ToolId::StructureSearch,
            &out,
        );
        let lead = &related(&out, "structureSearch");
        assert_eq!(lead["tool"], "structureSearch", "{out}");
        let row = &lead["input"]["queries"][0];
        for field in ["nameRegex", "include", "extensions"] {
            assert!(row.get(field).is_none(), "{field}: {row}");
        }
        assert_eq!(row["operation"], query["operation"], "{row}");
        assert!(
            out["hints"]
                .as_array()
                .is_some_and(|hints| hints.iter().any(|hint| hint
                    .as_str()
                    .is_some_and(|text| text.starts_with("No entry matches")))),
            "{out}"
        );
        let broadened = run(&root.0, row.clone()).expect("broadened listing");
        assert!(
            broadened.to_string().contains("lib.rs"),
            "{query}: {broadened}"
        );
    }
}

/// An empty listing its metadata filters alone emptied (no name filter, nothing
/// skipped to lift) leads to the same listing without them (`broadenSearch`),
/// as a name miss does.
#[test]
fn a_metadata_filter_miss_leads_to_the_listing_without_it() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src")).expect("dir");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("file");
    std::fs::write(root.0.join("src/main.rs"), "y\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"empty":true}),
    )
    .expect("empty listing");
    assert_eq!(out["status"], "empty", "{out}");
    let hint = out["hints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .find(|hint| hint.starts_with("No entry matches empty"))
        .unwrap_or_else(|| panic!("the miss is named: {out}"));
    assert!(hint.contains("lists without it"), "{hint}");
    let row = &related(&out, "structureSearch")["input"]["queries"][0];
    assert!(row.get("empty").is_none(), "{row}");
}

/// An empty listing a depth bound emptied (a tree lists one level unless told
/// otherwise) may be no name miss: the lead keeps the filters and lifts the
/// bound.
#[test]
fn an_empty_listing_a_depth_bound_emptied_leads_below_it() {
    let root = Fixture::new();
    root.write("src/deep/x.rs", "x\n");
    root.write("readme.txt", "x\n");
    for query in [
        json!({"path":root.0,"extensions":["rs"]}),
        json!({"operation":"files","path":root.0,"extensions":["rs"],"maxDepth":1}),
    ] {
        let out = run(&root.0, query).expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        let hint = out["hints"][0].as_str().expect("hint");
        assert!(hint.contains("every depth"), "{out}");
        let lead = &related(&out, "structureSearch")["input"]["queries"][0];
        assert_eq!(lead["operation"], "files", "{out}");
        assert_eq!(lead["extensions"], json_fixture(r#"["rs"]"#), "{out}");
        assert!(lead.get("maxDepth").is_none(), "{out}");
        let found = run(&root.0, lead.clone()).expect("lead");
        assert_ne!(found["status"], "empty", "{found}");
    }
}
