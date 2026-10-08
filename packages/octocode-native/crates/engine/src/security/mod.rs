//! Rust-owned secret detection and content sanitization.
//!
//! `patterns.rs` is the canonical ordered pattern set.

pub(crate) mod detector;
pub(crate) mod patterns;
pub(crate) mod sanitizer;
pub mod types;
