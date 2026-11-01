//! Native execution shared by the CLI commands and the stdio MCP server
//! (`octocode mcp`) of the one `octocode` binary. Every tool runs
//! here; the interfaces have no execution fallback.

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

pub mod cache;
mod civil_date;
pub mod config;
pub(crate) mod content;
pub mod contracts;
mod digest;
pub mod execution;
pub mod id;
pub(crate) mod num;
pub(crate) mod policy;
mod private_file;
#[cfg(test)]
mod process_fixture;
#[cfg(test)]
mod process_status;
pub mod providers;
pub mod response;
pub mod runtime;
pub mod security;
pub mod tools;
