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
use octocode_engine::search::walk::NestedRepositories;
use octocode_engine::types::TextSearchOptions;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

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
/// Bound of the text scan. A slower honored scan reports no files (no
/// `textOnlyFiles`, and importer recovery reports a failed scan); a slower
/// lifted scan reports the files it reached and says it did not finish
/// (see [`IgnoredMentions::unfinished`]).
const TEXT_SCAN_BUDGET: Duration = Duration::from_secs(3);
/// Bound of the scan with ignore rules lifted. What it finds is a lead
/// unless a file imports the name, so a large ignored tree is not worth a
/// longer wait on every request.
const LIFTED_SCAN_BUDGET: Duration = Duration::from_secs(1);
/// Walk threads of the lifted scan: its wall time is bounded, and this
/// bounds its CPU (a gitignored checkout of other repositories otherwise
/// keeps every core busy for the whole budget).
const LIFTED_SCAN_THREADS: u32 = 2;
/// How long a scope whose lifted scan hit its bound skips that scan. The
/// bound is reached by the size of the ignored tree, whatever the name, so
/// every later request would pay it again for the same unfinished answer.
const UNBOUNDED_IGNORED_TTL: Duration = Duration::from_secs(600);

/// Scopes (`root`, `include`, policy) whose lifted scan hit its bound.
static UNBOUNDED_IGNORED: octocode_io::memo::Memo<String, ()> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 64,
        max_weight: 64,
        keep_newest: false,
    });

