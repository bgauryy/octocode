//! Expanded scans, listing summaries and empty-listing probes.

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

/// Every directory the walk counted (`summary.dirs`) shows once: as a
/// `{dir, files}` group when it lists entries, else as a `name/` stub. A
/// directory whose subdirectories all have groups of their own lists no
/// entry, so it is the one that keeps its stub: dropping both would lose it.
#[test]
fn summary_dirs_equals_the_stubs_plus_the_groups_the_listing_shows() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("only/mid/leaf")).expect("dirs");
    std::fs::create_dir_all(root.0.join("only/other")).expect("dirs");
    std::fs::create_dir_all(root.0.join("plain/empty")).expect("dirs");
    std::fs::write(root.0.join("only/mid/leaf/deep.rs"), "x\n").expect("deep");
    std::fs::write(root.0.join("only/other/o.rs"), "x\n").expect("other");
    std::fs::write(root.0.join("plain/p.rs"), "x\n").expect("plain");
    std::fs::write(root.0.join("top.rs"), "x\n").expect("top");
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"maxDepth":4}),
    )
    .expect("tree");
    let (mut stubs, mut groups) = (0, 0);
    for group in out["files"].as_array().expect("files") {
        groups += usize::from(group["dir"] != ".");
        stubs += group["files"]
            .as_array()
            .expect("entries")
            .iter()
            .filter(|entry| entry.as_str().is_some_and(|entry| entry.ends_with('/')))
            .count();
    }
    assert_eq!(out["summary"]["dirs"], json!(stubs + groups), "{out}");
    assert_eq!(out["summary"]["dirs"], json!(6), "{out}");
    assert!(
        has_string(&out["files"], "mid/"),
        "a directory of groups keeps a stub: {out}"
    );
}

/// `files` counts `summary.dirs` from the walk, like `tree`: a chain of
/// single-child directories names each directory once (a group header or a
/// `name/` stub), and the summary counts every one of them.
#[test]
fn files_summary_dirs_counts_every_walked_directory() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("a/b/c/d/e")).expect("dirs");
    std::fs::write(root.0.join("a/b/c/d/e/leaf.txt"), "x\n").expect("leaf");
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"maxDepth":10}),
        )
        .expect("listing");
        let (mut stubs, mut groups) = (0, 0);
        for group in out["files"].as_array().expect("files") {
            groups += usize::from(group["dir"] != ".");
            stubs += group["files"]
                .as_array()
                .expect("entries")
                .iter()
                .filter(|entry| entry.as_str().is_some_and(|entry| entry.ends_with('/')))
                .count();
        }
        assert_eq!(out["summary"]["dirs"], 5, "{operation}: {out}");
        assert_eq!(
            stubs + groups,
            5,
            "{operation}: every directory shows once: {out}"
        );
    }
}

/// One root walked whole gives one summary in both views: the same files,
/// directories and file bytes (a symlink's own length is no content).
#[test]
fn files_and_tree_report_the_same_totals() {
    let root = Fixture::new();
    root.write("top.rs", "fn top() {}\n");
    root.write("a/one.txt", "x".repeat(300));
    root.write("a/b/two.md", "# two\n");
    root.write("a/b/c/empty.txt", "");
    root.write(".hidden/dot.cfg", "k=v\n");
    std::fs::create_dir_all(root.0.join("lonely")).expect("empty dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.0.join("a/one.txt"), root.0.join("link.txt")).expect("link");
    let summaries = [
        json!({"operation":"files","path":root.0,"maxDepth":10,"includeHidden":true}),
        json!({"operation":"tree","path":root.0,"maxDepth":10,"includeHidden":true}),
    ]
    .map(|query| {
        let out = run(&root.0, query.clone()).expect("listing");
        assert_ne!(out["status"], "empty", "{query}: {out}");
        out["summary"].clone()
    });
    let totals = |summary: &Value| {
        (
            summary["files"].clone(),
            summary["dirs"].clone(),
            summary["bytes"].clone(),
        )
    };
    assert_eq!(
        totals(&summaries[0]),
        totals(&summaries[1]),
        "files {} vs tree {}",
        summaries[0],
        summaries[1]
    );
    let files = if cfg!(unix) { 6 } else { 5 };
    let bytes = 12 + 300 + 6 + 4;
    assert_eq!(
        totals(&summaries[0]),
        (json!(files), json!(5), json!(bytes)),
        "{}",
        summaries[0]
    );
}

/// A non-empty listing whose walk left out `.gitignore`d entries leads to
/// the same listing over them.
#[test]
fn a_listing_with_ignored_entries_leads_to_include_ignored() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "ign/\n").expect("gitignore");
    std::fs::create_dir(root.0.join("ign")).expect("ignored dir");
    std::fs::write(root.0.join("ign/secret.ts"), "x\n").expect("ignored");
    std::fs::write(root.0.join("main.rs"), "x\n").expect("file");
    for operation in ["tree", "files"] {
        let out = run(&root.0, json!({"operation":operation,"path":root.0})).expect("listing");
        assert_eq!(out["summary"]["ignored"], 1, "{operation}: {out}");
        let lead = &out["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(lead["includeIgnored"], true, "{operation}: {out}");
        assert_eq!(lead["operation"], operation, "{out}");
        let all = run(&root.0, lead.clone()).expect("relaxed listing");
        assert!(all["next"].get("includeSkipped").is_none(), "{all}");
    }
}

