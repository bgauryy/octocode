//! The GitHub file, code, tree and repository tools through the runtime:
//! reads, recoveries, rate limits and bulk dispatch against a mock GitHub.

mod batches;
mod failures;
mod file_reads;
mod materialize;
mod missing_targets;
mod refs;
