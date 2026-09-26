use super::execute_structure;
use crate::{
    policy::path::{PathPolicy, PathPolicyConfig},
    security::ContentSecurity,
    tools::local_fetch::CancellationCheck,
};
use serde_json::{Value, json};
use std::path::PathBuf;

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

fn run(root: &std::path::Path, query: Value) -> super::StructureResult {
    let (paths, security) = simple_policy(root);
    execute_structure(query, &paths, &security, &Active)
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
        json!({"operation":"files","reasoning":"test","path":root.0,"detail":"full","sort":"lines","entryType":"f"}),
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalFiles"], 1);
    assert_eq!(files["files"][0]["lineCount"], 2);
    assert!(!files.to_string().contains("credentials"));
    assert!(!files.to_string().contains("tfstate"));
    let tree = run(
        &root.0,
        json!({"operation":"tree","reasoning":"test","path":root.0,"hidden":true}),
    )
    .expect("tree");
    assert_eq!(tree["entries"], json!(["visible.rs (21.0B)"]), "{tree}");
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
        let error = execute_structure(
            json!({"operation":operation,"reasoning":"test","path":root.0}),
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
    let files = execute_structure(
        json!({"operation":"files","reasoning":"test","path":root.0,"detail":"full","entryType":"f"}),
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
        let out = execute_structure(
            json!({"operation":"files","reasoning":"test","path":root.0,"entryType":"f","sort":"path"}),
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
    let page1 = execute_structure(
        json!({"operation":"files","reasoning":"test","path":root.0,"entryType":"f","pageSize":2}),
        &paths,
        &security,
        &Active,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    // Happy path: the freshly emitted snapshot must be accepted on page 2.
    let good = execute_structure(
        json!({"operation":"files","reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
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
    let page2 = execute_structure(
        json!({"operation":"files","reasoning":"test","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
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
}

#[test]
fn files_line_count_counts_lines_not_newlines_plus_one() {
    let root = Fixture::new();
    std::fs::write(root.0.join("two.rs"), "a\nb\n").expect("two");
    std::fs::write(root.0.join("partial.rs"), "a\nb").expect("partial");
    std::fs::write(root.0.join("empty.rs"), "").expect("empty");
    let out = run(
        &root.0,
        json!({"operation":"files","reasoning":"test","path":root.0,"detail":"full","entryType":"f"}),
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
        json!({"operation":"files","reasoning":"test","path":root.0,"sort":"path"}),
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
        json!({"operation":"tree","reasoning":"test","path":root.0,"maxDepth":0}),
    )
    .expect("top");
    assert_eq!(top["entries"], json!(["README.md (0.0B)", "src/"]), "{top}");
    assert!(top.get("pagination").is_none(), "{top}");
    let all = run(
        &root.0,
        json!({"operation":"tree","reasoning":"test","path":root.0,"maxDepth":5}),
    )
    .expect("all");
    assert_eq!(
        all["entries"],
        json!([
            "README.md (0.0B)",
            "src/",
            "src/deep/",
            "src/deep/leaf.rs (1.0B)",
            "src/lib.rs (2.0B)"
        ]),
        "{all}"
    );
    assert_eq!(all["summary"], "5 entries (3 files, 2 dirs, 3.0B)");
    let dirs = run(
        &root.0,
        json!({"operation":"tree","reasoning":"test","path":root.0,"maxDepth":5,"entryType":"d"}),
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
        json!({"operation":"tree","reasoning":"test","path":root.0,"pageSize":2}),
    )
    .expect("page1");
    assert_eq!(page1["pagination"]["totalPages"], 3, "{page1}");
    let next = &page1["next"]["nextPage"];
    assert_eq!(next["tool"], "structureSearch");
    assert_eq!(next["query"]["operation"], "tree");
    let page2 = run(&root.0, next["query"].clone()).expect("page2");
    assert_eq!(
        page2["entries"],
        json!(["f2.txt (1.0B)", "f3.txt (1.0B)"]),
        "{page2}"
    );
    std::fs::write(root.0.join("f9.txt"), "x").expect("mutate");
    let stale = run(&root.0, next["query"].clone()).expect("stale");
    assert_eq!(stale["errorCode"], "structure.snapshot.changed", "{stale}");
}

#[test]
fn old_ast_shapes_are_not_structure_queries() {
    let root = Fixture::new();
    for retired in [
        json!({"operation":"tree","reasoning":"test","treeKind":"filesystem","path":root.0}),
        json!({"operation":"syntaxTree","reasoning":"test","path":root.0}),
        json!({"operation":"tree","reasoning":"test","path":root.0,"langType":"rust"}),
    ] {
        let error = run(&root.0, retired).expect_err("not a structureSearch query");
        assert_eq!(error.code, "structure.input.invalid");
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
