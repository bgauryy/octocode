//! The index walks and the workspace snapshot that keeps an index valid.
//!
//! The lifted walk stamps every directory it enters (an added, removed, or
//! renamed entry changes a directory's stamp) and the ignore files beside
//! them. An ignore file or an ignored directory that changed stales the
//! index. A tracked directory changes with every edit an editor saves by
//! rename, so one that changed is listed again instead: it stales the index
//! only when it now holds a directory the walk never entered or a family
//! file that ignore rules hide and the index does not list.
use super::IgnoredIndex;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::scope::canonical;
use octocode_engine::search::text_search::TextSearchPathFilter;
use octocode_engine::search::walk::{IGNORE_FILES, IgnoreRules, NestedRepositories, walk_builder};
use octocode_engine::types::IgnorePolicy;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// A path's stamp: modification time and length; `None` when absent.
type Stamp = Option<(Option<SystemTime>, u64)>;

fn stamp(path: &Path) -> Stamp {
    std::fs::metadata(path)
        .ok()
        .map(|meta| (meta.modified().ok(), meta.len()))
}

/// What a stamped path is to the snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `.gitignore`, `.ignore`, or `.git/info/exclude`.
    IgnoreFile,
    /// A directory only the lifted walk entered.
    IgnoredDir,
    /// A directory the honored walk entered too.
    TrackedDir,
}

/// The stamps an index was built under.
#[derive(Debug, Default)]
pub(super) struct Snapshot {
    root: PathBuf,
    extensions: Vec<String>,
    stamps: Vec<(PathBuf, Stamp, Kind)>,
    /// Every directory the lifted walk entered.
    dirs: HashSet<PathBuf>,
}

impl Snapshot {
    /// Whether `index` (built under this snapshot) still lists every
    /// ignored family file.
    pub(super) fn holds(&self, index: &IgnoredIndex, stopped: &(dyn Fn() -> bool + Sync)) -> bool {
        let mut rules = None;
        self.stamps.iter().all(|(path, stamped, kind)| {
            !stopped()
                && (stamp(path) == *stamped
                    || (*kind == Kind::TrackedDir && self.lists_again(path, index, &mut rules)))
        })
    }

    /// Whether a changed tracked directory holds nothing the index misses:
    /// no directory the walk never entered (outside the default prunes,
    /// dot directories, and nested repositories), and no family file that
    /// ignore rules hide and the index does not list.
    fn lists_again(
        &self,
        dir: &Path,
        index: &IgnoredIndex,
        rules: &mut Option<IgnoreRules>,
    ) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        let prune = crate::policy::prune::OwnWalk::new(&self.root, []);
        for entry in entries {
            let Ok(entry) = entry else { return false };
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            let Ok(kind) = entry.file_type() else {
                return false;
            };
            if kind.is_dir() {
                let skipped = hidden
                    || prune.prunes(&path, true)
                    || std::fs::symlink_metadata(path.join(".git")).is_ok();
                if !skipped && !self.dirs.contains(&path) {
                    return false;
                }
            } else if !hidden && family(&self.extensions, &path) {
                let listed = index
                    .files
                    .binary_search(&canonical(&path.to_string_lossy()))
                    .is_ok();
                let rules = rules.get_or_insert_with(|| IgnoreRules::new(&self.root));
                if !listed && rules.why(&path, false).is_some() {
                    return false;
                }
            }
        }
        true
    }
}

/// Whether `path` has one of the family `extensions` (any, when empty).
fn family(extensions: &[String], path: &Path) -> bool {
    extensions.is_empty()
        || path.extension().is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| extension.to_str() == Some(wanted))
        })
}

/// The family files of one walk of `root`, and the directories it entered
/// with their stamps and those of the ignore files beside them, taken as
/// the walk enters each directory (before it lists it).
struct Walked {
    files: BTreeSet<String>,
    dirs: Vec<(PathBuf, Stamp)>,
    ignore_files: Vec<(PathBuf, Stamp)>,
    complete: bool,
}

/// The bytes one kept path weighs in an index: its text and its entry.
fn weight_of(path: &Path) -> usize {
    path.as_os_str().len() + std::mem::size_of::<(PathBuf, Stamp, Kind)>()
}

