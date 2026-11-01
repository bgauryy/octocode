mod bundled_tsgo;
pub mod client;
mod commands;
pub mod config;
pub mod diagnostics;
pub mod family;
pub(crate) mod grammar;
pub mod managed;
mod module_path;
pub mod pool;
mod project_config;
pub mod resolver;
mod spawn_limits;
pub(crate) mod transport;
pub mod types;
pub mod uri;
pub(crate) mod validation;
pub mod workspace;

pub use transport::with_caller_deadline;

/// Worst configured cold path before a request's own wait may be cut by its
/// caller's deadline, without process-start and transport overhead:
/// `initialize` (a background start no caller bounds) waiting out its
/// ceiling, `CEILING_FACTOR` times the request timeout, then readiness, the
/// content-modified retry delays, and the request's base timeout, which a
/// hung server gets before it fails. A progressing server is then waited for
/// up to the caller's deadline ([`with_caller_deadline`]). Host deadlines must
/// be strictly larger, so a cold request always reaches its base.
pub const MAX_COLD_LSP_EXECUTION_BUDGET_MS: u64 = (client::REQUEST_TIMEOUT_MS as u64)
    * (transport::CEILING_FACTOR as u64)
    + pool::MAX_READINESS_TIMEOUT_MS
    + client::CONTENT_MODIFIED_RETRY_DELAY_MS * client::CONTENT_MODIFIED_RETRIES as u64
    + client::REQUEST_TIMEOUT_MS as u64;

/// Largest source the LSP layer reads, synchronizes (`didOpen`), resolves
/// anchors in, or cuts snippets from. One bound for every path so they cannot
/// drift; sized for real monolithic sources (TypeScript's 3 MB checker.ts).
pub const MAX_LSP_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
