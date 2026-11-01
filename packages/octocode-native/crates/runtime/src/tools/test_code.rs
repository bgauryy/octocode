//! What a `testCode` scope counts as test code, one rule for every local
//! search: a test file or directory by its language's convention
//! ([`octocode_engine::text::test_paths::is_test_path`]), a Rust module
//! mounted only under `#[cfg(test)]` (its parent's `mod name;` sits in a
//! test-only item, or the parent is test code itself), and the Rust items
//! compiled only under test (`#[cfg(test)]`, `#![cfg(test)]`), read from the
//! file's cached declarations parse. A file is parsed only when a search
//! asks about it, so a scope costs one cached parse per file with hits.
//! A file is test code whole by where it lies (path or mount, decided
//! before it is read) or by its spans; one wholly on the side a scope
//! leaves out is left out as a file in every view and tool.
use crate::policy::path::PathPolicy;
use octocode_contract_types::TestCode;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) use octocode_engine::types::WHOLE_FILE;

/// Which side of the test-code line a scope keeps; `None` keeps both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keep {
    Production,
    Test,
}

impl Keep {
    pub(crate) fn of(scope: Option<TestCode>) -> Option<Self> {
        match scope? {
            TestCode::Include => None,
            TestCode::Exclude => Some(Self::Production),
            TestCode::Only => Some(Self::Test),
        }
    }

    /// Whether a site that is (or is not) test code stays.
    pub(crate) fn keeps(self, test: bool) -> bool {
        test == (self == Self::Test)
    }
}

/// The test code of the files one request asks about, memoized per file.
pub(crate) struct TestCodeIndex {
    paths: PathPolicy,
    /// Canonical file → its test-only line spans ([`WHOLE_FILE`] for a test file).
    spans: Mutex<HashMap<PathBuf, Arc<Vec<[u32; 2]>>>>,
    /// Canonical Rust file → mounted only under test.
    mounted: Mutex<HashMap<PathBuf, bool>>,
    /// Canonical Rust file → the modules it mounts only under test.
    test_mounts: Mutex<HashMap<PathBuf, Arc<Vec<String>>>>,
}

impl TestCodeIndex {
    pub(crate) fn new(paths: &PathPolicy) -> Self {
        Self {
            paths: paths.clone(),
            spans: Mutex::new(HashMap::new()),
            mounted: Mutex::new(HashMap::new()),
            test_mounts: Mutex::new(HashMap::new()),
        }
    }

    /// Whether the file `path` is test code whole by where it lies, without
    /// reading it: by its path, or by a parent that mounts it only under
    /// test.
    pub(crate) fn test_path(&self, path: &Path) -> bool {
        self.by_spelling(path) || self.mounted_under_test(path)
    }

    /// Whether `path` is test code by its spelling relative to the
    /// workspace (so directories above the workspace never count), else as
    /// given.
    fn by_spelling(&self, path: &Path) -> bool {
        octocode_engine::text::test_paths::is_test_path(&self.relative(path))
    }

    /// `path` spelled relative to the workspace, else as given.
    fn relative(&self, path: &Path) -> String {
        self.paths
            .workspace_relative(path)
            .unwrap_or_else(|| path.to_string_lossy().into_owned())
    }

    /// Whether a file whose path is not test code may still hold some: only
    /// Rust compiles items or whole modules under test.
    pub(crate) fn may_hold_test_code(path: &Path) -> bool {
        path.extension().is_some_and(|extension| extension == "rs")
    }

    /// Whether the directory `path` is test code by its path: everything
    /// under it is.
    pub(crate) fn test_dir(&self, path: &Path) -> bool {
        octocode_engine::text::test_paths::is_test_path(&format!("{}/", self.relative(path)))
    }

    /// Whether the file `path` is test code whole: by its path, its mount,
    /// or a `#![cfg(test)]` (or one test item spanning it).
    pub(crate) fn test_file(&self, path: &Path) -> bool {
        self.spans(path).as_slice() == [WHOLE_FILE]
    }

