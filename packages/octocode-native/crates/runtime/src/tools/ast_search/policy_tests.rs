use super::execute_ast;
use crate::{
    policy::path::{PathPolicy, PathPolicyConfig},
    security::{ContentSecurity, SecurityRegistry},
    tools::local_fetch::CancellationCheck,
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "octocode-ast-policy-{}-{}",
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

#[test]
fn descendant_policy_precedes_discovery_totals_and_line_reads() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".aws")).expect("sensitive directory");
    std::fs::write(root.0.join(".aws/credentials"), "hidden\n".repeat(50)).expect("secret");
    std::fs::write(root.0.join("generated.locked.rs"), "ignored\n".repeat(70)).expect("ignored");
    std::fs::write(root.0.join("visible.rs"), "pub fn visible() {\n}\n").expect("source");
    let mut registry = SecurityRegistry::default();
    registry
        .add_ignored_file_patterns([regex::Regex::new(r"\.locked(?:\.|$)").expect("pattern")])
        .expect("ignore");
    let paths = PathPolicy::with_registry(
        PathPolicyConfig {
            workspace_root: Some(root.0.clone()),
            ..Default::default()
        },
        &registry,
    )
    .expect("policy");
    let security = ContentSecurity::new(Arc::new(registry));
    let files = execute_ast(
        json!({"operation":"files","path":root.0,"detail":"full","sort":"lines","entryType":"f"}),
        &paths,
        &security,
        &Active,
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalFiles"], 1);
    assert_eq!(files["files"][0]["lineCount"], 3);
    assert!(!files.to_string().contains("credentials"));
    assert!(!files.to_string().contains("locked"));
    let symbols = execute_ast(
        json!({"operation":"symbols","path":root.0}),
        &paths,
        &security,
        &Active,
    )
    .expect("symbols");
    assert_eq!(symbols["filesScanned"], 1);
    assert_eq!(symbols["filesSkipped"], 0);
    assert_eq!(symbols["declarations"][0]["name"], "visible");

    std::fs::write(root.0.join(".aws/hidden.ts"), "oldCall(secret);\n").expect("hidden ast");
    std::fs::write(root.0.join("generated.locked.ts"), "oldCall(ignored);\n").expect("ignored ast");
    std::fs::write(root.0.join("visible.ts"), "oldCall(visible);\n").expect("visible ast");
    let matches = execute_ast(
        json!({
            "operation":"match","path":root.0,"langType":"typescript",
            "pattern":"oldCall($A)","hidden":true
        }),
        &paths,
        &security,
        &Active,
    )
    .expect("structural matches");
    assert_eq!(matches["stats"]["totalStructuralMatches"], 1);
    assert_eq!(matches["files"].as_array().expect("files").len(), 1);
    assert_eq!(matches["files"][0]["path"], "visible.ts");
    assert!(!matches.to_string().contains("hidden.ts"));
    assert!(!matches.to_string().contains("locked.ts"));
}

#[test]
fn structural_zero_is_empty_with_actionable_pattern_guidance() {
    let root = Fixture::new();
    let source = root.0.join("source.ts");
    std::fs::write(&source, "const answer = 42;\n").expect("source");
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));

    let missing = execute_ast(
        json!({"operation":"match","path":source,"pattern":"const $A = $B"}),
        &paths,
        &security,
        &Active,
    )
    .expect("structural zero");
    assert_eq!(missing["status"], "empty", "{missing}");
    assert_eq!(missing["complete"], true, "{missing}");
    let guidance = missing["diagnostics"][0]["message"]
        .as_str()
        .expect("no-match guidance");
    assert!(guidance.contains("trailing semicolons"), "{guidance}");
    assert!(guidance.contains("treeKind:\"syntax\""), "{guidance}");

    let found = execute_ast(
        json!({"operation":"match","path":source,"pattern":"const $A = $B;"}),
        &paths,
        &security,
        &Active,
    )
    .expect("structural match");
    assert_eq!(found["stats"]["totalStructuralMatches"], 1, "{found}");
    assert!(found.get("status").is_none(), "{found}");
}

