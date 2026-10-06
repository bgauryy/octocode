use crate::error::{Error, Result};
use crate::lsp::resolver::LineIndex;
use crate::lsp::spawn_limits;
use crate::lsp::transport::{
    ClientRequestContext, JsonRpcConnection, ProgressTracker, configuration_section_for_command,
};
use crate::lsp::types::{JsCodeSnippet, JsExactPosition, JsLanguageServerConfig, JsRange};
use crate::lsp::uri::{path_to_uri, uri_to_path};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, ChildStderr};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

pub(super) const REQUEST_TIMEOUT_MS: u32 = 30_000;
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

/// Give a spawned child process a bounded window to exit on its own (e.g.
/// after an LSP `exit` notification) before escalating to a hard kill.
/// Returns `true` if the process exited within `timeout_duration` without
/// needing to be killed.
async fn wait_for_graceful_exit(child: &mut Child, timeout_duration: Duration) -> bool {
    // Capture the group-leader pid BEFORE reaping: once `child.wait()` resolves,
    // `Child::id()` returns `None` and the group can no longer be swept, so a
    // clean-exit sweep would otherwise be a silent no-op.
    let pid = child.id();
    let exited = timeout(timeout_duration, child.wait()).await.is_ok();
    finish_graceful_exit(
        exited,
        || group_kill_pid(pid),
        || async {
            let _ = child.kill().await;
        },
    )
    .await;
    exited
}

/// Post-wait teardown shared by `wait_for_graceful_exit`. Runs the process-group
/// sweep UNCONDITIONALLY — descendants (proc-macro-srv, cargo/build scripts,
/// clangd workers) must be reaped on the clean-exit path too, not only on
/// timeout — and hard-kills the leader only when it outlived the graceful
/// window. Split out with injected `sweep_group`/`hard_kill` so the
/// "sweep always runs" contract is unit-testable without a real child process.
async fn finish_graceful_exit<S, K, KFut>(exited: bool, mut sweep_group: S, hard_kill: K)
where
    S: FnMut(),
    K: FnOnce() -> KFut,
    KFut: std::future::Future<Output = ()>,
{
    sweep_group();
    if !exited {
        hard_kill().await;
    }
}