    /// The 1-based line spans of the file `path` compiled only under test:
    /// [`WHOLE_FILE`] for a test file (by path, mount, or a test span
    /// covering it), else the file's `#[cfg(test)]` items. A file that never
    /// spells `cfg` has none and is not parsed.
    pub(crate) fn spans(&self, path: &Path) -> Arc<Vec<[u32; 2]>> {
        if let Some(known) = lock(&self.spans).get(path) {
            return Arc::clone(known);
        }
        let spans = if self.test_path(path) {
            vec![WHOLE_FILE]
        } else if Self::may_hold_test_code(path) {
            self.source(path)
                .filter(|content| content.contains("cfg"))
                .and_then(|content| {
                    Some(whole_or(
                        Self::parse(path, &content)?.test_lines.clone(),
                        &content,
                    ))
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let spans = Arc::new(spans);
        lock(&self.spans).insert(path.to_path_buf(), Arc::clone(&spans));
        spans
    }

    /// Whether 1-based `line` of the file `path` is test code.
    pub(crate) fn is_test_line(&self, path: &Path, line: u32) -> bool {
        in_spans(&self.spans(path), line)
    }

    /// The text of a readable file under the policy.
    fn source(&self, path: &Path) -> Option<String> {
        let validated = self.paths.validate_read(path).ok()?;
        let bytes = crate::tools::source::read_bounded(
            &validated.canonical,
            octocode_engine::signatures::MAX_PARSE_SIZE,
        )
        .ok()?;
        String::from_utf8(bytes).ok()
    }

    /// The cached declarations parse of `content`, the text of `path`.
    fn parse(
        path: &Path,
        content: &str,
    ) -> Option<Arc<octocode_engine::graph::GraphFactsDocument>> {
        crate::tools::declarations_cache::file_document(content, &path.to_string_lossy())
    }

    /// Whether the Rust file `path` is a module its parent mounts only under
    /// test: the parent's `mod name;` lies in a `#[cfg(test)]` item, or the
    /// parent is itself test code. Parents lie up the directory tree (a crate
    /// root mounts nothing above it), so the walk ends.
    fn mounted_under_test(&self, path: &Path) -> bool {
        if !Self::may_hold_test_code(path) {
            return false;
        }
        if let Some(known) = lock(&self.mounted).get(path) {
            return *known;
        }
        let mounted = module_name(path).is_some_and(|name| {
            parent_candidates(path)
                .into_iter()
                .any(|parent| self.test_mounts(&parent).contains(&name))
        });
        lock(&self.mounted).insert(path.to_path_buf(), mounted);
        mounted
    }

    /// The file modules (`mod name;`) the Rust file `parent` mounts only
    /// under test: inside a `#[cfg(test)]` item, or every one when `parent`
    /// is test code itself. Read once per parent, not once per child; a
    /// parent that is not test code and never spells `cfg` is not parsed.
    fn test_mounts(&self, parent: &Path) -> Arc<Vec<String>> {
        if let Some(known) = lock(&self.test_mounts).get(parent) {
            return Arc::clone(known);
        }
        let parent_test = self.test_path(parent);
        let names = self
            .source(parent)
            .filter(|content| parent_test || content.contains("cfg"))
            .and_then(|content| Self::parse(parent, &content))
            .map(|document| {
                document
                    .modules
                    .iter()
                    .filter(|module| !module.inline && module.scope.is_empty())
                    .filter(|module| parent_test || in_spans(&document.test_lines, module.line))
                    .map(|module| module.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let names = Arc::new(names);
        lock(&self.test_mounts).insert(parent.to_path_buf(), Arc::clone(&names));
        names
    }
}

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
    pub(crate) fn excluded(&self) -> Excluded {
        let whole = crate::num::u32_of_usize(lock(&self.whole).len());
        Excluded {
            files: self
                .files
                .load(std::sync::atomic::Ordering::Relaxed)
                .saturating_add(whole),
            lines: self.lines.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// What a scope left out (`testCodeExcluded`): files left out whole (unread
/// by where they lie, or read and wholly on the other side), and hit lines
/// (listings: entries) inside the other searched files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Excluded {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) files: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) lines: u32,
}

impl Excluded {
    /// The disclosure of a scope that left something out; `None` otherwise.
    pub(crate) fn stated(self) -> Option<Self> {
        (self.files > 0 || self.lines > 0).then_some(self)
    }
}

fn is_zero(count: &u32) -> bool {
    *count == 0
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

/// `spans` of `content`, or [`WHOLE_FILE`] when one covers every line
/// (`#![cfg(test)]` spans the file).
fn whole_or(spans: Vec<[u32; 2]>, content: &str) -> Vec<[u32; 2]> {
    let last = crate::num::u32_of_usize(content.lines().count().max(1));
    if spans.iter().any(|[start, end]| *start <= 1 && *end >= last) {
        vec![WHOLE_FILE]
    } else {
        spans
    }
}

/// Whether 1-based `line` lies in one of `spans`.
pub(crate) fn in_spans(spans: &[[u32; 2]], line: u32) -> bool {
    spans
        .iter()
        .any(|[start, end]| (*start..=*end).contains(&line))
}

/// The module a Rust file defines: its stem, or its directory for `mod.rs`;
/// `None` for a crate root (`lib.rs`, `main.rs`), which nothing mounts.
fn module_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    match stem.as_ref() {
        "lib" | "main" => None,
        "mod" => Some(path.parent()?.file_name()?.to_string_lossy().into_owned()),
        _ => Some(stem.into_owned()),
    }
}

/// The files that may declare the module `path` defines: its directory's
/// `mod.rs`, the sibling `<dir>.rs`, and a crate root in that directory
/// (for `mod.rs`, the same candidates one directory up).
fn parent_candidates(path: &Path) -> Vec<PathBuf> {
    let Some(mut dir) = path.parent() else {
        return Vec::new();
    };
    if path.file_stem().is_some_and(|stem| stem == "mod") {
        let Some(up) = dir.parent() else {
            return Vec::new();
        };
        dir = up;
    }
    let mut candidates = ["mod.rs", "lib.rs", "main.rs"]
        .into_iter()
        .map(|name| dir.join(name))
        .filter(|candidate| candidate != path)
        .collect::<Vec<_>>();
    if let (Some(up), Some(name)) = (dir.parent(), dir.file_name()) {
        candidates.push(up.join(format!("{}.rs", name.to_string_lossy())));
    }
    candidates.retain(|candidate| candidate.is_file());
    candidates
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
