//! Server start (spawn, `initialize`, encoding check) and stop.

use super::server_process::{
    GRACEFUL_EXIT_TIMEOUT_MS, StderrPipe, cleanup_failed_start, cpu_clock_for, lsp_spawn_program,
    memory_watchdog_for, wait_for_graceful_exit,
};
use super::stderr::spawn_stderr_reader;
use super::{NativeLspClient, REQUEST_TIMEOUT_MS};
use crate::error::{Error, Result};
use crate::lsp::spawn_limits;
use crate::lsp::transport::{ClientRequestContext, JsonRpcConnection, WaitBudget};
use crate::lsp::types::LspLanguageServerConfig;
use crate::lsp::uri::path_to_uri;
use serde_json::{Value, json};
use std::process::Stdio;
use std::sync::Arc;
use tokio::time::Duration;

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

impl NativeLspClient {
    pub async fn start(&self) -> Result<()> {
        self.start_until(std::future::pending()).await
    }

    /// [`Self::start`] for a start that `abandoned` ends once no caller waits
    /// for it any more. The `initialize` wait has no ceiling of its own: it
    /// lasts while the server shows progress (a server that indexes inside
    /// `initialize`, as jdtls and Metals do on a large project, is waited
    /// for), so `abandoned` is what ends the wait of a start whose callers
    /// left; the server tree is then stopped.
    pub(crate) async fn start_until(
        &self,
        abandoned: impl std::future::Future<Output = ()> + Send,
    ) -> Result<()> {
        let mut child_guard = self.inner.child.lock().await;
        if child_guard.is_some() {
            return Err(Error::new("LSP client already started"));
        }
        let began = self.inner.clock.now();
        {
            let mut stderr_lines = self
                .inner
                .stderr_lines
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            stderr_lines.clear();
        }
        self.reset_session_state();

        // OS-level memory cap: internal buffer bounds do not stop a runaway
        // server from OOMing the host. macOS and Linux bound the memory the
        // whole server tree holds with a memory watchdog
        // (`memory_watchdog_for`); Windows attaches a Job Object right after
        // spawn; other Unix targets cap the address space before exec.
        let memory_cap = spawn_limits::memory_cap_bytes(self.inner.config.max_memory_mb);
        let client = self.clone();
        tokio::task::spawn_blocking(move || client.prepare_project_files())
            .await
            .map_err(|error| {
                Error::new(format!(
                    "Could not prepare the language server's project files: {error}"
                ))
            })??;
        let mut command = server_command(&self.inner.config, memory_cap).await?;
        let stderr_pipe = StderrPipe::attach(&mut command).await?;
        let spawned = command.spawn();
        // The command holds this process's copy of the server's stderr end
        // on Windows; the pipe reaches EOF only once it is closed.
        drop(command);
        let mut child =
            spawned.map_err(|err| Error::new(format!("Failed to start language server: {err}")))?;
        let mut memory_cap_guard = match crate::process::Job::attach_async(&child, memory_cap) {
            Ok(guard) => guard,
            Err(error) => {
                cleanup_failed_start(&mut child, None, None).await;
                return Err(error);
            }
        };
        let stderr_task = stderr_pipe
            .reader(&mut child)
            .map(|stderr| spawn_stderr_reader(stderr, Arc::clone(&self.inner.stderr_lines)));
        let Some(stdout) = child.stdout.take() else {
            cleanup_failed_start(&mut child, Some(&mut memory_cap_guard), stderr_task).await;
            return Err(Error::new("Language server stdout pipe missing"));
        };
        let Some(stdin) = child.stdin.take() else {
            cleanup_failed_start(&mut child, Some(&mut memory_cap_guard), stderr_task).await;
            return Err(Error::new("Language server stdin pipe missing"));
        };

        let root_uri = match path_to_uri(&self.inner.config.workspace_root) {
            Ok(uri) => uri,
            Err(error) => {
                cleanup_failed_start(&mut child, Some(&mut memory_cap_guard), stderr_task).await;
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
                section_root: crate::lsp::config::configuration_section(
                    self.inner.config.language_id.as_deref(),
                )
                .map(str::to_owned),
                workspace_folders: json!([{ "uri": root_uri, "name": "workspace" }]),
            },
            Arc::clone(&self.inner.progress),
            cpu_clock_for(&child),
            self.inner.clock.clone(),
        ));
        // Watch the server tree's memory (macOS has no address-space cap;
        // Linux RLIMIT_AS bounds one process, not the tree). Armed before
        // `initialize` (indexing can start there); dropping the guard on any
        // failed-start path aborts it.
        let memory_watchdog = memory_watchdog_for(
            &child,
            memory_cap,
            &connection,
            Arc::clone(&self.inner.resident_bytes),
        );
        let initialized = tokio::select! {
            biased;
            result = initialize(&connection, &self.inner.config, &self.inner.clock) => result,
            () = abandoned => Err(Error::with_kind(
                crate::error::ErrorKind::Cancelled,
                "LSP start abandoned: no caller waits for it any more",
            )),
        };
        let initialize_result = match initialized {
            Ok(value) => value,
            Err(error) => {
                cleanup_failed_start(&mut child, Some(&mut memory_cap_guard), stderr_task).await;
                return Err(self.with_server_stderr(error));
            }
        };
        // Positions are UTF-16 throughout the tool contract. A server that
        // ignores our advertised encoding cannot supply trustworthy locations.
        let negotiated_encoding = extract_position_encoding(&initialize_result);
        if let Some(encoding) = negotiated_encoding.as_deref()
            && encoding != "utf-16"
        {
            cleanup_failed_start(&mut child, Some(&mut memory_cap_guard), stderr_task).await;
            return Err(Error::new(format!(
                "Unsupported language server positionEncoding '{encoding}': \
                         octocode advertises utf-16; semantic positions cannot be resolved safely"
            )));
        }
        self.record_server(&initialize_result);
        if let Err(error) = connection.notify("initialized", json!({})).await {
            cleanup_failed_start(&mut child, Some(&mut memory_cap_guard), stderr_task).await;
            return Err(self.with_server_stderr(error));
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
        self.record_wait(super::waits::WaitKind::Starting, began);
        Ok(())
    }

