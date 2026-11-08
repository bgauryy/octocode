//! The code graph: [`graph`] links one scan's syntax facts into a
//! [`types::BuiltGraph`]; [`store`] projects it into a persisted snapshot
//! (`graphBuild`) and answers questions from it (`graphQuery`).
mod algorithms;
mod aliases;
mod build_scope;
mod cargo;
mod classify;
mod edges;
mod freshness;
mod graph;
mod js_paths;
mod languages;
mod limits;
mod linking;
mod packages;
mod paths;
mod progress;
pub mod store;
mod types;
mod walk_scope;

/// Bump when persisted linking semantics change without a binary format
/// change: snapshots and extraction checkpoints of another version rebuild.
///
/// 26: a data member is shadowed only by a callable of its own type; a
/// function-typed member is called by its bare name inside its type (Scala,
/// C#, C++); declared-type chains guard cycles per path; a workspace
/// member reaches itself through its `node_modules` link.
const ANALYSIS_VERSION: u32 = 26;

#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod test_support;
