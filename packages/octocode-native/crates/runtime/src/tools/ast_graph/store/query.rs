//! Bounded, deterministic questions over a published snapshot.
//!
//! Every list operation pages with `limit`/`offset`, states its page as
//! `pagination`, and on a cut carries a typed `next` continuation that the
//! tool adapter makes executable; paging is success, so the exit stays 0.
use super::format::{
    Confidence, EdgeKind, EdgeRec, FLAG_EXPORTED, GraphTables, NONE, NodeKind, ProvenanceRec,
    decode_with_cancel,
};
use super::{
    Channel, GRAPH_FILE, GraphOutput, LATEST_FILE, Resume, MANIFEST_FILE, graph_home, list_snapshots,
    read_graph_file,
};
use crate::policy::path::PathPolicy;
use crate::response::exit::ExitClass;
use crate::tools::ast_graph::types::DiagnosticCode;
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

mod answers;
mod assess;
mod budget;
mod coverage;
mod evidence;
mod find;
mod flags;
mod health;
mod lean;
mod load;
mod lookup;
mod opened;
mod options;
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
pub use options::QueryOptions;
use paging::*;
pub(crate) use paging::query_row;
use walks::*;
use window::*;

/// The page and depth bounds of a graph query are the `graphQuery` tool
/// contract's, so the CLI and MCP surfaces page the same way.
#[expect(
    clippy::panic,
    reason = "a missing default is a contract defect caught by contract tests"
)]
fn default_limit() -> usize {
    crate::contracts::query_schema_number(crate::id::ToolId::GraphQuery, None, "limit", "default")
        .and_then(|limit| usize::try_from(limit).ok())
        .unwrap_or_else(|| panic!("contract declares no default for graphQuery limit"))
}
/// Impact beyond three hops is mostly noise for a change review (GitNexus
/// d1/d2/d3 buckets); `depth` widens it.
const DEFAULT_IMPACT_DEPTH: u32 = 3;
/// `issues` hides findings below this tier unless `minTier` says otherwise.
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

#[derive(Default)]
struct Graph {
    compact_provenance: bool,
    /// Characters a page's rows may take ([`budget::page_budget`]).
    page_budget: Option<usize>,
    /// The process-cache key of a reusable snapshot ([`opened`]).
    reuse: Option<opened::Key>,
    explicit_path: Option<PathBuf>,
    id: String,
    manifest: Value,
    t: GraphTables,
    selective: Option<std::cell::RefCell<super::format::SnapshotReader>>,
    /// The reads and integrity checks behind a query: test evidence of how
    /// little it decoded, never in a response.
    #[cfg(test)]
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
    graph.compact_provenance = !options.include_provenance;
    graph.page_budget = budget::page_budget(options.response_window);
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
    // `stale` is the freshness check itself: its answer states it once.
    let freshness = (options.check_freshness && !matches!(op, Op::Stale)).then(|| {
        let root = Path::new(graph.manifest["root"].as_str().unwrap_or_default());
        let checked = super::snapshot_changes(paths, root, &graph.t, &graph.manifest, true);
        let analysis_current = super::analysis_current(&graph.manifest);
        json!({"fresh": checked.changes.is_empty() && analysis_current,
            "analysisCurrent": analysis_current})
    });
    let result = (|| match op {
        Op::Stats => stats(g, cancel),
        Op::Candidates => query.candidates(),
        Op::Witnesses | Op::Unresolved => query.evidence(),
        Op::Find => {
            // No text: every node (of the kind), ranked like any match.
            let text = options.target.as_deref().unwrap_or_default();
            Ok(run_page(
                options,
                g,
                Map::new(),
                find(g, text, query.flags.kind, query.window(), cancel)?,
            ))
        }
        Op::Node => node(g, query.target("a node reference")?, cancel),
        Op::Symbols => {
            let roots = query.symbol_roots()?;
            let (extra, page) = symbols(g, &roots, query.window(), cancel)?;
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
    if graph.compact_provenance {
        lean::project(&mut output.value);
    }
    graph.close_sources(&mut output, cancel)?;
    #[cfg(test)]
    if let Some(reader) = &graph.selective {
        output.validation = reader.borrow().validation();
    } else if let Some(validation) = &graph.validation {
        output.validation = validation.clone();
    }
    if failed {
        return Err(output);
    }
    if options.edges.iter().any(|edge| edge == "passes") && first_page(options) {
        output.value["argumentCoverage"] = graph.manifest["argumentCoverage"].clone();
    }
    if let Some(freshness) = freshness {
        output.value["freshness"] = freshness;
    }
    graph.fit_page(&mut output, options, cancel)?;
    #[cfg(test)]
    if let Some(reader) = &graph.selective {
        output.validation = reader.borrow().validation();
    }
    cancel
        .check()
        .map_err(|message| GraphOutput::error(ErrorCode::Cancelled, message))?;
    // Only a completed query returns its snapshot for reuse.
    graph.release();
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
