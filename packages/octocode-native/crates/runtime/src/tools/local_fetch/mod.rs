mod block;
pub(crate) use block::{BLOCK_MAX_LINES, declaration_spans};
mod executor;
mod output;
pub(crate) use output::Output;
mod extraction;
mod large_source;
mod pagination;
mod types;
mod validation;
pub(crate) use executor::no_match_hint;
pub use executor::{execute_local_fetch, process_fetched_content};
pub use types::*;

#[cfg(test)]
mod tests;
