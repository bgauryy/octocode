use std::path::{Path, PathBuf};

use super::PathPolicy;
use crate::policy::PolicyErrorCode;
use crate::policy::discovery::is_sensitive_path;

impl PathPolicy {
    /// Discovery prunes denied subtrees. Let the walker account for ordinary
    /// filesystem errors itself so permission/not-found diagnostics stay intact.
    pub(super) fn permits_discovery(&self, input: impl AsRef<Path>) -> bool {
        match self.validate(input) {
            Ok(_) => true,
            Err(error) => !matches!(
                error.code,
                PolicyErrorCode::IgnoredPath
                    | PolicyErrorCode::OutsideAllowedRoots
                    | PolicyErrorCode::SymlinkEscape
                    | PolicyErrorCode::SymlinkLoop
            ),
        }
    }

    /// Discovery decisions for one walk ([`DiscoveryWalk::permits`]), equal
    /// to [`Self::permits_discovery`] at a fraction of its cost.
    pub(crate) fn discovery_walk(&self) -> DiscoveryWalk<'_> {
        DiscoveryWalk {
            policy: std::borrow::Cow::Borrowed(self),
            parents: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

/// [`PathPolicy::permits_discovery`] over one walk. An entry that is not a
/// symlink resolves to its parent directory's real path plus its own name,
/// so only each parent (once) and each symlink are canonicalized; the
/// decision then applies the same root and sensitive-path checks to the
/// same lexical and real paths.
pub(crate) struct DiscoveryWalk<'a> {
    policy: std::borrow::Cow<'a, PathPolicy>,
    /// Each walked parent's real path. Read-mostly and shared by a parallel
    /// walk's threads: a known parent takes only the read lock, and a new
    /// one is canonicalized outside any lock.
    parents: std::sync::RwLock<std::collections::HashMap<PathBuf, Option<PathBuf>>>,
}

impl DiscoveryWalk<'static> {
    /// [`PathPolicy::discovery_walk`] that owns its policy, for a walk
    /// filter the walk shares across threads (`Arc<dyn …>`).
    pub(crate) fn owned(policy: PathPolicy) -> Self {
        Self {
            policy: std::borrow::Cow::Owned(policy),
            parents: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl DiscoveryWalk<'_> {
    pub(crate) fn permits(&self, path: &Path) -> bool {
        self.permits_entry(path, None)
    }

    /// [`Self::permits`] for a walked entry whose directory-listing type the
    /// walk already has: a type that is not a symlink skips the `lstat`.
    pub(crate) fn permits_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        let lexical = self.policy.expand_and_resolve(path);
        let (Some(parent), Some(name)) = (lexical.parent(), lexical.file_name()) else {
            return self.policy.permits_discovery(path);
        };
        let plain = match file_type {
            Some(file_type) if lexical == path => !file_type.is_symlink(),
            _ => {
                std::fs::symlink_metadata(&lexical).is_ok_and(|meta| !meta.file_type().is_symlink())
            }
        };
        if !plain {
            return self.policy.permits_discovery(path);
        }
        // A known parent (every entry after its first sibling) allocates no
        // key and joins its real path without a copy.
        let known = self
            .parents
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(parent)
            .map(|parent_real| parent_real.as_ref().map(|real| real.join(name)));
        let real = match known {
            Some(real) => real,
            None => {
                let parent_real = std::fs::canonicalize(parent).ok();
                let real = parent_real.as_ref().map(|real| real.join(name));
                self.parents
                    .write()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(parent.to_path_buf(), parent_real);
                real
            }
        };
        let Some(real) = real else {
            return self.policy.permits_discovery(path);
        };
        // Under a parent with no symlink the real path is the lexical one:
        // check it once.
        self.policy.allowed(&real)
            && !is_sensitive_path(&lexical)
            && (real == lexical || !is_sensitive_path(&real))
    }
}
