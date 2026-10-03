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
    execute_structure(&query, paths, security, cancellation, None)
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

/// `files` rows expanded from their directory groups, one object per entry:
/// `path` (`dir` + `/` + name; `.` names `dir` itself), `type` for
/// directories and symlinks, and the entry's `size`, `lineCount` and
/// `modifiedMs` fields.
fn listed(out: &Value) -> Vec<Value> {
    out["files"]
        .as_array()
        .unwrap_or_else(|| panic!("files groups: {out}"))
        .iter()
        .flat_map(|group| {
            let dir = group["dir"].as_str().expect("dir").to_owned();
            group["files"]
                .as_array()
                .expect("group entries")
                .iter()
                .map(move |entry| listed_entry(&dir, entry.as_str().expect("entry text")))
        })
        .collect()
}

fn listed_entry(dir: &str, entry: &str) -> Value {
    let (name, fields) = match entry.strip_suffix(')').and_then(|e| e.rsplit_once(" (")) {
        Some((name, fields)) => (name, fields.split(", ").collect::<Vec<_>>()),
        None => (entry, Vec::new()),
    };
    let mut row = json!({});
    let name = match name.strip_suffix('/') {
        Some(name) => {
            row["type"] = json!("directory");
            name
        }
        None => name,
    };
    row["path"] = json!(match (dir, name) {
        (dir, ".") => dir.to_owned(),
        ("", name) => name.to_owned(),
        (dir, name) => format!("{dir}/{name}"),
    });
    for field in fields {
        if field == "symlink" {
            row["type"] = json!("symlink");
        } else if let Some(lines) = field.strip_prefix("lineCount=") {
            row["lineCount"] = json!(lines.parse::<u64>().expect("lineCount"));
        } else if let Some(modified) = field.strip_prefix("modifiedMs=") {
            row["modifiedMs"] = json!(modified.parse::<i64>().expect("modifiedMs"));
        } else {
            row["size"] = json!(field.parse::<i64>().expect("size"));
        }
    }
    row
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
            assert!(listed(&out)[0].get("modifiedMs").is_none());
        } else {
            assert!(listed(&out)[0]["modifiedMs"].is_i64(), "{out}");
            let actual = listed(&out)[0]["modifiedMs"].as_f64().expect("modifiedMs");
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
        listed(&out)[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/largest.rs")),
        "{out}"
    );
    assert_eq!(listed(&out)[0]["lineCount"], 7, "{out}");
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
    assert_eq!(listed(&files)[0]["lineCount"], 2);
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
        listed(&out)
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
fn continuation_snapshots_bind_page_size_and_rendered_metadata() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    for operation in ["tree", "files"] {
        let first = run(&root.0, json!({"operation":operation,"path":root.0,"pageSize":2,"goal":"test","reasoning":"test"})).expect("first");
        let mut resized = first["next"]["nextPage"]["query"].clone();
        resized["pageSize"] = json!(3);
        let rejected = run(&root.0, resized).expect("resized cursor");
        assert_eq!(
            rejected["errorCode"], "structure.snapshot.changed",
            "{operation}: {rejected}"
        );
        let restarted =
            run(&root.0, rejected["next"]["restart"]["query"].clone()).expect("restart");
        assert_eq!(restarted["pagination"]["currentPage"], 1);
    }
    let first = run(&root.0, json!({"operation":"files","path":root.0,"detail":"full","pageSize":2,"goal":"test","reasoning":"test"})).expect("first files");
    std::fs::write(root.0.join("f3.rs"), "changed\nmetadata\n").expect("change listed evidence");
    let rejected =
        run(&root.0, first["next"]["nextPage"]["query"].clone()).expect("unchanged cursor");
    assert_eq!(
        rejected["errorCode"], "structure.snapshot.changed",
        "{rejected}"
    );
}

