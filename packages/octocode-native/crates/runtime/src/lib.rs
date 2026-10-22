//! Native execution shared by the CLI and the MCP addon. Every tool runs
//! here; the interfaces have no execution fallback.
// Test-compiled code may assert with unwrap/expect/panic. A crate attribute,
// not clippy.toml's `allow-*-in-tests`: clippy does not treat modules mounted
// under `cfg(all(test, unix))` as test code.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

pub mod cache;
mod civil_date;
pub mod config;
pub(crate) mod content;
pub mod contracts;
mod digest;
pub mod execution;
mod git_head;
pub mod id;
pub(crate) mod num;
pub(crate) mod policy;
mod private_file;
#[cfg(test)]
mod process_fixture;
mod process_status;
pub mod providers;
mod regex_repair;
pub mod response;
pub mod runtime;
pub mod security;
pub mod tools;

/// Identifies the native boundary independently of generated tool contracts.
pub const NATIVE_ABI_VERSION: u32 = 7;
