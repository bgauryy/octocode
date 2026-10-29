//! Bounded, deterministic questions over a published snapshot.
//!
//! Every list operation pages with `limit`/`offset`, reports `total`, and on
//! a cut emits `truncated` plus a paste-ready `next` command; paging is
//! success, so the exit stays 0.
use super::format::{
    Confidence, EdgeKind, EdgeRec, FLAG_EXPORTED, GraphTables, NONE, NodeKind, ProvenanceRec,
    decode_with_cancel,
};
use super::{
    GRAPH_FILE, GraphOutput, LATEST_FILE, MANIFEST_FILE, graph_home, list_snapshots,
    read_graph_file,
};
use crate::policy::path::PathPolicy;
use crate::response::exit::ExitClass;
use crate::tools::ast_graph::types::DiagnosticCode;
use octocode_contract_types::ErrorCode;
use octocode_io::shell::quote;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

mod answers;
mod assess;
mod coverage;
mod evidence;
mod find;
mod flags;
mod health;
mod load;
mod lookup;
mod paging;
mod references;
mod sources;
mod traverse;
mod walks;
mod window;
use answers::*;
use find::*;
use flags::*;
use health::*;
use load::*;
use lookup::*;
use paging::*;
use walks::*;
use window::*;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 1000;
const MAX_DEPTH: u32 = 20;
/// Impact beyond three hops is mostly noise for a change review (GitNexus
/// d1/d2/d3 buckets); `--depth` widens it.
const DEFAULT_IMPACT_DEPTH: u32 = 3;
/// `issues` hides findings below this tier unless `--min-tier` says otherwise.
const DEFAULT_MIN_TIER: u8 = 90;
const MAX_CANDIDATES: usize = 20;
/// Ranked `find` rows an unknown reference suggests.
const MAX_SUGGESTIONS: usize = 5;
const MAX_CYCLE_MEMBERS: usize = 50;

