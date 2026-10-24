use super::*;

#[test]
fn snapshots_bind_corpus_and_response_budget_and_offer_fresh_restart() {
    for operation in ["files", "tree"] {
        let root = Fixture::new();
        for n in 0..8 {
            root.write(&format!("sub/f{n}.rs"), "one\ntwo\n");
        }
        let row = json!({"operation":operation,"path":root.0.join("sub")});
        let first = run_budget(&root.0, row, 1).expect("first");
        let mut next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        assert!(next.is_object(), "{first}");
        next["path"] = json!("sub");
        let second = run_budget(&root.0, next.clone(), 1).expect("same snapshot");
        assert_eq!(second["pagination"]["page"], 2, "{second}");
        assert!(second.get("errorCode").is_none(), "{second}");
        let resized = run_budget(&root.0, next.clone(), 5000).expect("different cut");
        assert_eq!(resized["errorCode"], "staleSnapshot", "{resized}");
        root.write("sub/f7.rs", "changed\nsource\nmetadata\n");
        let stale = run_budget(&root.0, next, 1).expect("changed source");
        assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
        let restart = &related(&stale, "structureSearch")["input"]["queries"][0];
        for field in crate::contracts::state_fields(crate::id::ToolId::StructureSearch) {
            assert!(restart.get(field).is_none(), "{field}: {restart}");
        }
        let fresh = run(&root.0, restart.clone()).expect("restart");
        assert_eq!(fresh["pagination"]["totalItems"], 8, "{fresh}");
    }
}

#[test]
fn files_disclose_policy_withheld_coverage_without_exposing_names() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    let out = run(&root.0, json!({"operation":"files","path":root.0,"include":[".env.production"],"includeIgnored":true,"includeHidden":true,"defaultExcludes":false})).expect("listing");
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
            json!({"operation":operation,"path":root.0.join("links")}),
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
            json!({"operation": operation, "path": root.0, "maxEntries": 2, "entryType": "file"}),
        )
        .expect("listing");
        assert_eq!(
            out["summary"],
            json!({"files": 2, "dirs": 0, "bytes": 4}),
            "{operation}: {out}"
        );
    }
}
