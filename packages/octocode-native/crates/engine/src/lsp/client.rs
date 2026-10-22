use crate::error::{Error, Result};
use crate::lsp::project_config::ProjectConfigWatch;
use crate::lsp::spawn_limits;
use crate::lsp::transport::{
    ClientRequestContext, CpuClock, JsonRpcConnection, ProgressTracker, Readiness,
    STARTUP_SETTLE_MS, WaitBudget,
};
use crate::lsp::types::{LspCodeSnippet, LspExactPosition, LspLanguageServerConfig, LspRange};
use crate::lsp::uri::{path_to_uri, uri_to_path};
use crate::stamp::SourceStamp;
use crate::text::LineIndex;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, ChildStderr};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

pub(super) const REQUEST_TIMEOUT_MS: u32 = 30_000;
/// After the first `didOpen` of a document, how long
/// [`NativeLspClient::open_document_and_wait`] waits by default for a
/// project load the open triggers to START (tsserver begins ~100 ms after
/// didOpen).
pub const OPEN_SETTLE_MS: u32 = 400;
/// Default bound on the whole post-open wait.
pub const OPEN_READY_TIMEOUT_MS: u32 = 15_000;
/// Largest post-open wait any caller may ask for.
const MAX_OPEN_READY_TIMEOUT_MS: u32 = 60_000;

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
const STDERR_RING_CAPACITY: usize = 100;
const STDERR_LINE_MAX_CHARS: usize = 2_000;
/// Bytes of one stderr line kept in memory (4 × the char cap, the UTF-8
/// worst case). The rest of an overlong line is read and discarded.
const STDERR_LINE_MAX_BYTES: usize = STDERR_LINE_MAX_CHARS * 4;
const MAX_SNIPPET_SOURCE_BYTES: u64 = super::MAX_LSP_SOURCE_BYTES;
/// Snippet `content` for a location whose file the [`SnippetReadPolicy`]
/// refused. The location itself is kept so the caller can apply its own
/// policy to it; no byte of the file was read.
pub const SNIPPET_CONTENT_WITHHELD: &str = "[content withheld — path not authorized for reading]";
/// Methods sent through the ContentModified/ServerCancelled retry loop,
/// advertised as `general.staleRequestSupport.retryOnContentModified`.
const RETRIED_METHODS: &[&str] = &[
    "textDocument/definition",
    "textDocument/references",
    "textDocument/hover",
    "textDocument/typeDefinition",
    "textDocument/implementation",
    "textDocument/documentSymbol",
    "textDocument/prepareCallHierarchy",
    "callHierarchy/incomingCalls",
    "callHierarchy/outgoingCalls",
    "workspace/symbol",
    "textDocument/prepareTypeHierarchy",
    "typeHierarchy/supertypes",
    "typeHierarchy/subtypes",
    "textDocument/diagnostic",
];
/// Upper bound on documents kept in the `didOpen` lifecycle at once. Without a
/// cap, `open_docs` (and the server-side document set it mirrors) grows
/// monotonically over a long session until the server's RLIMIT_AS / Job memory
/// cap kills it. When exceeded, the least-recently-synced document is evicted
/// and a `didClose` is issued for it.
const MAX_OPEN_DOCUMENTS: usize = 64;
/// Bound on how long `stop()` waits for the server to exit on its own after
/// `exit` before escalating to a hard kill. Keeps the common (graceful) path
/// from discarding the exit status while still bounding worst-case shutdown
/// latency — a server that never exits degrades to exactly today's
/// unconditional-kill behavior, never worse.
const GRACEFUL_EXIT_TIMEOUT_MS: u64 = 2_000;

/// Minimal, secret-free environment variables forwarded to spawned language
/// servers after `env_clear()`. Deliberately excludes everything octocode-
/// specific or credential-bearing (OCTOCODE_*, GITHUB_TOKEN, AWS_*, …). PATH is
/// required to resolve/launch servers; HOME (and its Windows equivalents) lets
/// servers find their per-user caches/toolchains (e.g. rust-analyzer → ~/.cargo).
/// Any additional environment must be requested explicitly via the server config.
const LSP_SERVER_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "SYSTEMROOT",
    "SystemRoot",
];
// Read from the process, so declared there (`crate::process_env`).
const _: () = {
    let mut index = 0;
    while index < LSP_SERVER_ENV_ALLOWLIST.len() {
        assert!(crate::process_env::declares(
            LSP_SERVER_ENV_ALLOWLIST[index]
        ));
        index += 1;
    }
};

/// Give a spawned child process a bounded window to exit on its own (e.g.
/// after an LSP `exit` notification) before escalating to a hard kill.
/// Returns `true` if the process exited within `timeout_duration` without
/// needing to be killed.
async fn wait_for_graceful_exit(child: &mut Child, timeout_duration: Duration) -> bool {
    end_child(child, timeout_duration, crate::process::kill_tree_blocking).await
}

/// Ends `child` and sweeps its process group with `sweep(pid)` on both paths:
/// descendants (proc-macro-srv, cargo/build scripts, clangd workers) must die
/// on a clean exit too, not only on timeout. The sweep signals `-pid`, so it
/// fires while the leader is still unreaped (alive, or a zombie that keeps its
/// pid reserved); after the reap the pid could be recycled by an unrelated
/// process leading its own group. The leader is reaped last, and hard-killed
/// only when it outlived the window.
async fn end_child<F: std::future::Future<Output = ()>>(
    child: &mut Child,
    timeout_duration: Duration,
    sweep: impl FnOnce(u32) -> F,
) -> bool {
    let exited = exited_unreaped(child, timeout_duration).await;
    if let Some(pid) = child.id() {
        sweep(pid).await;
    }
    if exited {
        let _ = child.wait().await;
    } else {
        let _ = child.kill().await;
    }
    exited
}

/// Waits up to `limit` for the leader to exit without reaping it, so its pid
/// stays reserved for the group sweep. A blocking `waitid(WNOWAIT)` on the
/// blocking pool returns when the leader exits; on timeout the caller kills
/// the leader, which releases that wait. `true` when it exited in time (or
/// was already reaped).
#[cfg(unix)]
async fn exited_unreaped(child: &mut Child, limit: Duration) -> bool {
    let Some(pid) = child.id() else {
        return true;
    };
    let exit =
        tokio::task::spawn_blocking(move || crate::process::wait_for_exit_without_reaping(pid));
    timeout(limit, exit).await.is_ok()
}

#[cfg(not(unix))]
async fn exited_unreaped(child: &mut Child, limit: Duration) -> bool {
    // No pid-addressed group sweep off Unix (the Job Object tears the tree
    // down), so reaping here cannot misdirect a signal.
    timeout(limit, child.wait()).await.is_ok()
}

async fn lsp_spawn_program(validated_command: &str, args: &mut Vec<String>) -> Result<String> {
    if executable_has_node_shebang(validated_command).await? {
        let node = tokio::task::spawn_blocking(super::config::current_node_command)
            .await
            .map_err(|err| Error::new(format!("Failed to resolve Node executable: {err}")))?
            .ok_or_else(|| {
                Error::new("Node executable is required to start this language server")
            })?;
        args.insert(0, validated_command.to_owned());
        return Ok(node);
    }
    Ok(validated_command.to_owned())
}

/// Async so the one-file inspection cannot stall an executor thread inside
/// `start()` (the only caller) on slow filesystems.
async fn executable_has_node_shebang(path: &str) -> Result<bool> {
    let mut file = tokio::fs::File::open(path).await.map_err(|err| {
        Error::new(format!(
            "Failed to inspect language server executable {path}: {err}"
        ))
    })?;
    let mut buf = [0_u8; 128];
    let read = tokio::io::AsyncReadExt::read(&mut file, &mut buf)
        .await
        .map_err(|err| {
            Error::new(format!(
                "Failed to inspect language server executable {path}: {err}"
            ))
        })?;
    let first_line = std::str::from_utf8(&buf[..read])
        .ok()
        .and_then(|text| text.lines().next())
        .unwrap_or_default();
    Ok(first_line.starts_with("#!") && first_line.contains("node"))
}

