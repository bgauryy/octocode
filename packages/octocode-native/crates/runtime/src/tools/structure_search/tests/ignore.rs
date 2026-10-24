//! Ignore rules, withheld and hidden entries, and what an empty listing
//! leads to.

use super::*;

/// The tree leaves out what localSearch leaves out (gitignored entries,
/// unless noIgnore) and says how many sensitive entries the path policy
/// withheld instead of dropping them silently.
#[test]
fn tree_follows_gitignore_and_reports_withheld_sensitive_entries() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "Cargo.lock\n").expect("gitignore");
    for name in ["Cargo.lock", "main.rs", ".env.production", ".npmrc"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let names = |out: &Value| {
        out["files"]
            .as_array()
            .expect("files")
            .iter()
            .flat_map(|group| group["files"].as_array().cloned().unwrap_or_default())
            .filter_map(|entry| entry.as_str()?.split(' ').next().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"includeHidden":true}),
    )
    .expect("tree");
    assert_eq!(names(&out), [".gitignore", "main.rs"], "{out}");
    // `.env.production`, `.npmrc`: named by policy, not by path. The
    // repository's own `.git`, withheld from every walk, is not news.
    let reasons = skipped_reasons(&out, "withheld");
    assert_eq!(reasons, [("credential file".into(), 2)], "{out}");
    let files = run(&root.0, json!({"operation":"files","path":root.0})).expect("files");
    crate::tools::test_support::checks::assert_absent(&files, ".git/");
    let all = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"includeHidden":true,"includeIgnored":true}),
    )
    .expect("tree");
    assert_eq!(
        names(&all),
        [".gitignore", "Cargo.lock", "main.rs"],
        "{all}"
    );
    // Without `hidden`, dot entries are out of view and not counted.
    let plain = run(&root.0, json!({"operation":"tree","path":root.0})).expect("tree");
    crate::tools::test_support::checks::assert_absent(&plain, "withheld");
}

/// `files` leaves out `.gitignore`d entries like the tree and localSearch, and
/// prunes ignored directories during the walk instead of listing their
/// contents; `includeIgnored` walks everything.
#[test]
fn files_follows_gitignore_and_prunes_ignored_directories() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "vendor/\n*.node\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("vendor/pkg")).expect("vendor");
    std::fs::create_dir_all(root.0.join("app")).expect("app");
    for name in [
        "package.json",
        "app/package.json",
        "vendor/pkg/package.json",
        "addon.node",
    ] {
        std::fs::write(root.0.join(name), "{}\n").expect("file");
    }
    let paths = |out: &Value| {
        let mut paths = listed(out)
            .iter()
            .filter_map(|row| row["path"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    let out = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["package.json","*.node"]}),
    )
    .expect("files");
    let base = ".";
    assert_eq!(
        paths(&out),
        [
            format!("{base}/app/package.json"),
            format!("{base}/package.json")
        ],
        "{out}"
    );
    let all = run(
        &root.0,
        json!({"operation":"files","path":root.0,"include":["package.json","*.node"],"includeIgnored":true}),
    )
    .expect("files");
    assert_eq!(paths(&all).len(), 4, "{all}");
}

/// A path whose children are all `.gitignore`d is not empty for the caller's
/// reasons: say ignore rules dropped the entries and offer an executable
/// retry that includes them, instead of the generic "broaden" fallback.
#[test]
fn all_ignored_listing_names_the_ignore_rules_and_offers_a_retry() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "repos/*\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("repos/java/src")).expect("repos");
    std::fs::write(root.0.join("repos/java/src/A.java"), "class A {}\n").expect("file");
    std::fs::write(root.0.join("repos/notes.txt"), "x\n").expect("file");
    let repos = root.0.join("repos");

    let tree = run(&root.0, json!({"operation":"tree","path":repos})).expect("tree");
    assert_eq!(tree["status"], "empty", "{tree}");
    // The lead is the next step; the finding is one hint, the summary has the counts.
    assert_eq!(tree["hints"].as_array().map(Vec::len), Some(1), "{tree}");
    assert!(
        tree["hints"][0]
            .as_str()
            .is_some_and(|h| h.contains("repos/*")),
        "{tree}"
    );
    let retry = &related(&tree, "structureSearch");
    assert_eq!(retry["tool"], "structureSearch", "{tree}");
    assert_eq!(
        retry["input"]["queries"][0]["includeIgnored"], true,
        "{tree}"
    );
    assert!(
        retry["input"]["queries"][0].get("snapshot").is_none(),
        "{tree}"
    );
    let retried = run(&root.0, retry["input"]["queries"][0].clone()).expect("retry");
    assert!(
        retried["files"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{retried}"
    );

    let files = run(
        &root.0,
        json!({"operation":"files","path":repos,"extensions":["java"]}),
    )
    .expect("files");
    assert_eq!(files["status"], "empty", "{files}");
    assert_eq!(files["hints"].as_array().map(Vec::len), Some(1), "{files}");
    assert!(skipped_count(&files, "ignored") > 0, "{files}");
    let retry = &related(&files, "structureSearch");
    assert_eq!(
        retry["input"]["queries"][0]["includeIgnored"], true,
        "{files}"
    );
    let found = run(&root.0, retry["input"]["queries"][0].clone()).expect("retry");
    assert_eq!(listed(&found).len(), 1, "{found}");
}

