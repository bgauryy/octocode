//! The search scope of a request's lexical scans and leads.
//!
//! The language server's workspace root is the *nearest* project marker (a
//! package's `package.json`, a member crate's `Cargo.toml`): right for the
//! server, too narrow for "who else mentions this name". Lexical scans
//! (importer candidates, the Rust cfg-gate count, `textOnlyFiles`) and the
//! `findText` lead use one wider scope instead:
//!
//! 0. the request's explicit `workspaceRoot`, when it holds the anchor:
//!    the caller chose the project, so candidates outside it are not
//!    scanned, and coverage says so (`importerScanRoot`);
//! 1. the nearest ancestor holding `.git` (a directory or a worktree file);
//! 2. else the outermost workspace manifest (`pnpm-workspace.yaml`,
//!    `lerna.json`, `package.json` with `workspaces`, `Cargo.toml` with
//!    `[workspace]`, `go.work`);
//! 3. else the language server's workspace root;
//!
//! clamped to the read policy: an unauthorized root becomes the outermost
//! authorized ancestor of the file. The scope is never the process cwd.
//! `include` is the anchor language's family globs, from the engine grammar
//! registry. A scanned file outside the anchor declaration's reach (see
//! `reach`) is out of reach, not text-only.

use super::blocking_cancellable;
use super::failure::LspFailure;
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::text::word_pattern;
use octocode_engine::types::TextSearchOptions;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

mod ignored_index;
mod paths;
mod reach;
mod scan;
mod text;
pub(super) use paths::{canonical, canonical_uri, widest_authorized_dir, with_canonical_memo};
use paths::{explicit_root, family_include, search_root};
pub(super) use scan::IgnoredMentions;
use scan::{Fingerprint, scan_text, shared_fingerprint};
pub(super) use text::imports_in_code;

