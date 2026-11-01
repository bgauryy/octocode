//! Warm CLI tool calls: `octocode serve`.
//!
//! What a user sees (which calls run on a server, what keys it, the
//! limits, the fallbacks) is the package README's "Warm tool server"
//! section. This module keeps those promises:
//!
//! - A socket name hashes `exe_identity`, `workspace_of` and
//!   `call_identity`, so any change of them reaches a fresh server and no
//!   secret appears in a name. The engine's [`PROCESS_ENV`] lists what it
//!   reads from the server's own process, which a call cannot bring.
//! - A request carries the client's working directory, environment
//!   (`served_env`) and call identity. `Daemon::runtime_for` keeps a
//!   runtime per directory and `env_digest` (`MAX_RUNTIMES`, least
//!   recently used out), built off the socket thread, one build per key,
//!   all on one language-server pool. A runtime whose identity differs is
//!   rebuilt; a call that still cannot match gets a `fallback` reply.
//!   `octocode auth login`/`logout` stop every server (`stop_all`).
//! - Only [`runs_on_server`] tools are sent. The reply is the call as
//!   printed in process ([`Printed`]) plus the notices raised in the call's
//!   own [`octocode_io::notice::Scope`], which its row work carries to
//!   other threads; the client prints its own configuration warnings.
//! - `MAX_SERVERS` per socket directory, least recently asked (`touch`)
//!   stopped first; a `STOP` request drains the calls taken. The server
//!   exits after `IDLE_EXIT` without a request.
//! - A failed exchange falls back to an in-process call and says so; no
//!   reply within `REPLY_DEADLINE` answers `timeout`, never a second run.
//!   Ctrl-C half-closes the client's side, which cancels the call on the
//!   server; the client drains, and a second Ctrl-C stops waiting.
//!
//! The Unix machinery lives in `unix`: `client` (the call side),
//! `identity` (what keys a server), `sockets` (the socket directory) and
//! `daemon` (the server). Elsewhere every call runs in process.
//!
//! [`PROCESS_ENV`]: octocode_engine::process_env::PROCESS_ENV
//! [`Printed`]: super::Printed

#[cfg(unix)]
mod unix;

use octocode_native::id::ToolId;
#[cfg(unix)]
pub(super) use unix::{call, run, stop_all, stop_all_and_wait};

/// Whether `tool` runs on the warm server: every read-only tool, so a
/// graphQuery reuses the snapshot an earlier call decoded. A tool that
/// writes a clone checkout or a graph snapshot runs in process, which also
/// prints a build's progress on this process's stderr.
pub(super) const fn runs_on_server(tool: ToolId) -> bool {
    match tool {
        ToolId::GhCloneRepo | ToolId::GraphBuild => false,
        ToolId::GraphQuery
        | ToolId::Clasify
        | ToolId::GhSearchRepo
        | ToolId::GhSearchCode
        | ToolId::GhStructure
        | ToolId::GhGetFileContent
        | ToolId::GhSearchHistory
        | ToolId::GhGetHistoryItem
        | ToolId::ArtifactSearch
        | ToolId::LocalSearch
        | ToolId::StructureSearch
        | ToolId::AstSearch
        | ToolId::LocalFetch
        | ToolId::LspSearch => true,
    }
}

#[cfg(not(unix))]
pub(super) async fn call(
    _tool: &str,
    _input: &serde_json::Value,
    _mode: super::OutputMode,
) -> Option<u8> {
    None
}

#[cfg(not(unix))]
pub(super) fn stop_all() {}

#[cfg(not(unix))]
pub(super) async fn run(_socket: &std::path::Path) -> u8 {
    super::emit_error(
        octocode_contract_types::ErrorCode::ToolUnavailable,
        "octocode serve needs Unix sockets.",
        false,
    );
    2
}

#[cfg(not(unix))]
pub(super) async fn stop_all_and_wait() -> Result<(), String> {
    Ok(())
}
