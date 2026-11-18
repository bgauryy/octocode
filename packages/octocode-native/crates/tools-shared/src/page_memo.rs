//! A store of results kept in memory, keyed by what proves them current: a
//! continuation's snapshot and the path policy that produced it, or the
//! digest of the content they were computed from (outlines, declarations,
//! mentions), which a fresh request may reuse. A value lives as issued
//! continuation state does ([`octocode_runtime_base::cache::CONTINUATION_RETENTION`]): it
//! expires [`CONTINUATION_IDLE`] after its last read. A hit is only a
//! candidate: each owner re-validates what it reads before serving a page.
use octocode_engine::stamp::{Link, Proof, exists};
use octocode_io::memo::Memo;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use octocode_runtime_base::cache::CONTINUATION_IDLE;

/// Values by snapshot, each stored with the policy that produced it.
pub struct PageMemo<V> {
    memo: Memo<String, (String, V)>,
}

impl<V: Send + Sync + 'static> Default for PageMemo<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Send + Sync + 'static> PageMemo<V> {
    /// A store drawing on the one process budget ([`octocode_engine::memory::MEMORY`]),
    /// the least recently used of every store going first; each value
    /// expires [`CONTINUATION_IDLE`] after it was last read.
    pub const fn new() -> Self {
        Self {
            memo: Memo::new(&octocode_engine::memory::MEMORY),
        }
    }

    /// `read` of the unexpired value stored for `snapshot` under `policy`;
    /// the read restarts its lifetime, so a cursor being paged stays served.
    pub fn get<T>(&self, snapshot: &str, policy: &str, read: impl FnOnce(&V) -> T) -> Option<T> {
        let hit = self.memo.get(snapshot)?;
        let (stored_policy, value) = hit.value.as_ref();
        if stored_policy != policy {
            return None;
        }
        self.memo.touch(snapshot, CONTINUATION_IDLE);
        Some(read(value))
    }

    /// The hit stored for `snapshot` under `policy` while it is current:
    /// `read` copies the stored value's parts and its [`Proof`] out of the
    /// store, and `serve` turns the parts into the hit once the proof holds.
    /// A value whose proof fails, or that `serve` refuses, is evicted.
    pub fn get_current<R, T>(
        &self,
        snapshot: &str,
        policy: &str,
        read: impl FnOnce(&V) -> (R, Proof),
        serve: impl FnOnce(R) -> Option<T>,
    ) -> Option<T> {
        let (parts, proof) = self.get(snapshot, policy, read)?;
        let hit = proof.holds().then(|| serve(parts)).flatten();
        if hit.is_none() {
            self.evict(snapshot);
        }
        hit
    }

    /// Store `value`, about `bytes` large, replacing any value for the same
    /// snapshot; expired values go first, then the least recently used of
    /// every store until the budget holds. A value over the whole budget is
    /// not kept. The key and policy count toward its weight.
    pub fn put(&self, snapshot: String, policy: String, bytes: usize, value: V) {
        let weight = bytes + snapshot.len() + policy.len();
        self.memo
            .insert(snapshot, (policy, value), weight, Some(CONTINUATION_IDLE));
    }

    /// Charge `bytes` more to the value stored for `snapshot`, for what a later
    /// page learned of it; false (nothing charged) when the budget cannot hold
    /// them without reclaiming, or nothing is stored ([`Memo::charge`]).
    pub fn charge(&self, snapshot: &str, bytes: usize) -> bool {
        self.memo.charge(snapshot, bytes)
    }

    /// Drop the value stored for `snapshot`, as expiry or eviction would.
    pub fn evict(&self, snapshot: &str) {
        self.memo.remove(snapshot);
    }

    /// Stored snapshots, least recently used first, and their total weight.
    #[cfg(any(test, feature = "test-support"))]
    pub fn contents(&self) -> (Vec<String>, usize) {
        self.memo.contents()
    }

    /// Drop every value stored under `policy`, as eviction would. Tests share
    /// the process's stores: one forgets what its own calls kept (a workspace
    /// root of its own is a policy of its own), and what the tests beside it
    /// kept stays.
    #[cfg(any(test, feature = "test-support"))]
    pub fn evict_policy(&self, policy: &str) {
        for snapshot in self.memo.contents().0 {
            if self
                .memo
                .get(&snapshot)
                .is_some_and(|hit| hit.value.0 == policy)
            {
                self.memo.remove(&snapshot);
            }
        }
    }
}

/// A stored value and what proves it current.
pub struct Proven<T: ?Sized> {
    proof: Proof,
    value: Arc<T>,
}

impl<T: ?Sized + Send + Sync + 'static> PageMemo<Proven<T>> {
    /// The value stored for `snapshot` under `policy` while its proof holds;
    /// one whose proof fails is evicted.
    pub fn current(&self, snapshot: &str, policy: &str) -> Option<Arc<T>> {
        self.get_current(
            snapshot,
            policy,
            |stored| (Arc::clone(&stored.value), stored.proof.clone()),
            Some,
        )
    }

    /// Store `value`, about `bytes` large, with what proves it current.
    pub fn put_proven(
        &self,
        snapshot: String,
        policy: String,
        bytes: usize,
        proof: Proof,
        value: Arc<T>,
    ) {
        self.put(snapshot, policy, bytes, Proven { proof, value });
    }
}

