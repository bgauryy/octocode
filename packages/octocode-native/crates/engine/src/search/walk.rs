//! The one walk policy every local tool shares: which ignore rules apply,
//! which dot entries and directory names are skipped, and how a walk is
//! cancelled.
//!
//! Ignore rules are standard gitignore semantics as the `ignore` crate
//! implements them: `.gitignore` files (inside a git repository only),
//! `.ignore` files, the global gitignore, `.git/info/exclude`, and the ignore
//! files of the directories above the walked root. `includeIgnored` lifts
//! every source at once. [`walk_builder`] applies the rules inside a walk;
//! [`IgnoreRules`] answers the same question for one path, and says which
//! rule decided it.

use crate::types::IgnorePolicy;
use std::collections::{BTreeSet, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};

use ignore::WalkBuilder;

mod ignore_rules;

pub use ignore_rules::{IGNORE_FILES, IgnoreReason, IgnoreRules, external_ignore_inputs};

/// A walk's cancellation: polled between entries; `true` ends the walk.
pub type Stop<'a> = dyn Fn() -> bool + Sync + 'a;

/// A walker over `root` under `policy`, with symlinks unfollowed.
/// `max_depth` is in `ignore` terms: the root is depth 0, its entries depth 1.
/// Callers add overrides, file types, ordering, and pruning.
pub fn walk_builder(root: &Path, policy: IgnorePolicy, max_depth: Option<usize>) -> WalkBuilder {
    let rules = !policy.include_ignored;
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!policy.include_hidden)
        .ignore(rules)
        .git_ignore(rules)
        .git_global(rules)
        .git_exclude(rules)
        .parents(rules)
        .require_git(true)
        .follow_links(false)
        .max_depth(max_depth);
    // A parallel walk takes this call's share of the process CPU budget
    // while other calls compute; alone, the walker's own default.
    let share = super::cpu::host().share();
    if share < super::cpu::host().cores() {
        builder.threads(share);
    }
    builder
}

/// Directory names a walk never descends into, at any depth below its root.
/// The engine has no default list: the runtime prune policy names them.
#[derive(Clone, Debug, Default)]
pub struct Prune(Arc<HashSet<OsString>>);

impl Prune {
    pub fn new<S: AsRef<OsStr>>(names: impl IntoIterator<Item = S>) -> Self {
        Self(Arc::new(
            names
                .into_iter()
                .map(|name| name.as_ref().to_os_string())
                .collect(),
        ))
    }

    /// Whether a directory named `name` is pruned.
    pub fn names(&self, name: &OsStr) -> bool {
        self.0.contains(name)
    }

    /// Whether a walked entry is a pruned directory below the root.
    pub fn prunes_entry(&self, entry: &ignore::DirEntry) -> bool {
        entry.depth() > 0
            && entry.file_type().is_some_and(|kind| kind.is_dir())
            && self.names(entry.file_name())
    }

    /// This list as a default prune of a walk of `root` (one a caller's own
    /// exclusions never go through): a directory with a listed name is
    /// pruned outside a git work tree, and inside one only when git ignores
    /// it (it or a directory on the way below `root`) or it is a
    /// repository's own `.git`, which git never tracks. A listed name git
    /// does not ignore is source the repository keeps (a crate's `build/`
    /// scripts, rustc's `coverage/` pass), so the walk enters it.
    pub fn at(self, root: &Path) -> RootedPrune {
        RootedPrune {
            names: self,
            root: Arc::from(root),
            repository: Arc::default(),
        }
    }
}

/// A [`Prune`] list bound to the root of one walk ([`Prune::at`]). The
/// repository's ignore rules load when the walk first meets a listed name.
#[derive(Clone)]
pub struct RootedPrune {
    names: Prune,
    root: Arc<Path>,
    /// `None` outside a git work tree.
    repository: Arc<std::sync::OnceLock<Option<IgnoreRules>>>,
}

impl std::fmt::Debug for RootedPrune {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootedPrune")
            .field("names", &self.names)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl RootedPrune {
    /// Whether the walk prunes `path`, an entry below its root.
    pub fn prunes(&self, path: &Path, is_dir: bool) -> bool {
        let Some(name) = path
            .file_name()
            .filter(|name| is_dir && self.names.names(name))
        else {
            return false;
        };
        name == ".git"
            || self
                .repository
                .get_or_init(|| {
                    let rules = IgnoreRules::new(&self.root);
                    rules.in_repository().then_some(rules)
                })
                .as_ref()
                .is_none_or(|rules| rules.why(path, true).is_some())
    }
}

/// The nested repositories a walk that lifts ignore rules leaves out, and
/// records: a directory below the walk's root holding its own `.git` (a
/// clone's directory, or the file a submodule or linked worktree keeps).
/// Lifting the rules finds what they hide in this repository; a nested one
/// is another repository (git itself never enters it), and a gitignored
/// checkout of others can hold more than the rest of the tree.
#[derive(Debug, Default)]
pub struct NestedRepositories {
    root: PathBuf,
    skipped: std::sync::Mutex<BTreeSet<PathBuf>>,
}

impl NestedRepositories {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            skipped: std::sync::Mutex::default(),
        }
    }

    /// Whether the walk skips `path`: a directory below the root that is a
    /// nested repository. A skipped one is recorded.
    pub fn skips(&self, path: &Path, is_dir: bool) -> bool {
        let nested = is_dir
            && path != self.root
            && path.starts_with(&self.root)
            && std::fs::symlink_metadata(path.join(".git")).is_ok();
        if nested {
            self.skipped
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(path.to_path_buf());
        }
        nested
    }

    /// The skipped repositories, root-relative with `/` separators, sorted.
    pub fn skipped(&self) -> Vec<String> {
        self.skipped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|path| crate::text::slash_path(path.strip_prefix(&self.root).unwrap_or(path)))
            .collect()
    }
}

#[cfg(test)]
mod tests;
