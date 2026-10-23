//! The one view engine over fetched source text: the localFetch query and
//! result types, extraction, outline, pagination, selection and read limits.
//! It performs no filesystem access, so `tools/local_fetch` (local files) and
//! `tools/gh_get_file_content` (remote files) render through the same code,
//! and `tools/local_search` sizes the reads it leads to by the same limits.
mod block;
pub(crate) use block::BLOCK_MAX_LINES;
pub(crate) mod extraction;
pub(crate) use extraction::omission_marker;
mod key_outline;
pub(crate) mod no_match;
pub(crate) mod outline;
mod page_scan;
pub(crate) mod pagination;
pub(crate) use pagination::LINE_PAGE_BYTES;
mod pattern_repair;
pub(crate) mod prepare;
pub(crate) use prepare::extraction_error;
pub(crate) mod selection;
pub(crate) mod types;
mod view;
pub use types::*;
pub use view::process_fetched_content;
pub(crate) use view::{
    fitted_lines, past_end, process_fetched_content_selected, stale_snapshot, unsupported_selection,
};
