//! A short-lived store of results kept for continuation pages, keyed by the
//! result's snapshot and the path policy that produced it. A hit is only a
//! candidate: each owner re-validates what it reads before serving a page.
use crate::tools::source::{Link, StampSet};
use octocode_io::memo::{Memo, MemoLimits};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

/// Values by snapshot, each stored with the policy that produced it.
pub(crate) struct PageMemo<V> {
    memo: Memo<String, (String, V)>,
    ttl: Duration,
}

impl<V: Send + Sync + 'static> PageMemo<V> {
    /// A store that keeps a value for `ttl`, at most `max_entries` values,
    /// and at most `max_weight` total weight; the least recently used go
    /// first.
    pub(crate) const fn new(ttl: Duration, max_entries: usize, max_weight: usize) -> Self {
        Self {
            memo: Memo::new(MemoLimits {
                max_entries,
                max_weight,
                keep_newest: false,
            }),
            ttl,
        }
    }

    /// `read` of the unexpired value stored for `snapshot` under `policy`.
    pub(crate) fn get<T>(
        &self,
        snapshot: &str,
        policy: &str,
        read: impl FnOnce(&V) -> T,
    ) -> Option<T> {
        let hit = self.memo.get(snapshot)?;
        let (stored_policy, value) = hit.value.as_ref();
        (stored_policy == policy).then(|| read(value))
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

    /// Store `value`, replacing any value for the same snapshot and dropping
    /// expired values, then the least recently used beyond the entry and
    /// weight budgets. A value over the weight budget is not kept.
    pub(crate) fn put(&self, snapshot: String, policy: String, weight: usize, value: V) {
        self.memo
            .insert(snapshot, (policy, value), weight, Some(self.ttl));
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

/// What proves a stored result current: the stamps of what its [`Watch`]
/// names, and whether each path named by existence alone still exists as
/// it did.
#[derive(Clone)]
pub(crate) struct Proof {
    stamps: StampSet,
    presence: Arc<[(PathBuf, bool)]>,
}

impl Proof {
    /// Nothing the proof names changed.
    pub(crate) fn holds(&self) -> bool {
        self.stamps.unchanged() && self.presence.iter().all(|(path, was)| exists(path) == *was)
    }
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
        Some(Proof {
            stamps: StampSet::capture(
                paths.into_iter().map(Path::to_path_buf),
                link,
                settled_before,
            )?,
            presence: presence.into(),
        })
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

/// Whether `path` exists (a dangling link counts: its name is taken).
fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn the_oldest_values_go_first_to_keep_both_budgets() {
        let memo = PageMemo::new(Duration::from_secs(60), 3, 10);
        for tag in 0..5 {
            memo.put(format!("s{tag}"), "p".into(), 3, ());
        }
        assert_eq!(
            memo.contents(),
            (vec!["s2".into(), "s3".into(), "s4".into()], 9)
        );
        memo.put("big".into(), "p".into(), 8, ());
        assert_eq!(memo.contents(), (vec!["big".into()], 8));
        // A value over the weight budget is not kept, and evicts nothing.
        memo.put("huge".into(), "p".into(), 11, ());
        assert_eq!(memo.contents(), (vec!["big".into()], 8));
    }

    #[test]
    fn a_value_is_served_under_its_own_policy_until_replaced_or_evicted() {
        let memo = PageMemo::new(Duration::from_secs(60), 4, 100);
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
        let memo = PageMemo::new(Duration::from_secs(60), 4, 100);
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

    #[test]
    fn an_expired_value_is_not_served() {
        let memo = PageMemo::new(Duration::ZERO, 4, 100);
        memo.put("s".into(), "a".into(), 1, ());
        assert_eq!(memo.get("s", "a", |()| ()), None);
    }
}
