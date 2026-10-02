use super::{StructureResult, StructureSearchQuery, execute_structure};
use crate::{
    policy::path::{PathPolicy, PathPolicyConfig},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Tests speak JSON rows; the runtime owns the typed parse.
fn execute_row(
    query: Value,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancellation: &dyn CancellationCheck,
) -> StructureResult {
    let query: StructureSearchQuery =
        serde_json::from_value(query).expect("typed structureSearch row");
    execute_structure(&query, paths, security, cancellation)
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "octocode-structure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("fixture");
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Active;
impl CancellationCheck for Active {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

fn simple_policy(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("policy");
    (paths, ContentSecurity::new())
}

fn run(root: &std::path::Path, query: Value) -> StructureResult {
    let (paths, security) = simple_policy(root);
    execute_row(query, &paths, &security, &Active)
}

#[test]
fn invalid_time_filters_are_rejected_instead_of_skipped() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source\n").expect("source");
    for field in ["modifiedWithin", "modifiedBefore", "accessedWithin"] {
        let error = run(
            &root.0,
            json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"time":{field:"banana"}}),
        )
        .expect_err("invalid filter must not broaden the result");
        assert_eq!(error.code, "invalidInput");
        assert!(error.message.contains(field), "{}", error.message);
    }
}

#[test]
fn invalid_size_filter_is_invalid_input_not_an_execution_failure() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source\n").expect("source");
    let error = run(
        &root.0,
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"size":{"greater":"10zz"}}),
    )
    .expect_err("an unparsable size must be rejected");
    assert_eq!(error.code, "invalidInput", "{error:?}");
    assert!(error.message.contains("10zz"), "{}", error.message);
}

#[test]
fn missing_path_is_not_found() {
    let root = Fixture::new();
    let error = run(
        &root.0,
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0.join("nope")}),
    )
    .expect_err("missing path");
    assert_eq!(error.code, "structure.policy.notFound", "{error:?}");
}

#[test]
fn modified_and_full_detail_return_the_file_modification_time() {
    let root = Fixture::new();
    let source = root.0.join("source.rs");
    std::fs::write(&source, "source\n").expect("source");
    let modified = source
        .metadata()
        .expect("metadata")
        .modified()
        .expect("modified")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("epoch")
        .as_millis() as f64;
    for detail in ["basic", "modified", "full"] {
        let out = run(&root.0, json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"entryType":"f","detail":detail,"sort":"name"})).expect("files");
        if detail == "basic" {
            assert!(out["files"][0].get("modifiedMs").is_none());
        } else {
            assert!(out["files"][0]["modifiedMs"].is_i64(), "{out}");
            let actual = out["files"][0]["modifiedMs"].as_f64().expect("modifiedMs");
            assert!((actual - modified).abs() < 1.0, "{out}");
        }
    }
}

#[test]
fn line_sort_and_full_counts_work_above_two_thousand_entries() {
    let root = Fixture::new();
    for i in 0..2001 {
        std::fs::write(root.0.join(format!("file-{i:04}.rs")), "line\n").expect("file");
    }
    std::fs::write(root.0.join("largest.rs"), "line\n".repeat(7)).expect("largest");
    let out = run(&root.0, json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"entryType":"f","detail":"full","sort":"lines","pageSize":1})).expect("files");
    assert_eq!(out["pagination"]["totalFiles"], 2002);
    assert!(
        out["files"][0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/largest.rs")),
        "{out}"
    );
    assert_eq!(out["files"][0]["lineCount"], 7, "{out}");
}

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".aws")).expect("sensitive directory");
    std::fs::write(root.0.join(".aws/credentials"), "hidden\n".repeat(50)).expect("secret");
    // Built-in sensitive file name: ignored by the path policy, not by config.
    std::fs::write(root.0.join("terraform.tfstate"), "ignored\n".repeat(70)).expect("ignored");
    std::fs::write(root.0.join("visible.rs"), "pub fn visible() {\n}\n").expect("source");
    let files = run(
        &root.0,
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"detail":"full","sort":"lines","entryType":"f"}),
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalFiles"], 1);
    assert_eq!(files["files"][0]["lineCount"], 2);
    assert!(!files.to_string().contains("credentials"));
    assert!(!files.to_string().contains("tfstate"));
    let tree = run(
        &root.0,
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0,"hidden":true}),
    )
    .expect("tree");
    assert_eq!(tree["entries"], json!(["visible.rs (21B)"]), "{tree}");
}

