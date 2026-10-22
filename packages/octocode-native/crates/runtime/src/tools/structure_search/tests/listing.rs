//! Listing order, budget pages, directory groups, read leads and the walk
//! ceiling.

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
        entry.starts_with("c (1).rs (4, totalLines=2, modifiedMs="),
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
    use super::super::{RowCost, page_ranges};
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

/// A tree without `hidden` skips dot entries, but says how many it skipped
/// and, on a filtered listing, offers the one retry that lifts every cause it
/// skipped by (`includeSkipped`); `.git`, which the path policy withholds,
/// stays out of the count.
#[test]
fn tree_discloses_skipped_dot_entries_and_offers_include_hidden() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".git")).expect("git");
    std::fs::create_dir_all(root.0.join(".github")).expect("github");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join(".editorconfig"), "x\n").expect("dotfile");
    std::fs::write(root.0.join("src/.keep"), "").expect("nested dotfile");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":2}),
    )
    .expect("tree");
    assert_eq!(
        out["files"],
        json!([{"dir":"src","files":["lib.rs (2)"]}]),
        "{out}"
    );
    // Every counted dot entry is reachable: the lead lifts includeHidden.
    assert_eq!(out["summary"]["hidden"], 3, "{out}");
    let lead = &out["next"]["includeSkipped"]["query"]["queries"][0];
    assert_eq!(lead["includeHidden"], true, "{out}");
    assert!(lead.get("defaultExcludes").is_none(), "{out}");
    let filtered = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":2,"extensions":["rs"]}),
    )
    .expect("filtered tree");
    // The lead is the fix: the summary keeps the count, not the flag.
    assert!(
        filtered["summary"]["hidden"].as_u64() > Some(0),
        "{filtered}"
    );
    let retry = &filtered["next"]["includeSkipped"];
    assert_eq!(retry["tool"], "structureSearch", "{out}");
    assert_eq!(retry["query"]["queries"][0]["includeHidden"], true, "{out}");
    assert!(
        retry["query"]["queries"][0]
            .get("defaultExcludes")
            .is_none()
            && retry["query"]["queries"][0].get("includeIgnored").is_none(),
        "only the cause it hit: {out}"
    );
    assert!(
        retry["query"]["queries"][0].get("snapshot").is_none(),
        "{out}"
    );
    let all = run(&root.0, retry["query"]["queries"][0].clone()).expect("retry");
    assert_eq!(
        all["files"],
        json!([{"dir":"src","files":["lib.rs (2)"]}]),
        "{all}"
    );
    assert!(all["next"].get("includeSkipped").is_none(), "{all}");
    assert!(all["summary"].get("hidden").is_none(), "{all}");
}

