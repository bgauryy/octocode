//! Listing order, budget pages and directory groups across pages.

use super::*;

/// With no sort, files are listed in path (git ls-files) order.
#[test]
fn files_default_to_path_order() {
    let root = Fixture::new();
    for name in ["b.rs", "a.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"entryType":"f"}),
    )
    .expect("files");
    let base = root.0.file_name().unwrap().to_string_lossy().into_owned();
    let paths = listed(&out)
        .iter()
        .map(|row| row["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            format!("{base}/a.rs"),
            format!("{base}/b.rs"),
            format!("{base}/c.rs")
        ],
        "{out}"
    );
}

#[test]
fn a_tree_page_past_the_end_is_flagged_out_of_range() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.py"), "x = 1\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","path":root.0,"page":999}),
    )
    .expect("tree");
    assert_eq!(out["pagination"]["outOfRange"], true, "{out}");
    assert_eq!(out["pagination"]["currentPage"], 999, "{out}");
}

#[test]
fn a_default_listing_is_one_page_within_the_budget_and_budget_pages_beyond_it() {
    let root = Fixture::new();
    for n in 0..150 {
        std::fs::write(root.0.join(format!("module_{n:03}.py")), "x = 1\n").expect("source");
    }
    let query = json!({"operation":"files","mainGoal":"test","reasoning":"test","path":root.0,"include":["*.py"]});
    let out = run(&root.0, query.clone()).expect("files");
    assert_eq!(listed(&out).len(), 150, "{out}");
    assert_eq!(out["pagination"]["hasMore"], false, "{out}");
    assert!(out["pagination"].get("pageSize").is_none(), "{out}");
    assert!(out["next"].get("nextPage").is_none(), "{out}");
    // Rows carry the byte count; the formatted size is a debug field.
    assert_eq!(listed(&out)[0]["size"], 6, "{out}");

    // A caller page size still pages by count.
    let mut sized = query.clone();
    sized["pageSize"] = json!(100);
    let out = run(&root.0, sized).expect("sized files");
    assert_eq!(listed(&out).len(), 100, "{out}");
    assert_eq!(out["pagination"]["pageSize"], 100, "{out}");
    assert!(out["next"]["nextPage"].is_object(), "{out}");

    // Past the budget, following nextPage reaches every row exactly once.
    for n in 150..4000 {
        std::fs::write(root.0.join(format!("module_{n:04}.py")), "x = 1\n").expect("source");
    }
    let pages = walk_pages(&root.0, query);
    for page in &pages {
        if let Some(call) = page["next"].get("nextPage") {
            assert!(
                call["query"]["queries"][0].get("pageSize").is_none(),
                "{call}"
            );
        }
    }
    let seen = pages.iter().map(|page| listed(page).len()).sum::<usize>();
    assert_eq!(seen, 4000);
    assert!((3..=6).contains(&pages.len()), "{} pages", pages.len());
}

/// `files` rows group consecutive entries by directory: each entry is its
/// name (`/` after a directory, `.` for the walked root itself) and its
/// ` (<size>[, …])` fields, so `dir` + `/` + name is the row path and the
/// listing keeps its order, every entry once.
#[test]
fn files_rows_group_by_directory_in_listing_order() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("sub/deep")).expect("dirs");
    std::fs::write(root.0.join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.0.join("sub/b.rs"), "b\n").expect("b");
    std::fs::write(root.0.join("sub/deep/c (1).rs"), "c\nc\n").expect("c");
    std::fs::write(root.0.join("z.rs"), "z\n").expect("z");
    let name = root.0.file_name().unwrap().to_string_lossy().into_owned();
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"sort":"path"}),
    )
    .expect("files");
    assert_eq!(
        out["files"],
        json!([{"dir":".","files":["a.rs (10)",
            "z.rs (2)"]},{"dir": "sub", "files": ["b.rs (2)"]},
            {"dir": "sub/deep", "files": ["c (1).rs (4)"]}]),
        "{out}"
    );
    // The directories' own groups name them; they are not listed twice.
    assert_eq!(out["pagination"]["totalItems"], 4, "{out}");
    let paths = listed(&out)
        .iter()
        .map(|row| row["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            format!("{name}/a.rs"),
            format!("{name}/z.rs"),
            format!("{name}/sub/b.rs"),
            format!("{name}/sub/deep/c (1).rs"),
        ]
    );

    let full = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0.join("sub/deep"),"extras":["details"]}),
    )
    .expect("full");
    let entry = full["files"][0]["files"][0].as_str().expect("entry");
    assert!(
        entry.starts_with("c (1).rs (4, totalLines=2, modified="),
        "{full}"
    );
    let row = &listed(&full)[0];
    assert_eq!(row["size"], 4);
    assert_eq!(row["totalLines"], 2);
    assert!(row["modifiedMs"].is_i64(), "{row}");

    // A file root lists its own name, bare.
    let file = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0.join("a.rs")}),
    )
    .expect("file root");
    assert_eq!(file["files"], json!([{"dir":".","files":["a.rs (10)"]}]));
}