/// What the last successful sync of a document sent: the file it was read
/// from, the digest of its text, and, once [`NativeLspClient::sync_open_documents`]
/// has proven that text is still the file's, the file's settled stamp.
#[derive(Clone, Debug)]
struct Synced {
    path: String,
    digest: u64,
    stamp: Option<SourceStamp>,
}

/// One open document compared with its file on disk.
enum DiskCheck {
    /// The file still holds the synced text; `stamp` lets a later check skip
    /// the read.
    Same {
        uri: String,
        digest: u64,
        stamp: Option<SourceStamp>,
    },
    /// The file now holds other text.
    Changed {
        uri: String,
        path: String,
        content: String,
        stamp: Option<SourceStamp>,
    },
    /// The file is gone, or no longer a readable UTF-8 file within the
    /// source bound.
    Gone { uri: String },
}

impl DiskCheck {
    /// `synced` against its file now: `None` when its stamp proves the file
    /// unchanged without reading it. The stat comes before the read, so a
    /// write racing the read leaves a stamp the next check sees changed.
    fn of(uri: String, synced: Synced) -> Option<Self> {
        let Ok(meta) = std::fs::metadata(&synced.path) else {
            return Some(Self::Gone { uri });
        };
        if synced.stamp.is_some() && SourceStamp::of(&meta) == synced.stamp {
            return None;
        }
        let stamp = SourceStamp::settled(&meta, crate::stamp::settle_clock());
        let Some(content) =
            crate::text::read_source(Path::new(&synced.path), super::MAX_LSP_SOURCE_BYTES)
                .ok()
                .and_then(|source| String::from_utf8(source.bytes).ok())
        else {
            return Some(Self::Gone { uri });
        };
        Some(if content_digest(&content) == synced.digest {
            Self::Same {
                uri,
                digest: synced.digest,
                stamp,
            }
        } else {
            Self::Changed {
                uri,
                path: synced.path,
                content,
                stamp,
            }
        })
    }
}

