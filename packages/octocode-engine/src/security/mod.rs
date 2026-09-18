//! Rust-owned secret detection and content sanitization.
//!
//! `patterns.rs` is the canonical ordered pattern set.

pub mod detector;
pub mod patterns;
pub mod sanitizer;
pub mod types;

pub use detector::mask_text;
pub use sanitizer::sanitize_content;
