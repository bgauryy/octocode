use super::*;
use octocode_test_support::json_fixture;

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
    let iso = octocode_runtime_base::civil_date::iso8601_millis(1_792_843_200_000);
    for extra in [
        json_fixture(r#"{"bytes": {"min": 2, "max": 2}}"#),
        json!({"modifiedAt":{"from":iso,"to":iso},"bytes":{"min":2,"max":2}}),
        json!({"accessedAt":{"from":iso,"to":iso},"bytes":{"min":2,"max":2}}),
    ] {
        let mut query = json!({"operation":"files","view":"details","path":root.0});
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
        json!({"operation":"files","path":root.0,"sort":{"by":"bytes","order":"desc"}}),
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

/// A `traversal` (walk-order) `files` page names each directory once, on
/// every page of the walk: a directory whose own files sit on both sides of
/// a subdirectory in the walk (`memory.rs`, `memory/tests.rs`, `tests.rs`)
/// is one group, and so is a `path`-ranked page. Name, size, line and time
/// ranks keep their global order exactly and list flat
/// ([`sorted_pages_walk_one_global_order_exactly_once`]).
#[test]
fn a_traversal_files_page_names_each_directory_once() {
    let root = Fixture::new();
    for path in [
        "f/evaluate.rs",
        "f/evaluate/facts.rs",
        "f/evaluate/ordered.rs",
        "f/evaluate/outcome.rs",
        "f/memory.rs",
        "f/memory/tests.rs",
        "f/tests.rs",
        "f/tests/a.rs",
        "f/tests/b.rs",
        "f/z.rs",
    ] {
        root.write(path, "line\n");
    }
    let start = json!({"operation":"files","path":root.0.join("f"),"limit":5,
        "sort":{"by":"traversal","order":"asc"}});
    let pages = walk_pages(&root.0, start.clone());
    assert!(pages.len() > 1, "{start}: pages");
    for page in &pages {
        let dirs = group_dirs(page);
        let mut unique = dirs.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(dirs.len(), unique.len(), "{start}: {page}");
    }
}

/// A `path`-ranked page names each directory once even where the rank
/// returns to it (`memory.rs` < `memory/tests.rs` < `tests.rs`).
#[test]
fn a_path_ranked_page_names_each_directory_once() {
    let root = Fixture::new();
    for path in ["f/memory.rs", "f/memory/tests.rs", "f/tests.rs"] {
        root.write(path, "line\n");
    }
    let start = json!({"operation":"files","path":root.0.join("f"),
        "sort":{"by":"path","order":"asc"}});
    for page in walk_pages(&root.0, start) {
        let dirs = group_dirs(&page);
        let mut unique = dirs.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(dirs.len(), unique.len(), "{page}");
    }
}

/// A sorted view is one global order: walking every page lists each file
/// once, in exact rank order across pages, whatever the key and direction.
/// A page lists its entries flat, so a directory the rank returns to pulls
/// no later row forward.
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
        ("totalLines", "totalLines"),
        ("bytes", "size"),
        ("modifiedAt", "modifiedMs"),
    ] {
        for order in ["asc", "desc"] {
            let row = json!({"operation":"files","path":root.0,"entryType":"file","maxDepth":4,
                "sort":{"by":by,"order":order}});
            for budget in [None, Some(400)] {
                let pages = walk_pages_budget(&root.0, row.clone(), budget);
                assert_eq!(pages.len() > 1, budget.is_some(), "{row}: {budget:?} pages");
                for page in &pages {
                    assert!(flat_entries(page).is_some(), "{row}: flat entries: {page}");
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

/// The page's entries when every one is a flat string, else `None`.
fn flat_entries(out: &Value) -> Option<Vec<&str>> {
    out["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(Value::as_str)
        .collect()
}

/// Files whose rank by size, lines and time alternates between directories,
/// one of them binary, under a root with a file of its own.
fn ranked_tree(root: &Fixture) -> usize {
    let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_792_843_200);
    let dirs = ["", "a/", "b/deep/", "a/m/"];
    let count = 16_usize;
    for n in 0..count {
        let path = format!("{}f{n:02}.rs", dirs[n % dirs.len()]);
        if n == 5 {
            std::fs::write(root.0.join(&path), [0_u8, 159, 146, 150, 0]).expect("binary");
        } else {
            root.write(&path, "x\n".repeat(n + 1));
        }
        std::fs::File::options()
            .write(true)
            .open(root.0.join(&path))
            .expect("file")
            .set_modified(base + std::time::Duration::from_secs(u64::try_from(n).expect("n")))
            .expect("time");
    }
    count
}

/// A sort that breaks directory grouping lists each entry as one string, its
/// name the path from the row's `path`: no entry is wrapped in a group of its
/// own. Each entry states the fields the grouped form states for the same
/// file (a `path` rank of the same scope), `binary` included, and a walk of
/// every page lists each file once. Walk order and the `path` rank keep
/// their groups.
#[test]
fn a_rank_that_breaks_directory_grouping_lists_flat_paths_with_the_group_entry_fields() {
    let root = Fixture::new();
    let count = ranked_tree(&root);
    let query = |by: &str, view: &str| {
        json!({"operation":"files","path":root.0,"entryType":"file","view":view,
            "sort":{"by":by,"order":"asc"}})
    };
    let sorted = |mut rows: Vec<Value>| {
        rows.sort_by_key(|row| row["path"].as_str().expect("path").to_owned());
        rows
    };
    let grouped = run(&root.0, query("path", "details")).expect("grouped");
    assert!(flat_entries(&grouped).is_none(), "{grouped}");
    let stated = sorted(listed(&grouped));
    assert_eq!(stated.len(), count, "{grouped}");
    assert!(
        stated.iter().any(|row| row["totalLines"] == "binary"),
        "the fixture's binary file: {grouped}"
    );
    let traversal = run(&root.0, query("traversal", "paths")).expect("walk order");
    assert!(flat_entries(&traversal).is_none(), "{traversal}");
    for by in ["bytes", "totalLines", "modifiedAt", "name"] {
        for budget in [None, Some(600)] {
            let pages = walk_pages_budget(&root.0, query(by, "details"), budget);
            assert_eq!(pages.len() > 1, budget.is_some(), "{by}: {budget:?} pages");
            for page in &pages {
                let entries = flat_entries(page).unwrap_or_else(|| panic!("{by}: {page}"));
                for (entry, row) in entries.iter().zip(listed(page)) {
                    let path = row["path"].as_str().expect("path");
                    let relative = std::path::Path::new(path)
                        .strip_prefix(page["path"].as_str().expect("row path"))
                        .expect("under the row path");
                    assert!(root.0.join(relative).is_file(), "{by}: {entry}");
                    assert!(
                        entry.starts_with(&format!("{} (", relative.display())),
                        "{by}: {entry}"
                    );
                }
            }
            let walked = sorted(pages.iter().flat_map(listed).collect());
            assert_eq!(walked, stated, "{by}, {budget:?}");
        }
        // The names-only view: the same paths, flat.
        let paths_view = run(&root.0, query(by, "paths")).expect("paths");
        assert!(flat_entries(&paths_view).is_some(), "{by}: {paths_view}");
        let mut named = paths(&paths_view);
        named.sort();
        let all = stated
            .iter()
            .map(|row| row["path"].as_str().expect("path").to_owned())
            .collect::<Vec<_>>();
        assert_eq!(named, all, "{by}");
    }
}

/// One path form on every local row: a ranked (flat) listing of a directory
/// below the workspace names each entry by its workspace-relative path, the
/// path the grouped listing spells as `dir + "/" + name` and the one a
/// localSearch row of the same file states, so it runs unchanged as another
/// tool's `path`. Pages are priced as the strings they emit, prefix
/// included, and a walk of every page lists each file once.
#[test]
fn a_ranked_entry_is_named_by_the_row_path_every_local_tool_takes() {
    let root = Fixture::new();
    ranked_tree(&root);
    let listed_dir = root.0.join("a");
    let query = |by: &str, view: &str| {
        json!({"operation":"files","path":listed_dir,"entryType":"file","view":view,
            "sort":{"by":by}})
    };
    let grouped = run(&root.0, query("path", "paths")).expect("grouped");
    let mut stated = paths(&grouped);
    stated.sort();
    assert!(stated.len() > 2, "{grouped}");
    // What localSearch states for the same files.
    let (policy, security) = simple_policy(&root.0);
    let search: octocode_contract_types::LocalSearchQuery =
        serde_json::from_value(octocode_runtime_base::test_support::opened(
            octocode_runtime_base::id::ToolId::LocalSearch,
            &json!({"path": listed_dir, "matchString": ["x"], "view": "files"}),
        ))
        .expect("typed localSearch row");
    let searched = crate::test_support::search_in(&search, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("row");
    for by in ["bytes", "totalLines", "modifiedAt", "name"] {
        for view in ["paths", "details"] {
            let pages = walk_pages_budget(&root.0, query(by, view), Some(160));
            assert!(pages.len() > 1, "{by} {view}: several pages");
            let mut walked = Vec::new();
            for page in &pages {
                let entries = flat_entries(page).unwrap_or_else(|| panic!("{by}: {page}"));
                for entry in entries {
                    let path = entry.rsplit_once(" (").map_or(entry, |(path, _)| path);
                    // The entry's path runs as written from the workspace.
                    assert!(root.0.join(path).is_file(), "{by} {view}: {entry}");
                    walked.push(path.to_owned());
                }
            }
            walked.sort();
            assert_eq!(walked, stated, "{by} {view}");
            // Every file localSearch names (the binary one holds no hit) is
            // named by the same path here.
            let found = searched["files"].as_array().expect("search rows");
            assert!(found.len() > 2, "{searched}");
            for row in found {
                let path = row["path"].as_str().expect("path").to_owned();
                assert!(walked.contains(&path), "{by} {view}: {path} in {walked:?}");
            }
        }
    }
    // A names-only page costs exactly the strings it emits.
    let first = run_budget(&root.0, query("bytes", "paths"), 160).expect("first page");
    let emitted = first["files"].as_array().expect("files");
    let prefix = emitted[0]
        .as_str()
        .and_then(|entry| entry.split_once('/'))
        .map(|(prefix, _)| prefix)
        .expect("a prefixed entry");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    let walk = super::super::memo::get(
        next["snapshot"].as_str().expect("snapshot"),
        &policy.identity(),
    )
    .expect("stored walk");
    let priced = super::super::rows::costs(
        &walk.rows,
        prefix,
        octocode_contract_types::StructureSearchView::Paths,
        false,
        Some(prefix),
    )[..emitted.len()]
        .iter()
        .map(|cost| cost.entry)
        .sum::<usize>();
    assert_eq!(
        priced,
        octocode_engine::json_size::utf16_units(emitted).expect("size") - 1,
        "{first}"
    );
}

/// The declared order of a sort is its key's own: a size, line or time rank
/// with no `order` lists the largest or newest first, a name rank ascends,
/// and stating the order changes nothing else.
#[test]
fn an_omitted_order_is_the_keys_own() {
    let root = Fixture::new();
    ranked_tree(&root);
    let ranked = |sort: Value| {
        let out = run(
            &root.0,
            json!({"operation":"files","path":root.0,"entryType":"file","sort":sort}),
        )
        .expect("ranked");
        paths(&out)
    };
    for (by, own) in [
        ("bytes", "desc"),
        ("totalLines", "desc"),
        ("modifiedAt", "desc"),
        ("name", "asc"),
        ("path", "asc"),
    ] {
        let omitted = ranked(json!({"by": by}));
        assert_eq!(omitted, ranked(json!({"by": by, "order": own})), "{by}");
        let other = if own == "asc" { "desc" } else { "asc" };
        assert_ne!(omitted, ranked(json!({"by": by, "order": other})), "{by}");
    }
}

/// The flat form states no directory twice: a ranked page is shorter than
/// the one group per rank run it replaces, by at least each run's wrapper.
#[test]
fn a_flat_ranked_page_is_shorter_than_one_group_per_run() {
    let root = Fixture::new();
    ranked_tree(&root);
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType":"file",
            "sort":{"by":"bytes","order":"desc"}}),
    )
    .expect("ranked");
    let rows = listed(&out)
        .into_iter()
        .map(|row| json!({"path": row["path"]}))
        .collect::<Vec<_>>();
    let runs = octocode_runtime_base::response::groups::group_rows(
        &rows,
        octocode_runtime_base::response::groups::Order::Runs,
    )
    .unwrap_or(rows);
    let size = |value: &Value| value.to_string().len();
    assert!(
        size(&out["files"]) < size(&json!(runs)),
        "{} against {}",
        out["files"],
        json!(runs)
    );
}
