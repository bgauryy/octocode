//! Fixtures shared by the local tools' tests.
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::security::ContentSecurity;
use crate::tools::cancel::CancellationCheck;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// A fresh directory under the system temp dir, removed on drop.
pub(crate) struct Fixture(pub PathBuf);

impl Fixture {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "octocode-tool-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
    std::fs::create_dir(root.0.join(".aws")).expect("sensitive directory");
    std::fs::write(root.0.join(".aws/credentials"), "hidden\n".repeat(50)).expect("secret");
    std::fs::write(root.0.join("terraform.tfstate"), "ignored\n".repeat(70)).expect("ignored");
    std::fs::write(root.0.join("visible.rs"), "pub fn visible() {\n}\n").expect("source");
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
