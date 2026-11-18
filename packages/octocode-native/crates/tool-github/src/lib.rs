//! The GitHub tools: search, structure, reads, history and clones.

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

pub mod gh_clone_repo;
pub mod gh_get_file_content;
pub mod gh_get_history_item;
pub mod gh_search_code;
pub mod gh_search_history;
pub mod gh_search_repo;
pub mod gh_structure;
#[cfg(test)]
mod provider_backed;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
