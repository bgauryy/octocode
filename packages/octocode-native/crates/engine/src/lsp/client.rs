use crate::lsp::project_config::ProjectConfigWatch;
use crate::lsp::spawn_limits;
use crate::lsp::transport::{JsonRpcConnection, ProgressTracker};
use crate::lsp::types::LspLanguageServerConfig;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;

mod absence;
mod lifecycle;
mod open_documents;
mod project_loads;
mod requests;
mod server_process;
mod snippets;
mod status;
mod stderr;
mod sync;
#[cfg(test)]
mod tests;
mod waits;
mod watched;

use open_documents::{MAX_OPEN_DOCUMENTS, OpenDocuments};
pub use absence::{AskedAt, EmptyAnswer};
pub use requests::LocationRequest;
pub use snippets::{SNIPPET_CONTENT_WITHHELD, SnippetPathAuthorizer, SnippetReadPolicy};
pub use waits::ServerWaits;

pub(super) const REQUEST_TIMEOUT_MS: u32 = 30_000;
/// After the first `didOpen` of a document, how long
/// [`NativeLspClient::open_document_and_wait`] waits by default for a
/// project load the open triggers to START (tsserver begins ~100 ms after
/// didOpen).
pub const OPEN_SETTLE_MS: u32 = 400;
/// Default bound on the whole post-open wait.
pub const OPEN_READY_TIMEOUT_MS: u32 = 15_000;

/// Options a client takes once, at construction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LspClientOptions {
    /// Longest any settle may last: the wait for a server that reports no
    /// progress to start (startup and first open) and a settled request's
    /// delay. Progress a server does report is still drained. `None` keeps
    /// each settle's own length; a harness whose fake servers answer at once
    /// sets zero.
    pub settle_cap: Option<Duration>,
}
pub(super) const CONTENT_MODIFIED_RETRIES: u8 = 3;
pub(super) const CONTENT_MODIFIED_RETRY_DELAY_MS: u64 = 500;

#[derive(Clone)]
pub struct NativeLspClient {
    inner: Arc<NativeLspClientInner>,
}

