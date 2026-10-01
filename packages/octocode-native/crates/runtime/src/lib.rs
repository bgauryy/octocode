//! Native execution shared by the CLI and the MCP addon.
//! Migration is incremental; an absent handler must never delegate to Node.

// Production code is held to the `expect_used`/`unwrap_used`/`panic` denials in
// Cargo.toml `[lints]`. Test code is exempt: tests legitimately assert with
// `.unwrap()`/`.expect()`/`panic!`.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

pub mod cache;
mod canonical_json;
mod civil_date;
pub mod config;
pub mod content;
pub mod contracts;
pub mod lsp;
pub mod policy;
mod process_status;
pub mod providers;
pub mod regex;
pub mod response;
pub mod runtime;
pub mod security;
pub mod skill_install;
pub mod tools;

/// Identifies the native boundary independently of generated tool contracts.
pub const NATIVE_ABI_VERSION: u32 = 3;
