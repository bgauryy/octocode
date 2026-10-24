//! The one view engine over fetched source text: the localFetch query and
//! result types, extraction, outline, pagination, selection and read limits.
//! It performs no filesystem access, so `tools/local_fetch` (local files) and
//! `tools/gh_get_file_content` (remote files) render through the same code,
//! and `tools/local_search` sizes the reads it leads to by the same limits.
mod block;
pub(crate) use block::BLOCK_MAX_LINES;
pub(crate) mod extraction;
pub(crate) use extraction::match_spans;
mod key_outline;
pub(crate) mod no_match;
pub(crate) mod outline;
mod page_scan;
pub(crate) mod pagination;
pub(crate) mod prepare;
pub(crate) mod selection;
mod types;
mod view;
#[cfg(test)]
pub use types::LineRange;
#[cfg(test)]
pub(crate) use types::test_local_fetch_query;
pub use types::{
    ByteOrigin, ExtractionError, FetchCall, LocalFetchResult, NextCalls, NotAFile, Pagination,
    PartialReason, PathAccess, PathFailure, ReadSource, ReadStatus, SourceFacts, ValidatedRead,
};
pub(crate) use types::{local_fetch_query, wire_count};
pub use view::process_fetched_content;
pub(crate) use view::{
    past_end, process_fetched_content_selected, stale_snapshot, unsupported_selection,
};
