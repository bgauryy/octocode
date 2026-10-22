//! The code graph: [`graph`] links one scan's syntax facts into a
//! [`types::BuiltGraph`]; [`store`] projects it into a persisted snapshot
//! (`octocode graph ingest`) and answers questions from it (`octocode graph
//! query`).
mod algorithms;
mod aliases;
mod cargo;
mod edges;
mod freshness;
mod graph;
mod js_paths;
mod linking;
mod packages;
pub mod store;
mod types;

#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod test_support;