fn walk_family(
    root: &Path,
    extensions: &[String],
    policy: &PathPolicy,
    ignores: IgnorePolicy,
    nested: Option<Arc<NestedRepositories>>,
    stopped: &(dyn Fn() -> bool + Sync),
    bytes: &std::cell::Cell<usize>,
    max_bytes: usize,
) -> Walked {
    let prune = crate::policy::prune::OwnWalk::new(root, []);
    // One cached read filter for the walk's directories and files: each
    // decided from its parent's real path, not canonicalized one by one.
    let filter = Arc::new(crate::policy::path::DiscoveryWalk::owned(policy.clone()));
    let dirs_policy = Arc::clone(&filter);
    let mut builder = walk_builder(root, ignores, None);
    builder.filter_entry(move |entry| {
        let path = entry.path();
        let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
        !is_dir
            || (!prune.prunes(path, true)
                && !nested
                    .as_ref()
                    .is_some_and(|nested| nested.skips(path, true))
                && dirs_policy.allows_entry(path, entry.file_type()))
    });
    let mut walked = Walked {
        files: BTreeSet::new(),
        dirs: Vec::new(),
        ignore_files: Vec::new(),
        complete: true,
    };
    // Each kept path is charged before it is kept; one past `max_bytes`
    // ends the walk, which is then incomplete.
    let charge = |weight: usize| {
        let total = bytes.get() + weight;
        bytes.set(total);
        total <= max_bytes
    };
    for entry in builder.build() {
        if stopped() {
            walked.complete = false;
            break;
        }
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            let files = IGNORE_FILES.map(|name| path.join(name));
            let weight = weight_of(path) + files.iter().map(|file| weight_of(file)).sum::<usize>();
            if !charge(weight) {
                walked.complete = false;
                break;
            }
            walked.dirs.push((path.to_path_buf(), stamp(path)));
            for file in files {
                let stamped = stamp(&file);
                walked.ignore_files.push((file, stamped));
            }
        } else if family(extensions, path) && filter.allows_entry(path, entry.file_type()) {
            if !charge(weight_of(path)) {
                walked.complete = false;
                break;
            }
            walked.files.insert(canonical(&path.to_string_lossy()));
        }
    }
    walked
}

/// Build the index of `root` (family `include`, read `policy`): every
/// family file the lifted walk lists that the honored walk does not. The
/// walks run to completion however long they take; only `stopped` and the
/// index's share of memory ([`super::INDEX_BYTES`]) end them early, and
/// then the index is incomplete.
pub(super) fn build(
    root: &Path,
    include: &[String],
    policy: &PathPolicy,
    stopped: &(dyn Fn() -> bool + Sync),
) -> IgnoredIndex {
    build_within(root, include, policy, stopped, super::INDEX_BYTES)
}

/// [`build`] whose walks keep at most `max_bytes` of paths.
pub(super) fn build_within(
    root: &Path,
    include: &[String],
    policy: &PathPolicy,
    stopped: &(dyn Fn() -> bool + Sync),
    max_bytes: usize,
) -> IgnoredIndex {
    let extensions = include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*.").map(str::to_owned))
        .collect::<Vec<_>>();
    let bytes = std::cell::Cell::new(0);
    let nested = Arc::new(NestedRepositories::new(root));
    let lifted = walk_family(
        root,
        &extensions,
        policy,
        IgnorePolicy {
            include_ignored: true,
            include_hidden: false,
        },
        Some(Arc::clone(&nested)),
        stopped,
        &bytes,
        max_bytes,
    );
    let honored = walk_family(
        root,
        &extensions,
        policy,
        IgnorePolicy::default(),
        None,
        stopped,
        &bytes,
        max_bytes,
    );
    let tracked = honored
        .dirs
        .into_iter()
        .map(|(dir, _)| dir)
        .collect::<HashSet<_>>();
    let exclude = root.join(".git").join("info").join("exclude");
    let excluded = stamp(&exclude);
    let mut stamps = lifted
        .ignore_files
        .into_iter()
        .chain([(exclude, excluded)])
        .map(|(file, stamped)| (file, stamped, Kind::IgnoreFile))
        .collect::<Vec<_>>();
    let mut dirs = HashSet::new();
    for (dir, stamped) in lifted.dirs {
        let kind = if tracked.contains(&dir) {
            Kind::TrackedDir
        } else {
            Kind::IgnoredDir
        };
        stamps.push((dir.clone(), stamped, kind));
        dirs.insert(dir);
    }
    IgnoredIndex {
        files: lifted.files.difference(&honored.files).cloned().collect(),
        nested: nested.skipped(),
        complete: lifted.complete && honored.complete,
        bytes: bytes.get().min(max_bytes),
        snapshot: Snapshot {
            root: root.to_path_buf(),
            extensions,
            stamps,
            dirs,
        },
    }
}
