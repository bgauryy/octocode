//! Fixtures shared by the tools' tests.
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::security::ContentSecurity;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

// Contract, lead and page-walk checks; the integration tests include the same
// file (tests/support), so both kinds of test assert one way.
use crate::contracts::{parsed_contract, validate, validate_output};
pub(crate) mod checks;
pub(crate) mod selection_fake;

/// A fresh temporary directory, removed (with everything under it) on drop.
/// `.0` is its path.
pub(crate) struct Fixture(
    pub PathBuf,
    #[expect(dead_code, reason = "held only to remove the directory on drop")] tempfile::TempDir,
);

impl Fixture {
    pub(crate) fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("octocode-tool-test-")
            .tempdir()
            .expect("fixture");
        Self(dir.path().to_path_buf(), dir)
    }

    /// Write `contents` to `relative` under the root, creating its parents.
    pub(crate) fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parents");
        }
        std::fs::write(&path, contents).expect("fixture write");
        path
    }
}

/// Creates a FIFO (named pipe) at `path`, owner read/write only.
#[cfg(unix)]
pub(crate) fn make_fifo(path: &Path) {
    let name = std::ffi::CString::new(path.to_string_lossy().as_bytes()).expect("path has no NUL");
    // SAFETY: `name` is a valid NUL-terminated C string that outlives the call.
    let made = unsafe { libc::mkfifo(name.as_ptr(), 0o600) };
    assert_eq!(made, 0, "mkfifo {}", path.display());
}

/// Whether `node` runs on this machine.
#[cfg(unix)]
pub(crate) fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Gate for a test that needs `node`: `true` when it can run. Without node a
/// local run skips the test; under `CI` it fails instead, so a runner that
/// lacks node cannot silently lose the coverage.
#[cfg(unix)]
pub(crate) fn require_node() -> bool {
    if node_available() {
        return true;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "node is required to run this test in CI"
    );
    eprintln!("skipping: node not available");
    false
}

/// A tool's raw executor `row` for `query`, shaped and finalized as the
/// response stage ships it: `{results:[row]}` with its public `next`.
pub(crate) fn published(tool: ToolId, query: &Value, row: &Value) -> Value {
    use crate::response::continuations::{Scope, Sources, finalize};
    let status = row["status"].as_str();
    let shaped = crate::response::rows::result_row(tool, 0, query, row.clone(), status);
    let mut public = serde_json::json!({ "results": [shaped] });
    finalize(&mut public, tool, &Sources::Rows(&[]), &Scope::everything())
        .expect("valid continuations");
    crate::response::channels::publish_hints(&mut public, tool);
    public
}

/// A tool's raw executor `row` for `query`, as [`published`], satisfies the
/// tool's output contract.
pub(crate) fn assert_row_contract(tool: ToolId, query: &Value, row: &Value) {
    checks::assert_output_contract(tool.as_str(), &published(tool, query, row));
}

/// The path policy of a workspace rooted at `root`.
pub(crate) fn workspace_policy(root: &Path) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("path policy")
}

/// [`workspace_policy`] with default content security.
pub(crate) fn simple_policy(root: &Path) -> (PathPolicy, ContentSecurity) {
    (workspace_policy(root), ContentSecurity::new())
}

/// A workspace with a credentials directory, a built-in sensitive file name
/// (ignored by the path policy, not by config) and one visible source file.
pub(crate) fn sensitive_fixture() -> Fixture {
    let root = Fixture::new();
    root.write(".aws/credentials", "hidden\n".repeat(50));
    root.write("terraform.tfstate", "ignored\n".repeat(70));
    root.write("visible.rs", "pub fn visible() {\n}\n");
    root
}

/// Cancels from its third check on: the root is admitted, its descendants
/// are not.
pub(crate) struct AfterRoot(pub AtomicUsize);

impl CancellationCheck for AfterRoot {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_add(1, Ordering::Relaxed) >= 2 {
            Err("stop traversal".into())
        } else {
            Ok(())
        }
    }
}

include!("test_support/production_source.rs");

/// What [`production_text`] keeps and blanks.
mod production_text_tests {
    use super::production_text;

    #[test]
    fn test_items_are_blanked_and_production_after_them_stays() {
        let source = "fn a() {}\n\
            #[cfg(test)]\n\
            fn only_in_tests() { let x = \"}\"; }\n\
            fn b() { let _ = '\\''; let _ = '{'; }\n\
            #[cfg(test)]\n\
            use helper::{x, y};\n\
            #[cfg(test)]\n\
            impl<A, B> Probe for Pair<A, B> where A: Clone, B: Copy { fn p() {} }\n\
            #[cfg(all(test, unix))]\n\
            mod unix_tests { fn t() { r#\"} {\"#; } }\n\
            fn c<'a>(x: &'a str) {\n    #[cfg(test)]\n    { note(1); }\n    #[cfg(test)]\n    note(2);\n    used();\n}\n\
            #[cfg(test)]\n\
            mod tests;\n\
            #[cfg(not(test))]\n\
            fn real() {}\n\
            #[cfg(test)]\n\
            thread_local! { static T: u8 = 0; }\n\
            fn d() {}\n\
            #[cfg(test)]\n\
            mod tests {\n    fn e() {}\n}\n\
            fn after_tests() {}\n";
        let text = production_text(source);
        assert_eq!(text.lines().count(), source.lines().count(), "lines kept");
        for kept in [
            "fn a() {}",
            "fn b() { let _ = '\\''; let _ = '{'; }",
            "fn c<'a>",
            "used();",
            "#[cfg(not(test))]",
            "fn real() {}",
            "fn d() {}",
            "fn after_tests() {}",
        ] {
            assert!(text.contains(kept), "{kept} kept:\n{text}");
        }
        for blanked in [
            "only_in_tests",
            "helper",
            "Probe",
            "unix_tests",
            "note(",
            "mod tests",
            "thread_local",
            "fn e()",
            "cfg(test)",
            "all(test",
        ] {
            assert!(!text.contains(blanked), "{blanked} blanked:\n{text}");
        }
        // A blanked item keeps every later byte on its line.
        let line = |text: &str, needle: &str| text.lines().position(|line| line.contains(needle));
        assert_eq!(
            line(&text, "fn after_tests"),
            line(source, "fn after_tests")
        );
    }

    #[test]
    fn attributes_in_strings_and_comments_gate_nothing() {
        let source = "// #[cfg(test)]\nfn a() {}\nconst S: &str = \"#[cfg(test)]\";\nfn b() {}\n";
        assert_eq!(production_text(source), source);
        let whole = "#![cfg(test)]\nfn a() {}\n";
        assert_eq!(production_text(whole).trim(), "");
    }
}
