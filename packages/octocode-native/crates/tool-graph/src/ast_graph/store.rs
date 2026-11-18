//! Persisted code graphs: `graphBuild` builds a snapshot once; `graphQuery`
//! answers bounded questions from it without rebuilding.
//!
//! ```text
//! <workspace>/.octocode/graph/  (or <local.graphHome>/octocode-graph when set)
//!   latest                                   # id of the newest snapshot
//!   20260928T101500Z-packages-app-1a2b3c/     # time, scope, root tag
//!     manifest.json                          # scope, counts, digests, gaps
//!     graph.bin                              # see `format.rs`
//! ```
//!
//! Snapshots are immutable. A build is written into a hidden temp directory
//! and renamed into place, so a reader never observes a partial snapshot.
mod argument_coverage;
mod build_lock;
mod detect;
mod drift;
mod format;
mod impact;
mod ingest;
mod manifest;
pub mod query;
pub mod snapshots;
mod staleness;
pub(super) mod tables;
pub mod tools;
mod workspace;
pub(super) use format::sources::PackedOrigins;

use super::graph::GraphScan;
use super::types::ImportOutcome;
use octocode_contract_types::{ErrorCode, GraphBuildQuery};
use octocode_io::atomic::write_atomic;
use octocode_runtime_base::response::exit::ExitClass;
use octocode_runtime_base::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};

pub use ingest::ingest;
use manifest::*;
#[cfg(test)]
pub(crate) use query::query;
pub use query::{QueryOptions, SavedRows};
use snapshots::*;
pub use snapshots::{configured_graph_home, default_workspace, graph_home};
use staleness::*;

pub(crate) const GRAPH_FILE: &str = "graph.bin";

/// A published snapshot past the byte bound needs a narrower ingest scope;
/// it is refused before decode can multiply allocations from untrusted
/// lengths.
#[cfg(test)]
fn read_graph_file(path: &Path) -> Result<Vec<u8>, String> {
    let limit = super::limits::LIMITS.snapshot_bytes;
    octocode_tools_shared::source::read_bounded(path, limit).map_err(|error| {
        format!(
            "cannot read {} within the {limit}-byte snapshot limit: {error:?}; build a narrower path",
            path.display(),
        )
    })
}
pub(crate) const MANIFEST_FILE: &str = "manifest.json";
const LATEST_FILE: &str = "latest";
const MANIFEST_KIND: &str = "octocode.graph";
/// A command result: the JSON printed to stdout and how the command ended.
#[derive(Debug)]
pub struct GraphOutput {
    pub value: Value,
    pub class: ExitClass,
    /// The calls that continue this answer, by `next` channel; the tool
    /// adapter states each as an executable call.
    pub(crate) next: Vec<(Channel, Resume)>,
    /// A build failed only because the tree changed while it ran: the same
    /// request succeeds once the tree settles, so the adapter leads to it.
    pub(crate) retry: bool,
    /// What the query read and verified: test evidence of how little it
    /// decoded, never in a response. Boxed so the test-only payload does not
    /// inflate every `Result<_, GraphOutput>` in test builds.
    #[cfg(test)]
    pub(crate) validation: Box<Value>,
}

impl GraphOutput {
    pub(crate) fn ok(value: Value) -> Self {
        Self {
            value,
            class: ExitClass::Success,
            next: Vec::new(),
            retry: false,
            #[cfg(test)]
            validation: Box::default(),
        }
    }
    pub(crate) fn empty(value: Value) -> Self {
        Self {
            value,
            class: ExitClass::Empty,
            next: Vec::new(),
            retry: false,
            #[cfg(test)]
            validation: Box::default(),
        }
    }
    /// A build refused because a source or config it read changed while it
    /// ran; the same request, once the tree stops changing, builds.
    pub(crate) fn changed_during_build(message: impl Into<String>) -> Self {
        Self {
            retry: true,
            ..Self::error(ErrorCode::ExecutionFailed, message)
        }
    }
    /// A failure with a declared `errorCodes` entry; its class decides the exit.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            value: json!({"error": message.into(), "errorCode": code}),
            class: ExitClass::of(code),
            next: Vec::new(),
            retry: false,
            #[cfg(test)]
            validation: Box::default(),
        }
    }
}

/// A `next` channel of an answer (core `continuationChannels`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Channel {
    /// The answer's rows after this page.
    NextPage,
    /// The members of the cycle on row `i` of this page, past its preview.
    Cycle(usize),
}

impl Channel {
    pub(crate) fn name(&self) -> String {
        match self {
            Self::NextPage => "nextPage".into(),
            Self::Cycle(index) => format!("cycle{index}"),
        }
    }
}

/// Where a continuation resumes.
#[derive(Clone, Debug)]
pub(crate) enum Resume {
    /// The same request from row `offset`: of evidence `list` when set, of
    /// the cycle that `cycle` leads when set.
    Row {
        offset: usize,
        list: Option<String>,
        cycle: Option<String>,
    },
    /// Another request from its own `offset`: the candidates of an
    /// ambiguous reference.
    Query(Box<QueryOptions>),
}

#[derive(Clone, Debug)]
pub struct IngestOptions {
    /// Keep extraction intermediates in memory; the requested snapshot is still published.
    pub memory_only: bool,
    /// Directory to scan.
    pub path: PathBuf,
    /// Workspace that owns `.octocode/graph`. The runtime fills it when
    /// unset ([`default_workspace`] of its working directory).
    pub workspace: Option<PathBuf>,
    /// `local.graphHome`: the snapshot home in place of the workspace's
    /// ([`snapshots::graph_home`]).
    pub home: Option<PathBuf>,
    pub exclude: Vec<String>,
    /// The default prune, ignore files and dot entries the walk keeps or lifts.
    pub walk: super::walk_scope::WalkScope,
    /// Snapshots of the same root to retain, newest first.
    pub keep: usize,
    /// Rebuild even when the latest snapshot of this scope is current.
    pub refresh: bool,
    /// AST parsers by root-relative glob (`{"cpp": ["include/**/*.h"]}`).
    pub language_globs: BTreeMap<String, Vec<String>>,
}

impl IngestOptions {
    /// The options a validated graphBuild `query` names for scanning `path`;
    /// its omitted fields carry the contract's defaults.
    pub(crate) fn of_query(query: &GraphBuildQuery, path: PathBuf) -> Self {
        Self {
            memory_only: false,
            path,
            workspace: None,
            home: None,
            exclude: query.exclude.iter().map(ToString::to_string).collect(),
            walk: super::walk_scope::WalkScope::of_query(query),
            keep: octocode_engine::num::saturating_usize(query.keep.get()),
            refresh: query.refresh,
            language_globs: query
                .parser_globs
                .as_ref()
                .map(octocode_tools_shared::ast_rule::parser_globs)
                .unwrap_or_default(),
        }
    }
}

/// A graphBuild of the contract's default query.
impl Default for IngestOptions {
    fn default() -> Self {
        Self::of_query(&GraphBuildQuery::default(), PathBuf::new())
    }
}

#[cfg(test)]
mod tests;