/// A `files` result's first page leads to its first outline-able file: a
/// localFetch `minify:"symbols"` read of its declarations. A listing of
/// only non-source files offers none, and later pages repeat none.
#[test]
fn a_files_result_leads_to_the_top_files_outline() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/blob.bin"), [0_u8, 1, 2]).expect("bin");
    std::fs::write(root.0.join("src/lib.rs"), "pub fn a() {}\n").expect("lib");
    std::fs::write(root.0.join("src/main.rs"), "fn main() {}\n").expect("main");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.rs","*.bin"]}),
    )
    .expect("files");
    let lead = &out["next"]["read"];
    assert_eq!(lead["tool"], "localFetch", "{out}");
    let path = lead["query"]["queries"][0]["path"].as_str().expect("path");
    assert!(path.ends_with("src/lib.rs"), "{out}");
    assert!(std::path::Path::new(path).is_absolute(), "{out}");
    assert_eq!(lead["query"]["queries"][0]["minify"], "symbols", "{out}");
    assert_eq!(
        lead["query"]["queries"][0].as_object().map(|q| q.len()),
        Some(2),
        "{out}"
    );
    crate::contracts::validate_query("localFetch", lead["query"]["queries"][0].clone())
        .expect("read is a valid localFetch query");

    for name in ["c.rs", "d.rs", "e.rs"] {
        std::fs::write(root.0.join("src").join(name), "fn x() {}\n").expect("more");
    }
    let wide = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.rs","*.bin"],"pageSize":2}),
    )
    .expect("files");
    assert!(wide.pointer("/next/read").is_some(), "{wide}");
    let later = run(
        &root.0,
        wide["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page 2");
    assert!(later.pointer("/next/read").is_none(), "{later}");
    let binary = run(
        &root.0,
        json!({"operation":"files","path":root.0.join("src"),"include":["*.bin"]}),
    )
    .expect("files");
    assert!(binary.pointer("/next/read").is_none(), "{binary}");
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

/// Default-pruned directories are disclosed: the tree summary names them,
/// and an empty listing leads to the same listing with them walked.
#[test]
fn default_pruned_directories_are_named_and_an_empty_listing_walks_them() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("build/gen")).expect("build");
    std::fs::create_dir_all(root.0.join("node_modules/pkg")).expect("deps");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("build/gen/electron.ts"), "x\n").expect("generated");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");

    let tree = run(&root.0, json!({"operation":"tree","path":root.0})).expect("tree");
    assert_eq!(
        tree["summary"]["pruned"],
        json!(["build", "node_modules"]),
        "{tree}"
    );
    assert!(tree["next"].get("includeSkipped").is_none(), "{tree}");

    let files = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"]}),
    )
    .expect("files");
    assert_eq!(files["status"], "empty", "{files}");
    assert!(files["summary"]["pruned"].is_array(), "{files}");
    assert!(
        files["hints"][0]
            .as_str()
            .is_some_and(|h| h.contains("build/")),
        "{files}"
    );
    let retry = &files["next"]["includeSkipped"]["query"]["queries"][0];
    assert_eq!(retry["defaultExcludes"], false, "{files}");
    assert!(retry.get("snapshot").is_none(), "{files}");
    let found = run(&root.0, retry.clone()).expect("retry");
    assert_eq!(listed(&found).len(), 1, "{found}");
}

/// The policy-withheld warning only counts entries the query's filters
/// could have listed.
#[test]
fn withheld_entries_count_only_when_the_filters_could_list_them() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    std::fs::write(root.0.join("main.go"), "package main\n").expect("go");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["go"],"includeIgnored":true}),
    )
    .expect("listing");
    assert_eq!(listed(&out).len(), 1, "{out}");
    crate::tools::test_support::checks::assert_absent(&out, "withheld");
}

/// The withheld count applies the walk's own glob dialect: `*` never crosses
/// `/`, so a withheld entry one directory below what the glob reaches is not
/// one the listing could have shown.
#[test]
fn withheld_entries_follow_the_walks_glob_dialect() {
    let root = Fixture::new();
    root.write("cfg/.env.production", "DUMMY_VALUE=placeholder\n");
    root.write("cfg/deep/.env.production", "DUMMY_VALUE=placeholder\n");
    root.write("cfg/app.production", "x\n");
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["cfg/*.production"],"includeIgnored":true,"includeHidden":true}),
    )
    .expect("listing");
    let paths = listed(&out)
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert!(
        paths.len() == 1 && paths[0].ends_with("cfg/app.production"),
        "{out}"
    );
    assert!(
        out["warnings"].as_array().is_some_and(|warnings| warnings
            .iter()
            .filter_map(Value::as_str)
            .any(|warning| warning.starts_with("1 entry withheld by path policy"))),
        "only cfg/.env.production is counted: {out}"
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

/// An omitted `maxDepth` lists `path`'s children; an `include` filter then
/// searches every level, and a bare word matches names containing it.
#[test]
fn tree_depth_defaults_to_children_and_include_searches_every_level() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b/c")).expect("dirs");
    std::fs::write(root.0.join("a/b/c/deep_helper.rs"), "x\n").expect("deep");
    std::fs::write(root.0.join("top.rs"), "x\n").expect("top");
    let outline = run(&root.0, json!({"operation":"tree","path":root.0})).expect("tree");
    assert_eq!(
        outline["files"],
        json!([{"dir":".","files":["a/", "top.rs (2)"]}]),
        "{outline}"
    );
    let found = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"include":["helper"]}),
    )
    .expect("include");
    assert!(
        found["files"]
            .as_array()
            .is_some_and(|dirs| dirs.iter().any(|dir| {
                dir["files"].as_array().is_some_and(|names| {
                    names.iter().any(|name| {
                        name.as_str()
                            .is_some_and(|n| n.starts_with("deep_helper.rs"))
                    })
                })
            })),
        "{found}"
    );
    crate::tools::test_support::checks::assert_absent(&found, "top.rs");
}

