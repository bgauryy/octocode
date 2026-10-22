//! Structural scans kept for continuation pages, keyed by the snapshot the
//! first page returned and the inputs that shaped the scan. Page 2 onward
//! then pages the stored scan instead of walking, reading and parsing the
//! scope again.
//!
//! A scan is stored only when every file it evaluated, every directory
//! holding one, observed discovery directories and ignore files, and the root
//! were last written at least
//! [`SETTLE`] before the scan started, so no write can race the
//! reads it was built from. A stored scan is served only while each of those
//! stamps (size, change times, inode) is unchanged: an edit, or an entry
//! created, removed or renamed beside a scanned file, drops it and the page
//! rescans. The executor recomputes the snapshot from the submitted query and
//! the stored files before serving a page. Discovery callers include admitted
//! empty directories and their existing ignore files in watched metadata, so new
//! candidates and filter edits invalidate the memo without another walk.
//!
//! Ignore rules outside the walk (each ancestor's `.gitignore`/`.ignore`,
//! the repository's `info/exclude`, the global excludes file and the git
//! config files that name it, from
//! [`octocode_engine::search::walk::external_ignore_inputs`]) are watched
//! the same way: a present one keeps its stamp, an absent one stays absent;
//! a repository marker (`.git`, `.jj`) above the scope keeps its presence.
//!
//! [`SETTLE`]: octocode_engine::stamp::SourceStamp::SETTLE
use super::super::page_memo::{PageMemo, Proof, Watch};
use crate::tools::source::Link;
#[cfg(test)]
use octocode_engine::stamp::SourceStamp;
#[cfg(test)]
use std::path::{Path, PathBuf};
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

const MAX_ENTRIES: usize = 8;
/// Estimated bytes one stored scan may hold; a larger scan is not stored, so
/// its continuations rescan instead.
const MAX_BYTES: usize = 16 * 1024 * 1024;
/// Estimated bytes all stored scans may hold together; the oldest go first.
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(60);

struct Stored<T> {
    proof: Proof,
    scan: Arc<T>,
}

/// One store per scan type (the executor keeps one).
pub(super) struct ScanMemo<T> {
    store: PageMemo<Stored<T>>,
}

impl<T> ScanMemo<T> {
    pub(super) const fn new() -> Self {
        Self {
            store: PageMemo::new(TTL, MAX_ENTRIES, MAX_TOTAL_BYTES),
        }
    }

    /// The scan stored for `snapshot` under `key` while every stamped entry
    /// is unchanged; a changed one is evicted.
    pub(super) fn get(&self, snapshot: &str, key: &str) -> Option<Arc<T>> {
        self.store.get_current(
            snapshot,
            key,
            |stored| (Arc::clone(&stored.scan), stored.proof.clone()),
            Some,
        )
    }

    /// Keep `scan` for the pages after the first, stamped by what it
    /// [`Watch`]es: the source and discovery paths it observed and their
    /// directories, the ignore rules outside the walk, and the repository
    /// markers' presence. Not stored when it is over budget, or when any
    /// stamped entry was written within the settle window before `started`
    /// (or is gone).
    pub(super) fn put(
        &self,
        (snapshot, key): (String, String),
        watch: Watch<'_>,
        started: SystemTime,
        scan: Arc<T>,
        bytes: usize,
    ) {
        if bytes > MAX_BYTES {
            return;
        }
        let Some(proof) = watch.capture(Link::Follow, Some(started)) else {
            return;
        };
        self.store.put(snapshot, key, bytes, Stored { proof, scan });
    }

    pub(super) fn evict(&self, snapshot: &str) {
        self.store.evict(snapshot);
    }

