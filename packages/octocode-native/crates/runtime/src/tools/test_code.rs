//! What a `testCode` scope counts as test code, one rule for every local
//! search: a test file or directory by its language's convention
//! ([`octocode_engine::text::test_paths::is_test_path`]), a Rust module
//! mounted only under `#[cfg(test)]` (its parent's `mod name;` sits in a
//! test-only item, or the parent is test code itself), and the Rust items
//! compiled only under test (`#[cfg(test)]`, `#![cfg(test)]`), read by a
//! linear scan that reports what the file's declarations parse does
//! ([`rust_scan`]; the parse reads only a text the scan finds malformed), kept
//! by the text's digest. A file is read only when a search asks about it, and
//! a text read once (by an earlier page, a repeat, or another file with the
//! same bytes) is not read again while the store keeps it.
//! A file is test code whole by where it lies (path or mount) or by its spans; one wholly on the side a scope
//! leaves out is left out as a file in every view and tool. A file that cannot
//! be read at all is named by [`TestCodeIndex::unread`]. A walk's verdict on a file it has yet to read
//! ([`TestCodeIndex::test_path_unread`]) asks module roots first; only a file
//! that verdict would leave out is confirmed against the `#[path]` mounts of
//! its crate, so a file production mounts is never left out unread.
use crate::policy::path::PathPolicy;
use octocode_contract_types::TestCode;
use octocode_engine::stamp::{Link, Proof, cache_stamp};
use octocode_engine::text::{ReadError, Source};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub(crate) use octocode_engine::types::WHOLE_FILE;

mod crate_targets;
mod module_heads;
mod mount_targets;
mod once_each;
mod path_declarers;
mod rust_scan;
mod rust_source;
mod scoped;

use crate_targets::CrateTargets;
use mount_targets::{Mount, Parent, Placement};
use once_each::OnceEach;
use rust_source::{may_be_test_whole, whole_or};
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

/// The test code of the files one request asks about, memoized per file.
pub(crate) struct TestCodeIndex {
    paths: PathPolicy,
    /// Canonical file → its test-only line spans ([`WHOLE_FILE`] for a test file).
    spans: Mutex<HashMap<PathBuf, Arc<Vec<[u32; 2]>>>>,
    /// Canonical Rust file → where its mounts place it.
    mounted: OnceEach<Placement>,
    /// The same, by what a directory's module roots mount
    /// ([`Self::test_path_unread`]).
    named: OnceEach<Placement>,
    /// The same two, when a verdict also looks for the mounts that place a
    /// file in production ([`Placement::Production`] for any file).
    placed: OnceEach<Placement>,
    placed_named: OnceEach<Placement>,
    /// Canonical Rust file → what it can mount.
    parents: OnceEach<Arc<Parent>>,
    /// Directory → the module roots in it or above it that may mount a file
    /// below it by name.
    candidates: OnceEach<Arc<[PathBuf]>>,
    /// Directory → the Rust files in it or above it that spell a `#[path]`.
    declarers: OnceEach<Arc<[PathBuf]>>,
    /// Directory → the Rust files directly in it that spell a `#[path]`.
    spelling: OnceEach<Arc<[PathBuf]>>,
    /// Crate directory → the Rust files of the crate that spell a `#[path]`.
    crate_declarers: OnceEach<Arc<[PathBuf]>>,
    /// Directory → the crate directory holding it.
    crate_dirs: OnceEach<Option<PathBuf>>,
    /// Crate directory → the files Cargo compiles as its crate roots.
    crate_targets: OnceEach<Arc<CrateTargets>>,
    /// Rust files a directory's scan read that never spell `cfg`: they hold no
    /// test item, so no verdict reads them again.
    never_cfg: Mutex<HashSet<PathBuf>>,
    /// Rust file → its text, read from disk once a request whoever asks first:
    /// the directory scan for `#[path]`, a parent's mounts or a file's spans.
    texts: OnceEach<Option<Arc<str>>>,
    /// Canonical Rust file → what its parse says, for spans and mounts alike.
    parsed: OnceEach<Arc<Parsed>>,
    /// Path → what one stat of it found (`None`: nothing there), taken once
    /// per request and shared by every verdict and the proof ([`Self::probe`]).
    probes: OnceEach<Option<Arc<Metadata>>>,
    /// Stats [`Self::probe`] made, by path.
    #[cfg(test)]
    stats: Mutex<HashMap<PathBuf, usize>>,
    /// Every path a verdict read or found absent ([`Self::proof`]).
    read: Mutex<BTreeSet<PathBuf>>,
    /// Every Rust file a verdict needed and could not read ([`Self::unread`]).
    unread: Mutex<BTreeSet<PathBuf>>,
    #[cfg(test)]
    parents_parsed: std::sync::atomic::AtomicUsize,
    /// Parent texts scanned for the modules they name.
    #[cfg(test)]
    heads_scanned: Arc<std::sync::atomic::AtomicUsize>,
    /// Canonical file → times this index read it from disk.
    #[cfg(test)]
    disk_reads: Mutex<HashMap<PathBuf, usize>>,
}