/// Continuation pages reuse the first page's walk while nothing listed
/// changed, and still restart when a listed file changes.
#[test]
fn continuation_pages_reuse_the_walk_and_restart_on_change() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let first = run(
        &root.0,
        json!({"operation":"files","path":root.0,"entryType":"f","pageSize":2}),
    )
    .expect("page 1");
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    let second = run(&root.0, next.clone()).expect("page 2");
    assert_eq!(second["pagination"]["currentPage"], 2, "{second}");
    let third = run(
        &root.0,
        second["next"]["nextPage"]["query"]["queries"][0].clone(),
    )
    .expect("page 3");
    assert_eq!(third["pagination"]["currentPage"], 3, "{third}");
    std::fs::write(root.0.join("f5.rs"), "changed size\n").expect("change");
    let stale = run(&root.0, next).expect("stale page");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
}

/// `maxDepth` counts the same in both operations: 1 lists `path`'s
/// children, 2 their children too.
#[test]
fn files_and_tree_count_max_depth_alike() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("top.rs"), "x\n").expect("top");
    std::fs::write(root.0.join("a/mid.rs"), "x\n").expect("mid");
    std::fs::write(root.0.join("a/b/low.rs"), "x\n").expect("low");
    for (depth, expected) in [(1, vec!["top.rs"]), (2, vec!["mid.rs", "top.rs"])] {
        for operation in ["tree", "files"] {
            let out = run(
                &root.0,
                json!({"operation":operation,"path":root.0,"maxDepth":depth,"extensions":["rs"]}),
            )
            .expect("listing");
            let text = out.to_string();
            for name in ["top.rs", "mid.rs", "low.rs"] {
                assert_eq!(
                    text.contains(name),
                    expected.contains(&name),
                    "{operation} maxDepth {depth} {name}: {out}"
                );
            }
        }
    }
}

/// A listing cut at the largest `maxEntries` keeps every listed row
/// reachable: each page with `hasMore` carries `next.nextPage`, and the
/// walk ceiling is a disclosed terminal limit with a narrowing lead, not a
/// silent end of paging.
#[test]
fn files_at_the_walk_ceiling_page_every_listed_row_and_lead_to_narrow() {
    let root = Fixture::new();
    for dir in 0..11 {
        let dir_path = root.0.join(format!("d{dir:02}"));
        std::fs::create_dir_all(&dir_path).expect("fixture dir");
        for file in 0..1_000 {
            std::fs::write(dir_path.join(format!("f{file:04}.ts")), "").expect("fixture file");
        }
    }
    // `walk_pages` fails on a row listed twice and on paging past its cap.
    let pages = walk_pages(
        &root.0,
        json!({"operation":"files","path":root.0,"extensions":["ts"],"pageSize":1000}),
    );
    let mut seen = std::collections::BTreeSet::new();
    for out in &pages {
        seen.extend(listed(out).iter().map(|row| row["path"].to_string()));
        if out["pagination"]["hasMore"] == true {
            assert!(
                out["next"]["nextPage"].is_object(),
                "hasMore without next: {}",
                out["pagination"]
            );
        }
        assert_eq!(out["terminalLimit"], true, "{}", out["pagination"]);
        // The walk ceiling is the stated cause, on every page.
        assert!(
            out["partialReasons"]
                .as_array()
                .is_some_and(|reasons| reasons
                    .iter()
                    .any(|reason| reason == "maxEntries" || reason == "walkLimit")),
            "{out}"
        );
    }
    let last = pages.last().expect("a page");
    let narrow = &last["next"]["narrowScope"]["query"]["queries"][0];
    assert_eq!(narrow["operation"], "tree", "{last}");
    assert_eq!(narrow["maxDepth"], 1, "{last}");
    assert_eq!(seen.len(), 10_000);
    assert!(pages.len() > 1);
}

