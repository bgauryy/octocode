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
    /// The global file, when it exists and holds a rule.
    file: Option<PathBuf>,
    /// The rules for the entries of a directory, by directory: those of the
    /// repository it is in, `None` outside one. A directory found shares its
    /// repository's rules with every directory below it.
    dirs: RwLock<HashMap<PathBuf, Option<Arc<Gitignore>>>>,
}

impl PerRepository {
    pub fn new(global: GlobalIgnore) -> Self {
        Self {
            file: global.file().filter(|file| holds_rules(file)),
            dirs: RwLock::default(),
        }
    }

    /// Whether no rule can apply: no global file, or one with no rule (only
    /// blank lines and comments).
    pub fn is_inert(&self) -> bool {
        self.file.is_none()
    }

    /// The global verdict on the entry `path`, by the repository its
    /// directory is in; `rule` reads the deciding glob.
    pub fn matched<T>(&self, path: &Path, is_dir: bool, rule: impl Fn(&Glob) -> T) -> Match<T> {
        let Some(rules) = path.parent().and_then(|dir| self.for_entries_of(dir)) else {
            return Match::None;
        };
        rules.matched(path, is_dir).map(rule)
    }

    /// The rules that decide the entries of `dir`: those of the repository
    /// it is in, `None` outside one or with no rule to apply. Found once per
    /// directory, so a walk that keeps them for a directory asks once.
    pub(crate) fn for_entries_of(&self, dir: &Path) -> Option<Arc<Gitignore>> {
        let file = self.file.as_ref()?;
        if let Some(known) = self
            .dirs
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(dir)
        {
            return known.clone();
        }
        let rules = if crate::search::repository::is_root(dir) {
            let mut builder = GitignoreBuilder::new(dir);
            let _ = builder.add(file);
            Some(Arc::new(
                builder.build().unwrap_or_else(|_| Gitignore::empty()),
            ))
        } else {
            dir.parent().and_then(|parent| self.for_entries_of(parent))
        };
        self.dirs
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(dir.to_path_buf(), rules.clone());
        rules
    }
}

/// Whether the ignore file `file` exists and holds a rule: a file of blank
/// lines and comments decides nothing, so no entry needs to ask it.
fn holds_rules(file: &Path) -> bool {
    if !file.is_file() {
        return false;
    }
    let mut builder = GitignoreBuilder::new(file.parent().unwrap_or(file));
    let _ = builder.add(file);
    builder
        .build()
        .is_ok_and(|rules| rules.num_ignores() + rules.num_whitelists() > 0)
}

#[cfg(test)]
mod tests;
