//! Expanded scans: every entry listed exactly once across scan windows.

use super::*;

/// The pages and widened scans from `query` (`walk_pages`): each page offers
/// at most one of `next.nextPage` and `next.continueWalk`, and the page that
/// offers neither ends the listing without a terminal limit. Returns every
/// listed entry and the calls made.
fn walk_chain(root: &std::path::Path, query: Value) -> (Vec<Value>, usize) {
    let pages = walk_pages(root, query);
    for out in &pages {
        let page = out["next"].get("nextPage");
        let expand = out["next"].get("continueWalk");
        assert!(
            page.is_none() || expand.is_none(),
            "one continuation per page: {out}"
        );
        if page.is_none() && expand.is_none() {
            assert!(out.get("terminalLimit").is_none(), "{out}");
        }
    }
    (pages.iter().flat_map(listed).collect(), pages.len())
}

fn listed_paths(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .map(|row| row["path"].as_str().expect("path").to_owned())
        .collect()
}

/// `next.continueWalk` resumes after the rows its window listed, so the
/// chain of pages and widened scans lists every entry exactly once (each
/// window groups its own rows by directory), for `files` under each sort and
/// for `tree`.
#[test]
fn expanded_scans_list_every_entry_exactly_once() {
    let root = Fixture::new();
    for dir in ["a", "a-b", "a/deep", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..7 {
            let body = "x\n".repeat(n + 1);
            std::fs::write(root.0.join(format!("{dir}/f{n}.go")), body).expect("file");
        }
    }
    for extra in [
        json!({"sort":"path"}),
        json!({"sort":"size"}),
        json!({"sort":"name"}),
    ] {
        let mut base = json!({"operation":"files","path":root.0,"extensions":["go"]});
        base.as_object_mut()
            .expect("query")
            .extend(extra.as_object().expect("sort").clone());
        let whole = listed_paths(&listed(&run(&root.0, base.clone()).expect("whole")));
        assert_eq!(whole.len(), 28);
        let mut cut = base.clone();
        cut["maxEntries"] = json!(3);
        cut["pageSize"] = json!(2);
        let (rows, calls) = walk_chain(&root.0, cut);
        let mut walked = listed_paths(&rows);
        assert!(calls > 4, "{calls}");
        walked.sort();
        let mut expected = whole.clone();
        expected.sort();
        assert_eq!(walked, expected, "{extra}: no duplicate, no gap");
    }
    // `tree` lists directories too: a directory a window lists bare whose
    // contents a later window groups is still one entry.
    let tree = json!({"operation":"tree","path":root.0,"maxDepth":5});
    let whole = listed_paths(&listed(&run(&root.0, tree.clone()).expect("tree")));
    let mut cut = tree;
    cut["maxEntries"] = json!(4);
    cut["pageSize"] = json!(3);
    let (rows, _) = walk_chain(&root.0, cut);
    let walked = listed_paths(&rows);
    let unique = walked.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), walked.len(), "no duplicate: {walked:?}");
    for path in &whole {
        assert!(unique.contains(path), "{path} missing: {walked:?}");
    }
    // Beyond the uncut listing, only directories a window named bare.
    assert!(
        rows.iter()
            .filter(|row| !whole.contains(&row["path"].as_str().unwrap_or("").to_owned()))
            .all(|row| row["type"] == "directory"),
        "{rows:?}"
    );
}

#[test]
fn minimum_scan_windows_recover_every_listing_entry() {
    let root = Fixture::new();
    for (name, body) in [("a.rs", "a\n"), ("b.rs", "bb\n"), ("c.rs", "ccc\n")] {
        std::fs::write(root.0.join(name), body).expect("file");
    }
    for base in [
        json!({"operation":"files","path":root.0,"sort":"path"}),
        json!({"operation":"files","path":root.0,"sort":"name"}),
        json!({"operation":"files","path":root.0,"sort":"size"}),
        json!({"operation":"tree","path":root.0}),
    ] {
        let mut expected = listed_paths(&listed(&run(&root.0, base.clone()).expect("whole")));
        expected.sort();
        assert_eq!(expected.len(), 3);
        for limit in [1, 2] {
            let mut query = base.clone();
            query["maxEntries"] = json!(limit);
            query["pageSize"] = json!(1);
            let first = run(&root.0, query.clone()).expect("first page");
            if listed(&first).is_empty() {
                let expand = &first["next"]["continueWalk"]["query"]["queries"][0];
                assert!(expand.get("scanOffset").is_none(), "{first}");
                assert!(expand["maxEntries"].as_u64().expect("limit") > limit);
            }
            let (rows, _) = walk_chain(&root.0, query);
            let mut actual = listed_paths(&rows);
            actual.sort();
            assert_eq!(actual, expected, "{base}, maxEntries:{limit}");
        }
    }
}

/// The widened scan is offered only on the last page of its window, and it
/// carries the window's end as `scanOffset`.
#[test]
fn expand_scan_rides_the_last_page_of_its_window() {
    let root = Fixture::new();
    for n in 0..9 {
        std::fs::write(root.0.join(format!("f{n}.go")), "x\n").expect("file");
    }
    let first = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["go"],"maxEntries":4,"pageSize":2}),
    )
    .expect("first");
    assert!(first["next"].get("continueWalk").is_none(), "{first}");
    let second = run(
        &root.0,
        first["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("second");
    let expand = &second["next"]["continueWalk"]["query"]["queries"][0];
    assert_eq!(expand["maxEntries"], 8, "{second}");
    assert_eq!(expand["scanOffset"], 4, "{second}");
    assert_eq!(expand["page"], 1, "{second}");
    let resumed = run(&root.0, expand.clone()).expect("resumed");
    let names = listed_paths(&listed(&resumed));
    assert_eq!(names.len(), 2, "{resumed}");
    assert!(names[0].ends_with("f4.go"), "{resumed}");
    assert_eq!(resumed["atLeast"], 9, "{resumed}");
}

/// An `include`-filtered `files` walk by `next.nextPage` lists every
/// matching file exactly once; no page repeats a row of another.
#[test]
fn an_include_files_walk_lists_every_file_exactly_once() {
    let root = Fixture::new();
    let mut expected = std::collections::BTreeSet::new();
    for dir in ["a", "a/b", "c", "c/d/e"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for name in ["m.py", "n.py", "skip.txt"] {
            std::fs::write(root.0.join(format!("{dir}/{name}")), "x").expect("file");
            if name.ends_with(".py") {
                expected.insert(format!("{dir}/{name}"));
            }
        }
    }
    let base = format!(
        "{}/",
        root.0.file_name().expect("fixture name").to_string_lossy()
    );
    // `walk_pages` fails on a row listed twice.
    let pages = walk_pages(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["**/*.py"],"pageSize":3}),
    );
    assert!(pages.len() > 1, "{pages:?}");
    let seen = pages
        .iter()
        .flat_map(listed)
        .map(|row| {
            let path = row["path"].as_str().expect("path");
            path.split_once(base.as_str())
                .map_or(path, |(_, rest)| rest)
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seen.iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    assert_eq!(seen.len(), expected.len());
}