/// A window's walk continues by one `continueWalk` at most, on its last page:
/// a limit cut widens (its query carries `maxEntries`), a time cut resumes
/// (its query carries `walkOffset`), whatever combination of cuts the window
/// met.
#[test]
fn a_window_offers_at_most_one_walk_continuation() {
    use super::super::TimeWindow;
    let ceiling = super::super::max_walk() as usize;
    let mut seen = std::collections::BTreeSet::new();
    for limit_cut in [false, true] {
        for scan_cut in [false, true] {
            for time_cut in [None, Some("src/a.rs".to_owned())] {
                for time_window in [
                    TimeWindow::Whole,
                    TimeWindow::EndedAtRows,
                    TimeWindow::SortedPastScanOffset,
                ] {
                    for requested in [10, ceiling] {
                        for total_pages in [1, 2] {
                            let cut = super::super::Cut {
                                page: 1,
                                total_pages,
                                total: requested,
                                requested,
                                available: requested + usize::from(limit_cut),
                                limit_cut,
                                scan_cut,
                                time_cut: time_cut.clone(),
                                time_window,
                                walk_position: requested + 5,
                                walk_offset: 0,
                                early_exit: false,
                                total_discovered: requested,
                                scan_offset: 0,
                                covered: requested,
                                ranked: false,
                                snapshot: "a".repeat(64),
                            };
                            let mut out = json!({});
                            cut.finish(
                                &mut out,
                                &json!({"operation":"files","path":"."}),
                                &mut Vec::new(),
                            );
                            let lead = out["next"].get("continueWalk").map(|lead| {
                                if lead["query"]["queries"][0].get("walkOffset").is_some() {
                                    "resume"
                                } else {
                                    "widen"
                                }
                            });
                            let expected = cut.walk_lead().map(|(_, changes)| {
                                if changes.get("walkOffset").is_some() {
                                    "resume"
                                } else {
                                    "widen"
                                }
                            });
                            assert_eq!(lead, expected, "{out}");
                            if total_pages > 1 {
                                assert!(lead.is_none(), "the window's own pages first: {out}");
                            }
                            seen.extend(lead);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(seen.len(), 2, "both continuations are reached: {seen:?}");
}

/// A sorted listing whose walk stopped at its ceiling knows only a lower
/// bound of its matches: it says `atLeast` (never an exact
/// `totalAvailable` of the stop count), and that its sort ranked only the
/// entries walked first.
#[test]
fn a_ranked_listing_cut_at_the_walk_ceiling_states_a_lower_bound() {
    let ceiling = super::super::max_walk() as usize;
    for ranked in [true, false] {
        let cut = super::super::Cut {
            page: 1,
            total_pages: 1,
            total: ceiling,
            requested: ceiling,
            available: ceiling + 1,
            limit_cut: true,
            scan_cut: true,
            time_cut: None,
            time_window: super::super::TimeWindow::Whole,
            walk_position: ceiling + 1,
            walk_offset: 0,
            early_exit: false,
            total_discovered: ceiling + 1,
            scan_offset: 0,
            covered: ceiling,
            ranked,
            snapshot: "snapshot".to_owned(),
        };
        let mut out = json!({});
        let mut warnings = Vec::new();
        cut.finish(
            &mut out,
            &json!({"operation":"files","path":"."}),
            &mut warnings,
        );
        assert_eq!(out["atLeast"], ceiling + 1, "{out}");
        assert!(out.get("totalAvailable").is_none(), "{out}");
        assert_eq!(out["terminalLimit"], true, "{out}");
        let warning = warnings.join(" ");
        assert!(
            warning.contains(&format!("with at least {} entries matched", ceiling + 1)),
            "{warning}"
        );
        assert_eq!(
            warning.contains("this sort ranks only the entries walked first"),
            ranked,
            "{warning}"
        );
    }
}

/// `tree` and `files` lead to the same read over the same rows: one
/// ranking ([`crate::tools::root_read::read_rank`]), filtered or not; an
/// outline-able file no tier names still leads when nothing better is
/// listed.
#[test]
fn tree_and_files_lead_to_the_same_read() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/util.rs"), "fn u() {}\n").expect("util");
    std::fs::write(root.0.join("src/main.rs"), "fn main() {}\n").expect("main");
    std::fs::create_dir(root.0.join("docs")).expect("docs");
    std::fs::write(root.0.join("docs/guide.md"), "# guide\n").expect("guide");
    let read = |query: Value| {
        let out = run(&root.0, query.clone()).expect("listing");
        out["next"]["read"]["query"]["queries"][0]["path"]
            .as_str()
            .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
    };
    for extra in [json!({}), json!({"extensions":["rs"]})] {
        let mut leads = Vec::new();
        for operation in ["tree", "files"] {
            let mut query = json!({"operation":operation,"path":root.0.join("src")});
            query
                .as_object_mut()
                .expect("query")
                .extend(extra.as_object().expect("extra").clone());
            leads.push(read(query));
        }
        assert_eq!(leads, vec![Some("main.rs".to_owned()); 2], "{extra}");
    }
    for operation in ["tree", "files"] {
        assert_eq!(
            read(json!({"operation":operation,"path":root.0.join("docs")})),
            Some("guide.md".to_owned()),
            "{operation}"
        );
    }
}

/// The files read lead skips generated and large files.
#[test]
fn files_read_lead_skips_generated_and_large() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a_types.generated.rs"),
        "pub struct A;\n".repeat(8_000),
    )
    .expect("generated");
    std::fs::write(root.0.join("b.rs"), "fn b() {}\n").expect("small");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    let read = out["next"]["read"]["query"]["queries"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("read lead: {out}"));
    assert!(read.ends_with("b.rs"), "{out}");
}

/// A root tree leads to its manifest, read whole.
#[test]
fn root_tree_leads_to_manifest() {
    let root = Fixture::new();
    std::fs::write(root.0.join("package.json"), "{\"name\":\"x\"}\n").expect("manifest");
    std::fs::write(root.0.join("README.md"), "# x\n").expect("readme");
    std::fs::create_dir(root.0.join("src")).expect("src");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("tree");
    let query = &out["next"]["read"]["query"]["queries"][0];
    assert!(
        query["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("package.json")),
        "{out}"
    );
    assert!(query.get("minify").is_none(), "{out}");
}

/// A tree ranks its read lead with the rule ghStructure uses
/// ([`crate::tools::root_read::read_rank`]): a code entry file, then the
/// manifest, README and guides, then loose source; a nested manifest or
/// README never leads; a bare-word-free root with only loose scripts and docs
/// reads the README before a script.
#[test]
fn tree_read_lead_ranks_like_ghstructure() {
    let lead = |files: &[&str]| {
        let root = Fixture::new();
        for file in files {
            let path = root.0.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(path, "# x\n").expect("file");
        }
        let out = run(
            &root.0,
            json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
        )
        .expect("tree");
        let query = out["next"]["read"]["query"]["queries"][0].clone();
        let path = query["path"].as_str().expect("read lead").to_owned();
        (path, query.get("minify").cloned())
    };
    let leads = |files: &[&str], expected: &str| {
        let (path, _) = lead(files);
        assert!(path.ends_with(&format!("/{expected}")), "{files:?}: {path}");
    };
    leads(&["package.json", "index.ts"], "index.ts");
    leads(&["mm.mjs", "README.md"], "README.md");
    leads(&["mm.mjs", "AGENTS.md"], "AGENTS.md");
    let (path, minify) = lead(&["README.md", "package.json"]);
    assert!(path.ends_with("/package.json"), "{path}");
    assert_eq!(minify, None);
    // Docs outline like ghStructure's read of the same file.
    let (path, minify) = lead(&["README.md"]);
    assert!(path.ends_with("/README.md"), "{path}");
    assert_eq!(minify, Some(json!("symbols")));
    // Only the listed directory's own manifest describes it.
    leads(&["pkg/package.json", "util.rs"], "util.rs");
}

/// Tree and files print the same exact byte size.
#[test]
fn tree_and_files_print_exact_byte_sizes() {
    let root = Fixture::new();
    std::fs::write(root.0.join("f.txt"), "x".repeat(9216)).expect("file");
    let tree = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("tree");
    let files = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    assert_eq!(
        tree["files"],
        json!([{"dir":".","files":["f.txt (9216)"]}]),
        "{tree}"
    );
    assert_eq!(
        files["files"],
        json!([{"dir":".","files":["f.txt (9216)"]}]),
        "{files}"
    );
}

/// A directory root is the listing, never one of its rows.
#[test]
fn files_never_lists_root_dot() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), "a\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0}),
    )
    .expect("files");
    assert!(!has_string(&out["files"], "./"), "{out}");
    assert_eq!(
        out["files"],
        json!([{"dir":".","files":["a.rs (2)"]}]),
        "{out}"
    );
}

/// A gitignored directory named like a default-pruned one is lifted
/// in one hop: the retry lifts both the ignore rules and the prune, and the
/// hint names the directory.
#[test]
fn include_ignored_lead_lifts_gitignore_and_prune_in_one_hop() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("target/debug")).expect("target");
    std::fs::write(root.0.join("target/debug/x.rlib"), "rlib\n").expect("rlib");
    std::fs::write(root.0.join("main.rs"), "fn main() {}\n").expect("main");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"include":["*.rlib"]}),
    )
    .expect("files");
    assert_eq!(out["status"], "empty", "{out}");
    let retry = out["next"]["includeSkipped"]["query"]["queries"][0].clone();
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    assert_eq!(retry["includeIgnored"], true, "{out}");
    assert!(
        out["hints"][0]
            .as_str()
            .is_some_and(|h| h.contains("target/")),
        "{out}"
    );
    crate::tools::test_support::checks::assert_absent(&out["warnings"], "target/");
    let found = run(&root.0, retry).expect("retry");
    assert!(
        found["files"]
            .as_array()
            .is_some_and(|dirs| dirs.iter().any(|dir| {
                dir["files"].as_array().is_some_and(|names| {
                    names
                        .iter()
                        .any(|name| name.as_str().is_some_and(|n| n.starts_with("x.rlib")))
                })
            })),
        "{found}"
    );
}