/// Lock order: `child` → `connection` → `stderr_task`. `start` and `stop`
/// both take `child` first and hold it for their whole run, so a `stop` that
/// overlaps a `start` waits for it and then tears down exactly what it
/// published (never a half-started server, and never a deadlock). Other
/// methods take `connection` alone and never while holding it acquire
/// `child`. The `StdMutex` fields are leaf locks held only for a copy.
struct NativeLspClientInner {
    config: LspLanguageServerConfig,
    child: Mutex<Option<Child>>,
    // Stored behind an `Arc` so callers can clone a handle out from under the
    // lock and then release the guard BEFORE awaiting the (potentially
    // multi-second) request, instead of serializing all LSP traffic on this
    // mutex. `JsonRpcConnection` is internally `Send + Sync` and supports
    // concurrent `request`/`notify` (its writer + pending map are each
    // `Arc<Mutex<..>>`), so cloned handles are safe to use in parallel.
    connection: Mutex<Option<Arc<JsonRpcConnection>>>,
    stderr_task: Mutex<Option<JoinHandle<()>>>,
    stderr_lines: Arc<StdMutex<VecDeque<String>>>,
    capabilities: StdMutex<Option<Value>>,
    server_info: StdMutex<Option<Value>>,
    readiness: StdMutex<Option<String>>,
    /// What callers waited on here: the start and the project loads.
    waits: StdMutex<waits::Waits>,
    progress: Arc<ProgressTracker>,
    /// One more than the progress begun when the server was last shown idle
    /// (`0`: never), see `absence.rs`.
    idle_at_begin: AtomicU64,
    /// A load outlasted the wait of an empty answer and has not been shown
    /// over since.
    load_outlasted: AtomicBool,
    /// Projects whose first open already waited (see `project_loads.rs`).
    project_loads: StdMutex<project_loads::ProjectLoads>,
    /// In-flight requests, document syncs, diagnostic waits, and caller
    /// [`LspLease`]s. Non-zero means busy: the pool never idles out or evicts
    /// a busy client.
    activity: Arc<AtomicUsize>,
    /// Serializes document syncs (and closes) from version reservation
    /// through the notification write, so `didOpen v1` always reaches the
    /// server before `didChange v2` of the same document.
    sync_lock: Mutex<()>,
    /// Open-document lifecycle state: `uri -> last sent version`, bounded by an
    /// LRU cap. Drives the LSP `didOpen` (once) → `didChange` (incrementing
    /// version) → `didClose` protocol so servers never see a second `didOpen`
    /// for the same document, and evicts the least-recently-synced document
    /// (with a `didClose`) once the cap is exceeded so the set cannot grow
    /// without bound over a long session.
    open_docs: StdMutex<OpenDocuments>,
    /// The [`CallId`] whose first request last brought the open documents
    /// in line with their files (`0`: none).
    documents_checked_for: AtomicU64,
    /// The project config files (compile database, `.clangd`) of every
    /// directory this server resolved a document through, as they were when
    /// it first looked. The server never re-reads them for a document it
    /// holds, so a change replaces the server (see
    /// [`NativeLspClient::project_config_changed`]).
    project_config: StdMutex<ProjectConfigWatch>,
    /// Windows: owns the Job Object enforcing the server's memory cap; must
    /// outlive the child and be released only after the child is reaped
    /// (closing a kill-on-close job hard-kills the tree). Unit on Unix, where
    /// the watchdog (or `pre_exec` on other Unix targets) applies the cap.
    memory_cap_guard: StdMutex<Option<crate::process::Job>>,
    /// macOS and Linux: the RSS watchdog enforcing `max_memory_mb` on the
    /// server tree. Dropped (aborted) by `stop` before the child is reaped.
    memory_watchdog: StdMutex<Option<spawn_limits::AbortOnDrop>>,
    /// The server tree's resident bytes at the watchdog's last sample; `0`
    /// before the first one or without a watchdog.
    resident_bytes: Arc<AtomicU64>,
    /// The scope's files (relative path → stamp) as of the last request,
    /// for a server that registered for `workspace/didChangeWatchedFiles`:
    /// the next request sends it the difference (see `watched.rs`).
    watched_files: StdMutex<Option<std::collections::BTreeMap<String, String>>>,
    options: LspClientOptions,
}

/// One tool call's identity, unique within the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallId(std::num::NonZeroU64);

impl CallId {
    /// An id no earlier call had.
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(id).unwrap_or(std::num::NonZeroU64::MIN))
    }

    fn get(self) -> u64 {
        self.0.get()
    }
}

mod answers;
pub use answers::ProviderAnswers;

/// A single tool call's selected provider evidence and document synchronization.
/// A fresh call owns an empty transcript. Only an issued continuation restores one.
#[derive(Clone, Debug)]
pub struct ResponseScope {
    pub generation: String,
    pub call: Option<CallId>,
    pub answers: Option<Arc<StdMutex<ProviderAnswers>>>,
}

impl Default for ResponseScope {
    fn default() -> Self {
        Self {
            generation: String::new(),
            call: Some(CallId::next()),
            answers: Some(Arc::default()),
        }
    }
}

tokio::task_local! {
    pub static RESPONSE_SCOPE: std::cell::RefCell<ResponseScope>;
}

impl NativeLspClient {
    pub fn new(config: LspLanguageServerConfig) -> Self {
        Self::with_options(config, LspClientOptions::default())
    }