/// Digest of a document's text, compared to skip a resync of text the
/// server already holds.
fn content_digest(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Bounded open-document lifecycle bookkeeping: `uri -> last sent version`
/// plus LRU recency. Capped so a long session cannot grow the open-document set
/// (and the mirrored server-side document memory) without bound. All state is
/// synchronous/pure so the open/evict decision is unit-testable without a real
/// server; the async `didClose` for an evicted URI is issued by the caller.
struct OpenDocuments {
    versions: HashMap<String, i32>,
    /// What the last successful sync sent, per URI.
    synced: HashMap<String, Synced>,
    /// Recency order, least-recently-synced at the front.
    lru: VecDeque<String>,
    cap: usize,
}

impl OpenDocuments {
    fn new(cap: usize) -> Self {
        Self {
            versions: HashMap::new(),
            synced: HashMap::new(),
            lru: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    fn clear(&mut self) {
        self.versions.clear();
        self.synced.clear();
        self.lru.clear();
    }

    fn version(&self, uri: &str) -> Option<i32> {
        self.versions.get(uri).copied()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.versions.len()
    }

    /// Reserve the next version for a sync of `uri`, refreshing its recency.
    /// Returns the version to send and, when the cap is exceeded, the URI of the
    /// evicted least-recently-used document (never the document being synced),
    /// which the caller must `didClose`.
    fn reserve(&mut self, uri: &str) -> (i32, Option<String>) {
        self.synced.remove(uri);
        let version = self.versions.get(uri).copied().unwrap_or(0) + 1;
        self.versions.insert(uri.to_owned(), version);
        self.touch(uri);
        let evicted = if self.versions.len() > self.cap {
            self.evict_lru(uri)
        } else {
            None
        };
        (version, evicted)
    }

    fn touch(&mut self, uri: &str) {
        self.lru.retain(|candidate| candidate != uri);
        self.lru.push_back(uri.to_owned());
    }

    /// Evict the least-recently-used document, skipping the one currently being
    /// synced so a cap of 1 never closes the document we are opening.
    fn evict_lru(&mut self, current: &str) -> Option<String> {
        while let Some(candidate) = self.lru.pop_front() {
            if candidate == current {
                self.lru.push_back(candidate);
                if self.lru.len() <= 1 {
                    return None;
                }
                continue;
            }
            self.versions.remove(&candidate);
            self.synced.remove(&candidate);
            return Some(candidate);
        }
        None
    }

    /// Undo a `reserve` whose notification failed to send, restoring the prior
    /// version (or removing the URI entirely for a failed first `didOpen`).
    fn rollback(&mut self, uri: &str, applied_version: i32) {
        self.synced.remove(uri);
        if self.versions.get(uri).copied() != Some(applied_version) {
            return;
        }
        if applied_version <= 1 {
            self.versions.remove(uri);
            self.lru.retain(|candidate| candidate != uri);
        } else {
            self.versions.insert(uri.to_owned(), applied_version - 1);
        }
    }

    /// The open version of `uri` when its last successful sync sent content
    /// with `digest`: the server already holds it, so no resync is needed.
    fn unchanged(&mut self, uri: &str, digest: u64) -> Option<i32> {
        if self.synced.get(uri).map(|synced| synced.digest) != Some(digest) {
            return None;
        }
        let version = self.version(uri)?;
        self.touch(uri);
        Some(version)
    }

    /// Remember what a sync of `version` sent: `path`'s text with `digest`.
    fn record(&mut self, uri: &str, version: i32, path: &str, digest: u64) {
        if self.version(uri) == Some(version) {
            self.synced.insert(
                uri.to_owned(),
                Synced {
                    path: path.to_owned(),
                    digest,
                    stamp: None,
                },
            );
        }
    }

    /// Every document whose last sync completed, to compare with disk.
    fn synced(&self) -> Vec<(String, Synced)> {
        self.synced
            .iter()
            .map(|(uri, synced)| (uri.clone(), synced.clone()))
            .collect()
    }

    /// The file's `stamp` once its text proved to be the synced `digest`
    /// (a sync since then keeps its own).
    fn stamp(&mut self, uri: &str, digest: u64, stamp: Option<SourceStamp>) {
        if let Some(synced) = self
            .synced
            .get_mut(uri)
            .filter(|synced| synced.digest == digest)
        {
            synced.stamp = stamp;
        }
    }

    /// Drop `uri` from the open set; `true` when it was open.
    fn forget(&mut self, uri: &str) -> bool {
        self.synced.remove(uri);
        self.lru.retain(|candidate| candidate != uri);
        self.versions.remove(uri).is_some()
    }
}

/// `didClose` for `uri`, dropping its push diagnostics first.
async fn notify_close(connection: &JsonRpcConnection, uri: &str) -> Result<()> {
    connection.clear_push_diagnostics(uri);
    connection
        .notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await
}

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
    progress: Arc<ProgressTracker>,
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
    /// Bumped each time the text of an open document changes on the server
    /// (`didChange`, or `didClose` of a document whose file changed or went
    /// away). Part of every cached response key: an answer given before the
    /// change is never reused after it, whatever the caller's generation.
    content_epoch: AtomicU64,
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
    /// Query responses (generation + `method` + params) from this server,
    /// reused only inside a [`RESPONSE_SCOPE`] that allows it.
    responses: moka::sync::Cache<String, CachedResponse>,
    options: LspClientOptions,
}

/// One cached response and its JSON size, measured once at insert so the
/// cache weigher reads a number instead of serializing the value again.
#[derive(Clone)]
struct CachedResponse {
    value: Arc<Value>,
    json_bytes: usize,
}

impl CachedResponse {
    fn new(value: Arc<Value>) -> Self {
        let json_bytes = crate::json_size::bytes(value.as_ref()).unwrap_or(0);
        Self { value, json_bytes }
    }
}

/// Response reuse for one lspSearch call. `generation` is part of every
/// cached key: the anchor document's content hash, and for incoming walks
/// also a fingerprint of every project file the answer can depend on, so an
/// edited anchor or project never reuses. `reuse` lets a request read the
/// cache (continuation pages, and first pages whose generation carries the
/// project fingerprint); without it requests reach the server and refresh
/// the cache. An empty generation bypasses the cache.
#[derive(Clone, Debug, Default)]
pub struct ResponseScope {
    pub reuse: bool,
    pub generation: String,
}

tokio::task_local! {
    pub static RESPONSE_SCOPE: std::cell::RefCell<ResponseScope>;
}

/// Bytes of cached responses per server process, and idle lifetime.
const RESPONSE_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const RESPONSE_CACHE_IDLE: std::time::Duration = std::time::Duration::from_secs(600);

fn response_cache() -> moka::sync::Cache<String, CachedResponse> {
    moka::sync::Cache::builder()
        .max_capacity(RESPONSE_CACHE_BYTES)
        .weigher(|key: &String, entry: &CachedResponse| {
            u32::try_from(key.len() + entry.json_bytes).unwrap_or(u32::MAX)
        })
        .time_to_idle(RESPONSE_CACHE_IDLE)
        .build()
}

impl NativeLspClient {
    pub fn new(config: LspLanguageServerConfig) -> Self {
        Self::with_options(config, LspClientOptions::default())
    }

    pub fn with_options(config: LspLanguageServerConfig, options: LspClientOptions) -> Self {
        // Every launch path (discovery, runtime, napi) gets the per-server
        // safe defaults; user-supplied options still win key by key.
        let mut config = config;
        crate::lsp::config::apply_server_default_options(&mut config, None);
        let config_command = config.command.clone();
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
                progress: ProgressTracker::new(),
                activity: Arc::new(AtomicUsize::new(0)),
                sync_lock: Mutex::new(()),
                open_docs: StdMutex::new(OpenDocuments::new(MAX_OPEN_DOCUMENTS)),
                content_epoch: AtomicU64::new(0),
                project_config: StdMutex::new(ProjectConfigWatch::new(
                    crate::lsp::config::project_config_files(&config_command),
                )),
                memory_cap_guard: StdMutex::new(None),
                memory_watchdog: StdMutex::new(None),
                resident_bytes: Arc::new(AtomicU64::new(0)),
                responses: response_cache(),
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

    pub async fn start(&self) -> Result<()> {
        let mut child_guard = self.inner.child.lock().await;
        if child_guard.is_some() {
            return Err(Error::new("LSP client already started"));
        }
        {
            let mut stderr_lines = self
                .inner
                .stderr_lines
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            stderr_lines.clear();
        }
        self.reset_session_state();

        let validated_command =
            crate::lsp::validation::validate_lsp_server_path(self.inner.config.command.clone())?;
        let mut command_args = self.inner.config.args.clone().unwrap_or_default();
        let command_program = lsp_spawn_program(&validated_command, &mut command_args).await?;
        let mut command = tokio::process::Command::new(&command_program);
        crate::process::contain(command.as_std_mut());
        command
            .args(command_args)
            .current_dir(&self.inner.config.workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // OS-level memory cap: internal buffer bounds do not stop a runaway
        // server from OOMing the host. macOS and Linux bound the resident
        // memory of the whole server tree with an RSS watchdog
        // (`memory_watchdog_for`); Windows attaches a Job Object right after
        // spawn; other Unix targets cap the address space before exec.
        let memory_cap = spawn_limits::memory_cap_bytes(self.inner.config.max_memory_mb);
        if let Some(cap_bytes) = memory_cap {
            spawn_limits::apply_pre_spawn_cap(&mut command, cap_bytes);
        }
        // Never leak octocode's own environment (OCTOCODE_CLASSIFICATION_API, GITHUB_TOKEN,
        // etc.) into a spawned language server. Start from an empty environment and
        // re-add only a minimal, secret-free allowlist plus any explicitly
        // configured server env. PATH must be preserved so built-in servers
        // (e.g. a `node`/`rust-analyzer` resolved via PATH) still launch.
        command.env_clear();
        for &key in LSP_SERVER_ENV_ALLOWLIST {
            if let Some(value) = crate::process_env::var_os(key) {
                command.env(key, value);
            }
        }
        if let Some(env) = &self.inner.config.env {
            for (key, value) in env {
                command.env(key, value);
            }
        }

        let mut child = command
            .spawn()
            .map_err(|err| Error::new(format!("Failed to start language server: {err}")))?;
        let memory_cap_guard = match crate::process::Job::attach_async(&child, memory_cap) {
            Ok(guard) => guard,
            Err(error) => {
                cleanup_failed_start(&mut child, None).await;
                return Err(error);
            }
        };
        let stderr_task = child
            .stderr
            .take()
            .map(|stderr| spawn_stderr_reader(stderr, Arc::clone(&self.inner.stderr_lines)));
        let Some(stdout) = child.stdout.take() else {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(Error::new("Language server stdout pipe missing"));
        };
        let Some(stdin) = child.stdin.take() else {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(Error::new("Language server stdin pipe missing"));
        };

        let root_uri = match path_to_uri(&self.inner.config.workspace_root) {
            Ok(uri) => uri,
            Err(error) => {
                cleanup_failed_start(&mut child, stderr_task).await;
                return Err(error);
            }
        };
        let connection = Arc::new(JsonRpcConnection::new(
            stdout,
            stdin,
            ClientRequestContext {
                configuration: self
                    .inner
                    .config
                    .initialization_options
                    .clone()
                    .unwrap_or_else(|| json!({})),
                section_root: super::config::configuration_section(
                    self.inner.config.language_id.as_deref(),
                )
                .map(str::to_owned),
                workspace_folders: json!([{ "uri": root_uri, "name": "workspace" }]),
            },
            Arc::clone(&self.inner.progress),
            cpu_clock_for(&child),
        ));
        // Watch the server tree's RSS (macOS has no address-space cap; Linux
        // RLIMIT_AS bounds one process, not the tree). Armed before
        // `initialize` (indexing can start there); dropping the guard on any
        // failed-start path aborts it.
        let memory_watchdog = memory_watchdog_for(
            &child,
            memory_cap,
            &connection,
            Arc::clone(&self.inner.resident_bytes),
        );
        let initialize_result = match initialize(&connection, &self.inner.config).await {
            Ok(value) => value,
            Err(error) => {
                cleanup_failed_start(&mut child, stderr_task).await;
                return Err(error);
            }
        };
        // Positions are UTF-16 throughout the tool contract. A server that
        // ignores our advertised encoding cannot supply trustworthy locations.
        let negotiated_encoding = extract_position_encoding(&initialize_result);
        if let Some(encoding) = negotiated_encoding.as_deref()
            && encoding != "utf-16"
        {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(Error::new(format!(
                "Unsupported language server positionEncoding '{encoding}': \
                         octocode advertises utf-16; semantic positions cannot be resolved safely"
            )));
        }
        {
            let mut capabilities = self
                .inner
                .capabilities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *capabilities = initialize_result.get("capabilities").cloned();
        }
        {
            let mut server_info = self
                .inner
                .server_info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *server_info = initialize_result.get("serverInfo").cloned();
        }
        if let Err(error) = connection.notify("initialized", json!({})).await {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(error);
        }

        *self.inner.connection.lock().await = Some(connection);
        *self.inner.stderr_task.lock().await = stderr_task;
        {
            let mut guard = self
                .inner
                .memory_cap_guard
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = Some(memory_cap_guard);
        }
        {
            let mut guard = self
                .inner
                .memory_watchdog
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = memory_watchdog;
        }
        *child_guard = Some(child);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        // Same lock order as `start` (child → connection): holding `child`
        // for the whole teardown makes a stop that overlaps a start wait for
        // it, then shut down the connection and child it published.
        let mut child_guard = self.inner.child.lock().await;
        let connection = self.inner.connection.lock().await.take();
        if let Some(connection) = connection {
            let _ = connection.request("shutdown", Value::Null, 1_000).await;
            // A slow `shutdown` retires the connection; `exit` must still go out.
            let _ = connection.notify_best_effort("exit", Value::Null).await;
        }
        // The watchdog samples the child's pid: stop it before the reap.
        {
            let mut guard = self
                .inner
                .memory_watchdog
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = None;
        }
        if let Some(mut child) = child_guard.take() {
            wait_for_graceful_exit(&mut child, Duration::from_millis(GRACEFUL_EXIT_TIMEOUT_MS))
                .await;
        }
        drop(child_guard);
        // Release the memory-cap Job Object only after the child is reaped;
        // closing it earlier would hard-kill a gracefully-exiting server.
        {
            let mut guard = self
                .inner
                .memory_cap_guard
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = None;
        }
        if let Some(task) = self.inner.stderr_task.lock().await.take() {
            task.abort();
        }
        self.reset_session_state();
        Ok(())
    }

    /// Wait for the server to finish any post-`initialized` indexing, returning
    /// a readiness descriptor so JS can tell a confirmed-idle server apart from
    /// one that never reported progress or is still busy. The returned string
    /// is one of `"progressIdle"`, `"settledWithoutProgress"`, or `"timeout"`.
    pub(crate) async fn wait_for_ready(&self, timeout_ms: Option<u32>) -> Result<String> {
        let timeout_ms = u64::from(timeout_ms.unwrap_or(45_000));
        let readiness = self
            .inner
            .progress
            .wait_until_idle(timeout_ms, self.settle_ms(STARTUP_SETTLE_MS))
            .await
            .as_str()
            .to_owned();
        {
            let mut slot = self
                .inner
                .readiness
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *slot = Some(readiness.clone());
        }
        Ok(readiness)
    }

    /// `false` if the client was never started/already stopped, or if its
    /// connection's read loop has observed the server process close (crash).
    /// Lets the JS client pool evict a stale pooled entry at the next
    /// `acquire()` instead of returning a client whose requests will just
    /// fail until the idle timer eventually reaps it.
    pub async fn is_alive(&self) -> bool {
        match self.inner.connection.lock().await.as_ref() {
            Some(connection) => connection.is_alive(),
            None => false,
        }
    }

    /// See [`crate::lsp::config::serves_requests_concurrently`].
    pub fn serves_requests_concurrently(&self) -> bool {
        crate::lsp::config::serves_requests_concurrently(&self.inner.config)
    }

    /// See [`crate::lsp::config::workspace_symbol_limit`].
    pub fn workspace_symbol_limit(&self) -> Option<usize> {
        crate::lsp::config::workspace_symbol_limit(&self.inner.config)
    }

    pub fn has_capability(&self, capability: String) -> bool {
        let capabilities = self
            .inner
            .capabilities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        capabilities
            .as_ref()
            .map(|value| capability_supported(value, &capability))
            .unwrap_or(false)
    }

    pub fn readiness(&self) -> Option<String> {
        self.inner
            .readiness
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// `settle` bounded by [`LspClientOptions::settle_cap`].
    fn settle(&self, settle: Duration) -> Duration {
        self.inner
            .options
            .settle_cap
            .map_or(settle, |cap| settle.min(cap))
    }

    /// [`Self::settle`] in milliseconds.
    fn settle_ms(&self, settle_ms: u64) -> u64 {
        u64::try_from(self.settle(Duration::from_millis(settle_ms)).as_millis())
            .unwrap_or(settle_ms)
    }

    fn readiness_is(&self, readiness: Readiness) -> bool {
        self.inner
            .readiness
            .lock()
            .is_ok_and(|slot| slot.as_deref() == Some(readiness.as_str()))
    }

    pub(crate) fn language_id(&self) -> Option<&str> {
        self.inner.config.language_id.as_deref()
    }

    /// A pooled client whose last readiness wait ended in `timeout` waits
    /// again, up to `timeout_ms`, before it is handed out: the server kept
    /// indexing since, so the next call can see it finish instead of failing
    /// on the stored `timeout`. Any other readiness returns at once. The
    /// re-wait never pays the silent-server settle again: a `timeout` means
    /// the server already reported progress. The wait holds a lease, so idle
    /// expiry cannot stop the server under it.
    pub(crate) async fn resume_timed_out_readiness(&self, timeout_ms: u32) {
        if !self.readiness_is(Readiness::Timeout) {
            return;
        }
        let _activity = self.lease();
        let _ = self.wait_for_ready(Some(timeout_ms)).await;
    }

    /// Sync a document's in-memory content to the server, honoring the LSP
    /// document lifecycle: the FIRST sync of a URI sends `textDocument/didOpen`
    /// (version 1); a later sync of changed content sends
    /// `textDocument/didChange` with an incremented version and a
    /// full-document content change, and a sync of the content the server
    /// already holds sends nothing. Re-sending
    /// `didOpen` is ignored or rejected by many servers and can make
    /// changed content resolve against the stale original.
    pub async fn open_document(&self, file_path: String, content: &str) -> Result<()> {
        self.sync_document(&file_path, content).await.map(|_| ())
    }

    /// [`Self::open_document`], then — only when this sync was the document's
    /// first `didOpen` — wait for any project load the open triggered to go
    /// idle before returning. Servers like `typescript-language-server` start
    /// loading the project only after a `didOpen` (`$/progress begin` arrives
    /// ~100 ms later); querying immediately races that load and returns
    /// incomplete references. `settle_ms` bounds how long to wait for such a
    /// wave to start (default [`OPEN_SETTLE_MS`]) and `timeout_ms` bounds the
    /// whole wait (default [`OPEN_READY_TIMEOUT_MS`], capped at 60 s).
    ///
    /// The settle is not stacked on a startup readiness wait that already
    /// confirmed the project loaded (`progressIdle`): that server announced
    /// and finished its project load before this open, so only progress
    /// already under way when the open was written is drained.
    ///
    /// Returns the readiness string (`progressIdle`, `settledWithoutProgress`, or
    /// `timeout`) for a first open, and `None` for a re-sync of an already open
    /// document, which does not wait.
    pub async fn open_document_and_wait(
        &self,
        file_path: String,
        content: &str,
        settle_ms: Option<u32>,
        timeout_ms: Option<u32>,
    ) -> Result<Option<String>> {
        let _activity = self.lease();
        let observed = self.inner.progress.subscribe();
        let version = self.sync_document(&file_path, content).await?;
        if version != Some(1) {
            return Ok(None);
        }
        let settle_ms = if self.readiness_is(Readiness::ProgressIdle) {
            0
        } else {
            self.settle_ms(u64::from(settle_ms.unwrap_or(OPEN_SETTLE_MS)))
        };
        let timeout_ms = u64::from(
            timeout_ms
                .unwrap_or(OPEN_READY_TIMEOUT_MS)
                .min(MAX_OPEN_READY_TIMEOUT_MS),
        );
        let readiness = self
            .inner
            .progress
            .wait_until_idle_after(observed, settle_ms, timeout_ms)
            .await;
        Ok(Some(readiness.as_str().to_owned()))
    }

    pub async fn get_hover(&self, file_path: String, line: u32, character: u32) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    /// `textDocument/documentSymbol`, shared with the response cache (a
    /// reused answer is not copied).
    pub async fn get_document_symbols(&self, file_path: String) -> Result<Arc<Value>> {
        let uri = path_to_uri(&file_path)?;
        self.cached_request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
            None,
        )
        .await
    }

    pub async fn prepare_call_hierarchy(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/prepareCallHierarchy",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    pub async fn incoming_calls(&self, item: Value) -> Result<Value> {
        self.request("callHierarchy/incomingCalls", json!({ "item": item }))
            .await
    }

    pub async fn outgoing_calls(&self, item: Value) -> Result<Value> {
        self.request("callHierarchy/outgoingCalls", json!({ "item": item }))
            .await
    }

    /// Project-wide fuzzy symbol search — `workspace/symbol`.
    /// Returns `WorkspaceSymbol[] | SymbolInformation[]` (raw JSON).
    /// `query` is the fuzzy name string; empty string returns all symbols.
    pub async fn workspace_symbol(&self, query: String) -> Result<Value> {
        self.request("workspace/symbol", json!({ "query": query }))
            .await
    }

    /// Prepare a type-hierarchy item at a given position — `textDocument/prepareTypeHierarchy`.
    /// Returns `TypeHierarchyItem[] | null` (raw JSON).
    pub async fn prepare_type_hierarchy(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/prepareTypeHierarchy",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    /// Retrieve supertypes (base classes / implemented interfaces) — `typeHierarchy/supertypes`.
    /// `item` is a `TypeHierarchyItem` previously returned by `prepareTypeHierarchy`.
    pub async fn type_hierarchy_supertypes(&self, item: Value) -> Result<Value> {
        self.request("typeHierarchy/supertypes", json!({ "item": item }))
            .await
    }

    /// Retrieve subtypes (subclasses / implementors) — `typeHierarchy/subtypes`.
    /// `item` is a `TypeHierarchyItem` previously returned by `prepareTypeHierarchy`.
    pub async fn type_hierarchy_subtypes(&self, item: Value) -> Result<Value> {
        self.request("typeHierarchy/subtypes", json!({ "item": item }))
            .await
    }

    /// Pull diagnostics for a single file — `textDocument/diagnostic` (LSP 3.17+).
    /// Returns `DocumentDiagnosticReport` with `kind: "full"|"unchanged"` and `items: Diagnostic[]`.
    /// Prefer pull diagnostics over push (`publishDiagnostics`) for agent/CLI use: you control
    /// *when* to request them and avoid a notification firehose.
    pub async fn get_diagnostics(&self, file_path: String) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await
    }

    /// Return the latest bounded `textDocument/publishDiagnostics` payload for
    /// a file, waiting briefly when the server has not published one yet.
    pub async fn get_push_diagnostics(
        &self,
        file_path: String,
        timeout_ms: Option<u32>,
    ) -> Result<Option<Value>> {
        let uri = path_to_uri(&file_path)?;
        let _activity = self.lease();
        let connection = self.connection_handle().await?;
        let min_version = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .version(&uri)
            .map(i64::from);
        Ok(connection
            .wait_for_push_diagnostics(&uri, timeout_ms.unwrap_or(1_500).min(10_000), min_version)
            .await)
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

impl NativeLspClient {
    /// Sync `content` for `file_path` and return the version that was sent
    /// (`1` means a fresh `didOpen`), or `None` when the server already holds
    /// `content`.
    async fn sync_document(&self, file_path: &str, content: &str) -> Result<Option<i32>> {
        let file_path = file_path.to_owned();
        let uri = path_to_uri(&file_path)?;
        let _activity = self.lease();
        // Held from version reservation until the notification is written:
        // two concurrent syncs of one document must reach the server in
        // version order (never `didChange v2` before `didOpen v1`).
        let _ordered = self.inner.sync_lock.lock().await;

        // Acquire the connection FIRST: if the client isn't started this fails
        // without mutating `open_docs`, so a doc is never marked open when its
        // didOpen/didChange was never actually sent.
        let connection = self.connection_handle().await?;
        // The server resolves this document's project setup now (at its
        // first `didOpen`): the baseline for noticing a later change.
        self.observe_project_config(Path::new(&file_path));
        // Content the server already holds is not sent again: a resync would
        // make the server re-analyze an unchanged document (and drop its
        // diagnostics) on every request.
        let digest = content_digest(content);
        if self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .unchanged(&uri, digest)
            .is_some()
        {
            return Ok(None);
        }
        // A content sync invalidates any push diagnostics for the prior
        // document version. The next diagnostic read waits for a fresh publish.
        connection.clear_push_diagnostics(&uri);

        // Decide didOpen-vs-didChange and reserve the version under the lock,
        // then release it before awaiting the notify (never hold a std mutex
        // across an await). `reserve` also enforces the LRU cap, handing back
        // the URI of any evicted least-recently-synced document so we can close
        // it out below and keep the open-document set bounded.
        let (next_version, evicted) = {
            let mut open_docs = self
                .inner
                .open_docs
                .lock()
                .map_err(|_| Error::new("open_docs lock poisoned"))?;
            open_docs.reserve(&uri)
        };

        let notification = if next_version == 1 {
            let language_id = crate::lsp::config::detect_language_id(&file_path)
                .or_else(|| self.inner.config.language_id.clone())
                .unwrap_or_else(|| "plaintext".to_owned());
            let params = json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": next_version,
                    "text": content
                }
            });
            connection.notify("textDocument/didOpen", params).await
        } else {
            let params = json!({
                "textDocument": { "uri": uri, "version": next_version },
                "contentChanges": [{ "text": content }]
            });
            connection.notify("textDocument/didChange", params).await
        };
        {
            let mut open_docs = self
                .inner
                .open_docs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match &notification {
                Ok(()) => open_docs.record(&uri, next_version, &file_path, digest),
                Err(_) => open_docs.rollback(&uri, next_version),
            }
        }
        if notification.is_ok() && next_version > 1 {
            self.inner.content_epoch.fetch_add(1, Ordering::AcqRel);
        }
        // Close the document the cap evicted (if any) so both our bookkeeping
        // and the server's document set stay bounded. Best-effort: a stopped or
        // wedged connection makes this moot, and it must not mask the primary
        // notification result.
        if let Some(evicted_uri) = evicted {
            let _ = notify_close(&connection, &evicted_uri).await;
        }
        notification.map(|()| Some(next_version))
    }

    fn observe_project_config(&self, path: &Path) {
        let mut project_config = self
            .inner
            .project_config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        project_config.observe(path);
    }

    /// Whether a project config file the server reads (`compile_commands.json`,
    /// `compile_flags.txt`, `.clangd`) was created, changed or deleted in a
    /// directory it resolved a document through since it first looked there.
    /// The server keeps the setup it saw until it is replaced, so the pool
    /// replaces a changed-under client once it is idle. Always `false` for a
    /// server that re-reads its own configuration.
    pub(crate) async fn project_config_changed(&self) -> bool {
        let watch = {
            let watch = self
                .inner
                .project_config
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !watch.is_active() {
                return false;
            }
            watch.clone()
        };
        tokio::task::spawn_blocking(move || watch.changed())
            .await
            .unwrap_or(false)
    }

    /// Bring every open document back in line with its file before a
    /// request. The server answers from an open document's synced text, not
    /// from disk, so a file edited after it was opened would otherwise keep
    /// answering with its old lines whichever document the request names. A
    /// document whose settled stamp still matches is not read; a changed one
    /// is re-synced (`didChange`); one whose file is gone or unreadable is
    /// closed, so the server reads the disk again.
    async fn sync_open_documents(&self) -> Result<()> {
        let synced = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .synced();
        if synced.is_empty() {
            return Ok(());
        }
        let _activity = self.lease();
        let checks = tokio::task::spawn_blocking(move || {
            synced
                .into_iter()
                .filter_map(|(uri, synced)| DiskCheck::of(uri, synced))
                .collect::<Vec<_>>()
        })
        .await
        .map_err(|error| Error::new(format!("open document check failed: {error}")))?;
        for check in checks {
            let (uri, digest, stamp) = match check {
                DiskCheck::Same { uri, digest, stamp } => (uri, digest, stamp),
                DiskCheck::Changed {
                    uri,
                    path,
                    content,
                    stamp,
                } => {
                    self.sync_document(&path, &content).await?;
                    (uri, content_digest(&content), stamp)
                }
                DiskCheck::Gone { uri } => {
                    self.close_document(&uri).await?;
                    continue;
                }
            };
            self.inner
                .open_docs
                .lock()
                .map_err(|_| Error::new("open_docs lock poisoned"))?
                .stamp(&uri, digest, stamp);
        }
        Ok(())
    }

    /// Close the open document `uri` (its file is gone or unreadable), so
    /// the server answers from its own view of the disk again.
    async fn close_document(&self, uri: &str) -> Result<()> {
        let _ordered = self.inner.sync_lock.lock().await;
        let connection = self.connection_handle().await?;
        let was_open = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .forget(uri);
        if !was_open {
            return Ok(());
        }
        let closed = notify_close(&connection, uri).await;
        self.inner.content_epoch.fetch_add(1, Ordering::AcqRel);
        closed
    }

    /// Clones the connection handle out from under the lock, releasing the
    /// guard before the caller awaits any request. This keeps the
    /// `connection` mutex uncontended (held only for the clone) so concurrent
    /// LSP requests are NOT serialized and cannot head-of-line block one
    /// another. Returns an error if the client has not been started.
    async fn connection_handle(&self) -> Result<Arc<JsonRpcConnection>> {
        self.inner
            .connection
            .lock()
            .await
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| Error::new("LSP client not initialized"))
    }

    /// Mark this client busy for as long as the returned guard lives. Hold
    /// one across a whole multi-request operation (document syncs, readiness
    /// waits, and the gaps between requests) so the pool's idle timer and LRU
    /// eviction never stop the server mid-operation. Requests, syncs, and
    /// diagnostic waits take their own lease internally.
    pub fn lease(&self) -> LspLease {
        LspLease::new(&self.inner.activity)
    }

    /// `true` when `other` is a clone of this client (same server process
    /// state), not merely one with an equal config.
    pub(crate) fn same_client(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// `true` while any request, sync, diagnostic wait, or [`LspLease`] is
    /// outstanding on this client (or a clone of it).
    pub(crate) fn is_busy(&self) -> bool {
        self.inner.activity.load(Ordering::Acquire) > 0
    }

    /// The server tree's resident bytes at the last watchdog sample.
    pub(crate) fn last_resident_bytes(&self) -> u64 {
        self.inner.resident_bytes.load(Ordering::Relaxed)
    }

    /// Definition, references, type-definition, or implementation locations
    /// with snippet content. Every server-supplied target path passes
    /// `policy` before any of its bytes are read; a refused path keeps its
    /// location with [`SNIPPET_CONTENT_WITHHELD`] as content. Reads are
    /// bounded to regular files of at most 1 MB.
    ///
    /// With `settle`, the answer is one asked at least `settle` after the
    /// call: an answer the server gave earlier (before a project load
    /// settled) is never returned, and a settled answer the response scope
    /// already holds is returned without waiting.
    pub async fn get_locations(
        &self,
        request: LocationRequest,
        file_path: String,
        line: u32,
        character: u32,
        policy: &SnippetReadPolicy,
        settle: Option<Duration>,
    ) -> Result<Vec<LspCodeSnippet>> {
        let uri = path_to_uri(&file_path)?;
        let mut params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });
        if let LocationRequest::References {
            include_declaration,
        } = request
        {
            params["context"] = json!({ "includeDeclaration": include_declaration });
        }
        let result = self
            .cached_request(request.method(), params, settle)
            .await?;
        snippets_from_locations(&result, policy).await
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.cached_request(method, params, None)
            .await
            .map(Arc::unwrap_or_clone)
    }

    /// `method` under the current [`RESPONSE_SCOPE`], once every open
    /// document matches its file: a reused answer is shared, not copied,
    /// and only one given since the open documents' text last changed is
    /// reused. A `settle` request keys its answer apart from the plain one
    /// and waits `settle` only when it reaches the server.
    async fn cached_request(
        &self,
        method: &str,
        params: Value,
        settle: Option<Duration>,
    ) -> Result<Arc<Value>> {
        self.sync_open_documents().await?;
        let settle = settle.map(|settle| self.settle(settle));
        let (reuse, generation) = RESPONSE_SCOPE
            .try_with(|scope| {
                let scope = scope.borrow();
                (scope.reuse, scope.generation.clone())
            })
            .unwrap_or_default();
        if generation.is_empty() {
            if let Some(settle) = settle {
                tokio::time::sleep(settle).await;
            }
            return self.send_request(method, params).await.map(Arc::new);
        }
        let variant = if settle.is_some() { "settled" } else { "" };
        let epoch = self.inner.content_epoch.load(Ordering::Acquire);
        let key = format!("{generation}\u{0}{epoch}\u{0}{variant}\u{0}{method}\u{0}{params}");
        if reuse && let Some(hit) = self.inner.responses.get(&key) {
            return Ok(hit.value);
        }
        if let Some(settle) = settle {
            tokio::time::sleep(settle).await;
        }
        let value = Arc::new(self.send_request(method, params).await?);
        self.inner
            .responses
            .insert(key, CachedResponse::new(Arc::clone(&value)));
        Ok(value)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let _activity = self.lease();
        // Acquire a cloned handle and DROP the guard before awaiting, so the
        // request + content-modified retry loop never holds the connection
        // mutex across `.await`.
        let connection = self.connection_handle().await?;
        // One budget for the request and its content-modified retries: the
        // starvation extension (see `transport::patience`) is granted once.
        let budget = WaitBudget::new(REQUEST_TIMEOUT_MS);
        let mut attempts = 0;
        loop {
            let response = if supports_partial_results(method) {
                connection
                    .request_with_partials(method, params.clone(), budget)
                    .await
            } else {
                connection.request(method, params.clone(), budget).await
            };
            match response {
                Ok(value) => return Ok(value),
                Err(error)
                    if is_retryable_error(&error)
                        && attempts < CONTENT_MODIFIED_RETRIES
                        && !budget.is_spent() =>
                {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        CONTENT_MODIFIED_RETRY_DELAY_MS,
                    ))
                    .await;
                }
                Err(error) => return Err(error),
            }
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

/// Which location request [`NativeLspClient::get_locations`] sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationRequest {
    Definition,
    References { include_declaration: bool },
    TypeDefinition,
    Implementation,
}

impl LocationRequest {
    fn method(self) -> &'static str {
        match self {
            Self::Definition => "textDocument/definition",
            Self::References { .. } => "textDocument/references",
            Self::TypeDefinition => "textDocument/typeDefinition",
            Self::Implementation => "textDocument/implementation",
        }
    }
}

