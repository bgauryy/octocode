//! Bounded, deterministic questions over a published snapshot.
//!
//! Every list operation pages with `limit`/`offset`, reports `total`, and on
//! a cut emits `truncated` plus a paste-ready `next` command; paging is
//! success, so the exit stays 0.
use super::format::{
    Confidence, EdgeKind, EdgeRec, FLAG_EXPORTED, GraphTables, NONE, NodeKind, decode,
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
mod flags;
mod health;
mod load;
mod lookup;
mod paging;
mod traverse;
mod walks;
use answers::*;
use flags::*;
use health::*;
use load::*;
use lookup::*;
use paging::*;
use walks::*;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 1000;
const MAX_DEPTH: u32 = 20;
/// Impact beyond three hops is mostly noise for a change review (GitNexus
/// d1/d2/d3 buckets); `--depth` widens it.
const DEFAULT_IMPACT_DEPTH: u32 = 3;
/// `issues` hides findings below this tier unless `--min-tier` says otherwise.
const DEFAULT_MIN_TIER: u8 = 90;
const MAX_VISITED: usize = 200_000;
const MAX_CANDIDATES: usize = 20;
const MAX_CYCLE_MEMBERS: usize = 50;

pub const OPS: &[&str] = &[
    "stats",
    "find",
    "node",
    "symbols",
    "deps",
    "dependents",
    "callers",
    "callees",
    "path",
    "walk",
    "cycles",
    "hubs",
    "diagnostics",
    "stale",
    "issues",
    "impact",
    "drift",
];

#[derive(Clone, Debug, Default)]
pub struct QueryOptions {
    pub op: String,
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

struct Graph {
    id: String,
    manifest: Value,
    t: GraphTables,
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
    match run(options, paths) {
        Ok(output) | Err(output) => output,
    }
}

fn run(options: &QueryOptions, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
    if !OPS.contains(&options.op.as_str()) {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            format!(
                "unknown op {:?}; expected one of: {}",
                options.op,
                OPS.join(", ")
            ),
        ));
    }
    let flags = Flags::parse(options)?;
    let graph = load(options, paths).map_err(|failure| *failure)?;
    let query = Query {
        options,
        g: &graph,
        flags,
    };
    let g = &graph;
    let freshness = options.check_freshness.then(|| {
        let root = Path::new(graph.manifest["root"].as_str().unwrap_or_default());
        let checked = super::snapshot_changes(paths, root, &graph.t, &graph.manifest, true);
        json!({"fresh": checked.changes.is_empty() && checked.added_checked, "addedChecked": checked.added_checked})
    });
    let mut output = match options.op.as_str() {
        "stats" => Ok(stats(g)),
        "find" => {
            let text = options.target.as_deref().ok_or_else(|| {
                GraphOutput::error(ErrorCode::InvalidInput, "`find` needs search text")
            })?;
            Ok(run_page(
                options,
                g,
                Map::new(),
                find(g, text, query.flags.kind),
            ))
        }
        "node" => Ok(node(g, query.target("a node reference")?, options)),
        "symbols" => {
            let root = query.target("a file or symbol")?;
            let (extra, page) = symbols(g, root);
            Ok(run_page(options, g, extra, page))
        }
        "deps" | "dependents" | "callers" | "callees" | "walk" => query.traversal(),
        "path" => query.path(),
        "cycles" => {
            let filter = query.filter(&[EdgeKind::Imports]).production();
            let page = cycles(g, &filter, options)?;
            let mut extra = edges_extra(&filter);
            if options.list.as_deref() == Some("cycleNodes") {
                extra.insert("list".into(), json!("cycleNodes"));
                extra.insert("cycle".into(), json!(options.target));
            }
            Ok(run_page(options, g, extra, page))
        }
        "hubs" => query.hubs(),
        "diagnostics" => Ok(run_page(
            options,
            g,
            Map::new(),
            diagnostics(g, options.target.as_deref()),
        )),
        "impact" => query.impact(),
        "issues" => query.issues(paths),
        "drift" => query.drift(paths),
        "stale" => {
            let (extra, page) = stale(g, paths);
            let mut out = run_page(options, g, extra, page);
            if out.value["fresh"] == true {
                out.class = ExitClass::Success;
            }
            Ok(out)
        }
        _ => Err(GraphOutput::error(ErrorCode::InvalidInput, "unknown op")),
    }?;
    if let Some(freshness) = freshness {
        output.value["freshness"] = freshness;
    }
    Ok(output)
}

/// `extra.edges`: the edge kinds a page followed.
fn edges_extra(filter: &EdgeFilter) -> Map<String, Value> {
    let mut extra = Map::new();
    extra.insert(
        "edges".into(),
        json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
    );
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
}

#[cfg(test)]
mod evidence_list_tests;
