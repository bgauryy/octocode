//! The code graph: [`graph`] links one scan's syntax facts into a
//! [`types::BuiltGraph`]; [`store`] projects it into a persisted snapshot
//! (`octocode graph ingest`) and answers questions from it (`octocode graph
//! query`).
mod algorithms;
mod aliases;
mod cargo;
mod classify;
mod edges;
mod freshness;
mod graph;
mod js_paths;
mod limits;
mod linking;
mod packages;
pub mod store;
mod types;

/// Bump when persisted linking semantics change without a binary format
/// change: snapshots and extraction checkpoints of another version rebuild.
const ANALYSIS_VERSION: u32 = 16;

#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod test_support;