/// What a stored result's freshness depends on.
pub struct Watch<'a> {
    /// The scope root: stamped, with every directory holding a source.
    pub root: &'a Path,
    /// Entries the scan read or walked: each keeps its stamp.
    pub sources: &'a [PathBuf],
    /// Files whose contents decide the walk's ignore rules: a present one
    /// keeps its stamp, an absent one stays absent.
    pub rules: &'a [PathBuf],
    /// Entries whose existence alone decides the rules (repository markers).
    pub markers: &'a [PathBuf],
}

impl Watch<'_> {
    /// The proof of this watch: the root, every source and each directory
    /// under the root holding one, and each present rule file stamped
    /// (`link` decides whether a link is stamped as itself or its target);
    /// each absent rule file and each marker by presence. `None` when a
    /// stamped entry is gone, or was written within the settle window
    /// before `settled_before`.
    pub fn capture(&self, link: Link, settled_before: Option<SystemTime>) -> Option<Proof> {
        let root = self.root;
        // Deduplicated borrowed, so each shared parent is copied once.
        let mut paths = std::collections::BTreeSet::<&Path>::new();
        paths.insert(root);
        for source in self.sources {
            if let Some(parent) = source.parent()
                && parent.starts_with(root)
            {
                paths.insert(parent);
            }
            paths.insert(source);
        }
        let mut presence = self
            .markers
            .iter()
            .map(|marker| (marker.clone(), exists(marker)))
            .collect::<Vec<_>>();
        for rule in self.rules {
            if exists(rule) {
                paths.insert(rule);
            } else {
                presence.push((rule.clone(), false));
            }
        }
        Some(
            Proof::capture(
                paths.into_iter().map(Path::to_path_buf),
                link,
                settled_before,
            )?
            .with_presence(presence),
        )
    }
}

/// What a walked directory adds to the ignore inputs a stored walk watches.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DirectoryInputs {
    /// Its present ignore files (an absent one shows as a change of the
    /// directory's own stamp when it is created).
    pub files: Vec<PathBuf>,
    /// A nested repository's rule files outside the walk (its own
    /// `info/exclude`), which may not exist.
    pub external: Vec<PathBuf>,
    /// An ignore file no stamp can watch: a link, another non-file entry,
    /// an unreadable one, or one `permits` withholds. A walk that depends
    /// on it is not stored.
    pub untracked: bool,
}

/// The ignore inputs of `dir`, a directory a walk of `root` entered: its
/// ignore files, each a regular file `permits` admits, and when `dir` is a
/// repository nested below `root`, the rule files outside the walk its
/// rules read (the root's own are [`external_ignore_inputs`] of the root).
///
/// [`external_ignore_inputs`]: octocode_engine::search::walk::external_ignore_inputs
pub fn directory_ignore_inputs_with(
    dir: &Path,
    root: &Path,
    global: octocode_engine::search::walk::GlobalIgnore,
    permits: impl Fn(&Path) -> bool,
) -> DirectoryInputs {
    let mut inputs = DirectoryInputs::default();
    if dir != root && dir.join(".git").exists() {
        inputs.external = octocode_engine::search::walk::external_ignore_inputs_with(dir, global);
    }
    for name in octocode_engine::search::walk::IGNORE_FILES {
        let ignore = dir.join(name);
        match std::fs::symlink_metadata(&ignore) {
            Ok(metadata) if metadata.is_file() && permits(&ignore) => inputs.files.push(ignore),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => inputs.untracked = true,
        }
    }
    inputs
}

/// The repository markers at and above `root` whose presence decides
/// whether a `.gitignore` applies ([`octocode_engine::search::repository`]).
pub fn repository_markers(root: &Path) -> Vec<PathBuf> {
    root.ancestors()
        .flat_map(|dir| octocode_engine::search::repository::MARKERS.map(|marker| dir.join(marker)))
        .collect()
}

/// A budget of `$bytes` of its own, for a test that must fill one: tests run
/// in parallel, so each owns its budget instead of sharing the process one.
#[cfg(any(test, feature = "test-support"))]
#[macro_export]
macro_rules! test_budget {
    ($bytes:expr) => {{
        fn limit() -> usize {
            $bytes
        }
        static BUDGET: octocode_io::memo::MemoryBudget =
            octocode_io::memo::MemoryBudget::new(limit);
        &BUDGET
    }};
}
#[cfg(any(test, feature = "test-support"))]
pub use test_budget;

/// A new budget of `bytes` on each call, for a test that searches for the
/// least budget some work fits in ([`test_budget!`] fixes its bytes at
/// compile time). A budget reads its limit once, on first use, so each one is
/// built and read under one lock.
#[cfg(test)]
pub(crate) fn sized_test_budget(bytes: usize) -> &'static octocode_io::memo::MemoryBudget {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LIMIT: AtomicUsize = AtomicUsize::new(0);
    static BUILDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn limit() -> usize {
        LIMIT.load(Ordering::SeqCst)
    }
    let _building = BUILDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    LIMIT.store(bytes, Ordering::SeqCst);
    let budget: &'static octocode_io::memo::MemoryBudget =
        Box::leak(Box::new(octocode_io::memo::MemoryBudget::new(limit)));
    assert_eq!(budget.limit(), bytes);
    budget
}

#[cfg(test)]
mod budget_tests;

#[cfg(test)]
mod tests;