/// Decides which server-supplied location paths may be read for snippet
/// content. The authorizer receives the path decoded from the server's URI
/// and returns the path to read (for example its canonical, policy-validated
/// form), or `None` to refuse. It runs before any filesystem access to that
/// file. The default allows every path as given (the napi methods use it).
#[derive(Clone, Default)]
pub struct SnippetReadPolicy {
    authorizer: Option<Arc<SnippetPathAuthorizer>>,
    /// See [`Self::shared_reads`].
    shared: Option<Arc<StdMutex<HashMap<String, CachedRead>>>>,
}

/// See [`SnippetReadPolicy`].
pub type SnippetPathAuthorizer = dyn Fn(&Path) -> Option<PathBuf> + Send + Sync;

impl SnippetReadPolicy {
    pub fn with_authorizer(
        authorizer: impl Fn(&Path) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        Self {
            authorizer: Some(Arc::new(authorizer)),
            shared: None,
        }
    }

    /// Share each path's authorization and read across every response read
    /// under this policy and its clones, so a file named by many responses
    /// is authorized, read, and line-indexed once. The reads live as long as
    /// the policy: build one per request, never per process, so a later
    /// request sees later edits.
    #[must_use]
    pub fn shared_reads(mut self) -> Self {
        self.shared = Some(Arc::default());
        self
    }

