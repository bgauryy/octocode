//! The package tools: artifactSearch, securityCheck and the artifact provider.

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

pub mod artifact;
pub mod artifact_search;
pub mod security_check;

pub use artifact::CallShare;