#[test]
fn cancellation_interrupts_descendant_traversal() {
    struct AfterRoot(std::sync::atomic::AtomicUsize);
    impl CancellationCheck for AfterRoot {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 2 {
                Err("stop traversal".into())
            } else {
                Ok(())
            }
        }
    }
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source").expect("source");
    let (paths, security) = simple_policy(&root.0);
    for operation in ["files", "tree"] {
        let error = execute_row(
            json!({"operation":operation,"goal": "test", "reasoning":"test","path":root.0}),
            &paths,
            &security,
            &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
        )
        .expect_err("cancel during walk");
        assert_eq!(error.code, "structure.execution.cancelled", "{operation}");
    }
}

#[cfg(unix)]
#[test]
fn escaped_links_are_pruned_before_line_counting() {
    let root = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(outside.0.join("outside.rs"), "outside\n").expect("outside");
    std::os::unix::fs::symlink(outside.0.join("outside.rs"), root.0.join("link.rs")).expect("link");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new();
    let files = execute_row(
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"detail":"full","entryType":"f"}),
        &paths,
        &security,
        &Active,
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalFiles"], 0);
}

#[test]
fn files_path_sort_is_lexicographic_and_stable() {
    let root = Fixture::new();
    for name in ["zebra.rs", "apple.rs", "mango.rs", "banana.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let run = || {
        let out = execute_row(
            json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"entryType":"f","sort":"path"}),
            &paths,
            &security,
            &Active,
        )
        .expect("files");
        out["files"]
            .as_array()
            .expect("files array")
            .iter()
            .map(|f| f["path"].as_str().expect("path").to_string())
            .collect::<Vec<_>>()
    };
    let first = run();
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted, "path sort must be lexicographic");
    let second = run();
    assert_eq!(first, second, "path sort must be stable across runs");
}

#[test]
fn files_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_row(
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2}),
        &paths,
        &security,
        &Active,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    // Happy path: the freshly emitted snapshot must be accepted on page 2.
    let good = execute_row(
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &Active,
    )
    .expect("good page2");
    assert!(
        good.get("errorCode").is_none(),
        "valid continuation must not be rejected; got {good}"
    );
    assert_eq!(good["pagination"]["currentPage"], json!(2));
    std::fs::write(root.0.join("newcomer.rs"), "x\n").expect("mutate corpus");
    let page2 = execute_row(
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &Active,
    )
    .expect("page2");
    assert_eq!(
        page2["errorCode"],
        json!("structure.snapshot.changed"),
        "got {page2}"
    );
    let restart = &page2["next"]["restart"];
    assert_eq!(restart["tool"], "structureSearch");
    assert_eq!(restart["query"]["page"], 1);
    assert!(restart["query"].get("snapshot").is_none());
    assert_eq!(restart["query"]["entryType"], "f");
    let fresh = execute_row(restart["query"].clone(), &paths, &security, &Active)
        .expect("execute returned restart unchanged");
    assert_eq!(fresh["pagination"]["currentPage"], 1);
    assert_eq!(fresh["pagination"]["totalFiles"], 7);
    assert_ne!(fresh["snapshot"], page1["snapshot"]);
}

#[test]
fn files_line_count_counts_lines_not_newlines_plus_one() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("two");
    std::fs::write(root.0.join("partial.rs"), "a\nb").expect("partial");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let out = run(
        &root.0,
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"detail":"full","entryType":"f"}),
    )
    .expect("files");
    let count = |name: &str| {
        out["files"]
            .as_array()
            .expect("files")
            .iter()
            .find(|f| f["path"].as_str().is_some_and(|p| p.ends_with(name)))
            .map(|f| f["lineCount"].clone())
            .unwrap_or_else(|| panic!("{name} in {out}"))
    };
    assert_eq!(count("two.rs"), 2);
    assert_eq!(count("partial.rs"), 2);
    // Zero-line files omit lineCount rather than reporting an extra line.
    assert!(count("empty.rs").is_null());
}

#[test]
fn file_rows_omit_the_default_file_type() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join("dir")).expect("dir");
    std::fs::write(root.0.join("dir/a.rs"), "fn a() {}\n").expect("a");
    let out = run(
        &root.0,
        json!({"operation":"files","goal": "test", "reasoning":"test","path":root.0,"sort":"path"}),
    )
    .expect("files");
    let rows = out["files"].as_array().expect("files");
    let file = rows
        .iter()
        .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with("a.rs")))
        .expect("file row");
    assert!(file.get("type").is_none(), "{file}");
    if let Some(dir) = rows
        .iter()
        .find(|r| r["path"].as_str().is_some_and(|p| p.ends_with("/dir")))
    {
        assert_eq!(dir["type"], "directory", "{dir}");
    }
}

