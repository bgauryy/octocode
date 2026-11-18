//! The MCP server's warm server: the read-only tools of a session whose
//! storage is persistent run on the workspace's `octocode serve`, which
//! keeps one language-server pool for every MCP session and CLI call there
//! (`serve.rs` states what keys it). The MCP process keeps the protocol:
//! its stdio, its listing and its progress and cancellation relay. A tool
//! that writes the workspace, a session on memory storage and any call no
//! server answers run in this process. Where the server does not exist
//! (Windows has no Unix sockets) every call does.

use octocode_native::runtime::error::McpResult;
use octocode_native::runtime::{RuntimeError, ToolRuntime};
use serde_json::Value;

/// How this MCP session reaches a warm server, decided once at start.
pub(super) struct Warm {
    #[cfg(unix)]
    remote: Option<crate::cli::serve::Remote>,
}

#[cfg(unix)]
impl Warm {
    /// The session's route: through the warm server when this process's
    /// storage is persistent and a socket can be used.
    pub(super) fn resolve() -> Self {
        let remote = crate::cli::serve::Remote::resolve(crate::cli::serve::Surface::Mcp, &mut None);
        Self { remote }
    }

    /// `tool`'s MCP result from the warm server, or `None` when the call
    /// runs in this process (a tool the server does not run, no route, or
    /// no server answered: that is said once on stderr). `cancelled` cancels
    /// the call on the server. The progress it raises goes to the notice
    /// scope the caller runs in.
    pub(super) async fn call(
        &self,
        tool: &str,
        input: &Value,
        cancelled: &tokio_util::sync::CancellationToken,
    ) -> Option<Result<McpResult, RuntimeError>> {
        use crate::cli::serve::Warm as Answer;
        let remote = self.remote.as_ref()?;
        if !octocode_runtime_base::id::ToolId::from_name(tool)
            .is_some_and(crate::cli::serve::runs_on_server)
        {
            return None;
        }
        match remote.call_mcp(tool, input, cancelled).await {
            Answer::Answered(result) => Some(Ok(result)),
            Answer::Ended(error) => Some(Err(error)),
            Answer::InProcess(reason) => {
                if let Some(reason) = reason {
                    eprintln!("[octocode-mcp] {reason}");
                }
                None
            }
        }
    }

    /// Warm the session's workspace under `lsp.prewarm` `all`: on the warm
    /// server, which keeps the language servers for every session, else in
    /// this process (what a runtime does by itself).
    pub(super) async fn prewarm(&self, runtime: &ToolRuntime) {
        let handle = tokio::runtime::Handle::current();
        let wanted = runtime.config().resolved.lsp.prewarm
            == octocode_runtime_base::config::LspPrewarm::All
            && runtime.is_available(octocode_runtime_base::id::ToolId::LspSearch.as_str());
        if wanted && let Some(remote) = &self.remote {
            match remote.prewarm().await {
                Ok(()) => return,
                Err(reason) => eprintln!("[octocode-mcp] prewarm in process ({reason})"),
            }
        }
        runtime.prewarm_lsp_workspace(&handle);
    }
}

#[cfg(not(unix))]
impl Warm {
    pub(super) fn resolve() -> Self {
        Self {}
    }

    pub(super) async fn call(
        &self,
        _tool: &str,
        _input: &Value,
        _cancelled: &tokio_util::sync::CancellationToken,
    ) -> Option<Result<McpResult, RuntimeError>> {
        None
    }

    pub(super) async fn prewarm(&self, runtime: &ToolRuntime) {
        runtime.prewarm_lsp_workspace(&tokio::runtime::Handle::current());
    }
}

#[cfg(all(test, windows))]
mod tests;
