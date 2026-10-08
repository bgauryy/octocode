//! `.gitignore` rules for native walks that do not go through ripgrep's
//! walker (topology scans, the structure tree), so every local tool leaves
//! out the same ignored files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// `.gitignore` awareness for the scan's per-path filter. Matchers load
/// lazily per directory (the walk visits directories before their children,
/// and an ignored directory is pruned whole). Precedence follows git: the
/// deepest `.gitignore` with a matching rule decides, then ancestors above
/// the scan root up to the repository root, then `.git/info/exclude`.
pub(crate) struct GitignoreFilter {
    root: PathBuf,
    /// Matchers above the scan root (nearest first), then info/exclude.
    outer: Vec<ignore::gitignore::Gitignore>,
    cache: Mutex<HashMap<PathBuf, Option<Arc<ignore::gitignore::Gitignore>>>>,
}

impl GitignoreFilter {
    const MAX_OUTER_LEVELS: usize = 32;

    pub(crate) fn new(root: &Path) -> Self {
        let mut outer = Vec::new();
        let mut repository = None;
        for directory in root.ancestors().take(Self::MAX_OUTER_LEVELS) {
            if directory != root
                && let Some(matcher) = Self::load(directory, &directory.join(".gitignore"))
            {
                outer.push(matcher);
            }
            if directory.join(".git").exists() {
                repository = Some(directory.to_path_buf());
                break;
            }
        }
        // Only a real repository scopes ancestor ignore files; without one,
        // unrelated ignore files above the root must not hide sources.
        let Some(repository) = repository else {
            outer.clear();
            return Self {
                root: root.to_path_buf(),
                outer,
                cache: Mutex::default(),
            };
        };
        if let Some(matcher) = Self::load(
            &repository,
            &repository.join(".git").join("info").join("exclude"),
        ) {
            outer.push(matcher);
        }
        Self {
            root: root.to_path_buf(),
            outer,
            cache: Mutex::default(),
        }
    }

    fn load(directory: &Path, file: &Path) -> Option<ignore::gitignore::Gitignore> {
        if !file.is_file() {
            return None;
        }
        let mut builder = ignore::gitignore::GitignoreBuilder::new(directory);
        // A malformed line is skipped; the remaining rules still apply.
        let _ = builder.add(file);
        builder.build().ok().filter(|matcher| !matcher.is_empty())
    }

    fn matcher(&self, directory: &Path) -> Option<Arc<ignore::gitignore::Gitignore>> {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A hit (every ancestor after the first visit) allocates no key.
        if let Some(matcher) = cache.get(directory) {
            return matcher.clone();
        }
        let matcher = Self::load(directory, &directory.join(".gitignore")).map(Arc::new);
        cache.insert(directory.to_path_buf(), matcher.clone());
        matcher
    }

    pub(crate) fn is_ignored(&self, path: &Path) -> bool {
        self.is_ignored_as(path, None)
    }

    /// [`Self::is_ignored`] for a path whose directory bit the caller already
    /// has (a walk's directory-entry type); `None` stats the path for it.
    pub(crate) fn is_ignored_as(&self, path: &Path, is_dir: Option<bool>) -> bool {
        if path == self.root || !path.starts_with(&self.root) {
            return false;
        }
        let is_dir = is_dir
            .unwrap_or_else(|| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()));
        let decide = |matcher: &ignore::gitignore::Gitignore| match matcher.matched(path, is_dir) {
            ignore::Match::Ignore(_) => Some(true),
            ignore::Match::Whitelist(_) => Some(false),
            ignore::Match::None => None,
        };
        for directory in path.ancestors().skip(1) {
            if !directory.starts_with(&self.root) {
                break;
            }
            if let Some(decision) = self.matcher(directory).as_deref().and_then(decide) {
                return decision;
            }
        }
        self.outer.iter().find_map(decide).unwrap_or(false)
    }
}