#[test]
fn tree_outlines_in_path_order_with_bounded_depth() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src/deep")).expect("dirs");
    std::fs::write(root.0.join("src/deep/leaf.rs"), "x").expect("leaf");
    std::fs::write(root.0.join("src/lib.rs"), "xy").expect("lib");
    std::fs::write(root.0.join("README.md"), "").expect("readme");
    let top = run(
        &root.0,
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0,"maxDepth":0}),
    )
    .expect("top");
    assert_eq!(top["entries"], json!(["README.md (0B)", "src/"]), "{top}");
    assert!(top.get("pagination").is_none(), "{top}");
    let all = run(
        &root.0,
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0,"maxDepth":5}),
    )
    .expect("all");
    assert_eq!(
        all["entries"],
        json!([
            "README.md (0B)",
            "src/",
            "src/deep/",
            "src/deep/leaf.rs (1B)",
            "src/lib.rs (2B)"
        ]),
        "{all}"
    );
    assert_eq!(all["summary"], "5 entries (3 files, 2 dirs, 3B)");
    let dirs = run(
        &root.0,
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0,"maxDepth":5,"entryType":"d"}),
    )
    .expect("dirs");
    assert_eq!(dirs["entries"], json!(["src/", "src/deep/"]), "{dirs}");
}

#[test]
fn tree_pages_through_next_and_rejects_stale_snapshots() {
    let root = Fixture::new();
    for i in 0..5 {
        std::fs::write(root.0.join(format!("f{i}.txt")), "x").expect("file");
    }
    let page1 = run(
        &root.0,
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0,"pageSize":2}),
    )
    .expect("page1");
    assert_eq!(page1["pagination"]["totalPages"], 3, "{page1}");
    let next = &page1["next"]["nextPage"];
    assert_eq!(next["tool"], "structureSearch");
    assert_eq!(next["query"]["operation"], "tree");
    let page2 = run(&root.0, next["query"].clone()).expect("page2");
    assert_eq!(
        page2["entries"],
        json!(["f2.txt (1B)", "f3.txt (1B)"]),
        "{page2}"
    );
    std::fs::write(root.0.join("f9.txt"), "x").expect("mutate");
    let stale = run(&root.0, next["query"].clone()).expect("stale");
    assert_eq!(stale["errorCode"], "structure.snapshot.changed", "{stale}");
    let restart = &stale["next"]["restart"];
    assert_eq!(restart["tool"], "structureSearch");
    assert_eq!(restart["query"]["operation"], "tree");
    assert_eq!(restart["query"]["page"], 1);
    assert!(restart["query"].get("snapshot").is_none());
    let fresh = run(&root.0, restart["query"].clone()).expect("execute returned restart unchanged");
    assert_eq!(fresh["pagination"]["currentPage"], 1);
    assert_eq!(fresh["pagination"]["totalEntries"], 6);
    assert_ne!(fresh["snapshot"], page1["snapshot"]);
}

#[test]
fn old_ast_shapes_are_not_structure_queries() {
    let root = Fixture::new();
    for retired in [
        json!({"operation":"tree","goal": "test", "reasoning":"test","treeKind":"filesystem","path":root.0}),
        json!({"operation":"syntaxTree","goal": "test", "reasoning":"test","path":root.0}),
        json!({"operation":"tree","goal": "test", "reasoning":"test","path":root.0,"langType":"rust"}),
    ] {
        assert!(
            serde_json::from_value::<StructureSearchQuery>(retired).is_err(),
            "not a structureSearch query"
        );
    }
}

/// Layout questions must never pay for (or depend on) a parser: the module
/// walks the filesystem only.
#[test]
fn structure_search_never_reaches_a_parser() {
    for (name, source) in [
        ("mod.rs", include_str!("mod.rs")),
        ("files.rs", include_str!("files.rs")),
        ("tree.rs", include_str!("tree.rs")),
    ] {
        for forbidden in [
            "structural::",
            "ast_search",
            "syntax_tree",
            "tree_sitter",
            "ast_grep",
            "language_extensions",
        ] {
            assert!(!source.contains(forbidden), "{name} references {forbidden}");
        }
    }
}

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
        out["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .filter_map(|entry| entry.as_str()?.split(' ').next().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let out = run(
        &root.0,
        json!({"operation":"tree","goal":"test","reasoning":"test","path":root.0,"hidden":true}),
    )
    .expect("tree");
    assert_eq!(names(&out), [".gitignore", "main.rs"], "{out}");
    assert!(
        out["summary"]
            .as_str()
            // `.git`, `.env.production`, `.npmrc`
            .is_some_and(|summary| summary.ends_with("3 sensitive entries withheld by path policy")),
        "{out}"
    );
    let all = run(
        &root.0,
        json!({"operation":"tree","goal":"test","reasoning":"test","path":root.0,"hidden":true,"noIgnore":true}),
    )
    .expect("tree");
    assert_eq!(
        names(&all),
        [".gitignore", "Cargo.lock", "main.rs"],
        "{all}"
    );
    // Without `hidden`, dot entries are out of view and not counted.
    let plain = run(
        &root.0,
        json!({"operation":"tree","goal":"test","reasoning":"test","path":root.0}),
    )
    .expect("tree");
    assert!(
        !plain["summary"]
            .as_str()
            .unwrap_or_default()
            .contains("withheld"),
        "{plain}"
    );
}

