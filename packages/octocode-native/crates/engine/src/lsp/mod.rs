pub mod client;
mod commands;
pub mod config;
pub mod grammar;
pub mod json_rpc;
pub mod pool;
mod push_diagnostics;
pub mod resolver;
mod spawn_limits;
pub mod symbol_kind;
pub mod types;
pub mod uri;
pub mod validation;
pub mod workspace;

/// Worst configured cold-start path before process-start and transport
/// overhead: initialize + readiness + one request with all content-modified
/// retries and their delays. Host deadlines must be strictly larger.
pub const MAX_COLD_LSP_EXECUTION_BUDGET_MS: u64 = (client::REQUEST_TIMEOUT_MS as u64)
    * (2 + client::CONTENT_MODIFIED_RETRIES as u64)
    + pool::MAX_READINESS_TIMEOUT_MS
    + client::CONTENT_MODIFIED_RETRY_DELAY_MS * client::CONTENT_MODIFIED_RETRIES as u64;