/// The tree summary states only the file and directory counts, and
/// totalItems counts the strings the pages show.
#[test]
fn tree_summary_matches_paged_row_counts() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b")).expect("dirs");
    std::fs::write(root.0.join("a/x.rs"), "x\n").expect("x");
    std::fs::write(root.0.join("a/b/y.rs"), "y\n").expect("y");
    std::fs::write(root.0.join("top.rs"), "t\n").expect("top");
    let out = run(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":3,"pageSize":1}),
    )
    .expect("tree");
    assert_eq!(
        out["summary"],
        json!({"files":3,"dirs":2,"bytes":6}),
        "{out}"
    );
    let total = out["pagination"]["totalItems"].as_u64().expect("total");
    let query = json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"maxDepth":3,"pageSize":1});
    let mut strings = 0;
    for page in walk_pages(&root.0, query) {
        for entry in page["files"].as_array().expect("files") {
            strings += entry
                .get("entries")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
        }
    }
    assert_eq!(strings as u64, total, "{out}");
}

/// An empty files listing names withheld entries once (warnings).
#[test]
fn files_empty_withheld_not_duplicated() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".aws")).expect("aws");
    std::fs::write(root.0.join(".aws/a.ts"), "x\n").expect("secret");
    std::fs::write(root.0.join("b.rs"), "x\n").expect("other");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"extensions":["ts"],"includeHidden":true}),
    )
    .expect("files");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        out.to_string().matches("withheld by path policy").count(),
        1,
        "{out}"
    );
}

