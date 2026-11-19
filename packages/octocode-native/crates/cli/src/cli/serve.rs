//! Warm CLI tool calls: `octocode serve`.
//!
//! What a user sees (which calls run on a server, what keys it, the
//! limits, the fallbacks) is the package README's "Warm tool server"
//! section. This module keeps those promises:
//!
//! - A socket name hashes `KEY_VERSION`, `exe_identity` (the executable's
//!   device and inode, version, size and modification time: no path),
//!   `workspace_of` and `process_identity` (what the engine reads from the
//!   server's own process, [`PROCESS_ENV`], and [`NETWORK_ENV`], which a call
//!   cannot bring), so any change of them reaches a fresh server and no
//!   secret appears in a name. The configuration (`call_identity`, which
//!   covers every declared `OCTOCODE_*` name) keys a runtime, not the socket.
//!   A request carries the client's process identity; a server under another
//!   answers `fallback`.
//! - A request carries the client's surface (`Surface`: the CLI, or an MCP
//!   server), working directory, environment (`served_env`) and call
//!   identity. `Daemon::runtime_for` keeps a runtime per directory, surface
//!   and `env_digest` (`MAX_RUNTIMES`, least recently used out), built off the
//!   socket thread, one build per key, all on one language-server pool. A
//!   runtime whose identity differs is rebuilt; a call that still cannot
//!   match gets a `fallback` reply. `octocode auth login`/`logout` stop every
//!   server (`stop_all`).
//! - Only [`runs_on_server`] tools are sent. The CLI's reply is the call as
//!   printed in process ([`Printed`]) plus the notices raised in the call's
//!   own [`octocode_io::notice::Scope`], which its row work carries to
//!   other threads; the client prints its own configuration warnings. An MCP
//!   server (`Remote`, `mcp_server/warm.rs`) is answered as an MCP result
//!   from `mcp_result`, so its text is the in-process server's bytes; its
//!   notices stay on the server, and its prewarm is a request the server
//!   answers once the runtime exists.
//!   The call's progress lines (quota waits, language-server start) are
//!   not in the reply: the server writes each as a frame line while the
//!   call runs and the client prints it at once (`unix/frames.rs`). One
//!   writer owns a connection's write side and is polled beside the call,
//!   so a client that stops reading never stalls the call; it gets the
//!   newest `BACKLOG` progress lines, and one that takes no byte for
//!   `READ_STALL` is gone, as one that closed.
//! - No count cap on servers: a server leaves only on `IDLE_EXIT` without a
//!   request (a connection that sends none does not count, nor keeps the
//!   server running) or a `STOP` request (`octocode auth login`/`logout`,
//!   cache clear), which closes the connections that asked nothing and
//!   drains the calls taken. A client waiting for the drain hears a
//!   heartbeat frame while it runs; `stop_and_wait_in` gives up on a server
//!   silent for `READ_STALL` and leaves the cache uncleared. A starting server never stops another, so one
//!   agent's warm language servers survive another's first call; the
//!   language servers themselves are bounded by the shared pool's idle
//!   timeout and memory budget. A server marks its socket's modification
//!   time (`touch`) on each request it reads.
//!   Windows has no server (every call runs in process), so no count or
//!   expiry applies there.
//! - A call no server answers starts one and waits for the outcome it
//!   announces on a private startup channel (`unix/startup.rs`): ready,
//!   another server answering, or why it cannot serve. A child that exits
//!   or closes the channel first ends the wait at once with its exit status
//!   and stderr; no interval abandons a start that is alive, and Ctrl-C
//!   ends the wait. The socket directory's lock makes one of the servers
//!   starting at once the owner. A start that fails is named on stderr and
//!   the call, not yet sent, runs in process.
//! - A failed exchange falls back to an in-process call and says so. The
//!   client waits for the reply as long as the server's request budget
//!   allows (`reply_deadline`; the CLI sets none) and the server keeps
//!   talking: it sends a frame at least each `HEARTBEAT` while a call runs,
//!   and one silent for `READ_STALL` (stopped, hung) is a failed exchange.
//!   No reply within a budget answers `timeout`, never a second run.
//!   Ctrl-C (or an MCP cancel) half-closes the client's side, which cancels
//!   the call on the server; the client drains, and a second Ctrl-C stops
//!   waiting. A server that dies mid-call is a failed exchange: the call
//!   runs in process once, and is not asked again of a fresh server.
//!
//! The Unix machinery lives in `unix`: `client` (the call side),
//! `frames` (progress lines on the socket), `identity` (what keys a
//! server), `sockets` (the socket directory), `startup` (starting one) and
//! `daemon` (the server), `surface` (CLI or MCP). Elsewhere every call runs
//! in process, MCP's included.
//!
//! [`PROCESS_ENV`]: octocode_engine::process_env::PROCESS_ENV
//! [`NETWORK_ENV`]: octocode_io::http::NETWORK_ENV
//! [`Printed`]: super::output::Printed

#[cfg(unix)]
mod unix;

use octocode_runtime_base::id::ToolId;
#[cfg(unix)]
pub(super) use unix::{Remote, Surface, Warm, call, run, stop_all, stop_all_and_wait};

/// Whether `tool` runs on the warm server: every read-only tool, so a
/// graphQuery reuses the snapshot an earlier call decoded. A tool that
/// writes workspace artifacts (contract `mutatesWorkspace`: a clone
/// checkout, a graph snapshot) runs in process, which also prints a
/// build's progress on this process's stderr.
pub(super) const fn runs_on_server(tool: ToolId) -> bool {
    !tool.mutates_workspace()
}

#[cfg(not(unix))]
pub(super) async fn call(
    _tool: &str,
    _input: &serde_json::Value,
    _mode: super::OutputMode,
    _loaded: &mut Option<octocode_runtime_base::config::ConfigInput>,
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
