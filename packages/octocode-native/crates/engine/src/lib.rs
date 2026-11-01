//! `octocode-engine` — Rust engine primitives: search, LSP, signatures,
//! structural analysis, minification, security, graph, and text
//! utilities.
//!
//! The runtime and CLI crates consume it as a plain Rust library.
// The workspace lint floor applies (Cargo.toml `[lints] workspace = true`).
// This library adds one rule the CLI host cannot share: output goes through
// its callers, never to stdout/stderr.
#![deny(clippy::print_stdout, clippy::print_stderr)]
// Numeric conversions are explicit (`try_from` with a stated outcome): a
// lossy `as` cast fails CI (`-D warnings`). Tests may cast fixture values.
#![cfg_attr(
    not(test),
    warn(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )
)]

pub(crate) mod deep_stack;
pub mod digest;
pub mod error;
pub mod fact_scan;
pub mod graph;
pub mod json_size;
pub mod lsp;
pub mod minify;
pub(crate) mod num;
pub mod process;
pub mod process_env;
pub mod scan_budget;
pub mod search;
pub mod security;
pub mod signatures;
pub mod stamp;
pub mod structural;
pub mod symbol_kind;
#[cfg(test)]
mod test_cpu;
#[cfg(test)]
pub(crate) mod test_support;
pub mod text;
pub mod types;
