//! Shared, transport-neutral Rust engine primitives for Octocode.
//!
//! `octocode-engine` owns N-API adapters and JavaScript host integration.
//! `octocode-native` owns tool/runtime orchestration and the native CLI.
//! Reusable engine behavior belongs here so both consumers execute the same
//! implementation.
//!
//! When built with `napi-addon`, this crate also exposes the `bindings` module
//! that contains the Node.js `#[napi]` function wrappers. These are compiled
//! into the `octocode-engine` cdylib and registered automatically at load time.

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

/// Node.js N-API function bindings. Only compiled when the `napi-addon` feature
/// is enabled. Consumed by the `octocode-engine` cdylib which links this crate
/// and re-exports this module for Rust callers.
#[cfg(feature = "napi-addon")]
pub mod bindings;

pub const ENGINE_CORE_API_VERSION: u32 = 1;
