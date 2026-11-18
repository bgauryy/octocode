//! Which projects a server already loaded, so only the first open in each
//! pays the open settle.
//!
//! typescript-language-server reports a tsserver project load
//! (`projectLoadingStart`/`projectLoadingFinish`) as one work-done progress,
//! and while a load is under way it routes References, Definition and the
//! other project queries to its syntax-only server. The settle after a first
//! `didOpen` exists to see such a load start. Once a project has loaded, or
//! an open in it proved the server silent, a later file of the same project
//! starts no load and a settle for it protects nothing.

use crate::lsp::transport::{ProgressTracker, Readiness};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

/// A project: the directory of the nearest `tsconfig.json`/`jsconfig.json`
/// of a TypeScript-family file, `None` for any other file (one key per
/// server for files outside such a project).
pub(super) type ProjectKey = Option<PathBuf>;

/// The project `file_path` belongs to.
pub(super) fn project_key(file_path: &str) -> ProjectKey {
    let language_id = crate::lsp::config::detect_language_id(file_path);
    if !crate::lsp::family::is_typescript_family(language_id.as_deref()) {
        return None;
    }
    Path::new(file_path)
        .parent()
        .and_then(crate::lsp::workspace::typescript_project_dir)
        .map(Path::to_path_buf)
}

/// Projects whose first open already waited, and the progress the memo
/// has accounted for.
#[derive(Debug, Default)]
pub(super) struct ProjectLoads {
    /// Projects a first open waited for to the end: the load it saw went
    /// idle, or the server stayed silent.
    settled: HashSet<ProjectKey>,
    /// Progress begins every finished wait has seen (see
    /// [`ProgressTracker::begins`]).
    accounted_begins: u64,
    /// First opens now waiting for a project not yet settled. Progress that
    /// begins meanwhile is theirs.
    waiting: usize,
}

impl ProjectLoads {
    /// A first open of a file in `key`, `begins` read before its `didOpen`:
    /// `true` when `key` already settled, so the open owes no settle.
    /// Progress that began while no open was waiting (the server reloading
    /// a project after a config edit, or loading one for a file the memo
    /// placed in a settled project) forgets every project first. `false`
    /// counts the open as waiting until [`Self::finish`].
    pub(super) fn start(&mut self, key: &ProjectKey, begins: u64) -> bool {
        if self.waiting == 0 && begins > self.accounted_begins {
            self.settled.clear();
            self.accounted_begins = begins;
        }
        if self.settled.contains(key) {
            return true;
        }
        self.waiting += 1;
        false
    }

    /// The end of a wait [`Self::start`] counted: `key` settles unless the
    /// wait ended before the server went idle. Once no open waits, every
    /// progress begun so far is accounted for.
    pub(super) fn finish(&mut self, key: ProjectKey, settled: bool, begins: u64) {
        self.waiting = self.waiting.saturating_sub(1);
        if settled {
            self.settled.insert(key);
        }
        if self.waiting == 0 {
            self.accounted_begins = self.accounted_begins.max(begins);
        }
    }

    /// Forget every project (a new server process, a changed project setup).
    pub(super) fn clear(&mut self) {
        self.settled.clear();
    }
}

/// A first open waiting for its project: [`Self::finish`] records the
/// outcome, and dropping it unfinished (a cancelled wait) leaves the project
/// unsettled.
pub(super) struct PendingLoad<'a> {
    loads: &'a StdMutex<ProjectLoads>,
    progress: &'a ProgressTracker,
    key: Option<ProjectKey>,
}

impl<'a> PendingLoad<'a> {
    /// `None` when `key` already settled; otherwise the open now waits.
    pub(super) fn start(
        loads: &'a StdMutex<ProjectLoads>,
        progress: &'a ProgressTracker,
        key: ProjectKey,
        begins: u64,
    ) -> Option<Self> {
        let settled = loads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .start(&key, begins);
        (!settled).then_some(Self {
            loads,
            progress,
            key: Some(key),
        })
    }

    /// The wait ended with `readiness`: an idle or silent server settled
    /// the project; a timeout did not.
    pub(super) fn finish(mut self, readiness: Readiness) {
        self.end(readiness != Readiness::Timeout);
    }

    fn end(&mut self, settled: bool) {
        if let Some(key) = self.key.take() {
            self.loads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .finish(key, settled, self.progress.begins());
        }
    }
}

impl Drop for PendingLoad<'_> {
    fn drop(&mut self) {
        self.end(false);
    }
}

#[cfg(test)]
mod tests;
