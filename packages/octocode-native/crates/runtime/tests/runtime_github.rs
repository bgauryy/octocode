//! The GitHub file, code, tree and repository tools through the runtime:
//! reads, recoveries, rate limits and bulk dispatch against a mock GitHub.

mod batches;
mod code_page_reads;
mod failures;
mod file_reads;
mod materialize;
mod missing_targets;
mod owner_pages;
mod refs;
mod renamed_paths;
mod search_pages;
