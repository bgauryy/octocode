//! Listing summaries and what a listing names: totals shared by `files` and
//! `tree`, ignored entries, globs above the path, and links it does not follow.

use super::*;
use octocode_test_support::json_fixture;

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
        json!({"operation":"tree","view":"details","path":root.0,"maxDepth":4}),
    )
    .expect("tree");
    let directories: std::collections::BTreeSet<_> = listed(&out)
        .into_iter()
        .filter(|entry| entry["type"] == "directory")
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect();
    assert_eq!(
        out["summary"]["dirCount"],
        json!(directories.len()),
        "{out}"
    );
    assert_eq!(directories.len(), 6, "{out}");
    assert!(directories.contains("./only/mid"), "{out}");
}

/// `files` counts `summary.dirs` from the walk, like `tree`: a chain of
/// single-child directories lists each directory as one `name/` row, and
/// the summary counts every one of them.
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
        let mut stubs = 0;
        for group in out["files"].as_array().expect("files") {
            stubs += group["files"]
                .as_array()
                .expect("entries")
                .iter()
                .filter(|entry| entry.as_str().is_some_and(|entry| entry.ends_with('/')))
                .count();
        }
        assert_eq!(out["summary"]["dirCount"], 5, "{operation}: {out}");
        assert_eq!(stubs, 5, "{operation}: every directory is one row: {out}");
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
        json!({"operation":"files","view":"details","path":root.0,"maxDepth":10,"includeHidden":true}),
        json!({"operation":"tree","view":"details","path":root.0,"maxDepth":10,"includeHidden":true}),
    ]
    .map(|query| {
        let out = run(&root.0, query.clone()).expect("listing");
        assert_ne!(out["status"], "empty", "{query}: {out}");
        out["summary"].clone()
    });
    let totals = |summary: &Value| {
        (
            summary["fileCount"].clone(),
            summary["dirCount"].clone(),
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

/// A non-empty listing whose walk left out `.gitignore`d entries counts them
/// in `skippedPaths` and offers no call; the same listing with
/// `includeIgnored` lists them and counts none.
#[test]
fn a_listing_with_ignored_entries_counts_them_and_offers_no_call() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "ign/\n").expect("gitignore");
    std::fs::create_dir(root.0.join("ign")).expect("ignored dir");
    std::fs::write(root.0.join("ign/secret.ts"), "x\n").expect("ignored");
    std::fs::write(root.0.join("main.rs"), "x\n").expect("file");
    for operation in ["tree", "files"] {
        let query = json!({"operation":operation,"path":root.0});
        let out = run(&root.0, query.clone()).expect("listing");
        assert_eq!(skipped_count(&out, "ignored"), 1, "{operation}: {out}");
        assert_listing_guides_nothing(&out);
        let lifted = lifting(query, json!({"includeIgnored": true}));
        let all = run(&root.0, lifted).expect("relaxed listing");
        assert_eq!(skipped_count(&all, "ignored"), 0, "{operation}: {all}");
        assert!(
            all["files"].to_string().contains("ign"),
            "{operation}: {all}"
        );
        assert_listing_guides_nothing(&all);
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
    let expected = octocode_runtime_base::policy::include::parent_glob_warning(
        &["../src/*.rs".to_owned()],
        &[],
    )
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
            let row = octocode_runtime_base::response::rows::result_row(0, window, None);
            assert!(
                names(&row["data"], "notices") && !names(&row["data"], "warnings"),
                "a resumed walk preserves the diagnostic: {operation} {key}: {row}"
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
        let reasons = skipped_reasons(&out, "withheld");
        assert_eq!(
            reasons,
            [("symlink not followed".into(), 1)],
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
            json!({"operation":operation,"view":"details","path":links,"extensions":["md"]}),
        )
        .expect("filtered listing");
        let reasons = skipped_reasons(&out, "withheld");
        assert_eq!(
            reasons,
            [("symlink not followed".into(), 1)],
            "{operation}: {out}"
        );
        assert!(out.get("warnings").is_none(), "{operation}: {out}");
        let plain = run(
            &root.0,
            json!({"operation":operation,"view":"details","path":links}),
        )
        .expect("listing");
        assert!(plain.get("warnings").is_none(), "{operation}: {plain}");
        // The link row names its own length (the target path's), then `symlink`.
        let length = std::fs::symlink_metadata(links.join("skill"))
            .expect("link")
            .len();
        let entries = listed(&plain);
        assert_eq!(entries.len(), 1, "{plain}");
        assert_eq!(entries[0]["type"], "symlink", "{plain}");
        assert_eq!(entries[0]["size"], length, "{plain}");
    }
}

/// C3eK A-R9: an empty listing states its zero once, as `files:[]` with the
/// `empty` status, not again as a zero `summary`. A listing with entries
/// keeps its summary.
#[test]
fn an_empty_listing_states_no_zero_summary() {
    let root = Fixture::new();
    for n in 0..4 {
        root.write(&format!("f{n:02}.rs"), "x\n");
    }
    let listing = run(&root.0, json!({"operation":"files","path":root.0})).expect("listing");
    assert_eq!(listing["summary"]["fileCount"], 4, "{listing}");
    let empty = run(
        &root.0,
        json!({"operation":"files","path":root.0,"nameRegex":"^zz_none$"}),
    )
    .expect("empty listing");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert_eq!(empty["files"], json!([]), "{empty}");
    assert!(empty.get("summary").is_none(), "{empty}");
}

/// C3eK A-R6: no listing row states its snapshot (each continuation carries
/// it in its cursor, and no input takes it from the row).
#[test]
fn rows_state_no_snapshot() {
    let root = Fixture::new();
    for n in 0..40 {
        root.write(&format!("f{n:02}.rs"), "x\n");
    }
    let pages = with_window(8, || {
        walk_pages(
            &root.0,
            json!({"operation":"files","path":root.0,"limit":10}),
        )
    });
    assert!(pages.len() > 1, "a paged listing");
    for page in &pages {
        assert!(page.get("snapshot").is_none(), "{page}");
    }
    let paged = pages[0]["next"]["nextPage"]["input"]["queries"][0]["snapshot"].clone();
    assert!(paged.is_string(), "the continuation pins it: {}", pages[0]);
}

/// A listing's `summary` counts the rows its `pagination.total` counts, on
/// every page that states it, whatever the walk window and the sort: when
/// the count is `complete` it is the whole listing's, so its files,
/// directories and bytes are the entries every page together lists.
#[test]
fn a_summary_counts_the_rows_its_pagination_total_counts() {
    let root = Fixture::new();
    for dir in ["a", "a/b", "c"] {
        for n in 0..3 {
            root.write(&format!("{dir}/f{n}.txt"), "x".repeat(n + 1));
        }
    }
    root.write("top.txt", "top\n");
    let base = json!({"operation":"files","path":root.0,"view":"details","limit":2});
    let mut queries = vec![
        json!({"operation":"tree","path":root.0,"view":"details","maxDepth":4,"limit":2}),
        base.clone(),
    ];
    for sort in [
        json_fixture(r#"{"by": "bytes", "order": "desc"}"#),
        json_fixture(r#"{"by": "traversal", "order": "asc"}"#),
    ] {
        let mut query = base.clone();
        query["sort"] = sort;
        queries.push(query);
    }
    for entry_type in ["file", "directory"] {
        let mut query = base.clone();
        query["entryType"] = json!(entry_type);
        queries.push(query);
    }
    for query in queries {
        let pages = with_window(3, || walk_pages(&root.0, query.clone()));
        let entries = pages.iter().flat_map(listed).collect::<Vec<_>>();
        let directory = |entry: &&Value| entry["type"] == "directory";
        let files = entries.iter().filter(|entry| !directory(entry)).count();
        let bytes: i64 = entries
            .iter()
            .filter_map(|entry| entry["size"].as_i64())
            .sum();
        let mut complete = 0;
        for page in pages.iter().filter(|page| page.get("summary").is_some()) {
            let summary = &page["summary"];
            let counted = summary["fileCount"].as_u64().expect("fileCount")
                + summary["dirCount"].as_u64().expect("dirCount");
            assert_eq!(
                json!(counted),
                page["pagination"]["total"],
                "{query}: the summary counts the total's rows: {page}"
            );
            if page["pagination"]["countScope"] == "complete" {
                complete += 1;
                assert_eq!(summary["fileCount"], json!(files), "{query}: {page}");
                assert_eq!(
                    summary["dirCount"],
                    json!(entries.len() - files),
                    "{query}: {page}"
                );
                assert_eq!(summary["bytes"], json!(bytes), "{query}: {page}");
            }
        }
        assert!(pages[0].get("summary").is_some(), "{query}: {}", pages[0]);
        let ranked = !query["sort"]["by"]
            .as_str()
            .is_some_and(|by| by == "traversal");
        if ranked && entries.len() > 3 {
            assert!(
                complete > 0,
                "{query}: a whole walk past its window: {pages:?}"
            );
        }
    }
}
