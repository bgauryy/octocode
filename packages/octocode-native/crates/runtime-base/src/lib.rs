//! The runtime layers below the tools: identifiers and numbers, the embedded
//! contracts, content, path and network policy, configuration, security,
//! the request kernel (`execution`), response shaping, retained work
//! (`cache`) and the providers. `octocode-native` runs every tool over them;
//! nothing here names a tool's internals or the runtime that drives them.

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
// Library output reaches people through its callers or the `octocode_io::notice`
// owner, never stray stdout/stderr prints; deliberate sites carry `#[expect]`.
#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod cache;
pub mod civil_date;
pub mod config;
pub mod content;
pub mod contracts;
pub mod digest;
pub mod execution;
pub mod id;
pub mod json_pointer;
pub mod num;
pub mod policy;
pub mod private_file;
#[cfg(any(test, feature = "test-support"))]
pub mod process_fixture;
#[cfg(any(test, feature = "test-support"))]
pub mod process_status;
pub mod providers;
pub mod response;
pub mod security;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