    /// What a start reads from disk before the server runs: clangd's copy of
    /// the project's compilation database, written under the Octocode home,
    /// and the files that database lists. Blocking file reads and a parse of
    /// a database that can be many megabytes: [`Self::start_until`] runs
    /// this on a blocking thread, never on the runtime thread that serves
    /// every other connection's reads, writes and timers.
    fn prepare_project_files(&self) -> Result<()> {
        let config = &self.inner.config;
        crate::lsp::clangd_database::prepare_mirror(config).map_err(|error| {
            Error::new(format!(
                "Could not prepare the clangd index directory under the Octocode home: {error}. Set storage.mode to memory to run without an index."
            ))
        })?;
        let _ = self.inner.started_at.set(std::time::SystemTime::now());
        if crate::lsp::clangd_database::indexes_project(config) {
            let _ = self
                .inner
                .listed_files
                .set(crate::lsp::clangd_database::listed_files(config));
        }
        Ok(())
    }

    /// `error` of a failed start, with what the server wrote to stderr before
    /// it failed (the ring's recent lines): a server that dies at launch says
    /// why there, and the closed connection alone does not.
    fn with_server_stderr(&self, mut error: Error) -> Error {
        let lines = self
            .inner
            .stderr_lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            error.reason.push_str("; server stderr: ");
            error.reason.push_str(&lines.join(" | "));
        }
        error
    }

    /// Keep what the server's `initialize` answer says about it: its
    /// capabilities and `serverInfo`.
    fn record_server(&self, initialize_result: &Value) {
        {
            let mut capabilities = self
                .inner
                .capabilities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *capabilities = initialize_result.get("capabilities").cloned();
        }
        let mut server_info = self
            .inner
            .server_info
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *server_info = initialize_result.get("serverInfo").cloned();
    }

    pub async fn stop(&self) -> Result<()> {
        // Same lock order as `start` (child → connection): holding `child`
        // for the whole teardown makes a stop that overlaps a start wait for
        // it, then shut down the connection and child it published.
        let mut child_guard = self.inner.child.lock().await;
        // Read the tree while the server is alive: once it exits, a
        // descendant in its own session (jdtls's Gradle daemon) is
        // re-parented away and no later walk finds it.
        let tree = match child_guard.as_ref().and_then(tokio::process::Child::id) {
            Some(pid) => crate::process::TreeSnapshot::read(pid).await,
            None => crate::process::TreeSnapshot::default(),
        };
        let connection = self.inner.connection.lock().await.take();
        if let Some(connection) = connection {
            // A fixed second: an evicted server mid-index shows progress for
            // minutes, and a caller's deadline must not extend its exit.
            let _ = connection
                .request(
                    "shutdown",
                    Value::Null,
                    WaitBudget::fixed(1_000, self.inner.clock.now()),
                )
                .await;
            // A request still pending on this server gets no answer after
            // `shutdown`: it fails now, saying the server was stopped, not
            // that the connection closed on its own.
            connection.retire("LSP connection closed: Octocode stopped the language server");
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
            wait_for_graceful_exit(
                &mut child,
                Duration::from_millis(GRACEFUL_EXIT_TIMEOUT_MS),
                tree,
            )
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
        if let Some(drain) = self.inner.stderr_task.lock().await.take() {
            drain.abort();
        }
        self.reset_session_state();
        Ok(())
    }
}