#[test]
fn directory_prefilter_skips_are_aggregated_once() {
    let root = Fixture::new();
    for name in ["one.ts", "two.ts", "three.ts"] {
        std::fs::write(root.0.join(name), "const value = 1;\n").expect("source");
    }
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
    let result = execute_ast(
        json!({
            "operation":"match",
            "path":root.0,
            "langType":"typescript",
            "pattern":"missingCall($A);"
        }),
        &paths,
        &security,
        &Active,
    )
    .expect("structural zero");
    let prefilter = result["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "structural.prefilter.skipped")
        .collect::<Vec<_>>();
    assert_eq!(prefilter.len(), 1, "{result}");
    assert!(
        prefilter[0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("3 file(s)")),
        "{result}"
    );
    assert_eq!(result["status"], "empty", "{result}");
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
    let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
    let files = execute_ast(
        json!({"operation":"files","path":root.0,"detail":"full","entryType":"f"}),
        &paths,
        &security,
        &Active,
    )
    .expect("files");
    assert_eq!(files["pagination"]["totalFiles"], 0);
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
    let paths = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
    let error = execute_ast(
        json!({"operation":"files","path":root.0}),
        &paths,
        &security,
        &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
    )
    .expect_err("cancel during walk");
    assert_eq!(error.code, "ast.execution.cancelled");
    let error = execute_ast(
        json!({
            "operation":"match","path":root.0,"langType":"rust",
            "pattern":"source"
        }),
        &paths,
        &security,
        &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
    )
    .expect_err("cancel structural walk");
    assert_eq!(error.code, "ast.execution.cancelled");
}

// Regression: astSearch `match` over a directory must not silently drop files
// past `maxFiles`. When the candidate scan is truncated, the result must carry
// an explicit truncation signal (a `structural.scan.truncated` diagnostic and a
// top-level `truncated`/`complete:false`) rather than reporting a bounded set as
// if it were the whole corpus.
#[test]
fn match_directory_scan_truncation_is_surfaced_not_silent() {
    let root = Fixture::new();
    // Three candidate files, each with the matchable identifier `source`.
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let paths = PathPolicy::with_registry(
        PathPolicyConfig {
            workspace_root: Some(root.0.clone()),
            ..Default::default()
        },
        &SecurityRegistry::default(),
    )
    .expect("policy");
    let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
    let out = execute_ast(
        json!({
            "operation":"match","path":root.0,"langType":"rust",
            "pattern":"pub fn source() {}","maxFiles":1
        }),
        &paths,
        &security,
        &Active,
    )
    .expect("match runs");

    assert_eq!(
        out["truncated"],
        json!(true),
        "capped scan must set truncated=true; got {out}"
    );
    assert_eq!(
        out["complete"],
        json!(false),
        "capped scan is not complete; got {out}"
    );
    let diagnostics = out["diagnostics"].as_array().cloned().unwrap_or_default();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == json!("structural.scan.truncated")),
        "expected a structural.scan.truncated diagnostic; got {out}"
    );
}

fn simple_policy(root: &std::path::Path) -> (PathPolicy, ContentSecurity) {
    let paths = PathPolicy::with_registry(
        PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        },
        &SecurityRegistry::default(),
    )
    .expect("policy");
    let security = ContentSecurity::new(Arc::new(SecurityRegistry::default()));
    (paths, security)
}

// Regression: `files` mode default sort must be deterministic. The scan
// primitive never provides mtimes, so a `modified` default ties every row at
// 0.0 and pages fall back to OS readdir order. The default order must instead
// be lexicographic by path and stable across identical invocations.
#[test]
fn files_default_sort_is_lexicographic_and_stable() {
    let root = Fixture::new();
    for name in ["zebra.rs", "apple.rs", "mango.rs", "banana.rs"] {
        std::fs::write(root.0.join(name), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let run = || {
        let out = execute_ast(
            json!({"operation":"files","path":root.0,"entryType":"f"}),
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
    assert_eq!(first, sorted, "default order must be lexicographic by path");
    let second = run();
    assert_eq!(first, second, "default order must be stable across runs");
}

#[test]
fn files_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for i in 0..6 {
        std::fs::write(root.0.join(format!("f{i}.rs")), "x\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_ast(
        json!({"operation":"files","path":root.0,"entryType":"f","pageSize":2}),
        &paths,
        &security,
        &Active,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    // Happy path: the freshly emitted snapshot must be accepted on page 2.
    let good = execute_ast(
        json!({"operation":"files","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
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
    let page2 = execute_ast(
        json!({"operation":"files","path":root.0,"entryType":"f","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &Active,
    )
    .expect("page2");
    assert_eq!(
        page2["errorCode"],
        json!("ast.snapshot.changed"),
        "got {page2}"
    );
}

#[test]
fn match_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("file");
    }
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_ast(
        json!({"operation":"match","path":root.0,"langType":"rust","pattern":"pub fn source() {}","pageSize":2}),
        &paths,
        &security,
        &Active,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    std::fs::write(root.0.join("e.rs"), "pub fn source() {}\n").expect("mutate corpus");
    let page2 = execute_ast(
        json!({"operation":"match","path":root.0,"langType":"rust","pattern":"pub fn source() {}","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &Active,
    )
    .expect("page2");
    assert_eq!(
        page2["errorCode"],
        json!("ast.snapshot.changed"),
        "got {page2}"
    );
}
