//! Secret detection & content sanitization.
//!
//! The canonical pattern list lives in `octocode-engine/src/security/regexes/*.ts`
//! and is compiled into this crate's `patterns.rs` by the engine package's
//! `scripts/gen-patterns.mjs`, so Rust evaluation order matches the TypeScript
//! fallback.

pub mod detector;
pub mod patterns;
pub mod sanitizer;
pub mod types;

pub use detector::mask_text;
pub use sanitizer::sanitize_content;
