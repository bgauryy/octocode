//! The global ignore rules of one walk ([`GlobalIgnore`]: the file git's
//! `core.excludesFile` names, resolved from the call's environment), matched
//! per repository.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder, Glob};

pub use crate::process_env::GlobalIgnore;

/// The global rules of one walk, matched per repository: git reads
/// `core.excludesFile` patterns relative to the root of the work tree an
/// entry is in, so an anchored pattern names the same path in each
/// repository the walk enters. The rules apply inside a work tree only.
pub struct PerRepository {
    /// The global file, when it exists.
    file: Option<PathBuf>,
    /// The repository root a directory is in, by directory.
    roots: RwLock<HashMap<PathBuf, Option<PathBuf>>>,
    /// The rules, by repository root.
    matchers: RwLock<HashMap<PathBuf, Arc<Gitignore>>>,
}

impl PerRepository {
    pub fn new(global: GlobalIgnore) -> Self {
        Self {
            file: global.file().filter(|file| file.is_file()),
            roots: RwLock::default(),
            matchers: RwLock::default(),
        }
    }

    /// Whether no rule can apply (no global file).
    pub fn is_inert(&self) -> bool {
        self.file.is_none()
    }

    /// The global verdict on the entry `path`, by the repository its
    /// directory is in; `rule` reads the deciding glob.
    pub fn matched<T>(&self, path: &Path, is_dir: bool, rule: impl Fn(&Glob) -> T) -> Match<T> {
        if self.is_inert() {
            return Match::None;
        }
        let Some(root) = path.parent().and_then(|dir| self.repository_of(dir)) else {
            return Match::None;
        };
        self.matcher(&root).matched(path, is_dir).map(rule)
    }

    fn repository_of(&self, dir: &Path) -> Option<PathBuf> {
        if let Some(known) = self
            .roots
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
        {
            return known.clone();
        }
        let root = if crate::search::repository::is_root(dir) {
            Some(dir.to_path_buf())
        } else {
            dir.parent().and_then(|parent| self.repository_of(parent))
        };
        self.roots
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(dir.to_path_buf(), root.clone());
        root
    }

    fn matcher(&self, root: &Path) -> Arc<Gitignore> {
        if let Some(known) = self
            .matchers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(root)
        {
            return Arc::clone(known);
        }
        let mut builder = GitignoreBuilder::new(root);
        if let Some(file) = &self.file {
            let _ = builder.add(file);
        }
        let built = Arc::new(builder.build().unwrap_or_else(|_| Gitignore::empty()));
        Arc::clone(
            self.matchers
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(root.to_path_buf())
                .or_insert(built),
        )
    }
}

#[cfg(test)]
mod tests;