/// Declares every operation once: its name (`OPS`, the CLI's choices) and the
/// variant `run` dispatches on, so a listed operation cannot lack a handler.
macro_rules! operations {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        pub const OPS: &[&str] = &[$($name),+];

        #[derive(Clone, Copy)]
        enum Op {
            $($variant),+
        }

        impl Op {
            fn parse(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

operations! {
    Stats => "stats",
    Find => "find",
    Candidates => "candidates",
    Node => "node",
    Symbols => "symbols",
    Deps => "deps",
    Dependents => "dependents",
    Callers => "callers",
    Callees => "callees",
    Path => "path",
    Walk => "walk",
    Cycles => "cycles",
    Hubs => "hubs",
    Diagnostics => "diagnostics",
    Witnesses => "witnesses",
    Unresolved => "unresolved",
    Stale => "stale",
    Issues => "issues",
    Impact => "impact",
    Drift => "drift",
}

#[derive(Clone, Debug, Default)]
pub struct QueryOptions {
    pub op: String,
    pub include_provenance: Option<bool>,
    pub check_freshness: bool,
    /// Node reference: key, absolute path, name, or `Class.method`.
    pub target: Option<String>,
    /// Second reference for `path`.
    pub to: Option<String>,
    /// Snapshot directory, `graph.bin` path, id, scope, or a substring of
    /// exactly one id.
    pub graph: Option<String>,
    pub workspace: Option<PathBuf>,
    /// Edge kinds to follow (`contains`, `imports`, `uses`, `calls`, `inherits`).
    pub edges: Vec<String>,
    /// Node kind filter (`file`, `symbol`, `package`).
    pub kind: Option<String>,
    /// `out`, `in`, or `both`.
    pub direction: Option<String>,
    pub depth: Option<u32>,
    /// Weakest call-edge confidence to follow (`high`, `medium`, `low`).
    pub confidence: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    /// Evidence list in `summary` or `baseline` to return as paged results.
    pub list: Option<String>,
    /// `issues`: detectors to run (default all).
    pub detectors: Vec<String>,
    /// `issues`: snapshot to diff findings against (new / existing / resolved).
    pub baseline: Option<String>,
    /// `issues`: drop findings scoring below this.
    pub min_score: Option<f64>,
    /// `impact`: additional changed references (files or symbols).
    pub changed: Vec<String>,
    /// `impact`: git revision; files changed since it (plus untracked) seed
    /// the blast radius.
    pub since: Option<String>,
    /// `issues`: lowest finding tier to show (100/90/60; default 90).
    pub min_tier: Option<u8>,
    /// `issues`: entrypoints (node references) in place of the inferred ones.
    pub entries: Vec<String>,
}

#[derive(Default)]
struct Graph {
    compact_provenance: bool,
    explicit_path: Option<PathBuf>,
    id: String,
    manifest: Value,
    t: GraphTables,
    selective: Option<std::cell::RefCell<super::format::SnapshotReader>>,
    validation: Option<Value>,
    source_budget: std::cell::RefCell<crate::response::source_evidence::ProjectionBudget>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Out,
    In,
    Both,
}

type Failure = Box<GraphOutput>;

fn fail(code: ErrorCode, message: impl Into<String>) -> Failure {
    Box::new(GraphOutput::error(code, message))
}

/// Runs one query against a published snapshot.
pub fn query(options: &QueryOptions, paths: &PathPolicy) -> GraphOutput {
    query_cancellable(options, paths, &crate::execution::cancel::NeverCancel)
}

pub(super) fn query_cancellable(
    options: &QueryOptions,
    paths: &PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> GraphOutput {
    match run(options, paths, cancel) {
        Ok(output) | Err(output) => output,
    }
}

fn run(
    options: &QueryOptions,
    paths: &PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<GraphOutput, GraphOutput> {
    run_with(options, paths, cancel, true)
}

fn run_with(
    options: &QueryOptions,
    paths: &PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
    selective: bool,
) -> Result<GraphOutput, GraphOutput> {
    cancel
        .check()
        .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
    let Some(op) = Op::parse(&options.op) else {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            format!(
                "unknown op {:?}; expected one of: {}",
                options.op,
                OPS.join(", ")
            ),
        ));
    };
    let flags = Flags::parse(options)?;
    let mut graph = load_with(options, paths, cancel, selective).map_err(|failure| *failure)?;
    graph.compact_provenance = options.include_provenance == Some(false);
    let mut pinned = options.clone();
    if let Some(path) = &graph.explicit_path {
        pinned.graph = Some(path.to_string_lossy().into_owned());
    }
    let options = &pinned;
    cancel
        .check()
        .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
    let query = Query {
        options,
        g: &graph,
        flags,
        cancel,
    };
    let g = &graph;
    let freshness = options.check_freshness.then(|| {
        let root = Path::new(graph.manifest["root"].as_str().unwrap_or_default());
        let checked = super::snapshot_changes(paths, root, &graph.t, &graph.manifest, true);
        let analysis_current = super::analysis_current(&graph.manifest);
        json!({"fresh": checked.changes.is_empty() && checked.added_checked && analysis_current,
            "addedChecked": checked.added_checked, "analysisCurrent": analysis_current})
    });
    let result = (|| match op {
        Op::Stats => stats(g, cancel),
        Op::Candidates => query.candidates(),
        Op::Witnesses | Op::Unresolved => query.evidence(),
        Op::Find => {
            let text = options.target.as_deref().ok_or_else(|| {
                GraphOutput::error(ErrorCode::InvalidInput, "`find` needs search text")
            })?;
            Ok(run_page(
                options,
                g,
                Map::new(),
                find(g, text, query.flags.kind, query.window(), cancel)?,
            ))
        }
        Op::Node => node(g, query.target("a node reference")?, options, cancel),
        Op::Symbols => {
            let root = query.target("a file or symbol")?;
            let (extra, page) = symbols(g, root, query.window(), cancel)?;
            Ok(run_page(options, g, extra, page))
        }
        Op::Deps | Op::Dependents | Op::Callers | Op::Callees | Op::Walk => query.traversal(),
        Op::Path => query.path(),
        Op::Cycles => {
            let filter = query.filter(&[EdgeKind::Imports]).production();
            let page = cycles(g, &filter, options)?;
            let mut extra = edges_extra(&filter);
            if options.list.as_deref() == Some("cycleNodes") {
                extra.insert("list".into(), json!("cycleNodes"));
                extra.insert("cycle".into(), json!(options.target));
            }
            Ok(run_page(options, g, extra, page))
        }
        Op::Hubs => query.hubs(),
        Op::Diagnostics => Ok(run_page(
            options,
            g,
            Map::new(),
            diagnostics(g, options.target.as_deref()),
        )),
        Op::Impact => query.impact(),
        Op::Issues => query.issues(paths),
        Op::Drift => query.drift(paths),
        Op::Stale => {
            let (extra, page) = stale(g, paths);
            let mut out = run_page(options, g, extra, page);
            if out.value["fresh"] == true {
                out.class = ExitClass::Success;
            }
            Ok(out)
        }
    })();
    let (mut output, failed) = match result {
        Ok(output) => (output, false),
        Err(output) => (output, true),
    };
    graph.close_sources(&mut output, cancel)?;
    if let Some(reader) = &graph.selective {
        output.value["validation"] = reader.borrow().validation();
    } else if let Some(validation) = &graph.validation {
        output.value["validation"] = validation.clone();
    }
    if failed {
        return Err(output);
    }
    if options.edges.iter().any(|edge| edge == "passes") {
        output.value["argumentCoverage"] = graph.manifest["argumentCoverage"].clone();
    }
    if let Some(freshness) = freshness {
        output.value["freshness"] = freshness;
    }
    cancel
        .check()
        .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
    Ok(output)
}

/// `extra.edges`: the edge kinds a page followed.
fn edges_extra(filter: &EdgeFilter) -> Map<String, Value> {
    let mut extra = Map::new();
    extra.insert(
        "edges".into(),
        json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
    );
    if filter.kinds.contains(&EdgeKind::Calls) {
        extra.insert("confidence".into(), json!(filter.weakest.as_str()));
    }
    if !filter.tests {
        extra.insert("testEdges".into(), json!("excluded"));
    }
    extra
}

/// One query against one loaded graph.
struct Query<'q> {
    options: &'q QueryOptions,
    g: &'q Graph,
    flags: Flags,
    cancel: &'q dyn crate::execution::cancel::CancellationCheck,
}

#[cfg(test)]
mod evidence_list_tests;
#[cfg(test)]
mod traversal_limits_tests;

#[cfg(test)]
mod window_tests;

#[cfg(test)]
mod selective_evidence_tests;
#[cfg(test)]
mod selective_tests;