/// An include or exclude glob with a `..` segment never matches under
/// `path`: both operations name it, with the localSearch wording. The note
/// reads the same for the whole result, so it is a static notice: stated on
/// the first page and never on a resumed walk's window.
#[test]
fn a_glob_that_reaches_above_path_is_named_by_both_operations() {
    let root = Fixture::new();
    std::fs::write(root.0.join("main.rs"), "x\n").expect("file");
    let expected = crate::policy::include::parent_glob_warning(&["../src/*.rs".to_owned()], &[])
        .expect("warning");
    let names = |row: &Value, key: &str| {
        row[key]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|note| *note == expected.as_str()))
    };
    for operation in ["tree", "files"] {
        for key in ["include", "exclude"] {
            let query = json!({"operation":operation,"path":root.0,key:["../src/*.rs"]});
            let out = run(&root.0, query.clone()).expect("listing");
            assert!(names(&out, "notices"), "{operation} {key}: {out}");
            assert!(
                !names(&out, "warnings"),
                "said once: {operation} {key}: {out}"
            );
            let mut resumed = query;
            resumed["walkOffset"] = json!(1);
            let window = run(&root.0, resumed.clone()).expect("resumed window");
            let row = crate::response::rows::result_row(
                crate::id::ToolId::StructureSearch,
                0,
                &resumed,
                window,
                None,
            );
            assert!(
                !names(&row["data"], "notices") && !names(&row["data"], "warnings"),
                "a resumed walk does not repeat it: {operation} {key}: {row}"
            );
        }
    }
}

/// A symlink to a target outside the allowed roots is withheld in a tree
/// too: the listing says one symlink was not followed instead of dropping
/// it silently, as `files` does.
#[cfg(unix)]
#[test]
fn a_tree_names_a_symlink_that_leaves_the_allowed_roots() {
    let root = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(root.0.join("main.rs"), "x\n").expect("file");
    std::os::unix::fs::symlink(&outside.0, root.0.join("outside")).expect("link");
    for operation in ["tree", "files"] {
        let out = run(&root.0, json!({"operation":operation,"path":root.0})).expect("listing");
        assert_eq!(
            out["warnings"][0], "1 entry withheld by path policy: 1 symlink not followed.",
            "{operation}: {out}"
        );
    }
}

/// A filtered listing walks past an admitted link to a directory without
/// following it, and hides the link row: it discloses the link with
/// localSearch's wording instead of an empty listing that blames its
/// filters. An unfiltered listing shows the link as its own row.
#[cfg(unix)]
#[test]
fn a_filtered_listing_names_the_links_it_does_not_follow() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("real/skill")).expect("target");
    std::fs::write(root.0.join("real/skill/SKILL.md"), "x\n").expect("doc");
    std::fs::create_dir(root.0.join("links")).expect("links");
    std::os::unix::fs::symlink(root.0.join("real/skill"), root.0.join("links/skill"))
        .expect("link");
    let links = root.0.join("links");
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":links,"extensions":["md"]}),
        )
        .expect("filtered listing");
        assert_eq!(
            out["warnings"],
            json!(["1 entry withheld by path policy: 1 symlink not followed."]),
            "{operation}: {out}"
        );
        let plain = run(&root.0, json!({"operation":operation,"path":links})).expect("listing");
        assert!(plain.get("warnings").is_none(), "{operation}: {plain}");
        // The link row names its own length (the target path's), then `symlink`.
        let length = std::fs::symlink_metadata(links.join("skill"))
            .expect("link")
            .len();
        assert!(
            has_string(&plain, &format!("skill ({length}, symlink)")),
            "{operation}: {plain}"
        );
    }
}

/// An empty listing's probe of what it skipped stops at its entry cap: it
/// proves "some" and names the reason without walking the whole directory.
#[test]
fn an_empty_listings_probe_stops_at_its_entry_cap() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("build/gen")).expect("build");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    let cap = super::super::TAGGED_PROBE_ENTRIES;
    for index in 0..cap + 40 {
        std::fs::write(root.0.join(format!("build/gen/f{index:04}.ts")), "x\n").expect("ts");
    }
    for operation in ["files", "tree"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "extensions": ["ts"]}),
        )
        .expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        let hint = out["hints"][0].as_str().expect("hint");
        assert_eq!(
            hint,
            format!(
                "At least {cap} entries sit behind skipped paths (build/); the lead lists them."
            ),
            "{out}"
        );
        let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(retry["defaultExcludes"], false, "{out}");
        // The lead itself lists every one of them.
        let found = run(&root.0, retry.clone()).expect("retry");
        assert_ne!(found["status"], "empty", "{found}");
    }
}

