use super::*;

#[test]
fn typed_size_and_date_bounds_are_inclusive_and_do_not_match_directories() {
    let root = Fixture::new();
    root.write("small.rs", "a");
    root.write("exact.rs", "ab");
    root.write("large.rs", "abc");
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    let instant = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_843_200);
    for name in ["small.rs", "exact.rs", "large.rs"] {
        std::fs::File::options()
            .write(true)
            .open(root.0.join(name))
            .expect("file")
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(instant)
                    .set_accessed(instant),
            )
            .expect("times");
    }
    let iso = crate::civil_date::iso8601_millis(1_792_843_200_000);
    for extra in [
        json!({"sizeBytes":{"min":2,"max":2}}),
        json!({"modifiedAt":{"from":iso,"to":iso},"sizeBytes":{"min":2,"max":2}}),
        json!({"accessedAt":{"from":iso,"to":iso},"sizeBytes":{"min":2,"max":2}}),
    ] {
        let mut query = json!({"operation":"files","path":root.0});
        query
            .as_object_mut()
            .expect("row")
            .extend(extra.as_object().expect("extra").clone());
        let out = run(&root.0, query).expect("bounded listing");
        let entries = listed(&out);
        assert_eq!(entries.len(), 1, "{out}");
        assert!(
            entries[0]["path"]
                .as_str()
                .expect("path")
                .ends_with("/exact.rs"),
            "{out}"
        );
        assert_eq!(entries[0]["size"], 2);
    }
}

#[test]
fn directory_entries_never_claim_content_size() {
    let root = Fixture::new();
    root.write("a.rs", "12345");
    std::fs::create_dir(root.0.join("empty")).expect("dir");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"sort":{"by":"sizeBytes","order":"desc"}}),
    )
    .expect("listing");
    let entries = listed(&out);
    assert_eq!(entries.len(), 2, "{out}");
    assert_eq!(entries[0]["size"], 5, "{out}");
    assert_eq!(entries[1]["type"], "directory", "{out}");
    assert!(entries[1].get("size").is_none(), "{out}");
}

/// Tree order names each directory once per page: the walk's entries of a
/// directory are adjacent once the page groups them by directory.
#[test]
fn a_tree_page_groups_each_directory_once() {
    let root = Fixture::new();
    for (path, lines) in [("a/a.rs", 5), ("a/m/x.rs", 4), ("a/z.rs", 3), ("b/b.rs", 6)] {
        root.write(path, "line\n".repeat(lines));
    }
    let query = json!({"operation":"tree","path":root.0,"maxDepth":3});
    let out = run(&root.0, query.clone()).expect("listing");
    let dirs = group_dirs(&out);
    let mut unique = dirs.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(dirs.len(), unique.len(), "{query}: {out}");
}

/// A sorted view is one global order: walking every page lists each file
/// once, in exact rank order across pages, whatever the key and direction.
/// A page groups only adjacent entries of one directory, so a directory the
/// rank returns to opens a new group instead of pulling later rows forward.
#[test]
fn sorted_pages_walk_one_global_order_exactly_once() {
    let root = Fixture::new();
    let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_843_200);
    // Ranks alternate directories, so a page that merged a directory's
    // rows would break the order.
    let dirs = ["a", "b", "a/m", "c"];
    for n in 0..24_usize {
        let path = format!("{}/f{n:02}.rs", dirs[n % dirs.len()]);
        root.write(&path, "x\n".repeat(n + 1));
        std::fs::File::options()
            .write(true)
            .open(root.0.join(&path))
            .expect("file")
            .set_modified(base + std::time::Duration::from_secs(u64::try_from(n).expect("n")))
            .expect("time");
    }
    for (by, field) in [
        ("lineCount", "totalLines"),
        ("sizeBytes", "size"),
        ("modifiedAt", "modifiedMs"),
    ] {
        for order in ["asc", "desc"] {
            let row = json!({"operation":"files","path":root.0,"entryType":"file","maxDepth":4,
                "sort":{"by":by,"order":order}});
            for budget in [None, Some(1200)] {
                let pages = walk_pages_budget(&root.0, row.clone(), budget);
                assert_eq!(pages.len() > 1, budget.is_some(), "{row}: {budget:?} pages");
                for page in &pages {
                    let dirs = group_dirs(page);
                    assert!(
                        dirs.windows(2).all(|pair| pair[0] != pair[1]),
                        "{row}: adjacent groups of one directory: {page}"
                    );
                }
                let keys = pages
                    .iter()
                    .flat_map(listed)
                    .map(|entry| {
                        entry[field]
                            .as_f64()
                            .unwrap_or_else(|| panic!("{field}: {entry}"))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(keys.len(), 24, "{row}: every file exactly once");
                let mut expected = keys.clone();
                expected.sort_by(f64::total_cmp);
                if order == "desc" {
                    expected.reverse();
                }
                assert_eq!(
                    keys, expected,
                    "{row}, {budget:?}: one global order across pages"
                );
            }
        }
    }
}

fn group_dirs(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| group["dir"].as_str().expect("dir").to_owned())
        .collect()
}
