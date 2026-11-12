use super::*;

#[test]
fn snapshots_bind_corpus_and_response_budget_and_offer_fresh_restart() {
    for operation in ["files", "tree"] {
        let root = Fixture::new();
        for n in 0..8 {
            root.write(&format!("sub/f{n}.rs"), "one\ntwo\n");
        }
        let row = json!({"operation":operation,"view":"details","path":root.0.join("sub")});
        let first = run_budget(&root.0, row, 1).expect("first");
        let mut next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        assert!(next.is_object(), "{first}");
        next["path"] = json!("sub");
        let second = run_budget(&root.0, next.clone(), 1).expect("same snapshot");
        assert_eq!(second["pagination"]["offset"], 1, "{second}");
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
        assert_eq!(fresh["pagination"]["total"], 8, "{fresh}");
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
            json!({"operation":operation,"view":"details","path":root.0.join("links")}),
        )
        .expect("listing");
        out["summary"].clone()
    };
    let tree = summary("tree");
    assert_eq!(tree["fileCount"], 2, "{tree}");
    // A link's own length is no file content: bytes count regular files.
    assert_eq!(tree["bytes"], 0, "{tree}");
    assert_eq!(tree, summary("files"));
}

/// Both operations' `summary` totals the listing (every page of it, within
/// the walk window), never entries the walk cut off.
#[test]
fn summaries_total_the_listing_not_the_whole_walk() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"] {
        std::fs::write(root.0.join(name), "xy").expect("file");
    }
    for operation in ["tree", "files"] {
        let out = with_window(2, || {
            run(
                &root.0,
                json!({"operation":operation,"view":"details", "path": root.0, "entryType": "file"}),
            )
        })
        .expect("listing");
        assert_eq!(
            out["summary"],
            json!({"fileCount": 2, "dirCount": 0, "bytes": 4}),
            "{operation}: {out}"
        );
    }
}

/// `empty` lists either side of the line: `true` the empty files and
/// directories, `false` every other one.
#[test]
fn empty_false_lists_what_empty_true_leaves_out() {
    let root = Fixture::new();
    root.write("full.rs", "fn f() {}\n");
    root.write("blank.rs", "");
    std::fs::create_dir(root.0.join("hollow")).expect("dir");
    let names = |empty: bool| {
        let row = json!({"operation":"files","path":root.0,"empty":empty,"view":"paths"});
        let mut names = listed(&run(&root.0, row).expect("listing"))
            .iter()
            .map(|entry| entry["path"].as_str().expect("path").to_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    assert_eq!(names(true), ["./blank.rs", "./hollow"]);
    assert_eq!(names(false), ["./full.rs"]);
}

/// On Unix `\` is a legal file-name byte: a file named `f\g.rs` lists as one
/// entry of that exact name, never as `g.rs` inside a directory `f`, and the
/// listed path names the file on disk.
#[cfg(unix)]
#[test]
fn a_backslash_in_a_file_name_is_one_entry_in_every_view() {
    let root = Fixture::new();
    root.write("f\\g.rs", "one\n");
    root.write("sub/a.rs", "two\n");
    for view in ["paths", "details"] {
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"entryType":"file","view":view}),
        )
        .expect("listing");
        let entries = listed(&out);
        let paths: Vec<_> = entries
            .iter()
            .map(|entry| entry["path"].as_str().expect("path").to_owned())
            .collect();
        assert_eq!(paths.len(), 2, "{view}: {paths:?}");
        let shown = paths
            .iter()
            .find(|path| path.contains('\\'))
            .unwrap_or_else(|| panic!("{view}: {paths:?}"));
        assert_eq!(shown.trim_start_matches("./"), "f\\g.rs", "{view}");
        assert!(root.0.join("f\\g.rs").is_file(), "{view}");
    }
}

/// An ignored file named `f\g.rs` is disclosed in `skippedPaths` under its
/// exact name, not under a directory `f`.
#[cfg(unix)]
#[test]
fn skipped_paths_name_an_ignored_backslash_file_whole() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    root.write(".gitignore", "f\\\\g.rs\n");
    root.write("f\\g.rs", "one\n");
    root.write("keep.rs", "two\n");
    let out = run(&root.0, json!({"operation":"files","path":root.0})).expect("listing");
    assert_eq!(skipped_names(&out, "ignored"), ["f\\g.rs"], "{out}");
}
