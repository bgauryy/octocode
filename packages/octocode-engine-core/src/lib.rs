//! Shared, transport-neutral Rust engine primitives for Octocode.
//!
//! `octocode-engine` owns N-API adapters and JavaScript host integration.
//! `octocode-native` owns tool/runtime orchestration and the native CLI.
//! Reusable engine behavior belongs here so both consumers execute the same
//! implementation.

pub mod error;
pub mod graph;
pub mod index;
pub mod lsp;
pub mod minify;
pub mod portable;
pub mod search;
pub mod security;
pub mod signatures;
pub mod structural;
pub mod text;
pub mod types;

pub const ENGINE_CORE_API_VERSION: u32 = 1;