/// Extracts the server-selected `positionEncoding` from an `InitializeResult`.
/// Returns `None` when the server omits it (which the LSP spec defines as the
/// utf-16 default).
/// The spawn command of the configured server: its validated program and
/// arguments in the workspace root, piped stdin and stdout (stderr is
/// [`StderrPipe`]'s), the pre-spawn memory cap where the platform applies
/// one, and a scrubbed environment.
async fn server_command(
    config: &LspLanguageServerConfig,
    memory_cap: Option<u64>,
) -> Result<tokio::process::Command> {
    // The server's whole environment: what discovery carried from the
    // call (`PATH`, the toolchain variables) and what its entry configures.
    let spawn_env = config.env.clone().unwrap_or_default();
    let validated_command = crate::lsp::validation::validate_lsp_server_path(
        config.command.clone(),
        Some(std::path::Path::new(&config.workspace_root)),
        &spawn_env,
    )?;
    let mut command_args = config.args.clone().unwrap_or_default();
    let command_program =
        lsp_spawn_program(&validated_command, &mut command_args, &spawn_env).await?;
    let mut command = tokio::process::Command::new(&command_program);
    crate::process::contain(command.as_std_mut());
    command
        .args(command_args)
        .current_dir(&config.workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cap_bytes) = memory_cap {
        spawn_limits::apply_pre_spawn_cap(&mut command, cap_bytes);
    }
    // Never leak octocode's own environment (OCTOCODE_CLASSIFICATION_API, GITHUB_TOKEN,
    // etc.) into a spawned language server: start from an empty environment
    // and give it only the server's configured environment, which discovery
    // fills from a minimal, secret-free allowlist of the call's own
    // (`server_process::LSP_SERVER_ENV_ALLOWLIST`; `PATH` must be there so
    // built-in servers resolved via `PATH` still launch). The process's
    // environment is never read: another caller's server is not started
    // under this process's.
    command.env_clear();
    command.envs(&spawn_env);
    Ok(command)
}

pub(super) fn extract_position_encoding(initialize_result: &Value) -> Option<String> {
    initialize_result
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("positionEncoding"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Sends `initialize` and waits for its answer while the server progresses:
/// the start runs in a supervisor task outside every caller's scope, and a
/// server that indexes inside `initialize` is not cut at a fixed ceiling
/// (see [`WaitBudget::while_progressing`]). A server that shows no activity
/// for the base window still fails.
pub(super) async fn initialize(
    connection: &JsonRpcConnection,
    config: &LspLanguageServerConfig,
    clock: &crate::lsp::clock::LspClock,
) -> Result<Value> {
    let params = initialize_params(config)?;
    connection
        .request(
            "initialize",
            params,
            WaitBudget::while_progressing(REQUEST_TIMEOUT_MS, clock.now()),
        )
        .await
}

/// Every `SymbolKind` the protocol defines (1 File to 26 TypeParameter): a
/// server without this set answers with the 1.0 kinds only.
fn symbol_kinds() -> Value {
    json!({ "valueSet": (1..=26).collect::<Vec<u32>>() })
}

pub(super) fn initialize_params(config: &LspLanguageServerConfig) -> Result<Value> {
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
                "documentSymbol": { "dynamicRegistration": false, "hierarchicalDocumentSymbolSupport": true, "symbolKind": symbol_kinds() },
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
                "symbol": { "dynamicRegistration": false, "symbolKind": symbol_kinds() },
                // A server that registers gets file events for the scope's
                // files at each request (`client/watched.rs`), so a file no
                // request opened still reaches its index.
                "didChangeWatchedFiles": { "dynamicRegistration": true, "relativePatternSupport": true }
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
