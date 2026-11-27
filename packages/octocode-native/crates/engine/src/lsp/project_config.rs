//! Detects changes to the per-project configuration a language server reads
//! once per project and never again for a document it already holds.
//!
//! clangd resolves a file's compile command from `compile_commands.json`,
//! `compile_flags.txt` and `.clangd` in the file's directory and every
//! ancestor. It caches that lookup (a found database is revalidated at most
//! every 5 s, a missing one every 30 s), re-resolves it only when a document
//! is opened or edited, and ignores `workspace/didChangeWatchedFiles` for
//! these files and registers no watchers of its own
//! (`ClangdLSPServer::onFileEvent`, FIXME "observe config files"). A pooled
//! server therefore keeps the project setup it saw at its first `didOpen`
//! until the process is replaced. [`ProjectConfigWatch`] is how the pool
//! notices that and replaces it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What one config file looked like. Unknown metadata cannot prove that a
/// pooled server still has the configuration on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FileStamp {
    Missing,
    Known(crate::stamp::SourceStamp),
    Unknown,
}

impl FileStamp {
    fn of(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(meta) => crate::stamp::SourceStamp::of(&meta).map_or(Self::Unknown, Self::Known),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::Missing,
            Err(_) => Self::Unknown,
        }
    }

    fn holds(&self, path: &Path) -> bool {
        !matches!(self, Self::Unknown) && *self == Self::of(path)
    }
}

/// The config files of every directory a server resolved a document
/// through, as they were when that directory was first observed.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectConfigWatch {
    /// File names relative to a directory (`build/compile_commands.json`);
    /// empty for a server that re-reads its own configuration.
    names: &'static [&'static str],
    seen: BTreeMap<PathBuf, Vec<FileStamp>>,
    /// Absolute files the server reads whatever the document (a clangd
    /// `--compile-commands-dir` database, see `lsp::clangd_database`).
    fixed: Vec<PathBuf>,
    /// Their stamps at the first observation; `None` until then.
    fixed_baseline: Option<Vec<FileStamp>>,
}

impl ProjectConfigWatch {
    pub(crate) fn new(names: &'static [&'static str]) -> Self {
        Self {
            names,
            seen: BTreeMap::new(),
            fixed: Vec::new(),
            fixed_baseline: None,
        }
    }

    /// Also watch `files`, absolute paths every document resolves through.
    pub(crate) fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.fixed = files;
        self
    }

    /// `false` for a server with nothing to watch.
    pub(crate) fn is_active(&self) -> bool {
        !self.names.is_empty()
    }

    /// Forget every baseline (a new server process starts from a clean one).
    pub(crate) fn clear(&mut self) {
        self.seen.clear();
        self.fixed_baseline = None;
    }

    /// Record `path` (a file or a directory): its directory and every
    /// ancestor become watched, each with the stamps it has now. A directory
    /// already watched keeps its older baseline, so a change made after the
    /// server first looked there is never absorbed by a later observation.
    pub(crate) fn observe(&mut self, path: &Path) {
        if !self.is_active() {
            return;
        }
        if self.fixed_baseline.is_none() {
            self.fixed_baseline = Some(self.fixed.iter().map(|file| FileStamp::of(file)).collect());
        }
        let start = if path.is_dir() {
            Some(path)
        } else {
            path.parent()
        };
        for dir in start.into_iter().flat_map(Path::ancestors) {
            if !self.seen.contains_key(dir) {
                self.seen.insert(dir.to_path_buf(), self.stamps(dir));
            }
        }
    }

    /// Whether any watched config file was created, changed or deleted since
    /// its directory was first observed.
    pub(crate) fn changed(&self) -> bool {
        let fixed_changed = self.fixed_baseline.as_ref().is_some_and(|baseline| {
            self.fixed
                .iter()
                .zip(baseline)
                .any(|(file, stamp)| !stamp.holds(file))
        });
        fixed_changed
            || self.seen.iter().any(|(dir, baseline)| {
                self.names
                    .iter()
                    .zip(baseline)
                    .any(|(name, stamp)| !stamp.holds(&dir.join(name)))
            })
    }

    fn stamps(&self, dir: &Path) -> Vec<FileStamp> {
        self.names
            .iter()
            .map(|name| FileStamp::of(&dir.join(name)))
            .collect()
    }
}

#[cfg(test)]
mod tests;
