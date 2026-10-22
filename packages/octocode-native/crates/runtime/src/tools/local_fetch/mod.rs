mod acquire;
mod capture;
pub(crate) use acquire::MAX_SOURCE_BYTES;
pub(crate) use capture::capture_text;
mod block;
pub(crate) use block::BLOCK_MAX_LINES;
pub(crate) use pagination::LINE_PAGE_BYTES;
mod executor;
mod output;
pub(crate) use output::Output;
mod extraction;
mod key_outline;
mod large_source;
mod no_match;
mod outline;
mod page_scan;
mod pagination;
mod pattern_repair;
mod prepare;
mod selection;
pub(crate) mod types;
pub(crate) use executor::process_fetched_content_selected;
pub use executor::{execute_local_fetch, process_fetched_content};
pub use types::*;

#[cfg(test)]
mod tests;
