use super::*;

fn entries(out: &Value) -> Vec<&str> {
    out["files"]
        .as_array()
        .expect("groups")
        .iter()
        .flat_map(|group| group["files"].as_array().expect("entries"))
        .map(|entry| entry.as_str().expect("entry"))
        .collect()
}

#[test]
fn the_default_listing_is_names_only_and_details_adds_the_fields() {
    let root = Fixture::new();
    root.write("source.rs", "one\ntwo\n");
    for operation in ["tree", "files"] {
        let query = json!({"operation": operation, "path": root.0});
        let default = run(&root.0, query.clone()).expect("default");
        let mut named = query.clone();
        named["view"] = json!("paths");
        let paths = run(&root.0, named).expect("paths");
        assert_eq!(paths["files"], default["files"]);
        assert_eq!(
            crate::tools::test_support::pinned_snapshot(&paths),
            crate::tools::test_support::pinned_snapshot(&default)
        );
        assert_eq!(entries(&default), ["source.rs"]);
        let mut query = query;
        query["view"] = json!("details");
        let detailed = run(&root.0, query).expect("details");
        assert_eq!(listed(&detailed)[0]["totalLines"], 2);
    }
}

#[test]
fn paths_preserves_names_and_kinds_without_reading_file_content() {
    let root = Fixture::new();
    root.write("report (2026)", "one\ntwo\n");
    root.write("naïve file.rs", "source\n");
    std::fs::create_dir(root.0.join("empty dir")).expect("directory");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.0.join("naïve file.rs"), root.0.join("source link"))
        .expect("symlink");
    let mut expected = vec!["empty dir/", "naïve file.rs", "report (2026)"];
    #[cfg(unix)]
    expected.push("source link (symlink)");
    expected.sort_unstable();
    for operation in ["tree", "files"] {
        let before = super::super::rows::line_count_reads(&root.0);
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "view": "paths", "maxDepth": 1}),
        )
        .expect("paths");
        assert_eq!(super::super::rows::line_count_reads(&root.0), before);
        let mut actual = entries(&out);
        actual.sort_unstable();
        assert_eq!(actual, expected, "{operation}: {out}");
    }
}

/// The names-only view under a size, line or time rank states the one value
/// it ranks by on each row, in the details view's own spelling, and nothing
/// else; a name or path rank, whose key is the name itself, states names only.
#[test]
fn paths_under_a_metric_sort_state_that_one_metric() {
    let root = Fixture::new();
    root.write("a-short.rs", "one\n");
    root.write("z-long.rs", "one\ntwo\nthree\n");
    std::fs::create_dir(root.0.join("dir")).expect("directory");
    let ranked = |by: &str, order: &str| {
        run(
            &root.0,
            json!({"operation": "files", "path": root.0, "view": "paths", "maxDepth": 1, "sort": {"by": by, "order": order}}),
        )
        .expect("ranked paths")
    };
    let before = super::super::rows::line_count_reads(&root.0);
    let out = ranked("totalLines", "desc");
    // The ranking reads each file once; emitting the rows reads none.
    assert_eq!(super::super::rows::line_count_reads(&root.0) - before, 2);
    assert_eq!(
        entries(&out),
        [
            "z-long.rs (totalLines=3)",
            "a-short.rs (totalLines=1)",
            "dir/"
        ]
    );
    // A directory has no content size: it ranks last and states none.
    assert_eq!(
        entries(&ranked("sizeBytes", "asc")),
        ["a-short.rs (4)", "z-long.rs (14)", "dir/"]
    );
    assert_eq!(
        entries(&ranked("sizeBytes", "desc")),
        ["z-long.rs (14)", "a-short.rs (4)", "dir/"]
    );
    let by_time = ranked("modifiedAt", "desc");
    let rows = entries(&by_time);
    assert_eq!(rows.len(), 3, "{by_time}");
    for row in rows {
        let (_, fields) = row.split_once(" (").expect("one field");
        assert!(
            fields.starts_with("modified=") && !fields.contains(','),
            "{row}"
        );
    }
    for (by, order) in [
        ("name", "asc"),
        ("path", "asc"),
        ("path", "desc"),
        ("traversal", "asc"),
    ] {
        let mut names = entries(&ranked(by, order))
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["a-short.rs", "dir/", "z-long.rs"], "{by} {order}");
    }
    // The details view is unchanged by the sort: all of its fields.
    let mut details = json!({"operation": "files", "path": root.0, "view": "details", "entryType": "file", "sort": {"by": "sizeBytes", "order": "desc"}});
    let detailed = run(&root.0, details.take()).expect("details");
    assert!(
        entries(&detailed)[0].starts_with("z-long.rs (14, totalLines=3, modified="),
        "{detailed}"
    );
}