/// Unfinished lifted scans by scope and symbol, for
/// [`UNBOUNDED_IGNORED_TTL`]: a repeated request replays what the first one
/// reached, so it reports the same ignored files and the same coverage
/// instead of an empty skipped scan.
static UNFINISHED_LIFTED: octocode_io::memo::Memo<String, Scanned> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 64,
        max_weight: 64,
        keep_newest: false,
    });

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
            .and_then(|bytes| String::from_utf8(bytes).ok())?;
        let lines = octocode_engine::lsp::resolver::code_occurrence_lines(&file, &text, symbol)?;
        lines
            .iter()
            .any(|line| u32::try_from(*line).is_ok_and(|line| !covered.contains(&line)))
            .then_some(file)
    }

    /// Files under the scope that spell `symbol` as a word (canonical
    /// paths), scanned once per request and symbol. `Ok(None)` when the
    /// scan failed or hit a bound.
    pub(super) async fn text_files(
        &self,
        symbol: &str,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<Option<Arc<BTreeSet<String>>>, LspFailure> {
        if let Some(scan) = self
            .text
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|scan| scan.symbol == symbol)
            .map(|scan| scan.files.clone())
        {
            return Ok(scan);
        }
        let stored = self.fingerprint().map(|fingerprint| {
            octocode_engine::digest::sha256(
                format!(
                    "{}\u{0}{}\u{0}{symbol}\u{0}{fingerprint}\u{0}{policy:?}",
                    self.root,
                    self.include.join("\u{0}")
                )
                .as_bytes(),
            )
        });
        let hit = stored
            .as_ref()
            .and_then(|key| TEXT_SCANS.get(key))
            .map(|hit| hit.value);
        let files = match hit {
            Some(files) => Some(files),
            None => {
                let files = scan_text(
                    &self.root,
                    &self.include,
                    symbol,
                    policy,
                    cancel,
                    Ignores::Honored,
                    TEXT_SCAN_BUDGET,
                )
                .await?
                .filter(|scan| scan.complete)
                .map(|scan| Arc::new(scan.files));
                if let (Some(key), Some(files)) = (stored, &files) {
                    TEXT_SCANS.insert(key, Arc::clone(files), 1, None);
                }
                files
            }
        };
        if let Ok(mut slot) = self.text.lock() {
            *slot = Some(TextScan {
                symbol: symbol.to_owned(),
                files: files.clone(),
            });
        }
        Ok(files)
    }

    /// Files under the scope that spell `symbol` as a word but that ignore
    /// rules keep out of every lexical scan (default-excluded directories
    /// such as `node_modules/` stay out), with the deciding rules of the
    /// first few; files in `skip` (already in the answer) are left out.
    /// Scanned per request: the scope fingerprint does not cover ignored
    /// files, and the walk reads every ignored file the default prune keeps
    /// (a gitignored `repos/` checkout costs seconds, so measuring it is
    /// the only way to know). A lifted scan that fails or hits
    /// [`LIFTED_SCAN_BUDGET`] says so in [`IgnoredMentions::unfinished`],
    /// with the files it reached before stopping; for
    /// [`UNBOUNDED_IGNORED_TTL`] the same symbol replays those files, and
    /// any other symbol of the scope skips the scan, reporting it
    /// unfinished.
    pub(super) async fn ignored_text_files(
        &self,
        symbol: &str,
        skip: &std::collections::HashSet<String>,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
    ) -> Result<IgnoredMentions, LspFailure> {
        self.ignored_text_files_within(symbol, skip, policy, cancel, LIFTED_SCAN_BUDGET)
            .await
    }

    async fn ignored_text_files_within(
        &self,
        symbol: &str,
        skip: &std::collections::HashSet<String>,
        policy: &PathPolicy,
        cancel: &dyn CancellationCheck,
        budget: Duration,
    ) -> Result<IgnoredMentions, LspFailure> {
        let Some(visible) = self.text_files(symbol, policy, cancel).await? else {
            return Ok(IgnoredMentions::unfinished());
        };
        let (unbounded, replay) = self.lifted_keys(symbol, policy);
        let every = match UNFINISHED_LIFTED.get(&replay) {
            Some(hit) => hit.value,
            None if UNBOUNDED_IGNORED.get(&unbounded).is_some() => {
                return Ok(IgnoredMentions::unfinished());
            }
            None => {
                let Some(every) = scan_text(
                    &self.root,
                    &self.include,
                    symbol,
                    policy,
                    cancel,
                    Ignores::Lifted,
                    budget,
                )
                .await?
                else {
                    return Ok(IgnoredMentions::unfinished());
                };
                let every = Arc::new(every);
                if !every.complete {
                    UNBOUNDED_IGNORED.insert(unbounded, (), 1, Some(UNBOUNDED_IGNORED_TTL));
                    UNFINISHED_LIFTED.insert(
                        replay,
                        Arc::clone(&every),
                        1,
                        Some(UNBOUNDED_IGNORED_TTL),
                    );
                }
                every
            }
        };
        let unfinished = !every.complete;
        let files = every
            .files
            .iter()
            .filter(|file| !visible.contains(*file) && !skip.contains(*file))
            .cloned()
            .collect::<Vec<_>>();
        let root = Path::new(&self.root);
        let mut rules = Vec::new();
        for file in files.iter().take(crate::tools::empty_walk::SAMPLE_FILES) {
            if let Some(rule) = crate::tools::empty_walk::ignore_rule(root, Path::new(file), false)
                && !rules.contains(&rule)
            {
                rules.push(rule);
            }
        }
        Ok(IgnoredMentions {
            files,
            rules,
            unfinished,
            nested_repositories: every.nested.clone(),
            ..IgnoredMentions::default()
        })
    }

    /// The memo keys of the lifted scan: the scope's (for
    /// [`UNBOUNDED_IGNORED`]) and the scope's with `symbol` (for
    /// [`UNFINISHED_LIFTED`]).
    fn lifted_keys(&self, symbol: &str, policy: &PathPolicy) -> (String, String) {
        let scope = format!(
            "{}\u{0}{}\u{0}{policy:?}",
            self.root,
            self.include.join("\u{0}")
        );
        let symbol = format!("{scope}\u{0}{symbol}");
        (scope, symbol)
    }

    /// Files the text scan of `symbol` found that the server's answer does
    /// not cover and that use the name in code: a file that spells it only
    /// in comments or strings holds no use the server could have missed (a
    /// file the grammar cannot parse counts its word matches). Past
    /// [`CODE_CHECK_MAX_FILES`] files, the rest count by their word matches.
    /// `None` when either side is unknown.
    pub(super) fn text_only_files(&self, symbol: &str) -> Option<Vec<String>> {
        let unanswered = {
            let text = self
                .text
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let files = text
                .as_ref()
                .filter(|scan| scan.symbol == symbol)?
                .files
                .clone()?;
            let answered = self
                .answered
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if answered.is_empty() {
                return None;
            }
            files
                .iter()
                .filter(|file| !answered.contains(*file))
                .cloned()
                .collect::<Vec<_>>()
        };
        Some(
            unanswered
                .into_iter()
                .enumerate()
                .filter(|(index, file)| {
                    *index >= CODE_CHECK_MAX_FILES || self.uses_in_code(symbol, file)
                })
                .map(|(_, file)| file)
                .collect(),
        )
    }

    /// Whether `file` (canonical) uses `symbol` in code; memoized per
    /// request. An unreadable or unparsable file
    /// counts as a use.
    fn uses_in_code(&self, symbol: &str, file: &str) -> bool {
        let key = format!("{symbol}\u{0}{file}");
        if let Some(known) = self
            .code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return *known;
        }
        let uses = crate::tools::source::read_bounded(Path::new(file), CODE_CHECK_MAX_BYTES)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .is_none_or(|text| {
                octocode_engine::lsp::resolver::code_occurrence_lines(file, &text, symbol)
                    .is_none_or(|lines| !lines.is_empty())
            });
        self.code_uses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, uses);
        uses
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

