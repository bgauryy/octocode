//! The search scope of a request's lexical scans and leads.
//!
//! The language server's workspace root is the *nearest* project marker (a
//! package's `package.json`, a member crate's `Cargo.toml`): right for the
//! server, too narrow for "who else mentions this name". Lexical scans
//! (importer candidates, the Rust cfg-gate count, `textOnlyFiles`) and the
//! `findText` lead use one wider scope instead:
//!
//! 1. the nearest ancestor holding `.git` (a directory or a worktree file);
//! 2. else the outermost workspace manifest (`pnpm-workspace.yaml`,
//!    `lerna.json`, `package.json` with `workspaces`, `Cargo.toml` with
//!    `[workspace]`, `go.work`);
//! 3. else the language server's workspace root;
//!
//! clamped to the read policy: an unauthorized root becomes the outermost
//! authorized ancestor of the file. The scope is never the process cwd.
//! `include` is the anchor language's family globs, from the engine grammar
//! registry.

use super::blocking_cancellable;
use super::failure::LspFailure;
use super::render::word_pattern;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::types::TextSearchOptions;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

mod ignored_index;
mod paths;
mod scan;
mod text;
pub(super) use paths::{canonical, canonical_uri, widest_authorized_dir, with_canonical_memo};
use paths::{family_include, search_root};
pub(super) use scan::IgnoredMentions;
use scan::{fingerprint, scan_text};
pub(super) use text::imports_in_code;

/// Project files whose edits change what a language server answers.
const PROJECT_FILES: [&str; 12] = [
    "package.json",
    "tsconfig.json",
    "jsconfig.json",
    "Cargo.toml",
    "Cargo.lock",
    "go.mod",
    "go.work",
    "pyproject.toml",
    "setup.cfg",
    "compile_commands.json",
    "compile_flags.txt",
    ".clangd",
];

/// Bounds of the scope fingerprint walk; past either, responses are not
/// reused across requests (correct, only slower).
const FINGERPRINT_MAX_FILES: usize = 20_000;
const FINGERPRINT_BUDGET: Duration = Duration::from_millis(150);
/// A file modified this recently also hashes its content: some filesystems
/// keep coarse mtimes, so a same-second edit keeps its length and mtime.
const FRESH_MTIME: Duration = Duration::from_secs(2);
/// A larger unsettled source disables cache reuse rather than reading it
/// whole merely to decide whether a lexical scan may be reused.
const FRESH_CONTENT_MAX_BYTES: usize = 1024 * 1024;
/// Files one check parses to tell a use from a mention: the text check
/// (a code use, not a comment or string) and the ignored-importer check (an
/// import, not a spelling; `importers`). Each reads them in turn, at most
/// [`CODE_CHECK_MAX_BYTES`] apiece, so this bounds the work a request adds
/// per check. A file past it is not proven free of a use, so it counts
/// against completeness: as a use in `textOnlyFiles`, as an unchecked
/// ignored file. The bound is one constant so both checks decide the same
/// number of files.
pub(super) const CODE_CHECK_MAX_FILES: usize = 64;
/// Largest file the code-use and ignored-importer checks read; a larger one
/// counts as a use, or as an unchecked ignored file.
pub(super) const CODE_CHECK_MAX_BYTES: usize = 4 * 1024 * 1024;
/// Bound of the text scan. A slower scan reports no files (no
/// `textOnlyFiles`, and importer recovery reports a failed scan).
const TEXT_SCAN_BUDGET: Duration = Duration::from_secs(3);
/// How long a request waits for its scope's ignored-file index (see
/// `ignored_index`) when no built one holds: a background walk builds it
/// while the server answers, and a request past this bound reports the
/// ignored scan unfinished.
const IGNORED_INDEX_WAIT: Duration = Duration::from_secs(2);
/// Bound of one request's read of the indexed ignored files for its name.
const IGNORED_READ_BUDGET: Duration = Duration::from_secs(1);

/// Text scans by `(root, include, symbol, scope fingerprint, policy)`. The
/// fingerprint covers every family file under the root, so an unchanged
/// fingerprint proves the scan would find the same files; a scan without a
/// fingerprint (walk past its bounds) is never stored.
/// The 256 most recently used are kept.
static TEXT_SCANS: octocode_io::memo::Memo<String, BTreeSet<String>> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 256,
        max_weight: 256,
        keep_newest: false,
    });

/// The files a word-bounded name appears in, for one symbol.
struct TextScan {
    symbol: String,
    /// Canonical paths; `None` when the scan failed or hit a bound.
    files: Option<Arc<BTreeSet<String>>>,
}

pub(super) struct Scope {
    /// Canonical directory every lexical scan and lead covers.
    pub(super) root: String,
    /// Family globs (`*.ts`, …); empty means every file.
    pub(super) include: Vec<String>,
    text: Mutex<Option<TextScan>>,
    /// Digest of every family and project file under `root`, when the
    /// request computed one within its bounds (see `fingerprint`).
    fingerprint: Mutex<Option<String>>,
    /// Canonical files the language server's answer covers (listed, or
    /// verified as importers), recorded by the operation.
    answered: Mutex<BTreeSet<String>>,
    /// Whether a text-scan file uses a symbol in code (not only in comments
    /// or strings), by symbol and canonical path; see [`Self::text_only_files`].
    code_uses: Mutex<HashMap<String, bool>>,
    /// The anchor file (canonical) and the zero-based lines of it the answer
    /// covers, the anchor's own line included; see [`Self::anchor_gap`].
    anchor_lines: Mutex<Option<(String, BTreeSet<u32>)>>,
}

