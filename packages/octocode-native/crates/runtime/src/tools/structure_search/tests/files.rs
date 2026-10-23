//! `files` listings: sort order, continuation snapshots, withheld coverage,
//! summaries and entry details shared with `tree`.

use super::*;

#[test]
fn files_path_sort_is_lexicographic_and_stable() {
    let root = Fixture::new();
    for name in ["zebra.rs", "apple.rs", "mango.rs", "banana.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let run = || {
        let out = execute_row(
            json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","sort":"path"}),
            &paths,
            &security,
            &NeverCancel,
        )
        .expect("files");
        listed(&out)
            .iter()
            .map(|f| f["path"].as_str().expect("path").to_string())
            .collect::<Vec<_>>()
    };
    let first = run();
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted, "path sort must be lexicographic");
    let second = run();
    assert_eq!(first, second, "path sort must be stable across runs");
}

#[test]
fn files_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    // Happy path: the freshly emitted snapshot must be accepted on page 2.
    let good = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("good page2");
    assert!(
        good.get("errorCode").is_none(),
        "valid continuation must not be rejected; got {good}"
    );
    assert_eq!(good["pagination"]["currentPage"], json!(2));
    std::fs::write(root.0.join("newcomer.rs"), "x\n").expect("mutate corpus");
    let page2 = execute_row(
        json!({"operation":"files","mainGoal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page2");
    assert_eq!(page2["errorCode"], json!("staleSnapshot"), "got {page2}");
    let restart = &page2["next"]["restart"];
    assert_eq!(restart["tool"], "structureSearch");
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(
            restart["query"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {restart}"
        );
    }
    assert_eq!(restart["query"]["queries"][0]["entryType"], "f");
    let fresh = execute_row(
        restart["query"]["queries"][0].clone(),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("execute returned restart unchanged");
    assert_eq!(fresh["pagination"]["currentPage"], 1);
    assert_eq!(fresh["pagination"]["totalItems"], 7);
    assert_ne!(fresh["snapshot"], page1["snapshot"]);
}

/// A continuation that names its root relative to the workspace (as the
/// response envelope spells it) continues the same snapshot; the page number
/// travels once, in `next.nextPage.query`.
#[test]
fn a_workspace_relative_continuation_keeps_its_snapshot() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("sub")).expect("dir");
    for i in 0..6 {
        std::fs::write(root.0.join(format!("sub/f{i}.rs")), "x\n").expect("file");
    }
    for operation in ["tree", "files"] {
        let first = run(&root.0, json!({"operation":operation,"path":root.0.join("sub"),"pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first");
        assert!(
            first["pagination"].get("nextPage").is_none(),
            "{operation}: {first}"
        );
        let mut relative = first["next"]["nextPage"]["query"]["queries"][0].clone();
        relative["path"] = json!("sub");
        let second = run(&root.0, relative).expect("second");
        assert!(second.get("errorCode").is_none(), "{operation}: {second}");
        assert_eq!(
            second["pagination"]["currentPage"], 2,
            "{operation}: {second}"
        );
    }
}

#[test]
fn continuation_snapshots_bind_page_size_and_rendered_metadata() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    for operation in ["tree", "files"] {
        let first = run(&root.0, json!({"operation":operation,"path":root.0,"pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first");
        let mut resized = first["next"]["nextPage"]["query"]["queries"][0].clone();
        resized["pageSize"] = json!(3);
        let rejected = run(&root.0, resized).expect("resized cursor");
        assert_eq!(
            rejected["errorCode"], "staleSnapshot",
            "{operation}: {rejected}"
        );
        let restarted = run(
            &root.0,
            rejected["next"]["restart"]["query"]["queries"][0].clone(),
        )
        .expect("restart");
        assert_eq!(restarted["pagination"]["currentPage"], 1);
    }
    let first = run(&root.0, json!({"operation":"files","path":root.0,"extras":["details"],"pageSize":2,"mainGoal":"test","reasoning":"test"})).expect("first files");
    std::fs::write(root.0.join("f3.rs"), "changed\nmetadata\n").expect("change listed evidence");
    let rejected = run(
        &root.0,
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("unchanged cursor");
    assert_eq!(rejected["errorCode"], "staleSnapshot", "{rejected}");
}

#[test]
fn files_disclose_policy_withheld_coverage_without_exposing_names() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    let out = run(&root.0, json!({"operation":"files","path":root.0,"include":[".env.production"],"includeIgnored":true,"includeHidden":true,"defaultExcludes":false,"mainGoal":"test","reasoning":"test"})).expect("listing");
    assert!(listed(&out).is_empty(), "{out}");
    // Counted at the walked scope: the withheld name stays withheld.
    let reasons = skipped_reasons(&out, "withheld");
    assert_eq!(reasons, [("credential file".into(), 1)], "{out}");
    crate::tools::test_support::checks::assert_absent(&out["skippedPaths"], ".env.production");
}

/// `tree` and `files` count a symlink alike in `summary`: a non-directory
/// entry of its own size.
#[cfg(unix)]
#[test]
fn tree_and_files_summaries_count_symlinks_alike() {
    let root = Fixture::new();
    std::fs::write(root.0.join("target.md"), "abc").expect("target");
    std::fs::create_dir(root.0.join("links")).expect("dir");
    for name in ["a", "b"] {
        std::os::unix::fs::symlink("../target.md", root.0.join("links").join(name)).expect("link");
    }
    let summary = |operation: &str| {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0.join("links"),"extras":["details"]}),
        )
        .expect("listing");
        out["summary"].clone()
    };
    let tree = summary("tree");
    assert_eq!(tree["files"], 2, "{tree}");
    // A link's own length is no file content: bytes count regular files.
    assert_eq!(tree["bytes"], 0, "{tree}");
    assert_eq!(tree, summary("files"));
}