/// What the verdicts ask of a Rust file's parse.
struct Parsed {
    test_lines: Vec<[u32; 2]>,
    /// The files its modules (`mod name;`) may mount.
    modules: Vec<Mount>,
}

impl TestCodeIndex {
    pub(crate) fn new(paths: &PathPolicy) -> Self {
        Self {
            paths: paths.clone(),
            spans: Mutex::new(HashMap::new()),
            mounted: OnceEach::new(),
            named: OnceEach::new(),
            placed: OnceEach::new(),
            placed_named: OnceEach::new(),
            parents: OnceEach::new(),
            candidates: OnceEach::new(),
            declarers: OnceEach::new(),
            spelling: OnceEach::new(),
            crate_declarers: OnceEach::new(),
            crate_dirs: OnceEach::new(),
            crate_targets: OnceEach::new(),
            never_cfg: Mutex::default(),
            texts: OnceEach::new(),
            parsed: OnceEach::new(),
            probes: OnceEach::new(),
            #[cfg(test)]
            stats: Mutex::default(),
            read: Mutex::default(),
            unread: Mutex::default(),
            #[cfg(test)]
            parents_parsed: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            heads_scanned: Arc::default(),
            #[cfg(test)]
            disk_reads: Mutex::default(),
        }
    }

    /// What proves every verdict so far: a kept result stays current while
    /// each file a verdict read keeps its stamp and each it found absent stays
    /// absent. `None` when one was written within the settle window before
    /// `started`, so it could race the read.
    pub(crate) fn proof(&self, started: SystemTime) -> Option<Proof> {
        let mut stamps = Vec::new();
        let mut absent = Vec::new();
        for path in self.watched() {
            match self.probe(&path) {
                Some(meta) => stamps.push((path, cache_stamp(&meta)?)),
                None => absent.push((path, false)),
            }
        }
        let proof = Proof::default()
            .with_taken(Link::Follow, stamps)
            .with_presence(absent);
        proof.settled_before(started).then_some(proof)
    }

    /// What `path` is, by the one stat of it this request makes: every
    /// verdict and the proof read the same answer.
    fn probe(&self, path: &Path) -> Option<Arc<Metadata>> {
        self.probes.of(path, || {
            #[cfg(test)]
            {
                *lock(&self.stats).entry(path.to_path_buf()).or_default() += 1;
            }
            std::fs::metadata(path).ok().map(Arc::new)
        })
    }

    /// Whether `path` is a file, from [`Self::probe`].
    fn is_file(&self, path: &Path) -> bool {
        self.probe(path).is_some_and(|meta| meta.is_file())
    }

    /// Every path a verdict so far read or found absent.
    pub(crate) fn watched(&self) -> Vec<PathBuf> {
        lock(&self.read).iter().cloned().collect()
    }

    /// The Rust files a verdict needed and could not read (past what the
    /// process may hold, or an I/O error): a verdict that needed one is
    /// unproven.
    pub(crate) fn unread(&self) -> Vec<PathBuf> {
        lock(&self.unread).iter().cloned().collect()
    }

    /// Whether the file `path` is test code whole by where it lies, without
    /// reading it: by its path, or by a parent that mounts it only under
    /// test.
    pub(crate) fn test_path(&self, path: &Path) -> bool {
        self.placed_as_test(path, self.placement(path))
    }