    fn shared_read(&self, path: &str) -> Option<CachedRead> {
        self.shared
            .as_ref()?
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(path)
            .cloned()
    }

    fn share_read(&self, path: &str, read: &CachedRead) {
        if let Some(shared) = self.shared.as_ref() {
            shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(path.to_owned(), read.clone());
        }
    }

    fn authorize(&self, path: &Path) -> Option<PathBuf> {
        match &self.authorizer {
            Some(authorizer) => authorizer(path),
            None => Some(path.to_path_buf()),
        }
    }
}

fn supports_partial_results(method: &str) -> bool {
    matches!(
        method,
        "textDocument/definition"
            | "textDocument/references"
            | "textDocument/typeDefinition"
            | "textDocument/implementation"
            | "textDocument/documentSymbol"
            | "workspace/symbol"
            | "callHierarchy/incomingCalls"
            | "callHierarchy/outgoingCalls"
            | "typeHierarchy/supertypes"
            | "typeHierarchy/subtypes"
            | "textDocument/diagnostic"
    )
}

/// A request failure worth re-sending: `ContentModified`, or
/// `ServerCancelled` when the server asked for a retrigger. Matches the typed
/// RPC error, never the rendered message text.
fn is_retryable_error(error: &Error) -> bool {
    error
        .rpc_error()
        .is_some_and(crate::error::RpcError::is_retryable)
}

