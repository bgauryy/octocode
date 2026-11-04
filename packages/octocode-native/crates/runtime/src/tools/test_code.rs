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
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) use octocode_engine::types::WHOLE_FILE;

mod once_each;
mod rust_source;
mod scoped;

use once_each::OnceEach;
use rust_source::{
    candidate_dir, candidates_in, first_gated_byte, may_be_test_whole, module_name, whole_or,
    word_at,
};
pub(crate) use scoped::{Scoped, text_scope};

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

/// Whether a result under `scope` may be kept for its later pages. A filtering
/// scope's verdicts read more than a kept result's proof watches: each file's
/// parent modules up to its crate root, outside the result's root too, and the
/// text of files it leaves out. Each page computes such a result afresh, and a
/// changed verdict changes its snapshot.
pub(crate) fn result_may_be_kept(scope: Option<TestCode>) -> bool {
    Keep::of(scope).is_none()
}

/// The test code of the files one request asks about, memoized per file.
pub(crate) struct TestCodeIndex {
    paths: PathPolicy,
    /// Canonical file → its test-only line spans ([`WHOLE_FILE`] for a test file).
    spans: Mutex<HashMap<PathBuf, Arc<Vec<[u32; 2]>>>>,
    /// Canonical Rust file → mounted only under test.
    mounted: OnceEach<bool>,
    /// Canonical Rust file → what it can mount.
    parents: OnceEach<Arc<Parent>>,
    /// Directory → the files in it or above it that may mount its modules.
    candidates: OnceEach<Arc<[PathBuf]>>,
    #[cfg(test)]
    parents_parsed: std::sync::atomic::AtomicUsize,
}

/// A Rust file its modules' mounts are asked of.
struct Parent {
    /// It is test code itself: it mounts under test every module it declares.
    test: bool,
    /// Its text and the byte from which it could name a module it mounts only
    /// under test; `None` when it can mount none, so nothing asks for its parse.
    gated: Option<(String, usize)>,
    /// The file modules it mounts only under test, once a mount asked.
    names: OnceLock<Vec<String>>,
}

impl Parent {
    /// Whether its text leaves the parse anything to find about `name`.
    fn may_mount(&self, name: &str) -> bool {
        self.gated
            .as_ref()
            .is_some_and(|(content, from)| word_at(content, name, *from).is_some())
    }
}

impl TestCodeIndex {
    pub(crate) fn new(paths: &PathPolicy) -> Self {
        Self {
            paths: paths.clone(),
            spans: Mutex::new(HashMap::new()),
            mounted: OnceEach::new(),
            parents: OnceEach::new(),
            candidates: OnceEach::new(),
            #[cfg(test)]
            parents_parsed: std::sync::atomic::AtomicUsize::new(0),
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
    /// or a `#![cfg(test)]` (or one test item spanning it). A file whose text
    /// can hold neither of those last two needs no parse.
    pub(crate) fn test_file(&self, path: &Path) -> bool {
        if let Some(known) = lock(&self.spans).get(path) {
            return known.as_slice() == [WHOLE_FILE];
        }
        self.test_path(path)
            || (Self::may_hold_test_code(path)
                && self
                    .source(path)
                    .is_some_and(|content| may_be_test_whole(&content))
                && self.spans(path).as_slice() == [WHOLE_FILE])
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
        #[cfg(test)]
        PARSES.with(|parses| parses.set(parses.get() + 1));
        crate::tools::declarations::file_document(content, &path.to_string_lossy())
    }

    /// Whether the Rust file `path` is a module its parent mounts only under
    /// test: the parent's `mod name;` lies in a `#[cfg(test)]` item, or the
    /// parent is itself test code. Parents lie up the directory tree (a crate
    /// root mounts nothing above it), so the walk ends.
    fn mounted_under_test(&self, path: &Path) -> bool {
        if !Self::may_hold_test_code(path) {
            return false;
        }
        self.mounted.of(path, || {
            let Some(name) = module_name(path) else {
                return false;
            };
            let Some(dir) = candidate_dir(path) else {
                return false;
            };
            let candidates = self.candidates.of(dir, || candidates_in(dir).into());
            candidates
                .iter()
                .filter(|candidate| candidate.as_path() != path)
                .any(|parent| self.mounts_under_test(parent, &name))
        })
    }

    /// Whether the Rust file `parent` mounts `name` only under test. Its text
    /// rules out what no parse could find; the parse decides the rest, once
    /// per parent, not once per child.
    fn mounts_under_test(&self, parent: &Path, name: &str) -> bool {
        let known = self.parents.of(parent, || {
            let test = self.test_path(parent);
            let gated = self.source(parent).and_then(|content| {
                let from = if test { 0 } else { first_gated_byte(&content)? };
                Some((content, from))
            });
            Arc::new(Parent {
                test,
                gated,
                names: OnceLock::new(),
            })
        });
        known.may_mount(name)
            && known
                .names
                .get_or_init(|| {
                    #[cfg(test)]
                    self.parents_parsed
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Self::parsed_mounts(parent, &known)
                })
                .iter()
                .any(|mounted| mounted == name)
    }

    /// The file modules (`mod name;`) `parent` mounts only under test, by its
    /// parse: inside a `#[cfg(test)]` item, or every one when it is test code
    /// itself.
    fn parsed_mounts(path: &Path, parent: &Parent) -> Vec<String> {
        parent
            .gated
            .as_ref()
            .and_then(|(content, _)| Self::parse(path, content))
            .map(|document| {
                document
                    .modules
                    .iter()
                    .filter(|module| !module.inline && module.scope.is_empty())
                    .filter(|module| parent.test || in_spans(&document.test_lines, module.line))
                    .map(|module| module.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
impl TestCodeIndex {
    /// Parents this index parsed for their mounts.
    fn parents_parsed(&self) -> usize {
        self.parents_parsed
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
thread_local! {
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Declarations parses this thread's test-code index asked for.
#[cfg(test)]
pub(crate) fn parses() -> usize {
    PARSES.with(std::cell::Cell::get)
}

/// Whether 1-based `line` lies in one of `spans`.
pub(crate) fn in_spans(spans: &[[u32; 2]], line: u32) -> bool {
    spans
        .iter()
        .any(|[start, end]| (*start..=*end).contains(&line))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