/// A genuinely empty directory keeps the generic empty result: no ignore
/// rule dropped anything, so no ignore hint or retry is offered.
#[test]
fn empty_listing_without_ignored_entries_offers_no_ignore_retry() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "target/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("empty")).expect("dir");
    let empty = root.0.join("empty");
    for query in [
        json!({"operation":"tree","path":empty}),
        json!({"operation":"files","path":empty,"extensions":["java"]}),
    ] {
        let out = run(&root.0, query).expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        assert!(out.get("hints").is_none(), "{out}");
        assert!(related(&out, "structureSearch").is_null(), "{out}");
    }
}

/// A non-empty tree still says how many entries `.gitignore` hid, so a
/// directory of ignored checkouts next to one tracked README does not read
/// as a directory holding only the README.
#[test]
fn tree_summary_counts_gitignored_entries_it_left_out() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "repos/*\n!repos/README.md\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("repos/java")).expect("repos");
    std::fs::create_dir_all(root.0.join("repos/go")).expect("repos");
    std::fs::write(root.0.join("repos/README.md"), "# repos\n").expect("file");
    let out = run(
        &root.0,
        json!({"operation":"tree","path":root.0.join("repos")}),
    )
    .expect("tree");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(1), "{out}");
    assert_eq!(skipped_count(&out, "ignored"), 2, "{out}");
    assert!(out.get("hints").is_none(), "{out}");
    let all = run(
        &root.0,
        json!({"operation":"tree","path":root.0.join("repos"),"includeIgnored":true}),
    )
    .expect("tree");
    assert_eq!(skipped_count(&all, "ignored"), 0, "{all}");
}

/// An unfiltered listing that left out ignored entries leads past every
/// cause its summary counts in one call: the dot entries it skipped too,
/// not only the ignored ones. Following the lead leaves nothing uncounted.
#[test]
fn an_unfiltered_listing_lead_lifts_every_counted_cause() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "out/\n").expect("gitignore");
    std::fs::create_dir_all(root.0.join("out")).expect("ignored");
    std::fs::write(root.0.join("out/x.txt"), "x\n").expect("ignored file");
    std::fs::create_dir_all(root.0.join(".cargo")).expect("hidden");
    std::fs::write(root.0.join(".cargo/config.toml"), "x\n").expect("hidden file");
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    std::fs::write(root.0.join("src/lib.rs"), "x\n").expect("lib");
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"maxDepth":1}),
        )
        .expect("listing");
        assert!(skipped_count(&out, "ignored") > 0, "{out}");
        assert!(skipped_count(&out, "hidden") > 0, "{out}");
        let lead = related(&out, "structureSearch")["input"]["queries"][0].clone();
        assert_eq!(lead["includeIgnored"], true, "{out}");
        assert_eq!(lead["includeHidden"], true, "{out}");
        let all = run(&root.0, lead).expect("lead");
        assert_eq!(skipped_count(&all, "ignored"), 0, "{all}");
        assert_eq!(skipped_count(&all, "hidden"), 0, "{all}");
    }
}

