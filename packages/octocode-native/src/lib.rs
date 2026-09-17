//! Native execution shared by the CLI and the optional MCP addon.
//! Migration is incremental; an absent handler must never delegate to Node.

#[cfg(feature = "napi-addon")]
mod adapter_napi;

pub mod cache;
pub mod config;
pub mod content;
pub mod contracts;
pub mod errors;
pub mod lsp;
pub mod policy;
mod process_status;
pub mod providers;
pub mod regex;
pub mod response;
pub mod runtime;
pub mod security;
pub mod tools;

/// Identifies the native boundary independently of generated tool contracts.
pub const NATIVE_ABI_VERSION: u32 = 2;
