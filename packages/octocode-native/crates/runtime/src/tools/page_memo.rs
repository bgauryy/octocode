//! A store of results kept for continuation pages, keyed by the result's
//! snapshot and the path policy that produced it. A value lives as issued
//! continuation state does ([`crate::cache::CONTINUATION_RETENTION`]): it
//! expires [`CONTINUATION_IDLE`] after its last read. A hit is only a
//! candidate: each owner re-validates what it reads before serving a page.
use octocode_engine::stamp::{Link, Proof, exists};
use octocode_io::memo::Memo;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use crate::cache::CONTINUATION_IDLE;

/// Values by snapshot, each stored with the policy that produced it.
pub(crate) struct PageMemo<V> {
    memo: Memo<String, (String, V)>,
}

impl<V: Send + Sync + 'static> PageMemo<V> {
    /// A store drawing on the one process budget ([`octocode_engine::memory::MEMORY`]),
    /// the least recently used of every store going first; each value
    /// expires [`CONTINUATION_IDLE`] after it was last read.
    pub(crate) const fn new() -> Self {
        Self {
            memo: Memo::new(&octocode_engine::memory::MEMORY),
        }
    }

    /// `read` of the unexpired value stored for `snapshot` under `policy`;
    /// the read restarts its lifetime, so a cursor being paged stays served.
    pub(crate) fn get<T>(
        &self,
        snapshot: &str,
        policy: &str,
        read: impl FnOnce(&V) -> T,
    ) -> Option<T> {
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
    pub(crate) fn get_current<R, T>(
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
    pub(crate) fn put(&self, snapshot: String, policy: String, bytes: usize, value: V) {
        let weight = bytes + snapshot.len() + policy.len();
        self.memo
            .insert(snapshot, (policy, value), weight, Some(CONTINUATION_IDLE));
    }

    /// Drop the value stored for `snapshot`, as expiry or eviction would.
    pub(crate) fn evict(&self, snapshot: &str) {
        self.memo.remove(snapshot);
    }

    /// Stored snapshots, least recently used first, and their total weight.
    #[cfg(test)]
    pub(crate) fn contents(&self) -> (Vec<String>, usize) {
        self.memo.contents()
    }
}

/// A stored value and what proves it current.
pub(crate) struct Proven<T: ?Sized> {
    proof: Proof,
    value: Arc<T>,
}

impl<T: ?Sized + Send + Sync + 'static> PageMemo<Proven<T>> {
    /// The value stored for `snapshot` under `policy` while its proof holds;
    /// one whose proof fails is evicted.
    pub(crate) fn current(&self, snapshot: &str, policy: &str) -> Option<Arc<T>> {
        self.get_current(
            snapshot,
            policy,
            |stored| (Arc::clone(&stored.value), stored.proof.clone()),
            Some,
        )
    }

    /// Store `value`, about `bytes` large, with what proves it current.
    pub(crate) fn put_proven(
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
pub(crate) struct Watch<'a> {
    /// The scope root: stamped, with every directory holding a source.
    pub(crate) root: &'a Path,
    /// Entries the scan read or walked: each keeps its stamp.
    pub(crate) sources: &'a [PathBuf],
    /// Files whose contents decide the walk's ignore rules: a present one
    /// keeps its stamp, an absent one stays absent.
    pub(crate) rules: &'a [PathBuf],
    /// Entries whose existence alone decides the rules (repository markers).
    pub(crate) markers: &'a [PathBuf],
}

impl Watch<'_> {
    /// The proof of this watch: the root, every source and each directory
    /// under the root holding one, and each present rule file stamped
    /// (`link` decides whether a link is stamped as itself or its target);
    /// each absent rule file and each marker by presence. `None` when a
    /// stamped entry is gone, or was written within the settle window
    /// before `settled_before`.
    pub(crate) fn capture(&self, link: Link, settled_before: Option<SystemTime>) -> Option<Proof> {
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
pub(crate) struct DirectoryInputs {
    /// Its present ignore files (an absent one shows as a change of the
    /// directory's own stamp when it is created).
    pub(crate) files: Vec<PathBuf>,
    /// A nested repository's rule files outside the walk (its own
    /// `info/exclude`), which may not exist.
    pub(crate) external: Vec<PathBuf>,
    /// An ignore file no stamp can watch: a link, another non-file entry,
    /// an unreadable one, or one `permits` withholds. A walk that depends
    /// on it is not stored.
    pub(crate) untracked: bool,
}

/// The ignore inputs of `dir`, a directory a walk of `root` entered: its
/// ignore files, each a regular file `permits` admits, and when `dir` is a
/// repository nested below `root`, the rule files outside the walk its
/// rules read (the root's own are [`external_ignore_inputs`] of the root).
///
/// [`external_ignore_inputs`]: octocode_engine::search::walk::external_ignore_inputs
pub(crate) fn directory_ignore_inputs(
    dir: &Path,
    root: &Path,
    permits: impl Fn(&Path) -> bool,
) -> DirectoryInputs {
    let mut inputs = DirectoryInputs::default();
    if dir != root && dir.join(".git").exists() {
        inputs.external = octocode_engine::search::walk::external_ignore_inputs(dir);
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
pub(crate) fn repository_markers(root: &Path) -> Vec<PathBuf> {
    root.ancestors()
        .flat_map(|dir| octocode_engine::search::repository::MARKERS.map(|marker| dir.join(marker)))
        .collect()
}

#[cfg(test)]
mod budget_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A walked directory's ignore inputs: its present regular ignore files,
    /// a nested repository's rule files outside the walk (never the root's:
    /// those are the root's external inputs), and a link, a non-file or a
    /// withheld ignore file as a dependency no stamp watches.
    #[cfg(unix)]
    #[test]
    fn a_walked_directory_names_its_ignore_inputs() {
        let root = tempfile::tempdir().expect("fixture");
        let root = root.path();
        std::fs::create_dir(root.join(".git")).expect("root repository");
        std::fs::write(root.join(".gitignore"), "*.log\n").expect("fixture");
        let all = |_: &Path| true;
        let inputs = directory_ignore_inputs(root, root, all);
        assert_eq!(
            inputs,
            DirectoryInputs {
                files: vec![root.join(".gitignore")],
                ..DirectoryInputs::default()
            }
        );
        let nested = root.join("nested");
        std::fs::create_dir_all(nested.join(".git")).expect("nested repository");
        std::fs::write(nested.join(".ignore"), "tmp\n").expect("fixture");
        let inputs = directory_ignore_inputs(&nested, root, all);
        assert_eq!(inputs.files, vec![nested.join(".ignore")]);
        let real = std::fs::canonicalize(&nested).expect("real path");
        assert!(
            inputs
                .external
                .contains(&real.join(".git").join("info").join("exclude")),
            "{:?}",
            inputs.external
        );
        assert!(!inputs.untracked);
        assert!(directory_ignore_inputs(&nested, root, |_| false).untracked);
        std::os::unix::fs::symlink(nested.join(".ignore"), nested.join(".gitignore"))
            .expect("link");
        assert!(directory_ignore_inputs(&nested, root, all).untracked);
        std::fs::create_dir(root.join("dir")).expect("fixture");
        std::fs::create_dir(root.join("dir").join(".ignore")).expect("a directory");
        assert!(directory_ignore_inputs(&root.join("dir"), root, all).untracked);
    }

    /// A value over the whole budget is not kept, and evicts nothing
    /// (oldest-first eviction across stores: `octocode_io::memo` tests).
    #[test]
    fn a_value_over_the_whole_budget_is_not_kept() {
        let memo = PageMemo::new();
        memo.put("small".into(), "p".into(), 3, ());
        memo.put(
            "huge".into(),
            "p".into(),
            octocode_engine::memory::MEMORY.limit(),
            (),
        );
        assert_eq!(memo.contents(), (vec!["small".into()], 3 + 5 + 1));
    }

    #[test]
    fn a_value_is_served_under_its_own_policy_until_replaced_or_evicted() {
        let memo = PageMemo::new();
        memo.put("s".into(), "a".into(), 1, 1);
        assert_eq!(memo.get("s", "a", |v| *v), Some(1));
        assert_eq!(memo.get("s", "b", |v| *v), None);
        memo.put("s".into(), "a".into(), 1, 2);
        assert_eq!(memo.get("s", "a", |v| *v), Some(2));
        assert_eq!(memo.contents().0.len(), 1);
        memo.evict("s");
        assert_eq!(memo.get("s", "a", |v| *v), None);
    }

    #[test]
    fn a_value_is_served_while_its_proof_holds_and_evicted_when_refused() {
        let root = tempfile::tempdir().expect("fixture");
        let marker = root.path().join("marker");
        let proof = || {
            Watch {
                root: root.path(),
                sources: &[],
                rules: &[],
                markers: std::slice::from_ref(&marker),
            }
            .capture(Link::Follow, None)
            .expect("proof")
        };
        let memo = PageMemo::new();
        let get = |serve: fn(u8) -> Option<u8>| {
            memo.get_current(
                "s",
                "a",
                |(value, proof): &(u8, Proof)| (*value, proof.clone()),
                serve,
            )
        };
        memo.put("s".into(), "a".into(), 1, (7, proof()));
        assert_eq!(get(Some), Some(7));
        // `serve` refusing the parts evicts them.
        assert_eq!(get(|_| None), None);
        assert!(memo.contents().0.is_empty());
        // A changed watched entry fails the proof: not served, and evicted.
        memo.put("s".into(), "a".into(), 1, (7, proof()));
        std::fs::write(&marker, "").expect("marker");
        assert_eq!(get(Some), None);
        assert!(memo.contents().0.is_empty());
    }

    /// `value` stored under policy `a` as if put `age` ago.
    fn put_aged(memo: &PageMemo<()>, age: Duration) {
        let stored_at = std::time::Instant::now()
            .checked_sub(age)
            .expect("monotonic clock covers the age");
        memo.memo.insert_full(
            "s".into(),
            ("a".into(), ()),
            (),
            1,
            Some(CONTINUATION_IDLE),
            stored_at,
        );
    }

    #[test]
    fn a_read_restarts_the_continuation_lifetime() {
        let memo = PageMemo::new();
        put_aged(&memo, CONTINUATION_IDLE - Duration::from_secs(5));
        assert_eq!(memo.get("s", "a", |()| ()), Some(()));
        let age = memo.memo.get("s").expect("still stored").age;
        assert!(
            age < Duration::from_secs(5),
            "read restarted the lifetime: {age:?}"
        );
        // A read under another policy is no use of this value.
        put_aged(&memo, CONTINUATION_IDLE - Duration::from_secs(5));
        assert_eq!(memo.get("s", "b", |()| ()), None);
        let age = memo.memo.get("s").expect("still stored").age;
        assert!(age >= CONTINUATION_IDLE - Duration::from_secs(5), "{age:?}");
    }

    #[test]
    fn an_expired_value_is_not_served() {
        let memo = PageMemo::new();
        put_aged(&memo, CONTINUATION_IDLE - Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(memo.get("s", "a", |()| ()), None);
    }
}