/// Extracts the server-selected `positionEncoding` from an `InitializeResult`.
/// Returns `None` when the server omits it (which the LSP spec defines as the
/// utf-16 default).
fn extract_position_encoding(initialize_result: &Value) -> Option<String> {
    initialize_result
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("positionEncoding"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn capability_supported(capabilities: &Value, capability: &str) -> bool {
    capabilities
        .get(capability)
        .map(capability_value_supported)
        .unwrap_or(false)
}

fn capability_value_supported(value: &Value) -> bool {
    match value {
        Value::Bool(enabled) => *enabled,
        Value::Null => false,
        Value::Object(_) => true,
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}

async fn initialize(
    connection: &JsonRpcConnection,
    config: &LspLanguageServerConfig,
) -> Result<Value> {
    let params = initialize_params(config)?;
    connection
        .request("initialize", params, REQUEST_TIMEOUT_MS)
        .await
}

fn initialize_params(config: &LspLanguageServerConfig) -> Result<Value> {
    let root_uri = path_to_uri(&config.workspace_root)?;
    let mut params = json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "octocode-engine", "version": env!("CARGO_PKG_VERSION") },
        "locale": "en",
        "rootUri": root_uri,
        "workspaceFolders": [{ "uri": root_uri, "name": "workspace" }],
        "capabilities": {
            "general": {
                // Advertise UTF-16 ONLY. Every position octocode sends is computed
                // in UTF-16 code units (`text::utf8_offsets`) and the
                // column/snippet layers are UTF-16 too, so a server that selected
                // utf-8 would misread our offsets on any line with non-ASCII text.
                // UTF-16 is the mandatory baseline encoding, so it is always
                // supported. The server's choice is read back and asserted in
                // `start()` via `extract_position_encoding`.
                "positionEncodings": ["utf-16"],
                // We cancel dropped requests and re-send these methods on
                // ContentModified (see `NativeLspClient::request`).
                "staleRequestSupport": {
                    "cancel": true,
                    "retryOnContentModified": RETRIED_METHODS
                }
            },
            "textDocument": {
                "definition": { "dynamicRegistration": false, "linkSupport": true },
                "references": { "dynamicRegistration": false },
                "hover": { "dynamicRegistration": false, "contentFormat": ["markdown", "plaintext"] },
                "typeDefinition": { "dynamicRegistration": false, "linkSupport": true },
                "implementation": { "dynamicRegistration": false, "linkSupport": true },
                "documentSymbol": { "dynamicRegistration": false, "hierarchicalDocumentSymbolSupport": true },
                "callHierarchy": { "dynamicRegistration": false },
                // LSP 3.17: type hierarchy — navigate supertypes (base classes/interfaces)
                // and subtypes (subclasses/implementors) without opening every file.
                "typeHierarchy": { "dynamicRegistration": false },
                // LSP 3.17: pull diagnostics — agent/CLI requests errors on demand instead
                // of receiving an unprompted push stream after every didChange.
                "diagnostic": { "dynamicRegistration": false, "relatedDocumentSupport": false },
                // Push diagnostics: servers without pull support (e.g.
                // typescript-language-server) publish nothing unless the client
                // advertises this. versionSupport lets them tag the document
                // version each report belongs to.
                "publishDiagnostics": { "versionSupport": true, "relatedInformation": false },
                // No didSave: octocode never saves, so it must not claim to.
                "synchronization": { "dynamicRegistration": false, "willSave": false, "willSaveWaitUntil": false }
            },
            "workspace": {
                "configuration": true,
                "workspaceFolders": true,
                "symbol": { "dynamicRegistration": false }
            },
            "window": {
                "workDoneProgress": true
            }
        },
        "initializationOptions": config.initialization_options.clone().unwrap_or(Value::Null)
    });
    // rust-analyzer only: `experimental/serverStatus {quiescent}` feeds
    // readiness (see `ProgressTracker::on_server_status`).
    if crate::lsp::config::is_rust_analyzer_command(&config.command) {
        params["capabilities"]["experimental"] = json!({ "serverStatusNotification": true });
    }
    Ok(params)
}

