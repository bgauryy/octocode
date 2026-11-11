mod bundled_tsgo;
pub mod clangd_database;
pub mod client;
mod commands;
pub mod config;
pub mod config_file;
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

/// Largest source the LSP layer reads, synchronizes (`didOpen`), resolves
/// anchors in, or cuts snippets from. One bound for every path so they cannot
/// drift; sized for real monolithic sources (TypeScript's 3 MB checker.ts).
pub const MAX_LSP_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
