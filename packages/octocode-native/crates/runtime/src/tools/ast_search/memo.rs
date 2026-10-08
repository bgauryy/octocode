//! Structural scans kept for continuation pages, keyed by the snapshot the
//! first page returned and the inputs that shaped the scan. Page 2 onward
//! then pages the stored scan instead of walking, reading and parsing the
//! scope again.
//!
//! A scan is stored only when every file it evaluated, every directory
//! holding one, and the root were last written at least
//! [`SourceStamp::SETTLE`] before the scan started, so no write can race the
//! reads it was built from. A stored scan is served only while each of those
//! stamps (size, change times, inode) is unchanged: an edit, or an entry
//! created, removed or renamed beside a scanned file, drops it and the page
//! rescans. The executor recomputes the snapshot from the submitted query and
//! the stored files before serving a page. An entry created in a directory
//! that held no scanned file, or an edit of an ignore file, is not seen until
//! the scan expires.
use super::super::page_memo::PageMemo;
use octocode_engine::graph::SourceStamp;
use std::{
    path::{Path, PathBuf},
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
    /// Shared so a page checks them without copying every stored path.
    stamps: Arc<[(PathBuf, SourceStamp)]>,
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
        let (scan, stamps) = self.store.get(snapshot, key, |stored| {
            (Arc::clone(&stored.scan), Arc::clone(&stored.stamps))
        })?;
        let unchanged = stamps.iter().all(|(path, stamp)| {
            std::fs::metadata(path)
                .ok()
                .and_then(|meta| SourceStamp::of(&meta))
                .as_ref()
                == Some(stamp)
        });
        if unchanged {
            return Some(scan);
        }
        self.evict(snapshot);
        None
    }

    /// Keep `scan` of `root` for the pages after the first, stamped by the
    /// `sources` it evaluated and their directories. Not stored when it is
    /// over budget, or when any entry was written within the settle window
    /// before `started` (or is gone).
    pub(super) fn put(
        &self,
        (snapshot, key): (String, String),
        root: &Path,
        sources: &[PathBuf],
        started: SystemTime,
        scan: Arc<T>,
        bytes: usize,
    ) {
        if bytes > MAX_BYTES {
            return;
        }
        let mut paths = std::collections::BTreeSet::new();
        paths.insert(root.to_path_buf());
        for source in sources {
            if let Some(parent) = source.parent() {
                paths.insert(parent.to_path_buf());
            }
            paths.insert(source.clone());
        }
        let mut stamps = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(stamp) = std::fs::metadata(&path)
                .ok()
                .and_then(|meta| SourceStamp::settled(&meta, started))
            else {
                return;
            };
            stamps.push((path, stamp));
        }
        let stamps = stamps.into();
        self.store
            .put(snapshot, key, bytes, Stored { stamps, scan });
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
            root.path(),
            &sources,
            SystemTime::now(),
            Arc::new(1),
            1,
        );
        assert!(memo.get("s", "k").is_none(), "a racy scan is never stored");
        // A scan that started after the settle window is stored and served.
        let later = SystemTime::now() + SourceStamp::SETTLE + Duration::from_secs(1);
        memo.put(
            ("s".into(), "k".into()),
            root.path(),
            &sources,
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
                root,
                sources,
                started,
                Arc::new(3),
                MAX_BYTES + 1,
            );
            self.get("big", "k").is_none()
        }
    }
}
