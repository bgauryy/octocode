//! Native execution shared by the CLI commands and the stdio MCP server
//! (`octocode mcp`) of the one `octocode` binary. Every tool runs
//! here, admitted and dispatched by [`runtime`]; the tools live in their
//! family crates. The interfaces have no execution fallback.

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

mod build_identity;
pub mod runtime;
/// Tests that cross the tool families or run the whole runtime, at the
/// module paths they had before the split.
#[cfg(test)]
mod tools;