/// Put the spawned language server in its own process group so the whole tree
/// (e.g. rust-analyzer's `proc-macro-srv`, `cargo`/build scripts) can be killed
/// as a unit. Without this a hard kill signals only the direct child and leaks
/// its grandchildren to init. On Windows the Job Object attached right after
/// spawn (`MemoryCapGuard`) serves the same tree-teardown role.
#[cfg(unix)]
fn configure_lsp_process_group(command: &mut tokio::process::Command) {
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_lsp_process_group(_command: &mut tokio::process::Command) {}

/// Best-effort SIGKILL of the child's entire process group before reaping the
/// leader, so grandchildren die with it. No-op on non-Unix, where the Job
/// Object handles tree teardown when the guard is dropped.
#[cfg(unix)]
fn group_kill(child: &Child) {
    group_kill_pid(child.id());
}

/// Group-kill by a previously-captured pid. Callers that have already reaped the
/// child (e.g. after a graceful `child.wait()`) must use this, because
/// `Child::id()` returns `None` post-reap and would make the sweep a no-op.
#[cfg(unix)]
fn group_kill_pid(pid: Option<u32>) {
    if let Some(pid) = pid {
        // SAFETY: the child is spawned as its own process-group leader (see
        // `configure_lsp_process_group`), so the negative pid targets exactly
        // that group. `kill` with an invalid/dead group is a harmless no-op.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
fn group_kill(_child: &Child) {}

#[cfg(not(unix))]
fn group_kill_pid(_pid: Option<u32>) {}

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

/// Bounded open-document lifecycle bookkeeping: `uri -> last sent version`
/// plus LRU recency. Capped so a long session cannot grow the open-document set
/// (and the mirrored server-side document memory) without bound. All state is
/// synchronous/pure so the open/evict decision is unit-testable without a real
/// server; the async `didClose` for an evicted URI is issued by the caller.
struct OpenDocuments {
    versions: HashMap<String, i32>,
    /// Digest of the content the last successful sync sent, per URI.
    digests: HashMap<String, u64>,
    /// Recency order, least-recently-synced at the front.
    lru: VecDeque<String>,
    cap: usize,
}

impl OpenDocuments {
    fn new(cap: usize) -> Self {
        Self {
            versions: HashMap::new(),
            digests: HashMap::new(),
            lru: VecDeque::new(),
            cap: cap.max(1),
        }
    }

    fn clear(&mut self) {
        self.versions.clear();
        self.digests.clear();
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
        self.digests.remove(uri);
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
            self.digests.remove(&candidate);
            return Some(candidate);
        }
        None
    }

    /// Undo a `reserve` whose notification failed to send, restoring the prior
    /// version (or removing the URI entirely for a failed first `didOpen`).
    fn rollback(&mut self, uri: &str, applied_version: i32) {
        self.digests.remove(uri);
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
        if self.digests.get(uri) != Some(&digest) {
            return None;
        }
        let version = self.version(uri)?;
        self.touch(uri);
        Some(version)
    }

    /// Remember the content digest a sync of `version` sent.
    fn record(&mut self, uri: &str, version: i32, digest: u64) {
        if self.version(uri) == Some(version) {
            self.digests.insert(uri.to_owned(), digest);
        }
    }
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
    config: JsLanguageServerConfig,
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
    /// The `positionEncoding` the server selected in its `InitializeResult`
    /// (LSP 3.17). We advertise UTF-16 only, so this should be `utf-16` or absent
    /// (absent ⇒ utf-16 by spec). Any other value means the server ignored our
    /// capability. Startup rejects it before serving positions in the wrong units.
    position_encoding: StdMutex<Option<String>>,
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
    /// Windows: owns the Job Object enforcing the server's memory cap; must
    /// outlive the child and be released only after the child is reaped
    /// (closing a kill-on-close job hard-kills the tree). Unit on Unix, where
    /// the cap is applied via `pre_exec` before spawn.
    memory_cap_guard: StdMutex<Option<spawn_limits::MemoryCapGuard>>,
    /// macOS: the RSS watchdog enforcing `max_memory_mb` (no `RLIMIT_AS`
    /// there). Dropped (aborted) by `stop` before the child is reaped.
    memory_watchdog: StdMutex<Option<spawn_limits::AbortOnDrop>>,
    /// Query responses (anchor generation + `method` + params) from this
    /// server, reused only inside [`RESPONSE_SCOPE`] continuation pages.
    responses: moka::sync::Cache<String, Arc<Value>>,
}

/// Response reuse for one lspSearch call. `generation` (the anchor
/// document's content hash) is part of every cached key, so an edited anchor
/// never reuses; `reuse` is set only on continuation pages (page > 1 with the
/// walk's snapshot). First pages always reach the server and refresh the cache.
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

fn response_cache() -> moka::sync::Cache<String, Arc<Value>> {
    moka::sync::Cache::builder()
        .max_capacity(RESPONSE_CACHE_BYTES)
        .weigher(|key: &String, value: &Arc<Value>| {
            u32::try_from(key.len() + value.to_string().len()).unwrap_or(u32::MAX)
        })
        .time_to_idle(RESPONSE_CACHE_IDLE)
        .build()
}

impl NativeLspClient {
    pub fn new(config: JsLanguageServerConfig) -> Self {
        // Every launch path (discovery, runtime, napi) gets the per-server
        // safe defaults; user-supplied options still win key by key.
        let mut config = config;
        crate::lsp::config::apply_server_default_options(&mut config, None);
        Self {
            inner: Arc::new(NativeLspClientInner {
                config,
                child: Mutex::new(None),
                connection: Mutex::new(None),
                stderr_task: Mutex::new(None),
                stderr_lines: Arc::new(StdMutex::new(VecDeque::new())),
                capabilities: StdMutex::new(None),
                server_info: StdMutex::new(None),
                position_encoding: StdMutex::new(None),
                readiness: StdMutex::new(None),
                progress: ProgressTracker::new(),
                activity: Arc::new(AtomicUsize::new(0)),
                sync_lock: Mutex::new(()),
                open_docs: StdMutex::new(OpenDocuments::new(MAX_OPEN_DOCUMENTS)),
                memory_cap_guard: StdMutex::new(None),
                memory_watchdog: StdMutex::new(None),
                responses: response_cache(),
            }),
        }
    }

    /// Forget everything learned from the previous server process.
    fn reset_session_state(&self) {
        if let Ok(mut capabilities) = self.inner.capabilities.lock() {
            *capabilities = None;
        }
        if let Ok(mut server_info) = self.inner.server_info.lock() {
            *server_info = None;
        }
        if let Ok(mut encoding) = self.inner.position_encoding.lock() {
            *encoding = None;
        }
        if let Ok(mut readiness) = self.inner.readiness.lock() {
            *readiness = None;
        }
        if let Ok(mut open_docs) = self.inner.open_docs.lock() {
            open_docs.clear();
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut child_guard = self.inner.child.lock().await;
        if child_guard.is_some() {
            return Err(Error::new("LSP client already started"));
        }
        if let Ok(mut stderr_lines) = self.inner.stderr_lines.lock() {
            stderr_lines.clear();
        }
        self.reset_session_state();

        let validated_command =
            crate::lsp::validation::validate_lsp_server_path(self.inner.config.command.clone())?;
        let mut command_args = self.inner.config.args.clone().unwrap_or_default();
        let command_program = lsp_spawn_program(&validated_command, &mut command_args).await?;
        let mut command = tokio::process::Command::new(&command_program);
        configure_lsp_process_group(&mut command);
        command
            .args(command_args)
            .current_dir(&self.inner.config.workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // OS-level memory cap: internal buffer bounds do not stop a runaway
        // server from OOMing the host. Supported Unix targets cap the address
        // space before exec; Windows attaches a Job Object right after spawn.
        // Darwin skips RLIMIT_AS (inherited virtual mappings make lowering it
        // in pre_exec fail every spawn with EINVAL) and runs an RSS watchdog
        // on the server tree instead (`memory_watchdog_for`).
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
            if let Some(value) = std::env::var_os(key) {
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
        let memory_cap_guard = match spawn_limits::MemoryCapGuard::attach(&child, memory_cap) {
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
                section_root: configuration_section_for_command(&self.inner.config.command),
                workspace_folders: json!([{ "uri": root_uri, "name": "workspace" }]),
            },
            Arc::clone(&self.inner.progress),
        ));
        // macOS has no enforceable address-space cap: watch the tree's RSS
        // instead. Armed before `initialize` (indexing can start there);
        // dropping the guard on any failed-start path aborts it.
        let memory_watchdog = memory_watchdog_for(&child, memory_cap, &connection);
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
        if let Ok(mut capabilities) = self.inner.capabilities.lock() {
            *capabilities = initialize_result.get("capabilities").cloned();
        }
        if let Ok(mut server_info) = self.inner.server_info.lock() {
            *server_info = initialize_result.get("serverInfo").cloned();
        }
        if let Ok(mut encoding) = self.inner.position_encoding.lock() {
            *encoding = negotiated_encoding;
        }
        if let Err(error) = connection.notify("initialized", json!({})).await {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(error);
        }

        *self.inner.connection.lock().await = Some(connection);
        *self.inner.stderr_task.lock().await = stderr_task;
        if let Ok(mut guard) = self.inner.memory_cap_guard.lock() {
            *guard = Some(memory_cap_guard);
        }
        if let Ok(mut guard) = self.inner.memory_watchdog.lock() {
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
        if let Ok(mut guard) = self.inner.memory_watchdog.lock() {
            *guard = None;
        }
        if let Some(mut child) = child_guard.take() {
            wait_for_graceful_exit(&mut child, Duration::from_millis(GRACEFUL_EXIT_TIMEOUT_MS))
                .await;
        }
        drop(child_guard);
        // Release the memory-cap Job Object only after the child is reaped;
        // closing it earlier would hard-kill a gracefully-exiting server.
        if let Ok(mut guard) = self.inner.memory_cap_guard.lock() {
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
    pub async fn wait_for_ready(&self, timeout_ms: Option<u32>) -> Result<String> {
        let timeout_ms = u64::from(timeout_ms.unwrap_or(45_000));
        let readiness = self
            .inner
            .progress
            .wait_until_idle(timeout_ms)
            .await
            .as_str()
            .to_owned();
        if let Ok(mut slot) = self.inner.readiness.lock() {
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

    pub fn has_capability(&self, capability: String) -> bool {
        let Ok(capabilities) = self.inner.capabilities.lock() else {
            return false;
        };
        capabilities
            .as_ref()
            .map(|value| capability_supported(value, &capability))
            .unwrap_or(false)
    }

    /// The `positionEncoding` the server selected at initialize time, if any.
    /// `None` means the server omitted it (implying the spec default, utf-16) or
    /// the client has not started yet. octocode advertises utf-16 only, so a
    /// value other than `Some("utf-16")` indicates a non-conformant server.
    pub fn position_encoding(&self) -> Option<String> {
        self.inner
            .position_encoding
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    pub fn readiness(&self) -> Option<String> {
        self.inner
            .readiness
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    /// Sync a document's in-memory content to the server, honoring the LSP
    /// document lifecycle: the FIRST sync of a URI sends `textDocument/didOpen`
    /// (version 1); a later sync of changed content sends
    /// `textDocument/didChange` with an incremented version and a
    /// full-document content change, and a sync of the content the server
    /// already holds sends nothing. Re-sending
    /// `didOpen` is ignored or rejected by many servers and can make
    /// changed content resolve against the stale original.
    pub async fn open_document(&self, file_path: String, content: String) -> Result<()> {
        self.sync_document(&file_path, content).await.map(|_| ())
    }

    /// [`Self::open_document`], then — only when this sync was the document's
    /// first `didOpen` — wait for any project load the open triggered to go
    /// idle before returning. Servers like `typescript-language-server` start
    /// loading the project only after a `didOpen` (`$/progress begin` arrives
    /// ~100 ms later); querying immediately races that load and returns
    /// incomplete references. `settle_ms` bounds how long to wait for such a
    /// wave to start (default 400 ms) and `timeout_ms` bounds the whole wait
    /// (default 15 s, capped at 60 s).
    ///
    /// Returns the readiness string (`progressIdle`, `settledWithoutProgress`, or
    /// `timeout`) for a first open, and `None` for a re-sync of an already open
    /// document, which does not wait.
    pub async fn open_document_and_wait(
        &self,
        file_path: String,
        content: String,
        settle_ms: Option<u32>,
        timeout_ms: Option<u32>,
    ) -> Result<Option<String>> {
        let _activity = self.lease();
        let observed = self.inner.progress.subscribe();
        let version = self.sync_document(&file_path, content).await?;
        if version != Some(1) {
            return Ok(None);
        }
        let settle_ms = u64::from(settle_ms.unwrap_or(400));
        let timeout_ms = u64::from(timeout_ms.unwrap_or(15_000).min(60_000));
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

    pub async fn get_document_symbols(&self, file_path: String) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
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
            group_kill(&child);
            let _ = child.start_kill();
            let _ = child.try_wait();
        }
        if let Ok(mut capabilities) = self.capabilities.lock() {
            *capabilities = None;
        }
        if let Ok(mut server_info) = self.server_info.lock() {
            *server_info = None;
        }
    }
}

impl NativeLspClient {
    /// Sync `content` for `file_path` and return the version that was sent
    /// (`1` means a fresh `didOpen`), or `None` when the server already holds
    /// `content`.
    async fn sync_document(&self, file_path: &str, content: String) -> Result<Option<i32>> {
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
        // Content the server already holds is not sent again: a resync would
        // make the server re-analyze an unchanged document (and drop its
        // diagnostics) on every request.
        let digest = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            content.hash(&mut hasher);
            hasher.finish()
        };
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
        if let Ok(mut open_docs) = self.inner.open_docs.lock() {
            match &notification {
                Ok(()) => open_docs.record(&uri, next_version, digest),
                Err(_) => open_docs.rollback(&uri, next_version),
            }
        }
        // Close the document the cap evicted (if any) so both our bookkeeping
        // and the server's document set stay bounded. Best-effort: a stopped or
        // wedged connection makes this moot, and it must not mask the primary
        // notification result.
        if let Some(evicted_uri) = evicted {
            connection.clear_push_diagnostics(&evicted_uri);
            let _ = connection
                .notify(
                    "textDocument/didClose",
                    json!({ "textDocument": { "uri": evicted_uri } }),
                )
                .await;
        }
        notification.map(|()| Some(next_version))
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
    pub fn same_client(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// `true` while any request, sync, diagnostic wait, or [`LspLease`] is
    /// outstanding on this client (or a clone of it).
    pub fn is_busy(&self) -> bool {
        self.inner.activity.load(Ordering::Acquire) > 0
    }

    /// Definition, references, type-definition, or implementation locations
    /// with snippet content. Every server-supplied target path passes
    /// `policy` before any of its bytes are read; a refused path keeps its
    /// location with [`SNIPPET_CONTENT_WITHHELD`] as content. Reads are
    /// bounded to regular files of at most 1 MB.
    pub async fn get_locations(
        &self,
        request: LocationRequest,
        file_path: String,
        line: u32,
        character: u32,
        policy: &SnippetReadPolicy,
    ) -> Result<Vec<JsCodeSnippet>> {
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
        let result = self.request(request.method(), params).await?;
        snippets_from_locations(result, policy).await
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let (reuse, generation) = RESPONSE_SCOPE
            .try_with(|scope| {
                let scope = scope.borrow();
                (scope.reuse, scope.generation.clone())
            })
            .unwrap_or_default();
        if generation.is_empty() {
            return self.send_request(method, params).await;
        }
        let key = format!("{generation}\u{0}{method}\u{0}{params}");
        if reuse && let Some(hit) = self.inner.responses.get(&key) {
            return Ok((*hit).clone());
        }
        let value = self.send_request(method, params).await?;
        self.inner.responses.insert(key, Arc::new(value.clone()));
        Ok(value)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let _activity = self.lease();
        // Acquire a cloned handle and DROP the guard before awaiting, so the
        // request + content-modified retry loop never holds the connection
        // mutex across `.await`.
        let connection = self.connection_handle().await?;
        let mut attempts = 0;
        loop {
            let response = if supports_partial_results(method) {
                connection
                    .request_with_partials(method, params.clone(), REQUEST_TIMEOUT_MS)
                    .await
            } else {
                connection
                    .request(method, params.clone(), REQUEST_TIMEOUT_MS)
                    .await
            };
            match response {
                Ok(value) => return Ok(value),
                Err(error) if is_retryable_error(&error) && attempts < CONTENT_MODIFIED_RETRIES => {
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
}

/// See [`SnippetReadPolicy`].
pub type SnippetPathAuthorizer = dyn Fn(&Path) -> Option<PathBuf> + Send + Sync;

impl SnippetReadPolicy {
    pub fn with_authorizer(
        authorizer: impl Fn(&Path) -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        Self {
            authorizer: Some(Arc::new(authorizer)),
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
    config: &JsLanguageServerConfig,
) -> Result<Value> {
    let params = initialize_params(config)?;
    connection
        .request("initialize", params, REQUEST_TIMEOUT_MS)
        .await
}

fn initialize_params(config: &JsLanguageServerConfig) -> Result<Value> {
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
    value: Value,
    policy: &SnippetReadPolicy,
) -> Result<Vec<JsCodeSnippet>> {
    let mut snippets = Vec::new();
    let mut content_cache = SnippetContentCache::new(policy.clone());
    match value {
        Value::Null => Ok(snippets),
        Value::Array(items) => {
            for item in items {
                if let Some(snippet) = snippet_from_location_like(&item, &mut content_cache).await?
                {
                    snippets.push(snippet);
                }
            }
            Ok(snippets)
        }
        object @ Value::Object(_) => {
            if let Some(snippet) = snippet_from_location_like(&object, &mut content_cache).await? {
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

enum CachedRead {
    Source(CachedSource),
    Withheld,
    Failed(Error),
}

/// Per-response cache of snippet sources keyed by the server-supplied path,
/// so a file named by many locations is authorized, read, and line-indexed
/// once (failures and refusals are cached too).
struct SnippetContentCache {
    policy: SnippetReadPolicy,
    files: HashMap<String, CachedRead>,
}

impl SnippetContentCache {
    fn new(policy: SnippetReadPolicy) -> Self {
        Self {
            policy,
            files: HashMap::new(),
        }
    }

    /// Whole-line snippet for `range`, [`SNIPPET_CONTENT_WITHHELD`] when the
    /// policy refuses the path, or the read error.
    async fn read_range_content(&mut self, file_path: &str, range: &JsRange) -> Result<String> {
        if !self.files.contains_key(file_path) {
            let entry = match self.policy.authorize(Path::new(file_path)) {
                None => CachedRead::Withheld,
                Some(authorized) => match read_snippet_source(authorized).await {
                    Ok(content) => CachedRead::Source(CachedSource {
                        lines: LineIndex::new(&content),
                        content,
                    }),
                    Err(error) => CachedRead::Failed(error),
                },
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
/// [`crate::lsp::read_regular_bounded`].
pub(crate) fn read_bounded_regular_file(path: &Path, max_bytes: u64) -> Result<String> {
    use crate::lsp::BoundedRead;
    let bytes = crate::lsp::read_regular_bounded(path, max_bytes).map_err(|error| match error {
        BoundedRead::Io(err) => Error::new(err.to_string()),
        BoundedRead::NotRegular => {
            Error::new("not a regular file; LSP snippet content is read only from regular files")
        }
        BoundedRead::TooLarge(len) => Error::new(format!(
            "file too large for LSP snippet content ({len} bytes > {max_bytes} bytes)"
        )),
    })?;
    String::from_utf8(bytes).map_err(|err| Error::new(format!("file is not valid UTF-8: {err}")))
}

async fn snippet_from_location_like(
    value: &Value,
    content_cache: &mut SnippetContentCache,
) -> Result<Option<JsCodeSnippet>> {
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
    let file_path = uri_to_path(uri)?;
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
    Ok(Some(JsCodeSnippet {
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

fn parse_range(value: &Value) -> Result<JsRange> {
    let start = value
        .get("start")
        .ok_or_else(|| Error::new("LSP range missing start"))?;
    let end = value
        .get("end")
        .ok_or_else(|| Error::new("LSP range missing end"))?;
    Ok(JsRange {
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
    let Ok(mut lines) = stderr_lines.lock() else {
        return;
    };
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

/// Arm the RSS watchdog for a freshly spawned server on macOS, where
/// `RLIMIT_AS` cannot cap it (see [`spawn_limits`]). Over the cap, the
/// connection fails with a "language server exceeded memory cap" error and
/// the server tree is killed. `None` when no cap is configured, off macOS
/// (the pre-spawn `RLIMIT_AS` / Job Object caps apply there), or when the
/// child already exited.
#[cfg(target_os = "macos")]
fn memory_watchdog_for(
    child: &Child,
    memory_cap: Option<u64>,
    connection: &Arc<JsonRpcConnection>,
) -> Option<spawn_limits::AbortOnDrop> {
    let (cap_bytes, pid) = memory_cap.zip(child.id())?;
    let alive = Arc::downgrade(connection);
    let failed = Arc::downgrade(connection);
    Some(spawn_limits::AbortOnDrop(tokio::spawn(
        spawn_limits::watch_memory(
            pid,
            cap_bytes,
            spawn_limits::MEMORY_WATCHDOG_INTERVAL,
            move || {
                alive
                    .upgrade()
                    .is_some_and(|connection| connection.is_alive())
            },
            move |rss| {
                if let Some(connection) = failed.upgrade() {
                    connection.fail(&spawn_limits::memory_cap_exceeded_message(rss, cap_bytes));
                }
            },
        ),
    )))
}

#[cfg(not(target_os = "macos"))]
fn memory_watchdog_for(
    _child: &Child,
    _memory_cap: Option<u64>,
    _connection: &Arc<JsonRpcConnection>,
) -> Option<spawn_limits::AbortOnDrop> {
    None
}

async fn cleanup_failed_start(child: &mut Child, stderr_task: Option<JoinHandle<()>>) {
    group_kill(child);
    let _ = child.kill().await;
    if let Some(task) = stderr_task {
        task.abort();
    }
}

fn parse_position(value: &Value) -> Result<JsExactPosition> {
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
    Ok(JsExactPosition {
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
fn slice_range_content(source: &CachedSource, range: &JsRange) -> String {
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