    /// Whether a scan is stored for `snapshot`.
    #[cfg(test)]
    pub(super) fn holds(&self, snapshot: &str) -> bool {
        self.store
            .contents()
            .0
            .iter()
            .any(|stored| stored == snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A watch of `root` and `sources` alone.
    fn watch<'a>(root: &'a Path, sources: &'a [PathBuf]) -> Watch<'a> {
        Watch {
            root,
            sources,
            rules: &[],
            markers: &[],
        }
    }

    /// An absent rule file that appears, a present one that changes, and a
    /// marker that appears or goes each drop the stored scan.
    #[test]
    fn external_rules_and_markers_keep_their_presence_and_stamps() {
        let outside = tempfile::tempdir().expect("outside");
        let root = outside.path().join("scope");
        std::fs::create_dir(&root).expect("scope");
        let present = outside.path().join(".ignore");
        std::fs::write(&present, "x/\n").expect("rule");
        let absent = outside.path().join(".gitignore");
        let marker = outside.path().join(".git");
        let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
        let rules = [present.clone(), absent.clone()];
        let markers = [marker.clone()];
        let stored = |memo: &ScanMemo<u32>| {
            memo.put(
                ("s".into(), "k".into()),
                Watch {
                    root: &root,
                    sources: &[],
                    rules: &rules,
                    markers: &markers,
                },
                later,
                Arc::new(1),
                1,
            );
            assert_eq!(memo.get("s", "k").as_deref(), Some(&1));
        };
        let memo = ScanMemo::<u32>::new();
        stored(&memo);
        std::fs::write(&absent, "y/\n").expect("new rule file");
        assert!(memo.get("s", "k").is_none(), "an absent rule file appeared");
        std::fs::remove_file(&absent).expect("remove");
        stored(&memo);
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&present, "z/\n").expect("edit rule");
        assert!(memo.get("s", "k").is_none(), "a rule file changed");
        stored(&memo);
        std::fs::create_dir(&marker).expect("marker");
        assert!(memo.get("s", "k").is_none(), "a repository marker appeared");
    }

    #[test]
    fn discovery_root_does_not_watch_entries_outside_the_scope() {
        let outside = tempfile::tempdir().expect("outside");
        let root = outside.path().join("scope");
        std::fs::create_dir(&root).expect("scope");
        let memo = ScanMemo::<u32>::new();
        let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
        memo.put(
            ("s".into(), "k".into()),
            watch(&root, std::slice::from_ref(&root)),
            later,
            Arc::new(1),
            1,
        );
        assert_eq!(memo.get("s", "k").as_deref(), Some(&1));
        std::fs::write(outside.path().join("unrelated.ts"), "unrelated();").expect("outside edit");
        assert_eq!(memo.get("s", "k").as_deref(), Some(&1));
        std::fs::write(root.join("new.ts"), "target();").expect("inside edit");
        assert!(memo.get("s", "k").is_none());
    }

    #[test]
    fn a_stored_scan_is_served_until_a_stamped_entry_changes() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("a.ts");
        std::fs::write(&file, "console.log(1);").expect("file");
        let memo = ScanMemo::<u32>::new();
        let sources = [file.clone()];
        // Written just now: not settled before the scan started.
        memo.put(
            ("s".into(), "k".into()),
            watch(root.path(), &sources),
            SystemTime::now(),
            Arc::new(1),
            1,
        );
        assert!(memo.get("s", "k").is_none(), "a racy scan is never stored");
        // A scan that started after the settle window is stored and served.
        let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
        memo.put(
            ("s".into(), "k".into()),
            watch(root.path(), &sources),
            later,
            Arc::new(2),
            1,
        );
        assert_eq!(memo.get("s", "k").as_deref(), Some(&2));
        assert!(memo.get("s", "other inputs").is_none());
        assert!(
            memo.put_over_budget_is_skipped(root.path(), &sources, later),
            "an over-budget scan is not stored"
        );
        // A sibling created after the scan changes the directory's stamp.
        std::fs::write(root.path().join("b.ts"), "console.log(2);").expect("newcomer");
        assert!(memo.get("s", "k").is_none());
        // The changed entry was evicted, not only skipped.
        assert!(memo.store.contents().0.is_empty());
    }

    impl ScanMemo<u32> {
        fn put_over_budget_is_skipped(
            &self,
            root: &Path,
            sources: &[PathBuf],
            started: SystemTime,
        ) -> bool {
            self.put(
                ("big".into(), "k".into()),
                watch(root, sources),
                started,
                Arc::new(3),
                MAX_BYTES + 1,
            );
            self.get("big", "k").is_none()
        }
    }
}
