//! `octocode-engine` — Rust engine primitives: search, LSP, signatures,
//! structural analysis, minification, security, graph, and text
//! utilities.
//!
//! The runtime and CLI crates consume it as a plain Rust library.
// The workspace lint floor applies (Cargo.toml `[lints] workspace = true`).
// This library adds one rule the CLI host cannot share: output goes through
// its callers, never to stdout/stderr.
#![deny(clippy::print_stdout, clippy::print_stderr)]

pub(crate) mod deep_stack;
pub mod digest;
pub mod error;
pub mod fact_scan;
pub mod graph;
pub mod json_size;
pub mod lsp;
pub mod minify;
pub mod process;
pub mod process_env;
pub mod scan_budget;
pub mod search;
pub mod security;
pub mod signatures;
pub mod stamp;
pub mod structural;
#[cfg(test)]
mod test_cpu;
#[cfg(test)]
pub(crate) mod test_support;
pub mod text;
pub mod types;
