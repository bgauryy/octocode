//! `octocode-engine` — Rust engine primitives: search, LSP, signatures,
//! structural analysis, minification, security, graph, and text
//! utilities.
//!
//! The runtime and CLI crates consume it as a plain Rust library.
// Test-compiled code may assert with unwrap/expect/panic. A crate attribute,
// not clippy.toml's `allow-*-in-tests`: clippy does not treat modules mounted
// under `cfg(all(test, unix))` as test code.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]
// The workspace lint floor applies (Cargo.toml `[lints] workspace = true`). A
// library adds to it: output goes through its callers, never to stdout/stderr,
// plus style lints that would be noise in the host crates.
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![warn(
    clippy::use_self,
    clippy::cloned_instead_of_copied,
    clippy::redundant_closure_for_method_calls,
    clippy::semicolon_if_nothing_returned
)]

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