    /// [`Self::test_path`] at what a walk can pay for a file with no hits.
    /// A module root's mount places the file as a verdict by name does, and
    /// a file it finds nowhere is production unless spelled as test code. Only
    /// a file left out by that verdict is confirmed by the full one, which
    /// reads the files above for a `#[path]` mount that places it in
    /// production: so no file is left out unread that production mounts, and
    /// files kept are read anyway.
    pub(crate) fn test_path_unread(&self, path: &Path) -> bool {
        self.placed_as_test(path, self.placement_by_name(path)) && self.test_path(path)
    }

    /// Whether a file `placement` puts on the test side: where its mounts
    /// place it decides (a production root mounting `tests.rs` makes it
    /// production), its spelling only when nothing places it.
    fn placed_as_test(&self, path: &Path, placement: Placement) -> bool {
        match placement {
            Placement::Test => true,
            Placement::Production => false,
            Placement::Unplaced => self.by_spelling(path),
        }
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

    /// Whether the directory `path` is test code whole: spelled as such and
    /// holding nothing a plain build compiles. A directory spelled as test
    /// code that its module root (`mod.rs`, or the sibling `<dir>.rs`) places
    /// in production is walked, and its files get their own verdicts.
    pub(crate) fn test_dir(&self, path: &Path) -> bool {
        octocode_engine::text::test_paths::is_test_path(&format!("{}/", self.relative(path)))
            && !self.root_in_production(path)
    }

    /// Whether a module root of the directory `dir` is placed in production,
    /// by what the module roots above mount (the other files of the crate are
    /// not read): one lookup per directory, however many files it holds.
    fn root_in_production(&self, dir: &Path) -> bool {
        let sibling = dir.file_name().and_then(|name| {
            dir.parent()
                .map(|parent| parent.join(format!("{}.rs", name.to_string_lossy())))
        });
        [Some(dir.join("mod.rs")), sibling]
            .into_iter()
            .flatten()
            .any(|root| {
                lock(&self.read).insert(root.clone());
                self.is_file(&root) && self.placement_by_name(&root) == Placement::Production
            })
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
                && !lock(&self.never_cfg).contains(path)
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
        } else if Self::may_hold_test_code(path) && !lock(&self.never_cfg).contains(path) {
            self.source(path)
                .filter(|content| content.contains("cfg"))
                .map(|content| whole_or(self.parsed(path, &content).test_lines.clone(), &content))
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
    fn source(&self, path: &Path) -> Option<Arc<str>> {
        self.texts.of(path, || {
            lock(&self.read).insert(path.to_path_buf());
            let validated = self.paths.validate_read(path).ok()?;
            lock(&self.read).insert(validated.canonical.clone());
            let bytes = self.bounded(&validated.canonical, octocode_engine::text::read_source)?;
            String::from_utf8(bytes).ok().map(Arc::from)
        })
    }

    /// The bytes of the file at `canonical`, as much as the process may hold.
    /// One gone since it was listed has none; any other failure is
    /// [`Self::unread`].
    fn bounded(
        &self,
        canonical: &Path,
        read: fn(&Path, u64) -> Result<Source, ReadError>,
    ) -> Option<Vec<u8>> {
        #[cfg(test)]
        {
            *lock(&self.disk_reads)
                .entry(canonical.to_path_buf())
                .or_default() += 1;
        }
        let limit = u64::try_from(octocode_engine::memory::MEMORY.limit()).unwrap_or(u64::MAX);
        match read(canonical, limit) {
            Ok(source) => Some(source.bytes),
            Err(ReadError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => {
                lock(&self.unread).insert(canonical.to_path_buf());
                None
            }
        }
    }

    /// The declarations parse of `content`, the text of `path`.
    fn parse(
        path: &Path,
        content: &str,
    ) -> Option<Arc<octocode_engine::graph::GraphFactsDocument>> {
        #[cfg(test)]
        DECLARATION_PARSES.with(|parses| parses.set(parses.get() + 1));
        crate::tools::declarations::file_document(content, &path.to_string_lossy())
    }

    /// What `content` says of test code, from [`FACTS`] when the same text
    /// (the same path's kind of source) was read before: the linear scan
    /// ([`rust_scan`]), which reports the test spans and modules the
    /// declarations parse does, and the parse only for a text the scan finds
    /// malformed (what the parse recovers from it is its own) and admits.
    fn facts(path: &Path, content: &str) -> Arc<rust_scan::Scanned> {
        #[cfg(test)]
        PARSES.with(|parses| parses.set(parses.get() + 1));
        let digest = octocode_engine::digest::sha256(content.as_bytes());
        let kind = path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(kept) = FACTS.get(&digest, &kind, Arc::clone) {
            return kept;
        }
        let scanned = rust_scan::scan(content);
        let facts = match scanned
            .malformed
            .then(|| Self::parse(path, content))
            .flatten()
        {
            Some(document) => rust_scan::Scanned {
                test_lines: document.test_lines.clone(),
                modules: document.modules.clone(),
                malformed: false,
            },
            None => scanned,
        };
        let facts = Arc::new(facts);
        FACTS.put(digest, kind, facts.weight(), Arc::clone(&facts));
        facts
    }

    /// What `content`, the text of `path`, says of test code: one read a
    /// request, whichever verdict asks first.
    fn parsed(&self, path: &Path, content: &str) -> Arc<Parsed> {
        self.parsed.of(path, || {
            let crate_root = self.crate_target(path).is_some();
            let facts = Self::facts(path, content);
            Arc::new(Parsed {
                test_lines: facts.test_lines.clone(),
                modules: mount_targets::file_modules(path, &facts.modules, crate_root),
            })
        })
    }
}

impl TestCodeIndex {
    /// What `content`, the text of the parent `path`, says of the modules it
    /// mounts: what a span read already found, else the linear scan
    /// ([`rust_scan`]) without the digest and store a span read keeps
    /// ([`Self::parsed`]); a malformed text goes to that read, which asks the
    /// parse. A walk asks every parent of every file it lists, and most
    /// parents are read for their mounts alone.
    fn mount_facts(&self, path: &Path, content: &str) -> Arc<Parsed> {
        self.parsed.known(path).unwrap_or_else(|| {
            let crate_root = self.crate_target(path).is_some();
            let facts = rust_scan::scan(content);
            if facts.malformed {
                return self.parsed(path, content);
            }
            Arc::new(Parsed {
                modules: mount_targets::file_modules(path, &facts.modules, crate_root),
                test_lines: facts.test_lines,
            })
        })
    }
}

#[cfg(test)]
impl TestCodeIndex {
    /// Parent texts this index scanned for the modules they name.
    fn heads_scanned(&self) -> usize {
        self.heads_scanned
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Parents this index parsed for their mounts.
    fn parents_parsed(&self) -> usize {
        self.parents_parsed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The files this index read from disk more than once.
    fn read_twice(&self) -> Vec<PathBuf> {
        lock(&self.disk_reads)
            .iter()
            .filter(|(_, times)| **times > 1)
            .map(|(path, _)| path.clone())
            .collect()
    }
}

#[cfg(test)]
thread_local! {
    static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Texts this thread's test-code index asked a parse of (a kept one included).
#[cfg(test)]
pub(crate) fn parses() -> usize {
    PARSES.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! {
    static DECLARATION_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Declarations parses this thread's test-code index ran ([`TestCodeIndex::parse`]).
#[cfg(test)]
fn declaration_parses() -> usize {
    DECLARATION_PARSES.with(std::cell::Cell::get)
}

/// What a Rust text says of test code, by its digest (and the extension the
/// parse reads it as): it depends on the text alone, so every request, page
/// and path holding the same text reads it once while the store keeps it.
static FACTS: crate::tools::page_memo::PageMemo<Arc<rust_scan::Scanned>> =
    crate::tools::page_memo::PageMemo::new();

/// Says what a verdict lost to the files it could not read, if any.
pub(crate) fn unread_note(unread: &[PathBuf]) -> Option<String> {
    unread.first().map(|first| {
        format!(
            "{}: unreadable, so the testCode verdict of the modules it may mount is unproven",
            first.display()
        )
    })
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
