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
//!
//! Which files a scan reads outlives the request while a real check proves it
//! unchanged (`listing`, `ignored_index`), and so does what each file spells
//! while its stamp does (`spelling_index`).

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

mod answered;
pub(super) use answered::Gap;
mod code_use_memo;
mod dependents;
pub(super) use dependents::Dependents;
mod fingerprint;
mod ignored_index;
mod listing;
#[cfg(test)]
mod path_counter;
mod paths;
#[cfg(test)]
pub(super) mod probe;
mod question;
mod reach;
pub(super) use reach::{local_packages, package_name, package_of};
mod scan;
mod spelling_index;
mod text;
use fingerprint::{Fingerprint, fingerprint};
pub(super) use fingerprint::{same_file, written_since};
pub(super) use paths::{
    canonical, canonical_root, canonical_uri, judged_files, widest_authorized_dir,
    with_canonical_memo, with_judged_files,
};
pub(super) use question::Question;
pub(super) use scan::IgnoredMentions;
use scan::scan_text;
pub(super) use text::IgnoredSpelling;
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

/// A file modified this recently before a fingerprint's instant also hashes
/// its content: some filesystems keep coarse mtimes, so a same-second edit
/// keeps its length and mtime. The fingerprint proves a continuation's
/// scope revision and the watched-file diff, so both need it.
pub(super) const FRESH_MTIME: Duration = Duration::from_secs(2);

/// The reach of the anchor's declaration of one symbol, once read (`None`:
/// no bound is known).
type ReachSlot = Mutex<Option<(String, Option<Arc<reach::Reach>>)>>;

/// The files a word-bounded name appears in, for one symbol.
struct TextScan {
    symbol: String,
    /// Canonical paths; `None` when the scan failed.
    files: Option<Arc<BTreeSet<String>>>,
    /// Canonical paths of the markup files it found ([`Scope::markup`]).
    markup: Arc<BTreeSet<String>>,
    /// Every file the scan listed ([`Scope::listed_files`]); `None` when it
    /// failed.
    listed: Option<Arc<[PathBuf]>>,
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
    /// Neither a repository nor a workspace manifest says where this project
    /// ends, so the scan stopped at the server's root: a module beside it is out
    /// of sight, and no answer can speak for it.
    pub(super) unbounded: bool,
    /// For an unbounded scope: the project directories beside the server's root
    /// (canonical), each holding a project marker; what its answer cannot speak for.
    /// Found lazily ([`Scope::resolve_siblings`]).
    pub(super) siblings: std::sync::OnceLock<Vec<String>>,
    /// The read policy that authorizes the lookup of `siblings`; set only for
    /// an unbounded scope.
    policy: Option<PathPolicy>,
    /// This request has no finished index behind it: text chose which files the
    /// server could read, so its answer is partial.
    opened_by_text: std::sync::atomic::AtomicBool,
    /// Globs of the markup the same scan reads, whose imports the anchor's
    /// server does not see (`importers::flag_markup_importers`).
    markup: Vec<String>,
    /// The language server's workspace root (canonical), whose ignored
    /// files the server may still compile; `None` when unknown.
    server_root: Option<PathBuf>,
    /// The server is rust-analyzer with build scripts off, so the Cargo
    /// cache directories it leaves out bear on no answer
    /// ([`Self::serve_with`]).
    cargo_caches_unloaded: bool,
    text: Mutex<Option<TextScan>>,
    /// The ignored files that use the name in code, once checked
    /// ([`Self::check_ignored_spellings`]).
    ignored_spellings: Mutex<Option<text::IgnoredSpelling>>,
    /// This request's one scope walk, once made ([`Self::walked`]).
    walked: Mutex<Option<Walked>>,
    /// What the request asks for ([`Self::asks`]).
    question: std::sync::OnceLock<Question>,
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
    /// The zero-based lines the answer lists in each file it covers, the
    /// anchor's own line included; see [`Self::answer_gaps`].
    listed: Mutex<answered::Listed>,
    /// What [`Self::answer_gaps`] found, by symbol, once looked for.
    gaps: Mutex<Option<(String, Arc<Vec<answered::Gap>>)>>,
    /// The other projects that may use the anchor ([`Self::dependents`]).
    dependents: std::sync::OnceLock<Dependents>,
    /// Language server discovery of the request: what roots each scanned
    /// file's own server ([`Self::project_root_of`]).
    discovery: octocode_engine::lsp::config::LspDiscoveryOptions,
}