async fn snippets_from_locations(
    value: &Value,
    policy: &SnippetReadPolicy,
) -> Result<Vec<LspCodeSnippet>> {
    let mut snippets = Vec::new();
    let mut content_cache = SnippetContentCache::new(policy.clone());
    match value {
        Value::Null => Ok(snippets),
        Value::Array(items) => {
            for item in items {
                if let Some(snippet) = snippet_from_location_like(item, &mut content_cache).await? {
                    snippets.push(snippet);
                }
            }
            Ok(snippets)
        }
        object @ Value::Object(_) => {
            if let Some(snippet) = snippet_from_location_like(object, &mut content_cache).await? {
                snippets.push(snippet);
            }
            Ok(snippets)
        }
        _ => Ok(snippets),
    }
}

/// One file's snippet source, split into lines once.
struct CachedSource {
    content: String,
    lines: LineIndex,
}

#[derive(Clone)]
enum CachedRead {
    Source(Arc<CachedSource>),
    Withheld,
    Failed(Error),
}

/// Per-response cache of snippet sources keyed by the server-supplied path,
/// so a file named by many locations is authorized, read, and line-indexed
/// once (failures and refusals are cached too); with
/// [`SnippetReadPolicy::shared_reads`], once per request.
struct SnippetContentCache {
    policy: SnippetReadPolicy,
    files: HashMap<String, CachedRead>,
    /// Decoded file path per location URI.
    paths: HashMap<String, String>,
}

impl SnippetContentCache {
    fn new(policy: SnippetReadPolicy) -> Self {
        Self {
            policy,
            files: HashMap::new(),
            paths: HashMap::new(),
        }
    }

    /// The file path a location URI names, decoded once per response.
    fn file_path(&mut self, uri: &str) -> Result<String> {
        if let Some(path) = self.paths.get(uri) {
            return Ok(path.clone());
        }
        let path = uri_to_path(uri)?;
        self.paths.insert(uri.to_owned(), path.clone());
        Ok(path)
    }

    /// Whole-line snippet for `range`, [`SNIPPET_CONTENT_WITHHELD`] when the
    /// policy refuses the path, or the read error.
    async fn read_range_content(&mut self, file_path: &str, range: &LspRange) -> Result<String> {
        if !self.files.contains_key(file_path) {
            let entry = match self.policy.shared_read(file_path) {
                Some(shared) => shared,
                None => {
                    let entry = match self.policy.authorize(Path::new(file_path)) {
                        None => CachedRead::Withheld,
                        Some(authorized) => match read_snippet_source(authorized).await {
                            Ok(content) => CachedRead::Source(Arc::new(CachedSource {
                                lines: LineIndex::new(&content),
                                content,
                            })),
                            Err(error) => CachedRead::Failed(error),
                        },
                    };
                    self.policy.share_read(file_path, &entry);
                    entry
                }
            };
            self.files.insert(file_path.to_owned(), entry);
        }
        match self.files.get(file_path) {
            Some(CachedRead::Source(source)) => Ok(slice_range_content(source, range)),
            Some(CachedRead::Withheld) => Ok(SNIPPET_CONTENT_WITHHELD.to_owned()),
            Some(CachedRead::Failed(error)) => Err(error.clone()),
            None => Ok(String::new()),
        }
    }
}

async fn read_snippet_source(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || read_bounded_regular_file(&path, MAX_SNIPPET_SOURCE_BYTES))
        .await
        .map_err(|err| Error::new(format!("snippet read task failed: {err}")))?
}

/// A UTF-8 regular file of at most `max_bytes`, read by
/// [`crate::text::read_source`].
pub(crate) fn read_bounded_regular_file(path: &Path, max_bytes: u64) -> Result<String> {
    use crate::text::ReadError;
    let bytes = crate::text::read_source(path, max_bytes)
        .map_err(|error| match error {
            ReadError::Io(err) => Error::new(err.to_string()),
            ReadError::NotRegular => Error::new(
                "not a regular file; LSP snippet content is read only from regular files",
            ),
            ReadError::TooLarge(len) => Error::new(format!(
                "file too large for LSP snippet content ({len} bytes > {max_bytes} bytes)"
            )),
        })?
        .bytes;
    String::from_utf8(bytes).map_err(|err| Error::new(format!("file is not valid UTF-8: {err}")))
}