tokio::task_local! {
    /// Canonical forms already resolved by this request (see `canonical`).
    static CANONICAL: RefCell<HashMap<String, String>>;
}

/// Run one request with its own canonical-path memo.
pub(super) async fn with_canonical_memo<F: std::future::Future>(request: F) -> F::Output {
    CANONICAL.scope(RefCell::default(), request).await
}

/// `path` with symlinks resolved, or as given when it cannot be (gone, or
/// not local). Inside [`with_canonical_memo`] each spelling is resolved
/// once per request: a server answer names the same few files hundreds of
/// times. Outside it (blocking workers, tests) every call resolves.
/// [`canonical`] of the path a `file://` URI names.
pub(super) fn canonical_uri(uri: &str) -> String {
    canonical(&super::render::uri_to_path(uri))
}

pub(super) fn canonical(path: &str) -> String {
    let resolve = || {
        std::fs::canonicalize(path)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_owned())
    };
    CANONICAL
        .try_with(|memo| {
            if let Some(hit) = memo.borrow().get(path) {
                return hit.clone();
            }
            let resolved = resolve();
            memo.borrow_mut().insert(path.to_owned(), resolved.clone());
            resolved
        })
        .unwrap_or_else(|_| resolve())
}

/// The scope root: VCS root, else outermost workspace manifest, else the
/// server root; clamped to the policy, never the process cwd.
fn search_root(anchor: &Path, server_root: &str, policy: &PathPolicy) -> String {
    let start = if anchor.is_dir() {
        anchor
    } else {
        anchor.parent().unwrap_or(anchor)
    };
    let chosen = start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .or_else(|| {
            start
                .ancestors()
                .filter(|dir| octocode_engine::lsp::workspace::is_workspace_manifest_root(dir))
                .last()
        })
        .map_or_else(|| PathBuf::from(server_root), Path::to_path_buf);
    let authorized = |dir: &Path| {
        policy
            .validate(dir)
            .ok()
            .filter(|valid| valid.canonical.is_dir())
            .map(|valid| valid.canonical.to_string_lossy().into_owned())
    };
    // The chosen root must contain the anchor and be readable; otherwise
    // the widest authorized directory on the anchor's own path.
    if start.starts_with(&chosen)
        && let Some(root) = authorized(&chosen)
    {
        return root;
    }
    widest_authorized_dir(start, policy).unwrap_or_else(|| start.to_string_lossy().into_owned())
}

/// The outermost directory of the unbroken run of authorized directories
/// from `start` upward (canonical): the widest root the policy allows on
/// `start`'s own path. `None` when `start` itself is not authorized.
pub(super) fn widest_authorized_dir(start: &Path, policy: &PathPolicy) -> Option<String> {
    start
        .ancestors()
        .map_while(|dir| {
            policy
                .validate(dir)
                .ok()
                .filter(|valid| valid.canonical.is_dir())
                .map(|valid| valid.canonical.to_string_lossy().into_owned())
        })
        .last()
}

/// `*.ext` globs of every grammar in `language_id`'s family; the anchor's
/// own extension for a language the registry does not know.
fn family_include(language_id: Option<&str>, anchor: &Path) -> Vec<String> {
    let family: Vec<&str> = match language_id {
        Some(id) => {
            octocode_engine::lsp::family::family_of(id).map_or_else(|| vec![id], <[&str]>::to_vec)
        }
        None => Vec::new(),
    };
    let mut globs = octocode_engine::signatures::languages::all_entries()
        .iter()
        .filter(|entry| entry.language_id.is_some_and(|id| family.contains(&id)))
        .flat_map(|entry| entry.extensions.iter())
        .map(|extension| format!("*.{extension}"))
        .collect::<Vec<_>>();
    if globs.is_empty()
        && let Some(extension) = anchor.extension().filter(|_| !anchor.is_dir())
    {
        globs.push(format!("*.{}", extension.to_string_lossy()));
    }
    globs
}

