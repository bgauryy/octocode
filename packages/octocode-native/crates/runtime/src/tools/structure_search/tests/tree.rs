//! `tree` outlines and their pages: path order, bounded depth, groups split
//! across pages, stale snapshots and ranked pages.

use super::*;

#[test]
fn tree_outlines_in_path_order_with_bounded_depth() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/deep")).expect("dirs");
    std::fs::write(root.0.join("src/deep/leaf.rs"), "x").expect("leaf");
    std::fs::write(root.0.join("src/lib.rs"), "xy").expect("lib");
    std::fs::write(root.0.join("README.md"), "").expect("readme");
    let top = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":1}),
    )
    .expect("top");
    assert_eq!(
        top["files"],
        json!([{"dir":".","files":["README.md (0)", "src/"]}]),
        "{top}"
    );
    // The tool states its block; the response stage drops a complete single
    // page's (`response::rows::walks_on`).
    assert_eq!(top["pagination"]["hasMore"], false, "{top}");
    assert_eq!(top["pagination"]["totalPages"], 1, "{top}");
    let all = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":6}),
    )
    .expect("all");
    // Entries of `path` itself are bare; a subdirectory's entries share one
    // `{dir, entries}` group (dir relative to `path`), directories in walk
    // order, so no path prefix repeats.
    assert_eq!(
        all["files"],
        json!([{"dir":".","files":["README.md (0)"]},{"dir":"src","files":["lib.rs (2)"]},
            {"dir":"src/deep","files":["leaf.rs (1)"]}]),
        "{all}"
    );
    assert_eq!(all["summary"], json!({"files":3,"dirs":2,"bytes":3}));
    let dirs = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"maxDepth":6,"entryType":"d"}),
    )
    .expect("dirs");
    assert_eq!(
        dirs["files"],
        json!([{"dir":"src","files":["deep/"]}]),
        "{dirs}"
    );
}

/// Groups follow the walk's directory order (`a/x` before `a-b`), keep each
/// directory's entries in walk order, and name a parent the filters left out.
#[test]
fn tree_groups_follow_walk_order_and_name_filtered_parents() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/x")).expect("dirs");
    std::fs::create_dir_all(root.0.join("a-b")).expect("dirs");
    std::fs::write(root.0.join("a/x/k.rs"), "x").expect("file");
    std::fs::write(root.0.join("a/m.rs"), "x").expect("file");
    std::fs::write(root.0.join("a-b/n.rs"), "x").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":4}),
    )
    .expect("tree");
    assert_eq!(
        out["files"],
        json!([
            {"dir":"a","files":["m.rs (1)"]},
            {"dir":"a/x","files":["k.rs (1)"]},
            {"dir":"a-b","files":["n.rs (1)"]}
        ]),
        "{out}"
    );
    let mut public = json!({"results":[{"index":0,"data":out}]});
    crate::response::continuations::finalize(
        &mut public,
        crate::id::ToolId::StructureSearch,
        &crate::response::continuations::Sources::Rows(&[]),
        &crate::response::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    crate::contracts::validate_output("structureSearch", &public).expect("contract tree groups");
    let rs = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":4,"extensions":["rs"]}),
    )
    .expect("filtered tree");
    assert_eq!(
        rs["files"],
        json!([
            {"dir":"a","files":["m.rs (1)"]},
            {"dir":"a/x","files":["k.rs (1)"]},
            {"dir":"a-b","files":["n.rs (1)"]}
        ]),
        "{rs}"
    );
}

/// Every page of a sorted, paged `files` listing (directories fold into
/// group headers, so the walk discovers more entries than it lists) carries
/// only declared pagination fields.
#[test]
fn sorted_paged_files_pages_satisfy_the_output_contract() {
    let root = Fixture::new();
    for dir in ["a/x", "b", "c"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dirs");
    }
    for file in ["a/x/k.rs", "a/m.rs", "b/n.rs", "c/o.rs", "p.rs"] {
        std::fs::write(root.0.join(file), "x\n").expect("file");
    }
    for sort in ["name", "size", "modified", "lines"] {
        let pages = walk_pages(
            &root.0,
            json!({"operation":"files","path":root.0,"sort":sort,"pageSize":2}),
        );
        assert!(pages.len() > 1, "sort {sort} pages");
    }
}

