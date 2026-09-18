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
    let tree = execute_ast(json!({"operation":"tree","treeKind":"filesystem","path":root.0,"hidden":true,"maxDepth":10,"detail":"full"}), &paths, &security, &Active).expect("tree");
    assert_eq!(tree["entries"].as_array().expect("entries").len(), 1);
    assert_eq!(tree["summary"], "1 entries (1 files, 0 dirs, 21.0B)");
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
    let tree = execute_ast(
        json!({"operation":"tree","path":root.0,"detail":"full"}),
        &paths,
        &security,
        &Active,
    )
    .expect("tree");
    assert_eq!(tree["entries"], json!([]));
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
    for operation in ["files", "tree"] {
        let error = execute_ast(
            json!({"operation":operation,"path":root.0}),
            &paths,
            &security,
            &AfterRoot(std::sync::atomic::AtomicUsize::new(0)),
        )
        .expect_err("cancel during walk");
        assert_eq!(error.code, "ast.execution.cancelled");
    }
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