/// A ranked page (any `sort` but `path`) keeps its rank order exactly: a
/// run of adjacent entries of one directory shares a group, and a directory
/// the ranking returns to opens a new one. Each entry keeps the key it is
/// sorted by, so the keys give the rank. A modified time reads as an ISO
/// 8601 UTC instant to the millisecond.
#[test]
fn a_ranked_page_groups_only_adjacent_entries_of_a_directory() {
    let root = Fixture::new();
    for dir in ["a", "b"] {
        std::fs::create_dir(root.0.join(dir)).expect("dir");
    }
    let base = 1_792_744_218_732_u64;
    for (file, bytes, at) in [
        ("a/big.rs", 50, 3),
        ("b/mid.rs", 30, 2),
        ("a/small.rs", 10, 1),
    ] {
        let path = root.0.join(file);
        std::fs::write(&path, "x".repeat(bytes)).expect("file");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|handle| {
                handle.set_modified(
                    std::time::UNIX_EPOCH + std::time::Duration::from_millis(base + at * 1_000),
                )
            })
            .expect("mtime");
    }
    let files = |sort: &str| {
        run(
            &root.0,
            json!({"operation":"files","path":root.0,"entryType":"f","sort":sort}),
        )
        .expect("files")["files"]
            .clone()
    };
    assert_eq!(
        files("size"),
        json!([{"dir":"a","files":["big.rs (50)"]},{"dir":"b","files":["mid.rs (30)"]},{"dir":"a","files":["small.rs (10)"]}])
    );
    let iso = |at: u64| crate::civil_date::iso8601_millis((base + at * 1_000) as i64);
    assert_eq!(
        files("modified"),
        json!([
            {"dir":"a","files":[format!("big.rs (50, modified={})", iso(3))]},
            {"dir":"b","files":[format!("mid.rs (30, modified={})", iso(2))]},
            {"dir":"a","files":[format!("small.rs (10, modified={})", iso(1))]}
        ])
    );
}

/// A page that continues a directory group repeats its `dir`, so each page
/// reads on its own, and the pages hold every entry exactly once.
#[test]
fn a_group_split_across_pages_repeats_its_directory_on_each_page() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    for n in 0..5 {
        std::fs::write(root.0.join(format!("src/f{n}.rs")), "x\n").expect("file");
    }
    let query = json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"entryType":"f","pageSize":2});
    let pages = walk_pages(&root.0, query)
        .iter()
        .map(|page| page["files"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        pages,
        [
            json!([{"dir": "src", "files": ["f0.rs (2)", "f1.rs (2)"]}]),
            json!([{"dir": "src", "files": ["f2.rs (2)", "f3.rs (2)"]}]),
            json!([{"dir": "src", "files": ["f4.rs (2)"]}]),
        ]
    );
}