/// An empty listing whose matches sit behind several skip causes (a dot
/// directory and a default-excluded one) walks once more to find why: one
/// walk with every cause lifted, its entries tagged with the causes that hid
/// them, answers every narrower relaxation the diagnosis asks about. The
/// lead lifts exactly the causes that hold matches, and the hint names both.
#[test]
fn an_empty_listing_diagnoses_every_skip_cause_from_one_probe_walk() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join(".notes")).expect("hidden dir");
    std::fs::create_dir_all(root.0.join("build")).expect("build");
    std::fs::create_dir(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    std::fs::write(root.0.join(".notes/a.ts"), "x\n").expect("hidden ts");
    std::fs::write(root.0.join("build/b.ts"), "x\n").expect("built ts");
    for operation in ["files", "tree"] {
        let before = super::super::listing::listing_walks();
        let out = run(
            &root.0,
            json!({"operation": operation, "path": root.0, "extensions": ["ts"]}),
        )
        .expect("listing");
        let walks = super::super::listing::listing_walks() - before;
        assert_eq!(out["status"], "empty", "{out}");
        assert_eq!(walks, 2, "the listing and one probe walk: {out}");
        assert_eq!(
            out["hints"][0],
            "2 entries sit behind skipped paths (hidden .notes/, build/); the lead lists them.",
            "{operation}: {out}"
        );
        let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(retry["includeHidden"], true, "{out}");
        assert_eq!(retry["defaultExcludes"], false, "{out}");
        let found = run(&root.0, retry.clone()).expect("retry");
        assert!(found.to_string().contains("a.ts"), "{found}");
        assert!(found.to_string().contains("b.ts"), "{found}");
    }
}

/// An empty listing names the ignore rule behind it workspace-relative,
/// like its rows, never relative to the listed directory.
#[test]
fn skip_reasons_name_ignore_files_workspace_relative() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "*.gen.ts\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("sub/inner")).expect("dirs");
    std::fs::write(root.0.join("sub/inner/a.gen.ts"), "x\n").expect("generated");
    for operation in ["files", "tree"] {
        let out = run(
            &root.0,
            json!({"operation": operation, "path": "sub/inner", "extensions": ["ts"]}),
        )
        .expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        assert_eq!(
            out["hints"][0],
            "1 entry sits behind skipped paths (.gitignore:1 '*.gen.ts'); the lead lists them.",
            "{operation}: {out}"
        );
    }
}

/// An entry the caller excluded is the caller's own filter: the walk drops
/// it before the path policy sees it, so it is never counted as withheld
/// or hidden.
#[test]
fn excluded_entries_are_not_counted_as_withheld_or_hidden() {
    let root = sensitive_fixture();
    for operation in ["tree", "files"] {
        let plain = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"includeHidden":true}),
        )
        .expect("listing");
        assert!(
            plain["warnings"].to_string().contains("withheld"),
            "{operation}: {plain}"
        );
        let excluded = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"includeHidden":true,"exclude":["terraform.tfstate",".aws"]}),
        )
        .expect("excluded listing");
        assert!(
            excluded.get("warnings").is_none(),
            "{operation}: {excluded}"
        );
        let hidden = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"exclude":[".aws"]}),
        )
        .expect("listing without hidden");
        assert!(
            !hidden.to_string().contains("\"hidden\""),
            "{operation}: {hidden}"
        );
    }
}

/// An empty listing its name filters alone emptied (nothing skipped to
/// lift) leads to the same listing without them (`broadenSearch`), which
/// lists what is there; a tree keeps the depth it walked.
#[test]
fn a_name_filter_miss_leads_to_the_listing_without_its_name_filters() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/deep")).expect("dir");
    std::fs::write(root.0.join("src/deep/lib.rs"), "x\n").expect("file");
    for query in [
        json!({"operation":"files","path":root.0,"nameRegex":"^nomatch$","entryType":"f"}),
        json!({"operation":"tree","path":root.0,"include":["*.nomatch"]}),
    ] {
        let out = run(&root.0, query.clone()).expect("empty listing");
        assert_eq!(out["status"], "empty", "{out}");
        let lead = &out["next"]["broadenSearch"];
        assert_eq!(lead["tool"], "structureSearch", "{out}");
        let row = &lead["query"]["queries"][0];
        for field in ["nameRegex", "include", "extensions"] {
            assert!(row.get(field).is_none(), "{field}: {row}");
        }
        assert_eq!(row["operation"], query["operation"], "{row}");
        assert!(
            out["hints"]
                .as_array()
                .is_some_and(|hints| hints.iter().any(|hint| hint
                    .as_str()
                    .is_some_and(|text| text.starts_with("No entry matches")))),
            "{out}"
        );
        let broadened = run(&root.0, row.clone()).expect("broadened listing");
        assert!(
            broadened.to_string().contains("lib.rs"),
            "{query}: {broadened}"
        );
    }
    // Nothing under the path at all: no lead to a listing that finds nothing.
    let empty = root.0.join("empty");
    std::fs::create_dir(&empty).expect("empty dir");
    let out = run(
        &root.0,
        json!({"operation":"files","path":empty,"nameRegex":"^nomatch$"}),
    )
    .expect("empty listing");
    assert!(out["next"].get("broadenSearch").is_none(), "{out}");
}