/// A page that continues a group repeats its `dir`, so each page reads on
/// its own and every entry arrives exactly once.
#[test]
fn a_tree_group_split_across_pages_repeats_its_dir() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("d")).expect("dir");
    for name in ["x", "y", "z"] {
        std::fs::write(root.0.join(format!("d/{name}")), "x").expect("file");
    }
    let page1 = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"pageSize":2,"maxDepth":2}),
    )
    .expect("page1");
    assert_eq!(
        page1["files"],
        json!([{"dir":"d","files":["x (1)","y (1)"]}]),
        "{page1}"
    );
    let page2 = run(
        &root.0,
        page1["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page2");
    assert_eq!(
        page2["files"],
        json!([{"dir":"d","files":["z (1)"]}]),
        "{page2}"
    );
    assert!(page2.get("next").is_none(), "{page2}");
}

#[test]
fn tree_pages_through_next_and_rejects_stale_snapshots() {
    let root = Fixture::new();
    for i in 0..5 {
        std::fs::write(root.0.join(format!("f{i}.txt")), "x").expect("file");
    }
    let page1 = run(
        &root.0,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","path":root.0,"pageSize":2}),
    )
    .expect("page1");
    assert_eq!(page1["pagination"]["totalPages"], 3, "{page1}");
    let next = &page1["next"]["nextPage"];
    assert_eq!(next["tool"], "structureSearch");
    assert_eq!(next["query"]["queries"][0]["operation"], "tree");
    let page2 = run(&root.0, next["query"]["queries"][0].clone()).expect("page2");
    assert_eq!(
        page2["files"],
        json!([{"dir":".","files":["f2.txt (1)", "f3.txt (1)"]}]),
        "{page2}"
    );
    // The walk summary (counts, hidden/ignored disclosure) rides page 1 only.
    assert!(page1.get("summary").is_some(), "{page1}");
    assert!(page2.get("summary").is_none(), "{page2}");
    std::fs::write(root.0.join("f9.txt"), "x").expect("mutate");
    let stale = run(&root.0, next["query"]["queries"][0].clone()).expect("stale");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    let restart = &stale["next"]["restart"];
    assert_eq!(restart["tool"], "structureSearch");
    assert_eq!(restart["query"]["queries"][0]["operation"], "tree");
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(
            restart["query"]["queries"][0].get(cursor).is_none(),
            "{cursor}: {restart}"
        );
    }
    let fresh = run(&root.0, restart["query"]["queries"][0].clone())
        .expect("execute returned restart unchanged");
    assert_eq!(fresh["pagination"]["currentPage"], 1);
    assert_eq!(fresh["pagination"]["totalItems"], 6);
    assert_ne!(fresh["snapshot"], page1["snapshot"]);
}

/// Ranking order is the contract: a ranked page (`sort:size`), flattened,
/// lists its entries in rank order even when directories interleave in it.
/// Only adjacent entries of one directory share a group.
#[test]
fn a_ranked_page_flattened_keeps_the_rank_order() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a")).expect("a");
    std::fs::create_dir_all(root.0.join("b")).expect("b");
    // Sizes interleave the directories: a/5000, b/4000, a/3000, b/2000, a/1000.
    for (path, size) in [
        ("a/big.bin", 5000),
        ("b/large.bin", 4000),
        ("a/mid.bin", 3000),
        ("b/small.bin", 2000),
        ("a/tiny.bin", 1000),
    ] {
        std::fs::write(root.0.join(path), vec![b'x'; size]).expect("file");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"entryType":"f","sort":"size"}),
    )
    .expect("files");
    let sizes = listed(&out)
        .iter()
        .map(|row| row["size"].as_i64().expect("size"))
        .collect::<Vec<_>>();
    assert_eq!(sizes, [5000, 4000, 3000, 2000, 1000], "{out}");
    assert_eq!(out["files"].as_array().expect("groups").len(), 5, "{out}");
}
