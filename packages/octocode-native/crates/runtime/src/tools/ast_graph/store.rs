//! Persisted code graphs: `octocode graph ingest` builds a snapshot once;
//! `octocode graph query` answers bounded questions from it without
//! rebuilding.
//!
//! ```text
//! <workspace>/.octocode/graph/
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
mod query;
mod snapshots;
mod staleness;
pub(super) mod tables;
pub(crate) mod tools;
mod workspace;

use super::graph::GraphScan;
use super::types::ImportOutcome;
use crate::response::exit::ExitClass;
use crate::{
    execution::cancel::CancellationCheck, policy::path::PathPolicy, security::ContentSecurity,
};
use octocode_contract_types::ErrorCode;
use octocode_io::atomic::write_atomic;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};

pub use ingest::ingest;
use manifest::*;
pub use query::{OPS, QueryOptions, query};
pub(crate) use snapshots::default_workspace;
use snapshots::*;
use staleness::*;

pub(crate) const GRAPH_FILE: &str = "graph.bin";

/// A published snapshot past the byte bound needs a narrower ingest scope;
/// it is refused before decode can multiply allocations from untrusted
/// lengths.
fn read_graph_file(path: &Path) -> Result<Vec<u8>, String> {
    let limit = super::limits::LIMITS.snapshot_bytes;
    crate::tools::source::read_bounded(path, limit).map_err(|error| {
        format!(
            "cannot read {} within the {limit}-byte snapshot limit: {error:?}; narrow the graph ingest scope",
            path.display(),
        )
    })
}
pub(crate) const MANIFEST_FILE: &str = "manifest.json";
const LATEST_FILE: &str = "latest";
const MANIFEST_KIND: &str = "octocode.graph";
const DEFAULT_KEEP: usize = 3;
/// Environment and tool-output directories the ingest prunes on top of the
/// shared syntax-visible policy and `.gitignore` (hidden directories are never
/// scanned).
const INGEST_EXCLUDES: &[&str] = &[
    "venv",
    "site-packages",
    "Pods",
    "storybook-static",
    "bower_components",
    "jspm_packages",
    "obj",
];

/// A command result: the JSON printed to stdout and how the command ended.
#[derive(Debug)]
pub struct GraphOutput {
    pub value: Value,
    pub class: ExitClass,
    /// Domain continuation intent; adapters never infer it from shell text.
    pub(crate) next_query: Option<Box<QueryOptions>>,
}

impl GraphOutput {
    pub(crate) fn ok(value: Value) -> Self {
        Self {
            value,
            class: ExitClass::Success,
            next_query: None,
        }
    }
    pub(crate) fn empty(value: Value) -> Self {
        Self {
            value,
            class: ExitClass::Empty,
            next_query: None,
        }
    }
    /// A failure with a declared `errorCodes` entry; its class decides the exit.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            value: json!({"error": message.into(), "errorCode": code}),
            class: ExitClass::of(code),
            next_query: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IngestOptions {
    /// Directory to scan.
    pub path: PathBuf,
    /// Workspace that owns `.octocode/graph`. The runtime fills it when
    /// unset ([`default_workspace`] of its working directory).
    pub workspace: Option<PathBuf>,
    pub exclude_dir: Vec<String>,
    pub max_files: Option<u32>,
    /// Snapshots of the same root to retain, newest first (default 3).
    pub keep: Option<usize>,
    /// Rebuild even when the latest snapshot of this scope is current.
    pub force: bool,
    /// AST parsers by root-relative glob (`{"cpp": ["include/**/*.h"]}`).
    pub language_globs: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
mod tests;