/// `includeHidden` alone lists the hidden directories (`.agents`, `.github`)
/// in both operations. A hidden directory a `.gitignore` rule names stays
/// counted as ignored, and the lead names the one flag that reaches it.
#[test]
fn include_hidden_alone_lists_hidden_directories_and_leads_to_the_ignored_ones() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), ".octocode/\n").expect("gitignore");
    for file in [
        "src/lib.rs",
        ".agents/notes.md",
        ".github/ci.yml",
        ".octocode/scratch.md",
    ] {
        let path = root.0.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "x\n").expect("file");
    }
    for operation in ["tree", "files"] {
        let out = run(
            &root.0,
            json!({"operation":operation,"path":root.0,"maxDepth":3,"includeHidden":true}),
        )
        .expect("listing");
        let listing = out.to_string();
        for listed in ["notes.md", "ci.yml", "lib.rs"] {
            assert!(
                listing.contains(listed),
                "{operation} lacks {listed}: {out}"
            );
        }
        assert!(!listing.contains("scratch.md"), "{operation}: {out}");
        assert_eq!(skipped_count(&out, "ignored"), 1, "{operation}: {out}");
        assert_eq!(skipped_count(&out, "hidden"), 0, "{operation}: {out}");
        let lead = related(&out, "structureSearch")["input"]["queries"][0].clone();
        assert_eq!(lead["includeIgnored"], true, "{operation}: {out}");
        assert!(lead.get("defaultExcludes").is_none(), "{operation}: {out}");
        let all = run(&root.0, lead).expect("lead");
        assert!(all.to_string().contains("scratch.md"), "{operation}: {all}");
    }
}

/// A path-ordered listing is the walk order, so the walk stops once the
/// requested limit is filled instead of scanning the whole tree.
#[test]
fn traversal_sorted_listing_stops_walking_at_the_limit() {
    let root = Fixture::new();
    for dir in ["a", "a-b", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..10 {
            std::fs::write(root.0.join(format!("{dir}/f{n}.go")), "x\n").expect("file");
        }
    }
    let listing = |limit: Option<u32>| {
        let mut query = json!({"operation":"files","path":root.0,"extensions":["go"],"sort":{"by":"traversal","order":"asc"}});
        if let Some(limit) = limit {
            query["maxEntries"] = json!(limit);
        }
        run(&root.0, query).expect("files")
    };
    let full = listing(None);
    let prefix = "./".to_owned();
    let names = |out: &Value| {
        listed(out)
            .iter()
            .map(|row| {
                let path = row["path"].as_str().expect("path");
                path.strip_prefix(&prefix).unwrap_or(path).to_owned()
            })
            .collect::<Vec<_>>()
    };
    let all = names(&full);
    assert_eq!(all.len(), 30);
    // Walk order: a directory's contents follow it (`a/…` before `a-b/…`).
    assert_eq!(all[9], "a/f9.go");
    assert_eq!(all[10], "a-b/f0.go");
    assert!(full.get("clipped").is_none(), "{full}");
    let cut = listing(Some(3));
    assert_eq!(names(&cut), all[..3], "{cut}");
    assert_eq!(cut["clipped"], true);
    assert_eq!(cut["partialReasons"], json!(["maxEntries"]));
    // The walk stopped early: the total is a lower bound, not a count.
    assert!(cut.get("totalAvailable").is_none(), "{cut}");
    assert_eq!(cut["atLeast"], 4, "{cut}");
    assert!(cut.get("terminalLimit").is_none(), "{cut}");
    assert_eq!(
        cut["next"]["continueWalk"]["input"]["queries"][0]["maxEntries"], 6,
        "{cut}"
    );
}

/// `tree` takes the same name filter as `files`; `files` takes `noIgnore`
/// like `tree` (gitignore only, unlike defaultExcludes:false).
#[test]
fn tree_filters_names_and_files_lists_ignored_entries_on_request() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    std::fs::write(root.0.join(".gitignore"), "gen.rs\n").expect("gitignore");
    std::fs::write(root.0.join("a_test.rs"), "x\n").expect("a");
    std::fs::write(root.0.join("lib.rs"), "x\n").expect("lib");
    std::fs::write(root.0.join("gen.rs"), "x\n").expect("gen");
    let tree = run(
        &root.0,
        json!({"operation":"tree","path":root.0,"include":["*_test.rs"]}),
    )
    .expect("tree");
    assert_eq!(paths(&tree), ["./a_test.rs"], "{tree}");
    let prefix = "./".to_owned();
    let files = |extra: Value| {
        let mut query = json!({"operation":"files","path":root.0,"extensions":["rs"]});
        query
            .as_object_mut()
            .expect("query")
            .extend(extra.as_object().expect("extra").clone());
        let out = run(&root.0, query).expect("files");
        listed(&out)
            .iter()
            .map(|row| {
                let path = row["path"].as_str().expect("path");
                path.strip_prefix(&prefix).unwrap_or(path).to_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(files(json!({})), ["a_test.rs", "lib.rs"]);
    assert_eq!(
        files(json!({"includeIgnored":true})),
        ["a_test.rs", "gen.rs", "lib.rs"]
    );
}