/// Project files whose edits change what a language server answers.
const PROJECT_FILES: [&str; 20] = [
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "go.sum",
    "uv.lock",
    "poetry.lock",
    "Pipfile.lock",
    "requirements.txt",
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

/// A file modified this recently also hashes its content: some filesystems
/// keep coarse mtimes, so a same-second edit keeps its length and mtime.
const FRESH_MTIME: Duration = Duration::from_secs(2);
/// A larger unsettled source disables cache reuse rather than reading it
/// whole merely to decide whether a lexical scan may be reused.
const FRESH_CONTENT_MAX_BYTES: usize = 1024 * 1024;

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

/// The reach of the anchor's declaration of one symbol, once read (`None`:
/// no bound is known).
type ReachSlot = Mutex<Option<(String, Option<Arc<reach::Reach>>)>>;

/// The files a word-bounded name appears in, for one symbol.
struct TextScan {
    symbol: String,
    /// Canonical paths; `None` when the scan failed.
    files: Option<Arc<BTreeSet<String>>>,
}

pub(super) struct Scope {
    /// Canonical directory every lexical scan and lead covers.
    pub(super) root: String,
    /// The request's explicit `workspaceRoot` narrowed `root` below the
    /// root the anchor's repository would give: files outside it are not
    /// scanned.
    pub(super) narrowed: bool,
    /// Family globs (`*.ts`, …); empty means every file.
    pub(super) include: Vec<String>,
    /// The language server's workspace root (canonical), whose ignored
    /// files the server may still compile; `None` when unknown.
    server_root: Option<PathBuf>,
    text: Mutex<Option<TextScan>>,
    /// Digest of every family and project file under `root`, when the
    /// request computed one within its bounds (see `fingerprint`).
    fingerprint: Mutex<Option<String>>,
    /// The files that fingerprint covers (relative path → stamp), for the
    /// file events a watching server is owed (see `watched_files_sync`).
    walk_entries: Mutex<Option<Arc<BTreeMap<String, String>>>>,
    /// Canonical files the language server's answer covers (listed, or
    /// verified as importers), recorded by the operation.
    answered: Mutex<BTreeSet<String>>,
    /// How a text-scan file uses a symbol (in code, only in comments or
    /// strings, or out of the declaration's reach), by symbol and canonical
    /// path; see [`Self::text_only_files`].
    code_uses: Arc<Mutex<HashMap<String, text::Use>>>,
    /// The request's anchor, whose declaration bounds the reach of the
    /// scanned files ([`Self::anchor_site`]).
    site: Mutex<Option<reach::Site>>,
    /// The reach of that declaration for a symbol, read once (`None`: no
    /// bound is known).
    reach: Arc<ReachSlot>,
    /// The anchor file (canonical) and the zero-based lines of it the answer
    /// covers, the anchor's own line included; see [`Self::anchor_gap`].
    anchor_lines: Mutex<Option<(String, BTreeSet<u32>)>>,
}

impl Scope {
    pub(super) fn new(root: String, include: Vec<String>) -> Self {
        Self {
            root,
            narrowed: false,
            include,
            server_root: None,
            text: Mutex::new(None),
            fingerprint: Mutex::new(None),
            walk_entries: Mutex::new(None),
            answered: Mutex::new(BTreeSet::new()),
            code_uses: Arc::new(Mutex::new(HashMap::new())),
            site: Mutex::new(None),
            reach: Arc::new(Mutex::new(None)),
            anchor_lines: Mutex::new(None),
        }
    }

    /// Record the request's anchor: canonical `path` and zero-based
    /// position. When it starts a declaration, that declaration's reach
    /// bounds which scanned files could reference it.
    pub(super) fn anchor_site(&self, path: &str, line: u32, character: u32) {
        if let Ok(mut slot) = self.site.lock() {
            *slot = Some(reach::Site {
                path: path.to_owned(),
                line,
                character,
            });
        }
    }

    /// The scope of a request anchored at `anchor` (a file or, for a
    /// workspace-root query, a directory), under the request's explicit
    /// `workspace_root` when it holds the anchor.
    pub(super) fn resolve(
        anchor: &str,
        server_root: &str,
        workspace_root: Option<&str>,
        policy: &PathPolicy,
        language_id: Option<&str>,
    ) -> Self {
        let anchor_path = Path::new(anchor);
        let wide = search_root(anchor_path, server_root, policy);
        let explicit = workspace_root.and_then(|root| explicit_root(anchor_path, root, policy));
        let mut scope = Self::new(
            explicit.clone().unwrap_or_else(|| wide.clone()),
            family_include(language_id, anchor_path),
        );
        scope.narrowed =
            explicit.is_some_and(|root| root != wide && Path::new(&root).starts_with(&wide));
        scope.server_root = std::fs::canonicalize(server_root).ok();
        scope
    }

    /// Record the scope fingerprint this request computed; text scans under
    /// it are shared with later requests that compute the same one.
    pub(super) fn set_fingerprint(&self, fingerprint: String) {
        if let Ok(mut slot) = self.fingerprint.lock() {
            *slot = Some(fingerprint);
        }
    }

    /// Record the walk entries behind the fingerprint.
    pub(super) fn set_walk_entries(&self, entries: Arc<BTreeMap<String, String>>) {
        if let Ok(mut slot) = self.walk_entries.lock() {
            *slot = Some(entries);
        }
    }

    /// The walk entries behind this request's fingerprint, if any.
    pub(super) fn walk_entries(&self) -> Option<Arc<BTreeMap<String, String>>> {
        self.walk_entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
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
        let text = crate::tools::source::read_bounded(
            Path::new(&file),
            octocode_engine::signatures::MAX_PARSE_SIZE,
        )
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

    /// This request's scope fingerprint ([`WalkInputs::fingerprint`]) on a
    /// blocking worker that follows `cancel`. A walk with no fingerprint
    /// (the request stopped it, or a file could not be stamped) is counted
    /// as failed in the LSP trace (`lsp.trace`): the request then reuses no
    /// response and sends no watched-file events, and that is never silent.
    pub(super) async fn walk_fingerprint(
        &self,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Fingerprint>, LspFailure> {
        let walked = self.walk_inputs();
        let measured = octocode_engine::lsp::diagnostics::Phase::ScopeFingerprint
            .measure(async {
                match blocking_cancellable(cancel, move |stopped| walked.fingerprint(stopped)).await
                {
                    Ok(Some(Some(found))) => Ok(found),
                    Ok(_) => Err(None),
                    Err(failure) => Err(Some(failure)),
                }
            })
            .await;
        match measured {
            Ok(found) => Ok(Some(found)),
            Err(None) => Ok(None),
            Err(Some(failure)) => Err(failure),
        }
    }

    /// The owned inputs of the fingerprint walk, for a blocking worker.
    pub(super) fn walk_inputs(&self) -> WalkInputs {
        WalkInputs {
            root: PathBuf::from(&self.root),
            server_root: self.server_root.clone(),
            include: self.include.clone(),
        }
    }
}

/// What the scope fingerprint walks: the scope root and its family globs.
pub(super) struct WalkInputs {
    root: PathBuf,
    server_root: Option<PathBuf>,
    include: Vec<String>,
}

impl WalkInputs {
    /// Digest of every family and project file under the scope (relative
    /// path, length, mtime; content too for a fresh mtime), so any edit,
    /// addition, or removal changes it; one walk for the queries that ask
    /// at once ([`shared_fingerprint`]). `None` when the request stopped
    /// the walk or a file could not be stamped.
    pub(super) fn fingerprint(&self, stopped: &(dyn Fn() -> bool + Sync)) -> Option<Fingerprint> {
        shared_fingerprint(
            &scan::FingerprintScope {
                root: &self.root,
                server_root: self.server_root.as_deref(),
                include: &self.include,
            },
            stopped,
        )
    }
}

#[cfg(test)]
mod tests;
