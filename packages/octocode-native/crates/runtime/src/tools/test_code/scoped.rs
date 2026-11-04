//! A scope over one request: which files a walk reads and which sites it keeps,
//! counting what it leaves out, and the same scope for the engine's text search.
use super::{Keep, TestCodeIndex, lock};
use crate::policy::path::PathPolicy;
use octocode_contract_types::TestCode;
use octocode_engine::types::TestCodeExcluded;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A scope over one request's walk: which files it reads and which sites
/// it keeps, counting what it leaves out.
pub(crate) struct Scoped {
    keep: Keep,
    index: TestCodeIndex,
    files: std::sync::atomic::AtomicU32,
    lines: std::sync::atomic::AtomicU32,
    /// Read files a site showed to be wholly on the side left out.
    whole: Mutex<HashSet<PathBuf>>,
}

impl Scoped {
    /// The scope a `testCode` field asks for; `None` keeps everything.
    pub(crate) fn of(scope: Option<TestCode>, paths: &PathPolicy) -> Option<Self> {
        Keep::of(scope).map(|keep| Self {
            keep,
            index: TestCodeIndex::new(paths),
            files: std::sync::atomic::AtomicU32::new(0),
            lines: std::sync::atomic::AtomicU32::new(0),
            whole: Mutex::new(HashSet::new()),
        })
    }

    /// Whether the walk reads the entry `path`: a directory always; a file
    /// unless where it lies puts it on the other side (counted, left
    /// unread).
    pub(crate) fn reads(&self, path: &Path, is_dir: bool) -> bool {
        if is_dir {
            return true;
        }
        let test = self.index.test_path(path);
        let read = match self.keep {
            Keep::Production => !test,
            Keep::Test => test || TestCodeIndex::may_hold_test_code(path),
        };
        if !read {
            self.files
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        read
    }

    /// Whether the site at 1-based `line` of the file `path` stays. One
    /// left out is counted: by its file when the file lies wholly on the
    /// side left out (as the text search counts it), else as a line.
    pub(crate) fn keeps(&self, path: &Path, line: u32) -> bool {
        let kept = self.sides(path, line);
        if !kept {
            let test_kept = self.keep == Keep::Test;
            if octocode_engine::types::wholly_left_out(test_kept, &self.index.spans(path)) {
                lock(&self.whole).insert(path.to_path_buf());
            } else {
                self.lines
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        kept
    }

    /// [`Self::keeps`] uncounted: for a pass that repeats one that counted.
    pub(crate) fn sides(&self, path: &Path, line: u32) -> bool {
        self.keep.keeps(self.index.is_test_line(path, line))
    }

    /// Whether a listing lists the entry `path`: a directory by its path,
    /// a file when it is (or is not) test code whole. A file only partly
    /// test code is production code here. Uncounted: a listing counts what
    /// it leaves out by walk position.
    pub(crate) fn lists(&self, path: &Path, is_dir: bool) -> bool {
        self.keep.keeps(if is_dir {
            self.index.test_dir(path)
        } else {
            self.index.test_file(path)
        })
    }

    /// What the scope left out so far.
    pub(crate) fn excluded(&self) -> TestCodeExcluded {
        let whole = octocode_engine::num::saturating_u32(lock(&self.whole).len());
        TestCodeExcluded {
            files: self
                .files
                .load(std::sync::atomic::Ordering::Relaxed)
                .saturating_add(whole),
            lines: self.lines.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// The engine text search's scope for `keep`, reading spans through `index`.
pub(crate) fn text_scope(
    keep: Keep,
    index: Arc<TestCodeIndex>,
) -> octocode_engine::types::TestCodeScope {
    let by_path = Arc::clone(&index);
    octocode_engine::types::TestCodeScope {
        keep_test: keep == Keep::Test,
        test_path: Arc::new(move |path| by_path.test_path(path)),
        may_hold: Arc::new(TestCodeIndex::may_hold_test_code),
        spans: Arc::new(move |path| index.spans(path)),
    }
}