/// Files that spell a name but that ignore rules keep out of the lexical
/// scans (see [`Scope::ignored_text_files`]).
#[derive(Debug, Default)]
pub(super) struct IgnoredMentions {
    /// Canonical paths, sorted.
    pub(super) files: Vec<String>,
    /// Distinct deciding rules (`.gitignore:1 'gen/'`) of the first files.
    pub(super) rules: Vec<String>,
    /// The scan failed or stopped at its bound: `files` is what it reached,
    /// and ignored files may mention the name beyond it.
    pub(super) unfinished: bool,
    /// Of `files`, those that import the name: the evidence that ignore
    /// rules hide an importer. A file that only spells the name (a bundled
    /// copy declaring its own) is not one.
    pub(super) importing: Vec<String>,
    /// Of `files`, those the import check could not decide (unreadable,
    /// too large, unparsable, or past its file bound): not importers, and
    /// not ruled out.
    pub(super) unchecked: Vec<String>,
    /// Nested repositories (root-relative) the scan did not enter
    /// ([`octocode_engine::search::walk::NestedRepositories`]): ignored
    /// files in them are not counted.
    pub(super) nested_repositories: Vec<String>,
}

impl IgnoredMentions {
    /// A scan that produced nothing: no file is known, and that is not
    /// proof of none.
    pub(super) fn unfinished() -> Self {
        Self {
            unfinished: true,
            ..Self::default()
        }
    }
}

/// What a text scan reached. `complete` is false when the scan stopped at
/// its bound (cancelled by the budget before the walk finished).
struct Scanned {
    files: BTreeSet<String>,
    complete: bool,
    /// A lifted scan's nested repositories it did not enter, root-relative
    /// and sorted.
    nested: Vec<String>,
}

/// The read policy of a lifted scan, which also leaves nested repositories
/// out ([`NestedRepositories`]).
struct Lifted {
    policy: PathPolicy,
    nested: Arc<NestedRepositories>,
}

impl octocode_engine::search::text_search::TextSearchPathFilter for Lifted {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        self.policy.allows(path, is_dir) && !self.nested.skips(path, is_dir)
    }

    fn skipped_hidden(&self, path: &Path, is_dir: bool) {
        self.policy.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &Path, is_dir: bool) {
        self.policy.skipped_ignored(path, is_dir);
    }
}

/// Whether a text scan honors ignore rules (every lexical scan does) or
/// lifts them to find what they hide.
#[derive(Clone, Copy)]
enum Ignores {
    Honored,
    Lifted,
}

/// Word-bounded files-only scan of `root` (family `include`, read policy),
/// observing `cancel` and the scan bounds. A scan that lifts ignore rules
/// still skips the default-excluded directories and nested repositories.
async fn scan_text(
    root: &str,
    include: &[String],
    symbol: &str,
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
    ignores: Ignores,
    budget: Duration,
) -> Result<Option<Scanned>, LspFailure> {
    if symbol.trim().is_empty() {
        return Ok(None);
    }
    let lifted = matches!(ignores, Ignores::Lifted);
    let options = TextSearchOptions {
        path: root.to_owned(),
        pattern: word_pattern(symbol),
        output: octocode_engine::types::SearchOutput::FilesWithMatch,
        include: (!include.is_empty()).then(|| include.to_vec()),
        ignore: octocode_engine::search::walk::IgnorePolicy {
            include_ignored: lifted,
            include_hidden: false,
        },
        exclude_dir: lifted.then(|| crate::policy::prune::directories(true)),
        walk_threads: lifted.then_some(LIFTED_SCAN_THREADS),
        ..TextSearchOptions::default()
    };
    let nested = Arc::new(NestedRepositories::new(Path::new(root)));
    let filter: Arc<dyn octocode_engine::search::text_search::TextSearchPathFilter> = if lifted {
        Arc::new(Lifted {
            policy: policy.clone(),
            nested: Arc::clone(&nested),
        })
    } else {
        Arc::new(policy.clone())
    };
    let base = root.to_owned();
    let scanned = blocking_cancellable(cancel, move |stopped| {
        let started = Instant::now();
        let bounded = || stopped() || started.elapsed() > budget;
        search_cancellable(options, filter, &bounded)
            .ok()
            .map(|parsed| Scanned {
                nested: nested.skipped(),
                complete: !parsed
                    .stats
                    .cap_reasons
                    .contains(&octocode_engine::types::CapReason::Cancelled),
                files: parsed
                    .files
                    .into_iter()
                    .map(|file| {
                        let path = Path::new(&file.path);
                        if path.is_absolute() {
                            canonical(&file.path)
                        } else {
                            canonical(&Path::new(&base).join(path).to_string_lossy())
                        }
                    })
                    .collect::<BTreeSet<_>>(),
            })
    })
    .await?
    .flatten();
    Ok(scanned)
}