    pub fn with_options(config: LspLanguageServerConfig, options: LspClientOptions) -> Self {
        // Every launch path (discovery, runtime) gets the per-server
        // safe defaults; user-supplied options still win key by key.
        let mut config = config;
        crate::lsp::config::apply_server_default_options(&mut config, None);
        let project_config = {
            let names = crate::lsp::config::project_config_files(&config.command);
            let fixed = if names.is_empty() {
                Vec::new()
            } else {
                crate::lsp::clangd_database::fixed_database_files(&config)
            };
            ProjectConfigWatch::new(names).with_files(fixed)
        };
        Self {
            inner: Arc::new(NativeLspClientInner {
                config,
                child: Mutex::new(None),
                connection: Mutex::new(None),
                stderr_task: Mutex::new(None),
                stderr_lines: Arc::new(StdMutex::new(VecDeque::new())),
                capabilities: StdMutex::new(None),
                server_info: StdMutex::new(None),
                readiness: StdMutex::new(None),
                waits: StdMutex::default(),
                progress: ProgressTracker::new(),
                idle_at_begin: AtomicU64::new(0),
                load_outlasted: AtomicBool::new(false),
                project_loads: StdMutex::default(),
                activity: Arc::new(AtomicUsize::new(0)),
                sync_lock: Mutex::new(()),
                open_docs: StdMutex::new(OpenDocuments::new(MAX_OPEN_DOCUMENTS)),
                documents_checked_for: AtomicU64::new(0),
                project_config: StdMutex::new(project_config),
                memory_cap_guard: StdMutex::new(None),
                memory_watchdog: StdMutex::new(None),
                resident_bytes: Arc::new(AtomicU64::new(0)),
                watched_files: StdMutex::new(None),
                options,
            }),
        }
    }

    /// Forget everything learned from the previous server process.
    fn reset_session_state(&self) {
        {
            let mut capabilities = self
                .inner
                .capabilities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *capabilities = None;
        }
        {
            // A new process indexes the disk afresh: no file events are owed.
            let mut watched = self
                .inner
                .watched_files
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *watched = None;
        }
        {
            let mut server_info = self
                .inner
                .server_info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *server_info = None;
        }
        {
            let mut readiness = self
                .inner
                .readiness
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *readiness = None;
        }
        self.inner.idle_at_begin.store(0, Ordering::Release);
        self.inner.load_outlasted.store(false, Ordering::Release);
        self.inner
            .project_loads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        {
            let mut open_docs = self
                .inner
                .open_docs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            open_docs.clear();
        }
        {
            let mut project_config = self
                .inner
                .project_config
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            project_config.clear();
            // Baseline taken before the server exists, so nothing it could
            // read is missing from it.
            project_config.observe(Path::new(&self.inner.config.workspace_root));
        }
    }
}

impl Drop for NativeLspClientInner {
    fn drop(&mut self) {
        self.connection.get_mut().take();
        if let Some(task) = self.stderr_task.get_mut().take() {
            task.abort();
        }
        if let Some(mut child) = self.child.get_mut().take() {
            // Signal the child to die, then opportunistically reap it so a killed
            // server does not linger as a zombie. `try_wait` collects the exit
            // status if it has already terminated; if it has not yet, `kill_on_drop`
            // on the spawn command remains the backstop when `child` is dropped
            // here (it registers the pid with tokio's orphan reaper).
            if let Some(pid) = child.id() {
                crate::process::kill_tree(pid);
            }
            let _ = child.start_kill();
            let _ = child.try_wait();
        }
        {
            let mut capabilities = self
                .capabilities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *capabilities = None;
        }
        {
            let mut server_info = self
                .server_info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *server_info = None;
        }
    }
}

/// RAII "this client is in use" guard from [`NativeLspClient::lease`].
/// Owned and `'static`, so it can be held across awaits and moved between
/// tasks; dropping it (including on cancellation) releases it. Deliberately
/// not `Clone`: each lease is one count.
#[must_use = "a lease marks the client busy only while it is held"]
pub struct LspLease {
    activity: Arc<AtomicUsize>,
}

impl LspLease {
    fn new(activity: &Arc<AtomicUsize>) -> Self {
        activity.fetch_add(1, Ordering::AcqRel);
        Self {
            activity: Arc::clone(activity),
        }
    }
}

impl Drop for LspLease {
    fn drop(&mut self) {
        self.activity.fetch_sub(1, Ordering::AcqRel);
    }
}