/// A bare word include lists the files inside directories named by it.
#[test]
fn bare_word_include_lists_dir_files() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/inner")).expect("dirs");
    std::fs::write(root.0.join("src/inner/a.rs"), "a\n").expect("a");
    std::fs::write(root.0.join("other.rs"), "o\n").expect("other");
    let out = run(
        &root.0,
        json!({"operation":"files","mainGoal":"t","reasoning":"t","path":root.0,"include":["src"]}),
    )
    .expect("files");
    let paths = listed(&out)
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert!(
        paths.iter().any(|path| path.ends_with("src/inner/a.rs")),
        "{out}"
    );
    crate::tools::test_support::checks::assert_absent(&out["files"], "other.rs");
}

/// A paged tree states the walk's withheld entries once, on its first page,
/// like its summary; later pages carry only their own warnings.
#[test]
fn walk_warnings_are_stated_on_the_first_page_only() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".npmrc"), "token\n").expect("credential");
    for n in 0..6 {
        std::fs::write(root.0.join(format!("f{n}.txt")), "x\n").expect("file");
    }
    let pages = walk_pages(
        &root.0,
        json!({"operation":"tree","mainGoal":"t","reasoning":"t","path":root.0,"includeHidden":true,"pageSize":2}),
    );
    assert!(pages.len() > 1, "{pages:?}");
    let withheld = |page: &Value| page["warnings"].to_string().contains("withheld");
    assert!(withheld(&pages[0]), "{}", pages[0]);
    assert!(pages[1..].iter().all(|page| !withheld(page)), "{pages:?}");
}