#[test]
fn files_disclose_policy_withheld_coverage_without_exposing_names() {
    let root = Fixture::new();
    std::fs::write(root.0.join(".env.production"), "DUMMY_VALUE=placeholder\n").expect("fixture");
    let out = run(&root.0, json!({"operation":"files","path":root.0,"names":[".env.production"],"noIgnore":true,"defaultExcludes":false,"goal":"test","reasoning":"test"})).expect("listing");
    assert!(listed(&out).is_empty(), "{out}");
    assert!(
        out["warnings"]
            .as_array()
            .is_some_and(|warnings| warnings.iter().any(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("withheld") && text.contains("policy")))),
        "{out}"
    );
    assert!(
        !out["warnings"].to_string().contains(".env.production"),
        "{out}"
    );
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
        listed(&out)
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
    let rows = listed(&out);
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
        let mut paths = listed(out)
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
    let retried = run(&root.0, retry["query"].clone()).expect("retry");
    assert!(
        retried["entries"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{retried}"
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
fn a_default_listing_is_one_page_within_the_budget_and_budget_pages_beyond_it() {
    let root = Fixture::new();
    for n in 0..150 {
        std::fs::write(root.0.join(format!("module_{n:03}.py")), "x = 1\n").expect("source");
    }
    let query = json!({"operation":"files","goal":"test","reasoning":"test","path":root.0,"names":["*.py"]});
    let out = run(&root.0, query.clone()).expect("files");
    assert_eq!(listed(&out).len(), 150, "{out}");
    assert_eq!(out["pagination"]["hasMore"], false, "{out}");
    assert!(out["pagination"].get("filesPerPage").is_none(), "{out}");
    assert!(out.get("next").is_none(), "{out}");
    // Rows carry the byte count; the formatted size is a debug field.
    assert_eq!(listed(&out)[0]["size"], 6, "{out}");

    // A caller page size still pages by count.
    let mut sized = query.clone();
    sized["pageSize"] = json!(100);
    let out = run(&root.0, sized).expect("sized files");
    assert_eq!(listed(&out).len(), 100, "{out}");
    assert_eq!(out["pagination"]["filesPerPage"], 100, "{out}");
    assert!(out["next"]["nextPage"].is_object(), "{out}");

    // Past the budget, following nextPage reaches every row exactly once.
    for n in 150..4000 {
        std::fs::write(root.0.join(format!("module_{n:04}.py")), "x = 1\n").expect("source");
    }
    let mut next = query;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let out = run(&root.0, next.clone()).expect("page");
        pages += 1;
        for row in listed(&out) {
            assert!(seen.insert(row["path"].as_str().expect("path").to_owned()));
        }
        let Some(call) = out["next"].get("nextPage") else {
            break;
        };
        assert!(call["query"].get("pageSize").is_none(), "{call}");
        next = call["query"].clone();
    }
    assert_eq!(seen.len(), 4000);
    assert!((3..=6).contains(&pages), "{pages} pages");
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
        json!({"operation":"files","goal":"t","reasoning":"t","path":root.0,"sort":"path"}),
    )
    .expect("files");
    assert_eq!(
        out["files"],
        json!([
            {"dir": name, "files": ["./", "a.rs (10)", "sub/"]},
            {"dir": format!("{name}/sub"), "files": ["b.rs (2)", "deep/"]},
            {"dir": format!("{name}/sub/deep"), "files": ["c (1).rs (4)"]},
            {"dir": name, "files": ["z.rs (2)"]}
        ]),
        "{out}"
    );
    assert_eq!(out["pagination"]["totalFiles"], 7, "{out}");
    let paths = listed(&out)
        .iter()
        .map(|row| row["path"].as_str().expect("path").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            name.clone(),
            format!("{name}/a.rs"),
            format!("{name}/sub"),
            format!("{name}/sub/b.rs"),
            format!("{name}/sub/deep"),
            format!("{name}/sub/deep/c (1).rs"),
            format!("{name}/z.rs"),
        ]
    );

    let full = run(
        &root.0,
        json!({"operation":"files","goal":"t","reasoning":"t","path":root.0.join("sub/deep"),"detail":"full"}),
    )
    .expect("full");
    let entry = full["files"][0]["files"][1].as_str().expect("entry");
    assert!(
        entry.starts_with("c (1).rs (4, lineCount=2, modifiedMs="),
        "{full}"
    );
    let row = &listed(&full)[1];
    assert_eq!(row["size"], 4);
    assert_eq!(row["lineCount"], 2);
    assert!(row["modifiedMs"].is_i64(), "{row}");

    // A file root is its name inside its parent (`""`, the root's parent).
    let file = run(
        &root.0,
        json!({"operation":"files","goal":"t","reasoning":"t","path":root.0.join("a.rs")}),
    )
    .expect("file root");
    assert_eq!(file["files"], json!([{"dir": "", "files": ["a.rs (10)"]}]));
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
    let name = root.0.file_name().unwrap().to_string_lossy().into_owned();
    let mut query = json!({"operation":"files","goal":"t","reasoning":"t","path":root.0,"entryType":"f","pageSize":2});
    let mut pages = Vec::new();
    loop {
        let out = run(&root.0, query.clone()).expect("page");
        pages.push(out["files"].clone());
        let Some(next) = out["next"].get("nextPage") else {
            break;
        };
        query = next["query"].clone();
    }
    assert_eq!(
        pages,
        [
            json!([{"dir": format!("{name}/src"), "files": ["f0.rs (2)", "f1.rs (2)"]}]),
            json!([{"dir": format!("{name}/src"), "files": ["f2.rs (2)", "f3.rs (2)"]}]),
            json!([{"dir": format!("{name}/src"), "files": ["f4.rs (2)"]}]),
        ]
    );
}

/// A budget page counts a group header when a row opens a group: on a new
/// directory, and again on the first row of every page.
#[test]
fn budget_pages_count_a_header_for_each_group_a_page_opens() {
    use super::{RowCost, page_ranges};
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
/// and offers the executable retry that includes them; pruned directories
/// such as `.git` stay out of the count, since `hidden:true` prunes them too.
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
        json!({"operation":"tree","goal":"t","reasoning":"t","path":root.0,"maxDepth":1}),
    )
    .expect("tree");
    assert_eq!(out["entries"], json!(["src/", "src/lib.rs (2B)"]), "{out}");
    assert!(
        out["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("3 dot entries skipped")),
        "{out}"
    );
    let retry = &out["next"]["includeHidden"];
    assert_eq!(retry["tool"], "structureSearch", "{out}");
    assert_eq!(retry["query"]["hidden"], true, "{out}");
    assert!(retry["query"].get("snapshot").is_none(), "{out}");
    let all = run(&root.0, retry["query"].clone()).expect("retry");
    assert_eq!(
        all["entries"],
        json!([
            ".editorconfig (2B)",
            ".github/",
            "src/",
            "src/.keep (0B)",
            "src/lib.rs (2B)"
        ]),
        "{all}"
    );
    assert!(all["next"].get("includeHidden").is_none(), "{all}");
    assert!(
        !all["summary"].as_str().unwrap_or_default().contains("dot"),
        "{all}"
    );
}