impl Scope {
    pub(super) fn new(root: String, include: Vec<String>) -> Self {
        Self {
            root,
            include,
            text: Mutex::new(None),
            fingerprint: Mutex::new(None),
            answered: Mutex::new(BTreeSet::new()),
            code_uses: Mutex::new(HashMap::new()),
            anchor_lines: Mutex::new(None),
        }
    }

    /// The scope of a request anchored at `anchor` (a file or, for a
    /// workspace-root query, a directory).
    pub(super) fn resolve(
        anchor: &str,
        server_root: &str,
        policy: &PathPolicy,
        language_id: Option<&str>,
    ) -> Self {
        Self::new(
            search_root(Path::new(anchor), server_root, policy),
            family_include(language_id, Path::new(anchor)),
        )
    }

    /// Record the scope fingerprint this request computed; text scans under
    /// it are shared with later requests that compute the same one.
    pub(super) fn set_fingerprint(&self, fingerprint: String) {
        if let Ok(mut slot) = self.fingerprint.lock() {
            *slot = Some(fingerprint);
        }
    }

    /// The scope fingerprint this request computed, if any.
    pub(super) fn fingerprint(&self) -> Option<String> {
        self.fingerprint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Whether the server's answer covers `file` (canonical path).
    pub(super) fn is_answered(&self, file: &str) -> bool {
        self.answered
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(file)
    }

    /// Record files (paths or `file://` uris) the server's answer covers.
    pub(super) fn answer(&self, files: impl IntoIterator<Item = impl AsRef<str>>) {
        if let Ok(mut answered) = self.answered.lock() {
            answered.extend(files.into_iter().map(|file| canonical_uri(file.as_ref())));
        }
    }

    /// Record the zero-based lines of the anchor file the whole answer (every
    /// page) covers, with the anchor's own `line`.
    pub(super) fn answer_anchor_lines(
        &self,
        anchor: &str,
        line: u32,
        lines: impl IntoIterator<Item = u32>,
    ) {
        if let Ok(mut slot) = self.anchor_lines.lock() {
            let mut covered = lines.into_iter().collect::<BTreeSet<_>>();
            covered.insert(line);
            *slot = Some((canonical_uri(anchor), covered));
        }
    }

    /// The anchor file (canonical) when it uses `symbol` in code on a line
    /// the answer does not cover: an answered file can still hide missed
    /// sites, which no file count shows. `None` when nothing was recorded,
    /// or the file cannot be read or parsed.
    pub(super) fn anchor_gap(&self, symbol: &str) -> Option<String> {
        let (file, covered) = self
            .anchor_lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()?;
        let text = crate::tools::source::read_bounded(Path::new(&file), CODE_CHECK_MAX_BYTES)
            .ok()
            .and_then(|bytes| super::source::admitted_text(&bytes).ok())?;
        let lines = octocode_engine::lsp::resolver::code_occurrence_lines(&file, &text, symbol)?;
        lines
            .iter()
            .any(|line| u32::try_from(*line).is_ok_and(|line| !covered.contains(&line)))
            .then_some(file)
    }

    /// The key of this scope's index under `policy`.
    fn index_key(&self, policy: &PathPolicy) -> String {
        format!(
            "{}\u{0}{}\u{0}{policy:?}",
            self.root,
            self.include.join("\u{0}")
        )
    }

    /// Start building this scope's ignored-file index under `policy` on a
    /// background thread (see `ignored_index`), unless one is held or
    /// building. Nothing waits for it.
    pub(super) fn warm_ignored_index(&self, policy: &PathPolicy) {
        ignored_index::start(
            self.index_key(policy),
            PathBuf::from(&self.root),
            self.include.clone(),
            policy.clone(),
        );
    }

    /// The owned inputs of the fingerprint walk, for a blocking worker.
    pub(super) fn walk_inputs(&self) -> WalkInputs {
        WalkInputs {
            root: PathBuf::from(&self.root),
            include: self.include.clone(),
        }
    }
}

/// What the scope fingerprint walks: the scope root and its family globs.
pub(super) struct WalkInputs {
    root: PathBuf,
    include: Vec<String>,
}

impl WalkInputs {
    /// Digest of every family and project file under the scope (relative
    /// path, length, mtime; content too for a fresh mtime), so any edit,
    /// addition, or removal changes it. `None` past the walk's bounds.
    pub(super) fn fingerprint(&self, stopped: &(dyn Fn() -> bool + Sync)) -> Option<String> {
        fingerprint(&self.root, &self.include, stopped, FINGERPRINT_BUDGET)
    }
}

#[cfg(test)]
mod tests;