/// A filtered listing whose walk pruned a default-excluded directory leads
/// to the same listing with the prune lifted (outside a git work tree a
/// listed name is pruned by name), and the lead reaches what it hid.
#[test]
fn a_filtered_listing_leads_past_pruned_directories() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/coverage")).expect("dir");
    std::fs::write(root.0.join("src/coverage/pass.rs"), "x\n").expect("pruned");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("visible");
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"mainGoal":"t","reasoning":"t","path":root.0,"extensions":["rs"]}),
        )
        .expect("listing");
        assert!(
            !out["files"].to_string().contains("pass.rs"),
            "{operation}: {out}"
        );
        let lead = out["next"]["includeSkipped"]["query"]["queries"][0].clone();
        assert_eq!(lead["defaultExcludes"], false, "{operation}: {out}");
        let all = run(&root.0, lead).expect("lead");
        assert!(
            all["files"].to_string().contains("pass.rs"),
            "{operation}: the lead reaches the pruned file: {all}"
        );
    }
}

/// `files` shares every local walk's hidden default: dot entries stay out
/// unless `includeHidden`, and the listing counts what it hid.
#[test]
fn files_hide_dot_entries_unless_include_hidden() {
    let root = Fixture::new();
    root.write(".cfg/a.ts", "x\n");
    root.write("b.ts", "x\n");
    let query = |hidden: bool| {
        let mut query = json!({"operation":"files","path":root.0,"extensions":["ts"]});
        if hidden {
            query["includeHidden"] = json!(true);
        }
        run(&root.0, query).expect("files")
    };
    let default = query(false);
    let paths = listed(&default)
        .iter()
        .filter_map(|entry| entry["path"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    assert!(paths.len() == 1 && paths[0].ends_with("b.ts"), "{default}");
    assert_eq!(default["summary"]["hidden"], 1, "{default}");
    assert_eq!(listed(&query(true)).len(), 2);
}

/// A directory's own length (its entry table) is no content: `sort:size`
/// ranks it after every file and names no size for it, and a size filter
/// lists no directory.
#[test]
fn directories_carry_no_size_to_rank_or_filter_by() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("crowded")).expect("dir");
    for index in 0..64 {
        std::fs::write(
            root.0
                .join(format!("crowded/entry-with-a-long-name-{index:03}")),
            "",
        )
        .expect("entry");
    }
    std::fs::write(root.0.join("a.txt"), "hello").expect("file");
    assert!(
        std::fs::metadata(root.0.join("crowded"))
            .expect("dir")
            .len()
            > 5
    );

    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"maxDepth":1,"sort":"size"}),
    )
    .expect("listing");
    let rows = listed(&out);
    assert_eq!(rows.len(), 2, "{out}");
    assert!(
        rows[0]["path"].as_str().expect("path").ends_with("a.txt"),
        "{out}"
    );
    assert_eq!(rows[0]["size"], 5, "{out}");
    assert_eq!(rows[1]["type"], "directory", "{out}");
    assert!(rows[1].get("size").is_none(), "{out}");

    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"maxDepth":1,"size":{"greater":"1"}}),
    )
    .expect("sized listing");
    let rows = listed(&out);
    assert_eq!(rows.len(), 1, "{out}");
    assert!(
        rows[0]["path"].as_str().expect("path").ends_with("a.txt"),
        "{out}"
    );
}
