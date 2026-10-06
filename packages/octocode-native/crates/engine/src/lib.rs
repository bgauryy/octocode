//! `octocode-engine` — Rust engine primitives: search, LSP, signatures,
//! structural analysis, minification, security, graph, and text
//! utilities.
//!
//! The runtime, CLI, and runtime N-API adapter crates consume it as a plain
//! Rust library.

// Production code is held to the `expect_used`/`unwrap_used`/`panic` denials in
// Cargo.toml `[lints]`. Test code is exempt: tests legitimately assert with
// `.unwrap()`/`.expect()`/`panic!` and forcing `Result`-returning tests only
// hurts readability without adding safety.
#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used, clippy::panic))]

pub mod digest;
pub mod error;
pub mod graph;
pub mod lsp;
pub mod minify;
pub mod portable;
pub mod search;
pub mod security;
pub mod signatures;
pub mod structural;
pub mod text;
pub mod types;
