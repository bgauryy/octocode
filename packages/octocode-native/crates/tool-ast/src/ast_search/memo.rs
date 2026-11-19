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
use octocode_engine::stamp::Link;
#[cfg(test)]
use octocode_engine::stamp::SourceStamp;
use octocode_tools_shared::page_memo::{PageMemo, Proven, Watch};
#[cfg(test)]
use std::path::{Path, PathBuf};
use std::{sync::Arc, time::SystemTime};

/// One store per scan type (the executor keeps one), weighed in estimated
/// bytes against the one process budget ([`octocode_engine::memory::MEMORY`]). One scan
/// may use all of it, since a scan that is not stored costs a whole scan per
/// page, which outweighs the entries it would displace.
pub(super) struct ScanMemo<T> {
    store: PageMemo<Proven<T>>,
}

impl<T: Send + Sync + 'static> ScanMemo<T> {
    pub(super) const fn new() -> Self {
        Self {
            store: PageMemo::new(),
        }
    }

    /// The scan stored for `snapshot` under `key` while every stamped entry
    /// is unchanged; a changed one is evicted.
    pub(super) fn get(&self, snapshot: &str, key: &str) -> Option<Arc<T>> {
        self.store.current(snapshot, key)
    }

    /// Keep `scan` for the pages after the first, stamped by what it
    /// [`Watch`]es: the source and discovery paths it observed and their
    /// directories, the ignore rules outside the walk, and the repository
    /// markers' presence. Not stored when it alone outweighs the store, or when any
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
        let Some(proof) = watch.capture(Link::Follow, Some(started)) else {
            return;
        };
        self.store.put_proven(snapshot, key, bytes, proof, scan);
    }

    pub(super) fn evict(&self, snapshot: &str) {
        self.store.evict(snapshot);
    }

    /// This request's turn at computing what is stored under `snapshot`
    /// ([`PageMemo::turn`]).
    pub(super) fn turn(
        &self,
        snapshot: &str,
        stop: &dyn Fn() -> bool,
    ) -> octocode_tools_shared::page_memo::Turn<'_> {
        self.store.turn(snapshot, stop)
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
mod tests;