#[test]
fn paths_pages_preserve_every_detailed_path_once_and_keep_the_view() {
    let root = Fixture::new();
    for dir in ["a", "b/deep", "c"] {
        for index in 0..5 {
            root.write(&format!("{dir}/f{index}.rs"), "one\ntwo\n");
        }
    }
    for operation in ["tree", "files"] {
        let mut query =
            json!({"operation":operation,"view":"details", "path": root.0, "maxDepth": 4});
        let detailed = run(&root.0, query.clone()).expect("details");
        // The names-only view lists every entry the details view lists.
        let mut expected = listed(&detailed)
            .iter()
            .map(|entry| entry["path"].as_str().expect("path").to_owned())
            .collect::<Vec<_>>();
        expected.sort();
        query["view"] = json!("paths");
        for budget in [1, 150] {
            let before = super::super::rows::line_count_reads(&root.0);
            let pages = walk_pages_budget(&root.0, query.clone(), Some(budget));
            assert_eq!(super::super::rows::line_count_reads(&root.0), before);
            assert!(pages.len() > 1);
            let mut actual: Vec<_> = pages.iter().flat_map(paths).collect();
            actual.sort();
            assert_eq!(actual, expected, "{operation}, budget {budget}");
            for page in pages {
                if let Some(next) = page["next"].get("nextPage") {
                    // The continuation states the view or leaves its default.
                    let reopened = crate::tools::test_support::opened(
                        crate::id::ToolId::StructureSearch,
                        &next["input"]["queries"][0],
                    );
                    assert_eq!(reopened["view"], "paths", "{page}");
                }
            }
        }
    }
}

#[test]
fn paths_page_costs_exclude_detail_fields() {
    let root = Fixture::new();
    for index in 0..12 {
        root.write(&format!("f{index:02}.rs"), "one\ntwo\n");
    }
    for operation in ["tree", "files"] {
        let mut query = json!({"operation":operation,"view":"details", "path": root.0});
        let detailed = run_budget(&root.0, query.clone(), 250).expect("details");
        query["view"] = json!("paths");
        let compact = run_budget(&root.0, query, 250).expect("paths");
        assert!(entries(&compact).len() > entries(&detailed).len());
    }
}

#[test]
fn changing_view_invalidates_a_continuation_even_with_identical_page_cuts() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs"] {
        root.write(name, "source\n");
    }
    for operation in ["tree", "files"] {
        for (old, new) in [("details", "paths"), ("paths", "details")] {
            let first = run_budget(
                &root.0,
                json!({"operation": operation, "path": root.0, "view": old}),
                1,
            )
            .expect("first");
            let mut next = first["next"]["nextPage"]["input"]["queries"][0].clone();
            next["view"] = json!(new);
            let stale = run_budget(&root.0, next, 1).expect("changed view");
            assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
            let restart = crate::tools::test_support::opened(
                crate::id::ToolId::StructureSearch,
                &related(&stale, "structureSearch")["input"]["queries"][0],
            );
            assert_eq!(restart["view"], new, "{stale}");
        }
    }
}