fn fingerprint(
    root: &Path,
    include: &[String],
    stopped: &(dyn Fn() -> bool + Sync),
    budget: Duration,
) -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let extensions = include
        .iter()
        .filter_map(|glob| glob.strip_prefix("*."))
        .collect::<Vec<_>>();
    let started = Instant::now();
    let now = SystemTime::now();
    let entries = Mutex::new(Vec::new());
    let abandoned = AtomicBool::new(false);
    // One digest line per family or project file: relative path, length,
    // mtime, and the content hash of a freshly modified file.
    let line = |entry: ignore::DirEntry| -> Option<String> {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            return None;
        }
        let path = entry.path();
        let name = path.file_name().map(|name| name.to_string_lossy());
        let family = path.extension().is_some_and(|extension| {
            extensions.is_empty() || extensions.contains(&extension.to_string_lossy().as_ref())
        });
        if !family && !name.is_some_and(|name| PROJECT_FILES.contains(&name.as_ref())) {
            return None;
        }
        let meta = entry.metadata().ok()?;
        let Some(stamp) = crate::tools::source::cache_stamp(&meta) else {
            abandoned.store(true, Ordering::Relaxed);
            return None;
        };
        let modified = meta.modified().ok();
        let fresh = modified
            .is_none_or(|time| !now.duration_since(time).is_ok_and(|age| age >= FRESH_MTIME));
        let content = if fresh {
            match crate::tools::source::read_bounded(path, FRESH_CONTENT_MAX_BYTES) {
                Ok(bytes) => octocode_engine::digest::sha256(&bytes),
                Err(_) => {
                    abandoned.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        } else {
            String::new()
        };
        let relative = path.strip_prefix(root).unwrap_or(path);
        Some(format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{content}",
            relative.to_string_lossy(),
            stamp.size,
            stamp.modified_ns,
            stamp.changed_ns,
            stamp.inode,
        ))
    };
    // A parallel walk: the digest is order-free (lines are sorted).
    crate::tools::pruned_walk(root, crate::tools::syntax_prune([]))
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if abandoned.load(Ordering::Relaxed) {
                    return ignore::WalkState::Quit;
                }
                if stopped() || started.elapsed() > budget {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                let Some(line) = entry.ok().and_then(line) else {
                    return ignore::WalkState::Continue;
                };
                let Ok(mut entries) = entries.lock() else {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                };
                entries.push(line);
                if entries.len() > FINGERPRINT_MAX_FILES {
                    abandoned.store(true, Ordering::Relaxed);
                    return ignore::WalkState::Quit;
                }
                ignore::WalkState::Continue
            })
        });
    if abandoned.load(Ordering::Relaxed) {
        return None;
    }
    let mut entries = entries.into_inner().ok()?;
    entries.sort_unstable();
    Some(octocode_engine::digest::sha256(
        entries.join("\n").as_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_support::workspace_policy;

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().canonicalize().expect("canonical");
        (dir, root)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }

    fn resolve(file: &Path, server_root: &Path, policy_root: &Path, id: &str) -> Scope {
        Scope::resolve(
            &file.to_string_lossy(),
            &server_root.to_string_lossy(),
            &workspace_policy(policy_root),
            Some(id),
        )
    }

    #[test]
    fn scope_prefers_vcs_root_over_nearest_package() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join(".git")).expect("git");
        write(
            &root.join("package.json"),
            r#"{"workspaces":["packages/*"]}"#,
        );
        write(&root.join("packages/a/package.json"), "{}");
        let file = root.join("packages/a/src/x.ts");
        write(&file, "export const x = 1;\n");
        let scope = resolve(&file, &root.join("packages/a"), &root, "typescript");
        assert_eq!(scope.root, root.to_string_lossy());
        for glob in ["*.ts", "*.tsx", "*.js", "*.mts", "*.cjs"] {
            assert!(
                scope.include.iter().any(|g| g == glob),
                "{glob}: {:?}",
                scope.include
            );
        }
        assert!(!scope.include.iter().any(|g| g == "*.rs"));
    }

    #[test]
    fn scope_without_vcs_uses_outermost_cargo_workspace() {
        let (_dir, root) = fixture();
        write(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"tokio\"]\n",
        );
        write(
            &root.join("tokio/Cargo.toml"),
            "[package]\nname = \"tokio\"\n",
        );
        let file = root.join("tokio/src/lib.rs");
        write(&file, "pub fn spawn_blocking() {}\n");
        let scope = resolve(&file, &root.join("tokio"), &root, "rust");
        assert_eq!(scope.root, root.to_string_lossy());
        assert_eq!(scope.include, vec!["*.rs".to_owned()]);
    }

    #[test]
    fn scope_is_clamped_to_policy_root_and_never_cwd() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join("repo/.git")).expect("git");
        let file = root.join("repo/packages/a/x.ts");
        write(&file, "x\n");
        let packages = root.join("repo/packages");
        let scope = resolve(
            &file,
            &root.join("repo/packages/a"),
            &packages,
            "typescript",
        );
        assert_eq!(scope.root, packages.to_string_lossy());
        // No marker anywhere and a server root outside the policy: the
        // widest authorized directory on the file's path, not the cwd.
        let lone = root.join("lone/deep/y.py");
        write(&lone, "y = 1\n");
        let cwd = std::env::current_dir().expect("cwd");
        let scope = resolve(&lone, &cwd, &root.join("lone"), "python");
        assert_eq!(scope.root, root.join("lone").to_string_lossy());
        assert_eq!(scope.include, vec!["*.py".to_owned(), "*.pyi".to_owned()]);
    }

    #[test]
    fn fingerprint_changes_on_edit_add_delete() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export const a = 1;\n");
        write(&root.join("notes.md"), "notes\n");
        // A generous bound: the test measures what the digest covers, not
        // the walk's speed under a loaded test runner.
        let scope = |stopped: &(dyn Fn() -> bool + Sync)| {
            fingerprint(
                &root,
                &["*.ts".to_owned()],
                stopped,
                Duration::from_secs(60),
            )
        };
        let never = || false;
        let first = scope(&never).expect("fingerprint");
        assert_eq!(scope(&never).as_ref(), Some(&first));
        // An unrelated extension does not change it.
        write(&root.join("notes.md"), "other notes\n");
        assert_eq!(scope(&never).as_ref(), Some(&first));
        // A same-length edit within the mtime granularity still does.
        write(&root.join("a.ts"), "export const a = 2;\n");
        let edited = scope(&never).expect("edited");
        assert_ne!(edited, first);
        write(&root.join("b.ts"), "export const b = 1;\n");
        let added = scope(&never).expect("added");
        assert_ne!(added, edited);
        std::fs::remove_file(root.join("b.ts")).expect("remove");
        assert_eq!(scope(&never).as_ref(), Some(&edited));
        // A project file edit changes it too.
        write(&root.join("tsconfig.json"), "{}");
        assert_ne!(scope(&never).as_ref(), Some(&edited));
        // A stopped walk reports no fingerprint.
        assert_eq!(scope(&|| true), None);
    }

    #[test]
    fn fingerprint_changes_after_same_size_edit_with_restored_old_mtime() {
        let (_dir, root) = fixture();
        let path = root.join("a.ts");
        write(&path, "export const a = 1;\n");
        let old_time = std::time::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        let set_old_time = || {
            std::fs::File::open(&path)
                .expect("file")
                .set_times(std::fs::FileTimes::new().set_modified(old_time))
                .expect("mtime");
        };
        set_old_time();
        let digest = || {
            fingerprint(
                &root,
                &["*.ts".to_owned()],
                &|| false,
                Duration::from_secs(60),
            )
            .expect("fingerprint")
        };
        let first = digest();
        write(&path, "export const a = 2;\n");
        set_old_time();
        assert_ne!(digest(), first);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn text_only_files_are_the_scanned_files_the_answer_misses() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export function f() {}\n");
        write(&root.join("b.ts"), "import { f } from './a';\nf();\n");
        write(&root.join("c.ts"), "// f is mentioned here\n");
        write(&root.join("d.rs"), "fn f() {}\n");
        write(
            &root.join("e.ts"),
            "const label = 'f';\nexport const g = () => f();\n",
        );
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
        let policy = workspace_policy(&root);
        let cancel = crate::execution::cancel::NeverCancel;
        let files = scope
            .text_files("f", &policy, &cancel)
            .await
            .expect("scan")
            .expect("files");
        assert_eq!(files.len(), 4, "{files:?}");
        // Unknown until the operation records its answer.
        assert_eq!(scope.text_only_files("f"), None);
        scope.answer([root.join("a.ts").to_string_lossy()]);
        scope.answer([format!("file://{}", root.join("b.ts").display())]);
        // c.ts only mentions the name in a comment: no use was missed.
        assert_eq!(
            scope.text_only_files("f"),
            Some(vec![root.join("e.ts").to_string_lossy().into_owned()])
        );
        assert_eq!(scope.text_only_files("g"), None);
    }

    /// The text check parses at most [`CODE_CHECK_MAX_FILES`] unanswered
    /// files, the bound the ignored-importer check shares; a file past it is
    /// not proven free of a use, so it counts as one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn text_check_files_past_the_parse_bound_count_as_uses() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export function f() {}\n");
        for index in 0..CODE_CHECK_MAX_FILES + 2 {
            write(
                &root.join(format!("c{index}.ts")),
                "// f is mentioned here\n",
            );
        }
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
        let files = scope
            .text_files(
                "f",
                &workspace_policy(&root),
                &crate::execution::cancel::NeverCancel,
            )
            .await
            .expect("scan")
            .expect("files");
        assert_eq!(files.len(), CODE_CHECK_MAX_FILES + 3, "{files:?}");
        scope.answer([root.join("a.ts").to_string_lossy()]);
        let uses = scope.text_only_files("f").expect("checked");
        assert_eq!(uses.len(), 2, "{uses:?}");
        assert!(uses.iter().all(|file| !file.ends_with("a.ts")), "{uses:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn text_scan_is_reused_under_an_unchanged_fingerprint() {
        let (_dir, root) = fixture();
        write(&root.join("a.ts"), "export function f() {}\n");
        write(&root.join("b.ts"), "f();\n");
        let policy = workspace_policy(&root);
        let cancel = &crate::execution::cancel::NeverCancel;
        let include = vec!["*.ts".to_owned()];
        let scan = |fingerprint: Option<&str>| {
            let scope = Scope::new(root.to_string_lossy().into_owned(), include.clone());
            if let Some(fingerprint) = fingerprint {
                scope.set_fingerprint(fingerprint.to_owned());
            }
            let policy = policy.clone();
            async move {
                scope
                    .text_files("f", &policy, cancel)
                    .await
                    .expect("scan")
                    .expect("files")
                    .len()
            }
        };
        assert_eq!(scan(Some("fp")).await, 2);
        // A new mention under the same fingerprint string: the earlier
        // scan answers (the fingerprint is what proves the files unchanged),
        // so no file is read again.
        write(&root.join("c.ts"), "f();\n");
        assert_eq!(scan(Some("fp")).await, 2);
        // Another fingerprint, or none (walk past its bounds), scans again.
        assert_eq!(scan(Some("fp2")).await, 3);
        write(&root.join("d.ts"), "f();\n");
        assert_eq!(scan(None).await, 4);
    }

    /// A lifted scan that finishes lists the ignored files; one that stops at
    /// its bound says so instead of reading as "no ignored file".
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_ignored_scan_past_its_bound_is_unfinished_not_empty() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join(".git")).expect("git");
        write(&root.join(".gitignore"), "gen/\n");
        write(&root.join("a.ts"), "export function f() {}\n");
        write(&root.join("b.ts"), "f();\n");
        for index in 0..50 {
            write(&root.join(format!("gen/g{index}.ts")), "f();\n");
        }
        let policy = workspace_policy(&root);
        let cancel = &crate::execution::cancel::NeverCancel;
        let skip = std::collections::HashSet::new();
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);

        let finished = scope
            .ignored_text_files("f", &skip, &policy, cancel)
            .await
            .expect("scan");
        assert!(!finished.unfinished);
        assert_eq!(finished.files.len(), 50);
        assert_eq!(finished.rules, [".gitignore:1 'gen/'"]);

        let stopped = scope
            .ignored_text_files_within("f", &skip, &policy, cancel, Duration::ZERO)
            .await
            .expect("scan");
        assert!(stopped.unfinished, "{stopped:?}");
        assert!(stopped.files.len() < 50, "{stopped:?}");
        // The visible files never count as ignored, finished or not.
        assert!(stopped.files.iter().all(|file| file.contains("/gen/")));
        // The scope's ignored tree outlasted the bound: later requests skip
        // the lifted scan, for any name, and report it unfinished.
        let skipped = scope
            .ignored_text_files("other", &skip, &policy, cancel)
            .await
            .expect("scan");
        assert!(
            skipped.unfinished && skipped.files.is_empty(),
            "{skipped:?}"
        );
    }

    /// The lifted scan does not enter a nested repository (a gitignored
    /// checkout of another project): its files are not this repository's
    /// ignored files, and the scan names the repository it left out.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_lifted_scan_skips_nested_repositories_and_names_them() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join(".git")).expect("git");
        write(&root.join(".gitignore"), "gen/\nrepos/\n");
        write(&root.join("a.ts"), "export function f() {}\n");
        write(&root.join("gen/g.ts"), "f();\n");
        write(
            &root.join("repos/other/.git/HEAD"),
            "ref: refs/heads/main\n",
        );
        write(&root.join("repos/other/x.ts"), "f();\n");
        let policy = workspace_policy(&root);
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);
        let found = scope
            .ignored_text_files(
                "f",
                &std::collections::HashSet::new(),
                &policy,
                &crate::execution::cancel::NeverCancel,
            )
            .await
            .expect("scan");
        assert!(!found.unfinished, "{found:?}");
        assert_eq!(
            found.files,
            [root.join("gen/g.ts").to_string_lossy().into_owned()],
            "{found:?}"
        );
        assert_eq!(found.nested_repositories, ["repos/other"], "{found:?}");
    }

    /// A warm request answers like the cold one: the symbol whose lifted
    /// scan stopped at its bound replays the files that scan reached, still
    /// unfinished, instead of reading as a skipped scan that found nothing.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_unfinished_ignored_scan_replays_what_it_reached() {
        let (_dir, root) = fixture();
        std::fs::create_dir_all(root.join(".git")).expect("git");
        write(&root.join(".gitignore"), "gen/\n");
        write(&root.join("a.ts"), "export function f() {}\n");
        let reached = root.join("gen/g.ts").to_string_lossy().into_owned();
        write(Path::new(&reached), "f();\n");
        let policy = workspace_policy(&root);
        let cancel = &crate::execution::cancel::NeverCancel;
        let skip = std::collections::HashSet::new();
        let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".to_owned()]);
        // What a cold scan reached before its bound (the bound is timing,
        // so the cold scan's reach is stated rather than raced).
        let (unbounded, replay) = scope.lifted_keys("f", &policy);
        UNBOUNDED_IGNORED.insert(unbounded, (), 1, Some(UNBOUNDED_IGNORED_TTL));
        UNFINISHED_LIFTED.insert(
            replay,
            Scanned {
                files: BTreeSet::from([reached.clone()]),
                complete: false,
                nested: Vec::new(),
            },
            1,
            Some(UNBOUNDED_IGNORED_TTL),
        );
        for _ in 0..2 {
            let warm = scope
                .ignored_text_files("f", &skip, &policy, cancel)
                .await
                .expect("scan");
            assert!(warm.unfinished, "{warm:?}");
            assert_eq!(warm.files, std::slice::from_ref(&reached), "{warm:?}");
            assert_eq!(warm.rules, [".gitignore:1 'gen/'"], "{warm:?}");
        }
    }

    /// The widest authorized directory on a path: the policy root for a
    /// file under it, nothing for a path outside every root.
    #[test]
    fn the_widest_authorized_dir_is_the_policy_root_on_the_path() {
        let (_dir, root) = fixture();
        write(&root.join("ws/crates/a/src/lib.rs"), "fn f() {}\n");
        let policy = workspace_policy(&root.join("ws/crates"));
        let canonical_crates = root.join("ws/crates").to_string_lossy().into_owned();
        assert_eq!(
            widest_authorized_dir(&root.join("ws/crates/a/src"), &policy),
            Some(canonical_crates)
        );
        assert_eq!(widest_authorized_dir(&root.join("ws"), &policy), None);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn canonical_paths_are_memoized_per_request() {
        let (_dir, root) = fixture();
        write(&root.join("one/x.ts"), "x\n");
        write(&root.join("two/x.ts"), "x\n");
        let link = root.join("link");
        std::os::unix::fs::symlink(root.join("one"), &link).expect("link");
        let spelled = link.join("x.ts").to_string_lossy().into_owned();
        let first = root.join("one/x.ts").to_string_lossy().into_owned();
        let second = root.join("two/x.ts").to_string_lossy().into_owned();
        let seen = with_canonical_memo(async {
            let before = canonical(&spelled);
            std::fs::remove_file(&link).expect("unlink");
            std::os::unix::fs::symlink(root.join("two"), &link).expect("relink");
            (before, canonical(&spelled))
        })
        .await;
        // One request resolves a spelling once; the next request sees the
        // repointed link.
        assert_eq!(seen, (first.clone(), first));
        assert_eq!(
            with_canonical_memo(async { canonical(&spelled) }).await,
            second
        );
        assert_eq!(canonical(&spelled), second);
    }
}