/// `files` leaves out `.gitignore`d entries like the tree and localSearch, and
/// prunes ignored directories during the walk instead of listing their
/// contents; `defaultExcludes:false` walks everything.
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
        let mut paths = out["files"]
            .as_array()
            .expect("files")
            .iter()
            .filter_map(|row| row["path"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    let out = run(
        &root.0,
        json!({"operation":"files","goal":"test","reasoning":"test","path":root.0,"names":["package.json","*.node"]}),
    )
    .expect("files");
    let base = root.0.file_name().unwrap().to_string_lossy().into_owned();
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
        json!({"operation":"files","goal":"test","reasoning":"test","path":root.0,"names":["package.json","*.node"],"defaultExcludes":false}),
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

    let tree = run(
        &root.0,
        json!({"operation":"tree","goal":"test","reasoning":"test","path":repos}),
    )
    .expect("tree");
    assert_eq!(tree["status"], "empty", "{tree}");
    let hints = tree["hints"].as_array().expect("hints");
    assert!(
        hints.iter().any(|hint| hint
            .as_str()
            .is_some_and(|hint| hint.contains(".gitignore") && hint.contains("noIgnore"))),
        "{tree}"
    );
    let retry = &tree["next"]["includeIgnored"];
    assert_eq!(retry["tool"], "structureSearch", "{tree}");
    assert_eq!(retry["query"]["noIgnore"], true, "{tree}");
    assert!(retry["query"].get("snapshot").is_none(), "{tree}");
    let listed = run(&root.0, retry["query"].clone()).expect("retry");
    assert!(
        listed["entries"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{listed}"
    );

    let files = run(
        &root.0,
        json!({"operation":"files","goal":"test","reasoning":"test","path":repos,"extensions":["java"]}),
    )
    .expect("files");
    assert_eq!(files["status"], "empty", "{files}");
    assert!(
        files["hints"]
            .as_array()
            .is_some_and(|hints| hints.iter().any(|hint| hint
                .as_str()
                .is_some_and(|hint| hint.contains(".gitignore") && hint.contains("noIgnore")))),
        "{files}"
    );
    let retry = &files["next"]["includeIgnored"];
    assert_eq!(retry["query"]["noIgnore"], true, "{files}");
    let found = run(&root.0, retry["query"].clone()).expect("retry");
    assert_eq!(found["files"].as_array().map(Vec::len), Some(1), "{found}");
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
        json!({"operation":"tree","goal":"test","reasoning":"test","path":empty}),
        json!({"operation":"files","goal":"test","reasoning":"test","path":empty,"extensions":["java"]}),
    ] {
        let out = run(&root.0, query).expect("listing");
        assert_eq!(out["status"], "empty", "{out}");
        assert!(out.get("hints").is_none(), "{out}");
        assert!(out["next"].get("includeIgnored").is_none(), "{out}");
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
        json!({"operation":"tree","goal":"test","reasoning":"test","path":root.0.join("repos")}),
    )
    .expect("tree");
    assert_eq!(out["entries"].as_array().map(Vec::len), Some(1), "{out}");
    assert!(
        out["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("2 entries hidden by .gitignore")),
        "{out}"
    );
    assert!(out.get("hints").is_none(), "{out}");
    let all = run(
        &root.0,
        json!({"operation":"tree","goal":"test","reasoning":"test","path":root.0.join("repos"),"noIgnore":true}),
    )
    .expect("tree");
    assert!(
        !all["summary"]
            .as_str()
            .unwrap_or_default()
            .contains(".gitignore"),
        "{all}"
    );
}

/// A path-ordered listing is the walk order, so the walk stops once the
/// requested limit is filled instead of scanning the whole tree.
#[test]
fn path_sorted_listing_stops_walking_at_the_limit() {
    let root = Fixture::new();
    for dir in ["a", "a-b", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..10 {
            std::fs::write(root.0.join(format!("{dir}/f{n}.go")), "x\n").expect("file");
        }
    }
    let listing = |limit: Option<u32>| {
        let mut query = json!({"operation":"files","goal":"test","reasoning":"test","path":root.0,"extensions":["go"],"sort":"path"});
        if let Some(limit) = limit {
            query["limit"] = json!(limit);
        }
        run(&root.0, query).expect("files")
    };
    let full = listing(None);
    let prefix = format!("{}/", root.0.file_name().unwrap().to_string_lossy());
    let names = |out: &Value| {
        out["files"]
            .as_array()
            .expect("files")
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
    assert!(full.get("truncated").is_none(), "{full}");
    let cut = listing(Some(3));
    assert_eq!(names(&cut), all[..3], "{cut}");
    assert_eq!(cut["truncated"], true);
    assert_eq!(cut["partialReasons"], json!(["limit"]));
    // The walk stopped early: the total is a lower bound, not a count.
    assert!(cut.get("totalAvailable").is_none(), "{cut}");
    assert_eq!(cut["atLeast"], 4, "{cut}");
    assert!(cut.get("terminalLimit").is_none(), "{cut}");
    assert_eq!(cut["next"]["expandLimit"]["query"]["limit"], 6, "{cut}");
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
        json!({"operation":"tree","goal":"t","reasoning":"t","path":root.0,"names":["*_test.rs"]}),
    )
    .expect("tree");
    assert_eq!(tree["entries"], json!(["a_test.rs (2B)"]), "{tree}");
    let prefix = format!("{}/", root.0.file_name().unwrap().to_string_lossy());
    let files = |extra: Value| {
        let mut query = json!({"operation":"files","goal":"t","reasoning":"t","path":root.0,"extensions":["rs"]});
        query
            .as_object_mut()
            .expect("query")
            .extend(extra.as_object().expect("extra").clone());
        let out = run(&root.0, query).expect("files");
        out["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|row| {
                let path = row["path"].as_str().expect("path");
                path.strip_prefix(&prefix).unwrap_or(path).to_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(files(json!({})), ["a_test.rs", "lib.rs"]);
    assert_eq!(
        files(json!({"noIgnore":true})),
        ["a_test.rs", "gen.rs", "lib.rs"]
    );
}

/// With no sort, files are listed in path (git ls-files) order.
#[test]
fn files_default_to_path_order() {
    let root = Fixture::new();
    for name in ["b.rs", "a.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let out = run(
        &root.0,
        json!({"operation":"files","goal":"t","reasoning":"t","path":root.0,"entryType":"f"}),
    )
    .expect("files");
    let base = root.0.file_name().unwrap().to_string_lossy().into_owned();
    let paths = out["files"]
        .as_array()
        .expect("files")
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
fn a_default_listing_is_one_page_within_the_budget_and_budget_pages_beyond_it() {
    let root = Fixture::new();
    for n in 0..150 {
        std::fs::write(root.0.join(format!("module_{n:03}.py")), "x = 1\n").expect("source");
    }
    let query = json!({"operation":"files","goal":"test","reasoning":"test","path":root.0,"names":["*.py"]});
    let out = run(&root.0, query.clone()).expect("files");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(150), "{out}");
    assert_eq!(out["pagination"]["hasMore"], false, "{out}");
    assert!(out["pagination"].get("filesPerPage").is_none(), "{out}");
    assert!(out.get("next").is_none(), "{out}");
    // Rows carry the byte count; the formatted size is a debug field.
    assert_eq!(out["files"][0]["size"], 6, "{out}");

    // A caller page size still pages by count.
    let mut sized = query.clone();
    sized["pageSize"] = json!(100);
    let out = run(&root.0, sized).expect("sized files");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(100), "{out}");
    assert_eq!(out["pagination"]["filesPerPage"], 100, "{out}");
    assert!(out["next"]["nextPage"].is_object(), "{out}");

    // Past the budget, following nextPage reaches every row exactly once.
    for n in 150..1500 {
        std::fs::write(root.0.join(format!("module_{n:04}.py")), "x = 1\n").expect("source");
    }
    let mut next = query;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let out = run(&root.0, next.clone()).expect("page");
        pages += 1;
        for row in out["files"].as_array().expect("files") {
            assert!(seen.insert(row["path"].as_str().expect("path").to_owned()));
        }
        let Some(call) = out["next"].get("nextPage") else {
            break;
        };
        assert!(call["query"].get("pageSize").is_none(), "{call}");
        next = call["query"].clone();
    }
    assert_eq!(seen.len(), 1500);
    assert!((3..=6).contains(&pages), "{pages} pages");
}