impl Scope {
    pub(super) fn new(root: String, include: Vec<String>) -> Self {
        Self {
            root,
            narrowed: false,
            unbounded: false,
            siblings: std::sync::OnceLock::new(),
            policy: None,
            opened_by_text: std::sync::atomic::AtomicBool::new(false),
            include,
            markup: Vec::new(),
            server_root: None,
            cargo_caches_unloaded: false,
            text: Mutex::new(None),
            ignored_spellings: Mutex::new(None),
            question: std::sync::OnceLock::new(),
            walked: Mutex::new(None),
            answered: Mutex::new(BTreeSet::new()),
            code_uses: Arc::new(Mutex::new(HashMap::new())),
            site: Mutex::new(None),
            reach: Arc::new(Mutex::new(None)),
            listed: Mutex::default(),
            gaps: Mutex::new(None),
            dependents: std::sync::OnceLock::new(),
            discovery: octocode_engine::lsp::config::LspDiscoveryOptions::default(),
        }
    }

    /// Record the request's anchor: canonical `path` and zero-based
    /// position. When it starts a declaration, that declaration's reach
    /// bounds which scanned files could reference it.
    pub(super) fn anchor_site(&self, path: &str, line: u32, character: u32) {
        *self
            .site
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(reach::Site {
            path: path.to_owned(),
            line,
            character,
        });
    }

    pub(super) fn mark_opened_by_text(&self) {
        self.opened_by_text
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn opened_by_text(&self) -> bool {
        self.opened_by_text
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether this request already walked its scope.
    pub(super) fn is_walked(&self) -> bool {
        self.walked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// The request's one scope walk: the first caller walks (as of
    /// `observed`, see [`Self::walk_fingerprint`]) and every later caller
    /// reuses it, whatever instant it would have observed. Continuations use
    /// this pinned observation; watched-file synchronization always captures a
    /// current fingerprint under its lock and may supply the first observation. A stopped
    /// walk is not kept.
    pub(super) async fn walked(
        &self,
        observed: SystemTime,
        cancel: &dyn CancellationCheck,
    ) -> Result<Walked, LspFailure> {
        if let Some(walked) = self
            .walked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            return Ok(walked);
        }
        let walked = Walked {
            observed,
            fingerprint: self.walk_fingerprint(observed, cancel).await?,
        };
        Ok(self.remember_walk(walked))
    }

    /// Keep the first observation for continuation proof. A successful watcher
    /// capture can supply it, but must never replace a continuation's pinned one.
    pub(super) fn remember_walk(&self, walked: Walked) -> Walked {
        self.walked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_insert(walked)
            .clone()
    }

    /// This request's scope fingerprint as of `observed`
    /// ([`WalkInputs::fingerprint`]) on a blocking worker that follows
    /// `cancel`. After a walk with no fingerprint (the request stopped it, or
    /// a file could not be stamped or digested), watched-file synchronization
    /// must refuse to accept the server's index as current.
    pub(super) async fn walk_fingerprint(
        &self,
        observed: SystemTime,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Fingerprint>, LspFailure> {
        let walked = self.walk_inputs();
        #[cfg(test)]
        let probe = probe::current();
        #[cfg(test)]
        if let Some(work) = &probe {
            work.walks
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let found =
            blocking_cancellable(cancel, move |stopped| walked.fingerprint(observed, stopped))
                .await?
                .flatten();
        #[cfg(test)]
        if let (Some(work), Some(found)) = (&probe, &found) {
            work.stamped
                .fetch_add(found.1.len(), std::sync::atomic::Ordering::Relaxed);
        }
        Ok(found)
    }

    /// The owned inputs of the fingerprint walk, for a blocking worker.
    pub(super) fn walk_inputs(&self) -> WalkInputs {
        WalkInputs {
            root: PathBuf::from(&self.root),
            server_root: self.server_root.clone(),
            include: self.include.clone(),
            cargo_caches_unloaded: self.cargo_caches_unloaded,
        }
    }
}

/// A scope walk: the instant it observed the scope at, and its
/// fingerprint (`None` when the walk proved none).
#[derive(Clone)]
pub(super) struct Walked {
    pub(super) observed: SystemTime,
    pub(super) fingerprint: Option<Fingerprint>,
}

/// What the scope fingerprint walks: the scope root and its family globs.
pub(super) struct WalkInputs {
    root: PathBuf,
    server_root: Option<PathBuf>,
    include: Vec<String>,
    cargo_caches_unloaded: bool,
}

impl WalkInputs {
    /// Digest of every family and project file under the scope (relative
    /// path, length, mtime; content too for an mtime near `observed`), so
    /// any edit, addition, or removal changes it. `None` when the request
    /// stopped the walk or a file could not be stamped.
    pub(super) fn fingerprint(
        &self,
        observed: SystemTime,
        stopped: &(dyn Fn() -> bool + Sync),
    ) -> Option<Fingerprint> {
        fingerprint(
            &fingerprint::FingerprintScope {
                root: &self.root,
                server_root: self.server_root.as_deref(),
                include: &self.include,
                cargo_caches_unloaded: self.cargo_caches_unloaded,
            },
            observed,
            stopped,
        )
    }
}

#[cfg(test)]
mod tests;
