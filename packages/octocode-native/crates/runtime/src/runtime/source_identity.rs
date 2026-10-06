//! Whether a stored page's sources are still what they were when it ran, so
//! its replay equals a re-execution. A source that cannot be proven unchanged
//! is never replayed: the call re-executes, exactly as a fresh runtime does.
use crate::policy::path::PathPolicy;
use crate::tools::id::{ToolFamily, ToolId};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

/// Entries one check may stat; a larger source re-executes every page.
const MAX_ENTRIES: usize = 20_000;
/// An entry modified this close to the original call may have changed while
/// it ran (or within the file system's timestamp granularity).
const SETTLE: Duration = Duration::from_secs(2);

/// True when the sources `queries` read provably did not change since the
/// call that started at `started`. Local rows: no entry under their `path`
/// (the root included; symlinks unfollowed) was modified, created, renamed,
/// or removed since `started` minus [`SETTLE`]: a creation, rename, or
/// removal touches its directory. GitHub rows: each pins a commit SHA, which
/// never moves. Other sources: never.
pub(super) fn unchanged_since(
    tool: ToolId,
    queries: &[Value],
    paths: &PathPolicy,
    started: SystemTime,
) -> bool {
    match tool.family() {
        ToolFamily::Local => {
            let settled = started
                .checked_sub(SETTLE)
                .unwrap_or(SystemTime::UNIX_EPOCH);
            queries.iter().all(|row| {
                let root = row["path"].as_str().unwrap_or(".");
                settled_tree(&paths.expand_and_resolve(Path::new(root)), settled)
            })
        }
        ToolFamily::GitHub => queries
            .iter()
            .all(|row| row["ref"].as_str().is_some_and(is_commit_sha)),
        ToolFamily::Remote => false,
    }
}

fn is_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Every entry under `root` exists, is readable, and was last modified
/// before `settled`, within [`MAX_ENTRIES`].
fn settled_tree(root: &Path, settled: SystemTime) -> bool {
    let fresh = AtomicBool::new(true);
    let count = AtomicUsize::new(0);
    ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .follow_links(false)
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                let settled_entry = count.fetch_add(1, Ordering::Relaxed) < MAX_ENTRIES
                    && entry
                        .ok()
                        .and_then(|entry| entry.metadata().ok())
                        .and_then(|metadata| metadata.modified().ok())
                        .is_some_and(|modified| modified < settled);
                if settled_entry {
                    ignore::WalkState::Continue
                } else {
                    fresh.store(false, Ordering::Relaxed);
                    ignore::WalkState::Quit
                }
            })
        });
    fresh.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_support::workspace_policy as policy;

    fn age(path: &Path) {
        let then = SystemTime::now() - Duration::from_secs(10);
        std::fs::File::open(path)
            .unwrap()
            .set_modified(then)
            .unwrap();
    }

    #[test]
    fn local_sources_are_unchanged_until_any_entry_moves() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let src = root.join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "one").unwrap();
        let paths = policy(&root);
        let rows = [serde_json::json!({"path": "src"})];
        let check = || unchanged_since(ToolId::LocalSearch, &rows, &paths, SystemTime::now());
        assert!(!check(), "just written: not yet settled");
        age(&src.join("a.txt"));
        age(&src);
        assert!(check());
        std::fs::write(src.join("a.txt"), "two").unwrap();
        assert!(!check(), "edit");
        age(&src.join("a.txt"));
        assert!(check());
        std::fs::write(src.join("new.txt"), "").unwrap();
        std::fs::remove_file(src.join("new.txt")).unwrap();
        assert!(!check(), "a created and removed file touches its directory");
        age(&src);
        std::fs::remove_dir_all(&src).unwrap();
        assert!(!check(), "a missing root is never unchanged");
    }

    #[test]
    fn github_rows_are_unchanged_only_at_commit_shas() {
        let dir = tempfile::tempdir().unwrap();
        let paths = policy(dir.path());
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let check = |tool, rows: &[Value]| unchanged_since(tool, rows, &paths, SystemTime::now());
        assert!(check(
            ToolId::GhGetFileContent,
            &[serde_json::json!({"ref": sha})]
        ));
        assert!(!check(
            ToolId::GhGetFileContent,
            &[
                serde_json::json!({"ref": sha}),
                serde_json::json!({"ref": "main"})
            ]
        ));
        assert!(!check(ToolId::GhSearchCode, &[serde_json::json!({})]));
        assert!(!check(ToolId::ArtifactSearch, &[serde_json::json!({})]));
    }
}
