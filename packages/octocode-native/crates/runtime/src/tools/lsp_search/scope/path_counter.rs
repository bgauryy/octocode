//! A test-only count per path: how many times a piece of work ran for each
//! file or scope root.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

pub(super) struct PathCounter(Mutex<BTreeMap<PathBuf, usize>>);

impl PathCounter {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(BTreeMap::new()))
    }

    pub(super) fn note(&self, path: &Path) {
        *self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(path.to_path_buf())
            .or_default() += 1;
    }

    pub(super) fn of(&self, path: &Path) -> usize {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .copied()
            .unwrap_or(0)
    }
}