/// Both operations' `summary` totals the listing (every page of it, within
/// `maxEntries`), never entries the walk cut off.
#[test]
fn summaries_total_the_listing_not_the_whole_walk() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"] {
        std::fs::write(root.0.join(name), "xy").expect("file");
    }
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "maxEntries": 2, "entryType": "f"}),
        )
        .expect("listing");
        assert_eq!(
            out["summary"],
            json!({"files": 2, "dirs": 0, "bytes": 4}),
            "{operation}: {out}"
        );
    }
}

/// A tree entry has the one listing-entry shape `files` uses and the output
/// schema states: `extras: ["details"]` adds `totalLines=N` and
/// `modified=<ISO instant>` to each file, and a symlink is its own size plus the `symlink` field.
#[cfg(unix)]
#[test]
fn tree_entries_carry_details_and_symlink_fields_like_files() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("file");
    std::os::unix::fs::symlink(root.0.join("two.rs"), root.0.join("link.rs")).expect("link");
    let tree = |extras: Value| {
        run(
            &root.0,
            json!({"operation":"tree","path":root.0,"maxDepth":1,"extras":extras}),
        )
        .expect("tree")
    };
    let basic = tree(json!([]));
    assert!(
        basic["files"][0]["files"]
            .as_array()
            .is_some_and(|entries| entries.contains(&json!("two.rs (4)"))),
        "{basic}"
    );
    let link = basic["files"][0]["files"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .find(|entry| entry.starts_with("link.rs "))
        })
        .unwrap_or_else(|| panic!("link.rs entry: {basic}"));
    assert!(
        link.ends_with(", symlink)"),
        "the link's size, then symlink: {basic}"
    );
    let detailed = tree(json!(["details"]));
    let entry = detailed["files"][0]["files"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .find(|entry| entry.starts_with("two.rs "))
        })
        .unwrap_or_else(|| panic!("two.rs entry: {detailed}"));
    assert!(
        entry.starts_with("two.rs (4, totalLines=2, modified="),
        "{detailed}"
    );
}

/// The field a listing is sorted by is default output (L6-8): a modified
/// sort shows each file's `modified` time without `extras`, as the lines sort
/// shows `totalLines`; another sort leaves it to `extras: ["details"]`.
#[test]
fn a_modified_sort_shows_its_key_without_extras() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("file");
    let entry = |sort: &str| {
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"entryType":"f","sort":sort}),
        )
        .expect("files");
        out["files"][0]["files"][0]
            .as_str()
            .unwrap_or_else(|| panic!("entry: {out}"))
            .to_owned()
    };
    assert!(entry("modified").starts_with("two.rs (4, modified="));
    assert_eq!(entry("size"), "two.rs (4)");
}
