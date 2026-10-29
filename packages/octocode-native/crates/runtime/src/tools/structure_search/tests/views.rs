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
fn explicit_details_preserves_the_default_listing() {
    let root = Fixture::new();
    root.write("source.rs", "one\ntwo\n");
    for operation in ["tree", "files"] {
        let query = json!({"operation": operation, "path": root.0});
        let default = run(&root.0, query.clone()).expect("default");
        let mut query = query;
        query["view"] = json!("details");
        let detailed = run(&root.0, query).expect("details");
        assert_eq!(detailed["files"], default["files"]);
        assert_eq!(detailed["snapshot"], default["snapshot"]);
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
        let before = super::super::rows::line_count_reads();
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "view": "paths", "maxDepth": 1}),
        )
        .expect("paths");
        assert_eq!(super::super::rows::line_count_reads(), before);
        let mut actual = entries(&out);
        actual.sort_unstable();
        assert_eq!(actual, expected, "{operation}: {out}");
    }
}

#[test]
fn paths_line_sort_reads_for_ranking_but_emits_only_names() {
    let root = Fixture::new();
    root.write("a-short.rs", "one\n");
    root.write("z-long.rs", "one\ntwo\nthree\n");
    let before = super::super::rows::line_count_reads();
    let out = run(
        &root.0,
        json!({"operation": "files", "path": root.0, "view": "paths", "sort": {"by": "lineCount", "order": "desc"}}),
    )
    .expect("ranked paths");
    assert_eq!(super::super::rows::line_count_reads(), before);
    assert_eq!(entries(&out), ["z-long.rs", "a-short.rs"]);
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
        let mut query = json!({"operation": operation, "path": root.0, "maxDepth": 4});
        let detailed = run(&root.0, query.clone()).expect("details");
        let mut expected = paths(&detailed);
        expected.sort();
        query["view"] = json!("paths");
        for budget in [1, 150] {
            let before = super::super::rows::line_count_reads();
            let pages = walk_pages_budget(&root.0, query.clone(), Some(budget));
            assert_eq!(super::super::rows::line_count_reads(), before);
            assert!(pages.len() > 1);
            let mut actual: Vec<_> = pages.iter().flat_map(paths).collect();
            actual.sort();
            assert_eq!(actual, expected, "{operation}, budget {budget}");
            for page in pages {
                if let Some(next) = page["next"].get("nextPage") {
                    assert_eq!(next["input"]["queries"][0]["view"], "paths", "{page}");
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
        let mut query = json!({"operation": operation, "path": root.0});
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
            assert_eq!(
                related(&stale, "structureSearch")["input"]["queries"][0]["view"],
                new,
                "{stale}"
            );
        }
    }
}