/// A budget page counts a group header when a row opens a group: on a new
/// directory, and again on the first row of every page.
#[test]
fn budget_pages_count_a_header_for_each_group_a_page_opens() {
    use super::super::cut::{RowCost, page_ranges};
    let row = |continues| RowCost {
        entry: 10,
        header: 25,
        continues,
    };
    // One group of six rows: 35 + 10 + 10 = 55 fits 60; the fourth row
    // would make 65, so it opens the next page, header included.
    let costs = [false, true, true, true, true, true].map(row);
    assert_eq!(page_ranges(&costs, None, 60), [0..3, 3..6]);
    // A new group mid-page costs its header too: 35 + 35 > 60.
    let costs = [false, false, true].map(row);
    assert_eq!(page_ranges(&costs, None, 60), [0..1, 1..3]);
    // A caller page size pages by count.
    assert_eq!(page_ranges(&costs, Some(2), 0), [0..2, 2..3]);
}

/// A directory listed by its own `{dir, entries}` group on the same page is
/// not also listed as a `"name/"` entry of its parent; an empty or
/// depth-cut directory (no group) keeps its `"name/"` entry.
#[test]
fn a_tree_directory_with_a_group_is_not_listed_twice() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/inner")).expect("dirs");
    std::fs::create_dir(root.0.join("empty")).expect("empty");
    std::fs::write(root.0.join("a.rs"), "fn a() {}\n").expect("a");
    std::fs::write(root.0.join("src/lib.rs"), "fn b() {}\n").expect("lib");
    std::fs::write(root.0.join("src/inner/deep.rs"), "fn c() {}\n").expect("deep");
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"maxDepth":2}),
    )
    .expect("tree");
    let entries = out["files"].as_array().expect("files");
    let bare = entries
        .iter()
        .find(|entry| entry["dir"] == ".")
        .and_then(|group| group["files"].as_array())
        .expect("root group")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(bare.contains(&"empty/"), "{out}");
    assert!(!bare.contains(&"src/"), "{out}");
    let src = entries
        .iter()
        .find(|entry| entry["dir"] == "src")
        .expect("src group");
    // `src/inner` has no group at maxDepth 2, so it stays listed.
    assert!(
        src["files"].as_array().unwrap().contains(&json!("inner/")),
        "{out}"
    );
}

/// `files` groups like `tree`: entries of `path` itself are bare strings,
/// each subdirectory is one `{dir, files}` group with `dir` relative to
/// `path` (walk order, merged), and a directory whose group is on the page
/// is not also listed as `"name/"`.
#[test]
fn files_group_like_tree_relative_to_path() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("a/x.rs"), "x").expect("x");
    std::fs::write(root.0.join("a/b/y.rs"), "y").expect("y");
    std::fs::write(root.0.join("a/z.rs"), "z").expect("z");
    std::fs::write(root.0.join("top.rs"), "t").expect("top");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["*.rs","a","b"]}),
    )
    .expect("files");
    assert_eq!(
        out["files"],
        json!([{"dir":".","files":["top.rs (1)"]},{"dir":"a","files":["x.rs (1)","z.rs (1)"]},
            {"dir":"a/b","files":["y.rs (1)"]}]),
        "{out}"
    );
}

/// A directory listed under its own group is never also listed bare under
/// its parent, even when the two land on different pages: every entry is
/// shown exactly once across the pages.
#[test]
fn a_grouped_directory_is_listed_once_across_pages() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("sub")).expect("sub");
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("root file");
    }
    std::fs::write(root.0.join("sub/d.rs"), "x\n").expect("sub file");
    for operation in ["tree", "files"] {
        let query = json!({"operation":operation,"path":root.0,"maxDepth":2,"pageSize":2});
        // The walk fails on an entry shown twice; `sub` is named once, by
        // its group, never also by a bare `sub/` stub.
        let pages = walk_pages(&root.0, query);
        let entries = pages.iter().flat_map(listed).collect::<Vec<_>>();
        assert!(
            entries.iter().all(|entry| !entry["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/sub"))
                || entry["type"] != "directory"),
            "{operation}: {entries:?}"
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("sub/d.rs")))
                .count(),
            1,
            "{operation}: {entries:?}"
        );
    }
}