async fn snippet_from_location_like(
    value: &Value,
    content_cache: &mut SnippetContentCache,
) -> Result<Option<LspCodeSnippet>> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))
        .and_then(Value::as_str);
    let range_value = value.get("range").or_else(|| value.get("targetRange"));
    let (Some(uri), Some(range_value)) = (uri, range_value) else {
        return Ok(None);
    };
    let context_range = parse_range(range_value)?;
    // LocationLink separates the symbol selection from its enclosing declaration.
    // Keep provider selection coordinates for navigation and enclosing source for context.
    let range = match value.get("targetSelectionRange") {
        Some(selection) => parse_range(selection)?,
        None => context_range.clone(),
    };
    let file_path = content_cache.file_path(uri)?;
    // A read failure here is real evidence ("target file is missing/unreadable/
    // generated"), not "no useful definition". Surface it as explicit content
    // instead of an empty string so callers don't misread it — and keep the
    // request resilient (one bad target must not drop the other locations).
    let content = match content_cache
        .read_range_content(&file_path, &context_range)
        .await
    {
        Ok(text) => text,
        Err(err) => format!("[content unavailable — could not read {uri}: {err}]"),
    };
    Ok(Some(LspCodeSnippet {
        uri: uri.to_owned(),
        range,
        content,
        symbol_kind: None,
        display_range: value.get("targetSelectionRange").map(|_| {
            json!({
                "startLine": context_range.start.line + 1,
                "endLine": if context_range.end.character == 0 {
                    context_range.end.line.max(context_range.start.line + 1)
                } else {
                    context_range.end.line + 1
                }
            })
        }),
    }))
}

fn parse_range(value: &Value) -> Result<LspRange> {
    let start = value
        .get("start")
        .ok_or_else(|| Error::new("LSP range missing start"))?;
    let end = value
        .get("end")
        .ok_or_else(|| Error::new("LSP range missing end"))?;
    Ok(LspRange {
        start: parse_position(start)?,
        end: parse_position(end)?,
    })
}

fn spawn_stderr_reader(
    stderr: ChildStderr,
    stderr_lines: Arc<StdMutex<VecDeque<String>>>,
) -> JoinHandle<()> {
    tokio::spawn(drain_stderr(stderr, stderr_lines))
}

/// Drain the server's stderr into the bounded ring until EOF or an I/O
/// error. It must keep reading whatever the bytes are: a stopped drain lets
/// the pipe fill and blocks the server. Each line is capped in memory at
/// [`STDERR_LINE_MAX_BYTES`] and decoded lossily.
async fn drain_stderr<R: AsyncRead + Unpin>(
    stderr: R,
    stderr_lines: Arc<StdMutex<VecDeque<String>>>,
) {
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::with_capacity(256);
    loop {
        line.clear();
        match read_capped_line(&mut reader, &mut line, STDERR_LINE_MAX_BYTES).await {
            Ok((0, _)) | Err(_) => return,
            Ok((_, truncated)) => {
                let mut text = String::from_utf8_lossy(&line).into_owned();
                if truncated {
                    text.push_str("...");
                }
                push_stderr_line(&stderr_lines, text);
            }
        }
    }
}

/// Read one `\n`-terminated line, keeping at most `cap` bytes of it in
/// `line` and consuming (discarding) the rest. The terminator and a
/// preceding `\r` are not kept. Returns the bytes consumed (0 = EOF) and
/// whether the line was cut.
async fn read_capped_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    line: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<(usize, bool)> {
    let mut consumed = 0;
    let mut truncated = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            break;
        }
        let newline = available.iter().position(|&byte| byte == b'\n');
        let content = &available[..newline.unwrap_or(available.len())];
        let keep = content.len().min(cap.saturating_sub(line.len()));
        line.extend_from_slice(&content[..keep]);
        truncated |= keep < content.len();
        let used = newline.map_or(available.len(), |index| index + 1);
        reader.consume(used);
        consumed += used;
        if newline.is_some() {
            break;
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok((consumed, truncated))
}

fn push_stderr_line(stderr_lines: &Arc<StdMutex<VecDeque<String>>>, line: String) {
    let mut lines = stderr_lines
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while lines.len() >= STDERR_RING_CAPACITY {
        lines.pop_front();
    }
    lines.push_back(truncate_stderr_line(line));
}

fn truncate_stderr_line(line: String) -> String {
    if line.chars().count() <= STDERR_LINE_MAX_CHARS {
        return line;
    }
    let mut truncated = line.chars().take(STDERR_LINE_MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}

/// The CPU clock of a freshly spawned server tree, which lets a request
/// outwait a server that is starved but progressing (see
/// `transport::patience`). `None` on platforms with no CPU reader, or
/// when the child already exited.
fn cpu_clock_for(child: &Child) -> Option<CpuClock> {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let clock = crate::process::TreeCpuClock::new(child.id()?);
        Some(Arc::new(move || clock.read()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = child;
        None
    }
}

/// Arm the RSS watchdog for a freshly spawned server on macOS and Linux (see
/// [`spawn_limits`]). Over the cap, the connection fails with a "language
/// server exceeded memory cap" error and the server tree is killed. `None`
/// when no cap is configured, on other platforms (the pre-spawn `RLIMIT_AS` /
/// Job Object caps apply there), or when the child already exited.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn memory_watchdog_for(
    child: &Child,
    memory_cap: Option<u64>,
    connection: &Arc<JsonRpcConnection>,
    resident: Arc<AtomicU64>,
) -> Option<spawn_limits::AbortOnDrop> {
    let (cap_bytes, pid) = memory_cap.zip(child.id())?;
    let alive = Arc::downgrade(connection);
    let failed = Arc::downgrade(connection);
    Some(spawn_limits::AbortOnDrop(tokio::spawn(
        spawn_limits::watch_memory(
            pid,
            cap_bytes,
            spawn_limits::MEMORY_WATCHDOG_INTERVAL,
            resident,
            move || {
                alive
                    .upgrade()
                    .is_some_and(|connection| connection.is_alive())
            },
            move |rss| {
                if let Some(connection) = failed.upgrade() {
                    connection.fail_resource_exhausted(&spawn_limits::memory_cap_exceeded_message(
                        rss, cap_bytes,
                    ));
                }
            },
        ),
    )))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn memory_watchdog_for(
    _child: &Child,
    _memory_cap: Option<u64>,
    _connection: &Arc<JsonRpcConnection>,
    _resident: Arc<AtomicU64>,
) -> Option<spawn_limits::AbortOnDrop> {
    None
}

async fn cleanup_failed_start(child: &mut Child, stderr_task: Option<JoinHandle<()>>) {
    if let Some(pid) = child.id() {
        crate::process::kill_tree_blocking(pid).await;
    }
    let _ = child.kill().await;
    if let Some(task) = stderr_task {
        task.abort();
    }
}

fn parse_position(value: &Value) -> Result<LspExactPosition> {
    // Required, numeric fields. Coercing a missing/malformed line or character
    // to 0 silently mis-positions snippets and masks protocol/server corruption,
    // so validate and surface InvalidArg instead.
    let line = value
        .get("line")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new("LSP position missing numeric 'line'"))?;
    let character = value
        .get("character")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new("LSP position missing numeric 'character'"))?;
    let out_of_range = |field: &str| Error::new(format!("LSP position '{field}' exceeds u32"));
    Ok(LspExactPosition {
        line: u32::try_from(line).map_err(|_| out_of_range("line"))?,
        character: u32::try_from(character).map_err(|_| out_of_range("character"))?,
    })
}

/// Slice `content` to an LSP range, returning whole lines for snippet context.
/// LSP ranges are **end-exclusive**: a range ending at `{line: N, character: 0}`
/// stops at the end of line `N-1` and must not include line `N`. When
/// `end.character > 0` the end line is partially covered, so it is included
/// (whole-line — column truncation would shrink a single-line definition
/// snippet down to the bare identifier).
///
/// Lines break on `\r\n`, `\n`, and lone `\r` (as the server counts them),
/// using the source's precomputed line index.
fn slice_range_content(source: &CachedSource, range: &LspRange) -> String {
    let line_count = source.lines.content_len();
    let start = range.start.line as usize;
    if start >= line_count {
        return String::new();
    }
    let end = range.end.line as usize;
    let last_inclusive = if range.end.character == 0 {
        match end.checked_sub(1) {
            Some(v) => v,
            None => return String::new(),
        }
    } else {
        end
    };
    let last_inclusive = last_inclusive.min(line_count.saturating_sub(1));
    if last_inclusive < start {
        return String::new();
    }
    (start..=last_inclusive)
        .map(|line| source.lines.line(&source.content, line).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
