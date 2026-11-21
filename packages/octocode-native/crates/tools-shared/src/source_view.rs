//! The one view engine over fetched source text: the localFetch query and
//! result types, extraction, outline, pagination, selection and read limits.
//! `tools/local_fetch` acquires local files and `tools/gh_get_file_content`
//! acquires remote files; both render through this engine,
//! and `tools/local_search` sizes the reads it leads to by the same limits.
mod block;
pub mod extraction;
pub use extraction::match_spans;
mod key_outline;
pub mod local_query;
pub(crate) mod no_match;
pub use no_match::masked_unsearched;
pub(crate) mod outline;
mod page_scan;
pub mod pagination;
pub mod prepare;
pub mod retained;
pub mod types;
pub mod view;
#[cfg(any(test, feature = "test-support"))]
pub use types::LineRange;
#[cfg(any(test, feature = "test-support"))]
pub use types::test_local_fetch_query;
pub use types::{
    ByteOrigin, ExtractionError, FetchCall, LocalFetchResult, NextCalls, NotAFile, Pagination,
    PartialReason, PathAccess, PathFailure, ReadSource, ReadStatus, SourceFacts, ValidatedRead,
};
pub use types::{local_fetch_query, wire_count};
pub use view::process_fetched_content;
pub use view::{
    past_end, process_fetched_content_as, refuse_binary, refuse_binary_with_head, stale_snapshot,
};
