//! Bounded, deterministic questions over a published snapshot.
//!
//! Every list operation pages with `limit`/`offset`, reports `total`, and on
//! a cut emits `truncated` plus a paste-ready `next` command (exit 6).
use super::format::{
    Confidence, EdgeKind, EdgeRec, FLAG_EXPORTED, GraphTables, NONE, NodeKind, decode,
};
use super::{GRAPH_FILE, GraphOutput, LATEST_FILE, MANIFEST_FILE, graph_home, list_snapshots};
use crate::policy::path::PathPolicy;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

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
];

#[derive(Clone, Debug, Default)]
pub struct QueryOptions {
    pub op: String,
    /// Node reference: key, absolute path, name, or `Class.method`.
    pub target: Option<String>,
    /// Second reference for `path`.
    pub to: Option<String>,
    /// Snapshot directory, `graph.bin` path, id, or id substring.
    pub graph: Option<String>,
    pub workspace: Option<PathBuf>,
    /// Edge kinds to follow (`contains`, `imports`, `calls`).
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
}

struct Graph {
    id: String,
    dir: PathBuf,
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

fn fail(exit: u8, code: &str, message: impl Into<String>) -> Failure {
    Box::new(GraphOutput::error(exit, code, message))
}

fn locate(options: &QueryOptions) -> Result<PathBuf, Failure> {
    if let Some(spec) = options.graph.as_deref() {
        let path = PathBuf::from(spec);
        if path.is_file() {
            return Ok(path.parent().map(Path::to_path_buf).unwrap_or_default());
        }
        if path.join(GRAPH_FILE).is_file() {
            return Ok(path);
        }
    }
    let home = graph_home(options.workspace.as_deref())
        .map_err(|message| fail(5, "graph.workspace", message))?;
    let ids = list_snapshots(&home);
    let chosen = match options.graph.as_deref() {
        Some(spec) => ids
            .iter()
            .rev()
            .find(|id| id.as_str() == spec)
            .or_else(|| ids.iter().rev().find(|id| id.contains(spec)))
            .cloned(),
        None => std::fs::read_to_string(home.join(LATEST_FILE))
            .ok()
            .map(|id| id.trim().to_owned())
            .filter(|id| ids.contains(id))
            .or_else(|| ids.last().cloned()),
    };
    chosen.map(|id| home.join(id)).ok_or_else(|| {
        let message = match options.graph.as_deref() {
            Some(spec) => format!(
                "no graph matches {spec:?} under {}; available: {}",
                home.display(),
                if ids.is_empty() {
                    "none".into()
                } else {
                    ids.join(", ")
                }
            ),
            None => format!(
                "no graph under {}; run `octocode graph ingest <path>` first",
                home.display()
            ),
        };
        fail(3, "graph.notFound", message)
    })
}

fn load(options: &QueryOptions, paths: &PathPolicy) -> Result<Graph, Failure> {
    let dir = locate(options)?;
    let dir = paths
        .validate(&dir)
        .map_err(|error| fail(2, "graph.pathDenied", error.message))?
        .canonical;
    let manifest = std::fs::read(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| {
            fail(
                5,
                "graph.corrupt",
                format!("{} has no readable manifest", dir.display()),
            )
        })?;
    let bytes = std::fs::read(dir.join(GRAPH_FILE)).map_err(|error| {
        fail(
            5,
            "graph.corrupt",
            format!("cannot read {GRAPH_FILE}: {error}"),
        )
    })?;
    let (t, digest) = decode(&bytes).map_err(|message| fail(5, "graph.corrupt", message))?;
    if manifest["graph"]["sha256"].as_str() != Some(digest.as_str()) {
        return Err(fail(
            5,
            "graph.corrupt",
            "graph.bin does not match its manifest digest; re-run `octocode graph ingest`",
        ));
    }
    let id = manifest["id"].as_str().unwrap_or_default().to_owned();
    Ok(Graph {
        id,
        dir,
        manifest,
        t,
    })
}

impl Graph {
    fn key(&self, id: u32) -> &str {
        self.t.str(self.t.nodes[id as usize].key)
    }

    fn node_json(&self, id: u32) -> Map<String, Value> {
        let node = &self.t.nodes[id as usize];
        let mut out = Map::new();
        out.insert("id".into(), json!(self.key(id)));
        out.insert("kind".into(), json!(node.kind.as_str()));
        let detail = self.t.str(node.detail);
        match node.kind {
            NodeKind::File => {
                if !detail.is_empty() {
                    out.insert("language".into(), json!(detail));
                }
                let roles = super::classify::role_names(node.flags);
                if !roles.is_empty() {
                    out.insert("roles".into(), json!(roles));
                }
            }
            NodeKind::Symbol => {
                out.insert("name".into(), json!(self.t.str(node.name)));
                out.insert("symbolKind".into(), json!(detail));
                out.insert("file".into(), json!(self.key(node.file)));
                out.insert("line".into(), json!(node.line));
                if node.end_line != node.line {
                    out.insert("endLine".into(), json!(node.end_line));
                }
                if node.flags & FLAG_EXPORTED != 0 {
                    out.insert("exported".into(), json!(true));
                }
                if node.flags & super::format::FLAG_TEST != 0 {
                    out.insert("test".into(), json!(true));
                }
            }
            NodeKind::Package => {
                out.insert("ecosystem".into(), json!(detail));
            }
        }
        out
    }

    fn edge_fields(&self, out: &mut Map<String, Value>, edge: &EdgeRec) {
        out.insert("edge".into(), json!(edge.kind.as_str()));
        let via = self.t.str(edge.detail);
        if !via.is_empty() && edge.kind != EdgeKind::Contains {
            out.insert("via".into(), json!(via));
        }
        if edge.line != NONE {
            out.insert("edgeLine".into(), json!(edge.line));
        }
        if edge.kind == EdgeKind::Calls {
            out.insert("confidence".into(), json!(edge.confidence.as_str()));
        }
    }

    /// How complete call edges are for `node`'s language: the share of call
    /// sites naming an in-repo declaration that the syntax linker resolved.
    fn coverage(&self, node: u32) -> Value {
        let file = self.t.nodes[node as usize].file;
        let language = if file == NONE {
            ""
        } else {
            self.t.str(self.t.nodes[file as usize].detail)
        };
        let recall = self.manifest["callRecallByLanguage"][language]
            .as_f64()
            .or_else(|| self.manifest["callInternalRecall"].as_f64())
            .unwrap_or(1.0);
        let mut coverage = json!({"language": language, "callInternalRecall": recall});
        if recall < 0.9 {
            coverage["warning"] = json!(format!(
                "Syntax-linked calls: about {:.0}% of {} call sites that name code in this repo are linked; \
                 method calls on values of unknown type are missing. Treat call results as a lower bound \
                 and confirm with lspSearch references.",
                recall * 100.0,
                if language.is_empty() { "all" } else { language }
            ));
        }
        coverage
    }

    fn summary(&self) -> Value {
        json!({"id": self.id, "root": self.manifest["root"], "createdAt": self.manifest["createdAt"]})
    }

    /// Resolves a user reference to exactly one node.
    fn resolve(&self, reference: &str, kind: Option<NodeKind>) -> Result<u32, Failure> {
        let admit = |id: &u32| kind.is_none_or(|k| self.t.nodes[*id as usize].kind == k);
        let trimmed = reference.trim().trim_start_matches("./");
        let root = self.manifest["root"].as_str().unwrap_or_default();
        // The snapshot root is canonical; canonicalize the reference too so a
        // symlinked spelling (macOS `/var` → `/private/var`) still matches.
        let absolute = std::fs::canonicalize(trimmed).unwrap_or_else(|_| PathBuf::from(trimmed));
        let relative = absolute
            .strip_prefix(root)
            .ok()
            .map(|rel| rel.to_string_lossy().replace('\\', "/"));
        for key in [Some(trimmed.to_owned()), relative].into_iter().flatten() {
            if let Some(id) = self.t.by_key(&key).filter(admit) {
                return Ok(id);
            }
        }
        let mut found = self
            .name_range(&trimmed.to_lowercase(), false)
            .filter(|id| self.t.str(self.t.nodes[*id as usize].name) == trimmed)
            .filter(admit)
            .collect::<Vec<_>>();
        if found.is_empty() && trimmed.contains(['.', '#']) {
            let suffix = format!("#{}", trimmed.trim_start_matches('#'));
            found = (0..self.t.nodes.len() as u32)
                .filter(|id| self.key(*id).ends_with(&suffix))
                .filter(admit)
                .collect();
        }
        if found.is_empty() {
            let suffix = format!("/{trimmed}");
            found = (0..self.t.nodes.len() as u32)
                .filter(|id| {
                    self.t.nodes[*id as usize].kind == NodeKind::File
                        && self.key(*id).ends_with(&suffix)
                })
                .filter(admit)
                .collect();
        }
        match found.as_slice() {
            [only] => Ok(*only),
            [] => {
                let suggestions = self
                    .name_range(&trimmed.to_lowercase(), true)
                    .filter(admit)
                    .take(5)
                    .map(|id| self.key(id).to_owned())
                    .collect::<Vec<_>>();
                let mut out = GraphOutput::error(
                    3,
                    "graph.nodeNotFound",
                    format!(
                        "no node matches {reference:?}; try `octocode graph query find <text>`"
                    ),
                );
                if !suggestions.is_empty() {
                    out.value["suggestions"] = json!(suggestions);
                }
                Err(Box::new(out))
            }
            many => {
                let mut out = GraphOutput::error(
                    2,
                    "graph.ambiguous",
                    format!(
                        "{} nodes match {reference:?}; pass one of the candidate ids (or --kind)",
                        many.len()
                    ),
                );
                out.value["candidates"] = json!(
                    many.iter()
                        .take(MAX_CANDIDATES)
                        .map(|id| Value::Object(self.node_json(*id)))
                        .collect::<Vec<_>>()
                );
                Err(Box::new(out))
            }
        }
    }

    /// Node ids whose lowercase name equals (or, with `prefix`, starts with)
    /// `lower`, via the sorted name index.
    fn name_range<'a>(&'a self, lower: &'a str, prefix: bool) -> impl Iterator<Item = u32> + 'a {
        let lowered = |id: u32| self.t.str(self.t.nodes[id as usize].name).to_lowercase();
        let start = self
            .t
            .name_index
            .partition_point(|id| lowered(*id).as_str() < lower);
        self.t.name_index[start..]
            .iter()
            .copied()
            .take_while(move |id| {
                let name = lowered(*id);
                if prefix {
                    name.starts_with(lower)
                } else {
                    name == lower
                }
            })
    }

    fn neighbors<'a>(
        &'a self,
        node: u32,
        direction: Direction,
        filter: &'a EdgeFilter,
    ) -> impl Iterator<Item = (u32, &'a EdgeRec)> + 'a {
        let out = (direction != Direction::In)
            .then(|| self.t.out(node).iter().map(|edge| (edge.dst, edge)))
            .into_iter()
            .flatten();
        let incoming = (direction != Direction::Out)
            .then(|| self.t.incoming(node).map(|edge| (edge.src, edge)))
            .into_iter()
            .flatten();
        out.chain(incoming).filter(|(_, edge)| filter.admits(edge))
    }
}

struct EdgeFilter {
    kinds: Vec<EdgeKind>,
    weakest: Confidence,
}

impl EdgeFilter {
    fn admits(&self, edge: &EdgeRec) -> bool {
        self.kinds.contains(&edge.kind)
            && (edge.kind != EdgeKind::Calls || edge.confidence <= self.weakest)
    }
}

struct Page {
    rows: Vec<Value>,
    total: usize,
}

fn run_page(
    options: &QueryOptions,
    graph: &Graph,
    extra: Map<String, Value>,
    page: Page,
) -> GraphOutput {
    let offset = options.offset.unwrap_or(0);
    let limit = options.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let total = page.total;
    let rows = page
        .rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let shown = rows.len();
    let mut out = Map::new();
    out.insert("graph".into(), graph.summary());
    out.insert("op".into(), json!(options.op));
    out.extend(extra);
    out.insert("total".into(), json!(total));
    if offset > 0 {
        out.insert("offset".into(), json!(offset));
    }
    out.insert("results".into(), Value::Array(rows));
    let mut exit = if total == 0 { 1 } else { 0 };
    if offset + shown < total {
        out.insert("truncated".into(), json!(true));
        out.insert(
            "next".into(),
            json!(next_command(options, &graph.id, offset + shown)),
        );
        exit = 6;
    }
    GraphOutput {
        value: Value::Object(out),
        exit,
    }
}

fn quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@,=+".contains(c))
    {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

fn next_command(options: &QueryOptions, id: &str, offset: usize) -> String {
    let mut parts = vec!["octocode graph query".to_owned(), quote(&options.op)];
    parts.extend(options.target.iter().map(|t| quote(t)));
    parts.extend(options.to.iter().map(|t| quote(t)));
    parts.push(format!("--graph {}", quote(id)));
    if !options.edges.is_empty() {
        parts.push(format!("--edge {}", quote(&options.edges.join(","))));
    }
    for (flag, value) in [
        ("--kind", options.kind.clone()),
        ("--direction", options.direction.clone()),
        ("--depth", options.depth.map(|d| d.to_string())),
        ("--confidence", options.confidence.clone()),
        ("--limit", options.limit.map(|l| l.to_string())),
        (
            "--detector",
            (!options.detectors.is_empty()).then(|| options.detectors.join(",")),
        ),
        ("--baseline", options.baseline.clone()),
        ("--min-score", options.min_score.map(|m| m.to_string())),
        (
            "--changed",
            (!options.changed.is_empty()).then(|| options.changed.join(",")),
        ),
        ("--since", options.since.clone()),
        ("--min-tier", options.min_tier.map(|t| t.to_string())),
    ] {
        if let Some(value) = value {
            parts.push(format!("{flag} {}", quote(&value)));
        }
    }
    parts.push(format!("--offset {offset}"));
    parts.join(" ")
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Root-relative files changed since `rev` (committed, staged, unstaged) plus
/// untracked files, via git.
fn git_changed(root: &Path, rev: &str) -> Result<Vec<String>, String> {
    if rev.starts_with('-') {
        return Err(format!("invalid revision {rev:?}"));
    }
    let run = |args: &[&str]| -> Result<Vec<String>, String> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .map_err(|error| format!("cannot run git: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect())
    };
    let mut files = run(&["diff", "--name-only", "--relative", rev, "--"])?;
    files.extend(run(&["ls-files", "--others", "--exclude-standard"])?);
    files.sort();
    files.dedup();
    Ok(files)
}

/// Runs one query against a published snapshot.
pub fn query(options: &QueryOptions, paths: &PathPolicy) -> GraphOutput {
    match run(options, paths) {
        Ok(output) | Err(output) => output,
    }
}

fn run(options: &QueryOptions, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
    let unbox = |failure: Failure| *failure;
    if !OPS.contains(&options.op.as_str()) {
        return Err(GraphOutput::error(
            2,
            "graph.input",
            format!(
                "unknown op {:?}; expected one of: {}",
                options.op,
                OPS.join(", ")
            ),
        ));
    }
    let kind = match options.kind.as_deref() {
        None => None,
        Some(value) => Some(NodeKind::parse(value).ok_or_else(|| {
            GraphOutput::error(
                2,
                "graph.input",
                format!("unknown --kind {value:?}; expected file, symbol, or package"),
            )
        })?),
    };
    let mut edge_kinds = Vec::new();
    for value in options
        .edges
        .iter()
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        edge_kinds.push(EdgeKind::parse(value).ok_or_else(|| {
            GraphOutput::error(
                2,
                "graph.input",
                format!("unknown --edge {value:?}; expected contains, imports, or calls"),
            )
        })?);
    }
    let direction = match options.direction.as_deref() {
        None => None,
        Some("out") => Some(Direction::Out),
        Some("in") => Some(Direction::In),
        Some("both") => Some(Direction::Both),
        Some(other) => {
            return Err(GraphOutput::error(
                2,
                "graph.input",
                format!("unknown --direction {other:?}; expected out, in, or both"),
            ));
        }
    };
    let weakest = match options.confidence.as_deref() {
        None | Some("low") => Confidence::Low,
        Some("medium") => Confidence::Medium,
        Some("high") => Confidence::High,
        Some(other) => {
            return Err(GraphOutput::error(
                2,
                "graph.input",
                format!("unknown --confidence {other:?}; expected high, medium, or low"),
            ));
        }
    };
    let depth = options.depth.map(|d| d.clamp(1, MAX_DEPTH));
    let graph = load(options, paths).map_err(unbox)?;
    let target = |required: &str| -> Result<u32, GraphOutput> {
        let reference = options.target.as_deref().ok_or_else(|| {
            GraphOutput::error(
                2,
                "graph.input",
                format!("`{}` needs {required}", options.op),
            )
        })?;
        graph.resolve(reference, kind).map_err(unbox)
    };
    let filter = |default: &[EdgeKind]| EdgeFilter {
        kinds: if edge_kinds.is_empty() {
            default.to_vec()
        } else {
            edge_kinds.clone()
        },
        weakest,
    };
    let g = &graph;
    match options.op.as_str() {
        "stats" => Ok(stats(g)),
        "find" => {
            let text = options
                .target
                .as_deref()
                .ok_or_else(|| GraphOutput::error(2, "graph.input", "`find` needs search text"))?;
            Ok(run_page(options, g, Map::new(), find(g, text, kind)))
        }
        "node" => Ok(node(g, target("a node reference")?)),
        "symbols" => {
            let root = target("a file or symbol")?;
            let (extra, page) = symbols(g, root);
            Ok(run_page(options, g, extra, page))
        }
        "deps" | "dependents" | "callers" | "callees" | "walk" => {
            let root = target("a node reference")?;
            let node_kind = g.t.nodes[root as usize].kind;
            let natural = if node_kind == NodeKind::Symbol {
                EdgeKind::Calls
            } else {
                EdgeKind::Imports
            };
            let (default_edges, default_direction, default_depth) = match options.op.as_str() {
                "deps" => (vec![natural], Direction::Out, 1),
                "dependents" => (vec![natural], Direction::In, 1),
                "callers" => (vec![EdgeKind::Calls], Direction::In, 1),
                "callees" => (vec![EdgeKind::Calls], Direction::Out, 1),
                _ => (EdgeKind::ALL.to_vec(), Direction::Out, 2),
            };
            // A file's callers/callees are those of every symbol it declares.
            let seeds = if matches!(options.op.as_str(), "callers" | "callees")
                && node_kind == NodeKind::File
            {
                descendants(g, root)
            } else {
                vec![root]
            };
            let filter = filter(&default_edges);
            let direction = direction.unwrap_or(default_direction);
            let page = walk(
                g,
                &seeds,
                direction,
                &filter,
                depth.unwrap_or(default_depth),
                kind,
            );
            let mut extra = Map::new();
            extra.insert("target".into(), Value::Object(g.node_json(root)));
            extra.insert(
                "edges".into(),
                json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
            );
            if filter.kinds.contains(&EdgeKind::Calls) {
                extra.insert("coverage".into(), g.coverage(root));
            }
            Ok(run_page(options, g, extra, page))
        }
        "path" => {
            let from = target("a source reference")?;
            let reference = options.to.as_deref().ok_or_else(|| {
                GraphOutput::error(2, "graph.input", "`path` needs a target: path <from> <to>")
            })?;
            let to = g.resolve(reference, kind).map_err(unbox)?;
            let both_symbols = g.t.nodes[from as usize].kind == NodeKind::Symbol
                && g.t.nodes[to as usize].kind == NodeKind::Symbol;
            let default = if both_symbols {
                vec![EdgeKind::Calls]
            } else {
                vec![EdgeKind::Imports]
            };
            let filter = filter(&default);
            let mut out = path(g, from, to, direction.unwrap_or(Direction::Out), &filter);
            if filter.kinds.contains(&EdgeKind::Calls) {
                out.value["coverage"] = g.coverage(from);
            }
            Ok(out)
        }
        "cycles" => {
            let filter = filter(&[EdgeKind::Imports]);
            let page = cycles(g, &filter);
            let mut extra = Map::new();
            extra.insert(
                "edges".into(),
                json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
            );
            Ok(run_page(options, g, extra, page))
        }
        "hubs" => {
            let filter = filter(&[EdgeKind::Imports]);
            let rank_by = direction.unwrap_or(Direction::In);
            let page = hubs(g, &filter, kind, rank_by);
            let mut extra = Map::new();
            extra.insert(
                "edges".into(),
                json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
            );
            extra.insert(
                "rankBy".into(),
                json!(if rank_by == Direction::Out {
                    "out"
                } else {
                    "in"
                }),
            );
            Ok(run_page(options, g, extra, page))
        }
        "diagnostics" => Ok(run_page(
            options,
            g,
            Map::new(),
            diagnostics(g, options.target.as_deref()),
        )),
        "impact" => {
            let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
            let mut refs = options.target.iter().cloned().collect::<Vec<_>>();
            refs.extend(
                options
                    .changed
                    .iter()
                    .flat_map(|c| c.split(','))
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned),
            );
            let mut global = Vec::new();
            let mut unmapped = Vec::new();
            if let Some(rev) = &options.since {
                let changed = git_changed(&root, rev)
                    .map_err(|message| GraphOutput::error(5, "graph.git", message))?;
                for path in changed {
                    if super::impact::is_global_config(&path) {
                        global.push((parent_of(&path).to_owned(), path));
                    } else if g.t.by_key(&path).is_some() {
                        refs.push(path);
                    } else {
                        unmapped.push(path);
                    }
                }
            }
            if refs.is_empty() && global.is_empty() {
                let mut out = GraphOutput::error(
                    2,
                    "graph.input",
                    "`impact` needs a reference, --changed, or --since <rev>",
                );
                if !unmapped.is_empty() {
                    out.value["unmapped"] = json!(unmapped);
                    out.exit = 1;
                }
                return Err(out);
            }
            let mut seeds = Vec::new();
            for reference in &refs {
                seeds.push(g.resolve(reference, kind).map_err(unbox)?);
            }
            seeds.sort_unstable();
            seeds.dedup();
            let max_depth = depth.unwrap_or(DEFAULT_IMPACT_DEPTH);
            let impact = super::impact::run(&g.t, &seeds, &global, max_depth);
            let mut extra = Map::new();
            let mut summary = impact.summary;
            summary["maxDepth"] = json!(max_depth);
            if let Some(first) = seeds.first() {
                extra.insert("coverage".into(), g.coverage(*first));
            }
            if !unmapped.is_empty() {
                summary["unmapped"] = json!(
                    unmapped
                        .into_iter()
                        .take(MAX_CANDIDATES * 5)
                        .collect::<Vec<_>>()
                );
            }
            extra.insert("summary".into(), summary);
            let rows = impact.rows;
            Ok(run_page(
                options,
                g,
                extra,
                Page {
                    total: rows.len(),
                    rows,
                },
            ))
        }
        "issues" => {
            let detectors = options
                .detectors
                .iter()
                .flat_map(|d| d.split(','))
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let invalid = |message: String| GraphOutput::error(2, "graph.input", message);
            let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
            let scan_root = root.is_dir().then_some(root.as_path());
            let report = super::detect::run(&g.t, &detectors, scan_root).map_err(invalid)?;
            let min_tier = options.min_tier.unwrap_or(DEFAULT_MIN_TIER);
            let keep = |f: &super::detect::Finding| {
                options.min_score.is_none_or(|min| f.score >= min) && f.tier >= min_tier
            };
            let total_before = report.findings.len();
            let findings = report.findings.into_iter().filter(keep).collect::<Vec<_>>();
            let mut extra = Map::new();
            let mut summary = report.summary;
            summary["minTier"] = json!(min_tier);
            summary["hiddenBelowTier"] = json!(total_before - findings.len());
            extra.insert("summary".into(), summary);
            let mut status = BTreeMap::<String, &str>::new();
            if let Some(base) = &options.baseline {
                let base_options = QueryOptions {
                    graph: Some(base.clone()),
                    workspace: options.workspace.clone(),
                    ..Default::default()
                };
                let base_graph = load(&base_options, paths).map_err(unbox)?;
                let base_root =
                    PathBuf::from(base_graph.manifest["root"].as_str().unwrap_or_default());
                let before = super::detect::run(
                    &base_graph.t,
                    &detectors,
                    base_root.is_dir().then_some(base_root.as_path()),
                )
                .map_err(invalid)?;
                let old = before
                    .findings
                    .iter()
                    .map(|f| f.id.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                let now = findings
                    .iter()
                    .map(|f| f.id.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                for finding in &findings {
                    status.insert(
                        finding.id.clone(),
                        if old.contains(&finding.id) {
                            "existing"
                        } else {
                            "new"
                        },
                    );
                }
                let resolved = before
                    .findings
                    .iter()
                    .filter(|f| !now.contains(&f.id))
                    .map(|f| json!({"id": f.id, "detector": f.detector, "subject": f.subject, "title": f.title}))
                    .collect::<Vec<_>>();
                extra.insert(
                    "baseline".into(),
                    json!({
                        "graph": base_graph.id,
                        "new": status.values().filter(|s| **s == "new").count(),
                        "existing": status.values().filter(|s| **s == "existing").count(),
                        "resolved": resolved.len(),
                        "resolvedFindings": resolved.into_iter().take(MAX_CANDIDATES * 5).collect::<Vec<_>>(),
                    }),
                );
            }
            let rows = findings
                .iter()
                .map(|finding| {
                    let mut row = finding.to_json();
                    if let Some(state) = status.get(&finding.id) {
                        row["status"] = json!(state);
                    }
                    row
                })
                .collect::<Vec<_>>();
            Ok(run_page(
                options,
                g,
                extra,
                Page {
                    total: rows.len(),
                    rows,
                },
            ))
        }
        "stale" => {
            let (extra, page) = stale(g);
            let mut out = run_page(options, g, extra, page);
            if out.value["fresh"] == true {
                out.exit = 0;
            }
            Ok(out)
        }
        _ => Err(GraphOutput::error(2, "graph.input", "unknown op")),
    }
}

fn stats(g: &Graph) -> GraphOutput {
    let imports = EdgeFilter {
        kinds: vec![EdgeKind::Imports],
        weakest: Confidence::Low,
    };
    let calls = EdgeFilter {
        kinds: vec![EdgeKind::Calls],
        weakest: Confidence::Low,
    };
    let top = |filter: &EdgeFilter, kind: NodeKind| -> Vec<Value> {
        hubs(g, filter, Some(kind), Direction::In)
            .rows
            .into_iter()
            .take(5)
            .collect()
    };
    let m = &g.manifest;
    GraphOutput::ok(json!({
        "graph": g.summary(),
        "op": "stats",
        "dir": g.dir.to_string_lossy(),
        "octocodeVersion": m["octocodeVersion"],
        "scan": m["scan"],
        "counts": m["counts"],
        "imports": m["imports"],
        "calls": m["calls"],
        "languages": m["languages"],
        "diagnostics": m["diagnostics"],
        "mostImportedFiles": top(&imports, NodeKind::File),
        "mostImportedPackages": top(&imports, NodeKind::Package),
        "mostCalledSymbols": top(&calls, NodeKind::Symbol),
        "evidence": m["evidence"],
    }))
}

fn find(g: &Graph, text: &str, kind: Option<NodeKind>) -> Page {
    let lower = text.to_lowercase();
    let admit = |id: u32| kind.is_none_or(|k| g.t.nodes[id as usize].kind == k);
    let mut seen = vec![false; g.t.nodes.len()];
    let mut rows = Vec::new();
    let mut emit = |id: u32, how: &str, rows: &mut Vec<Value>| {
        if admit(id) && !seen[id as usize] {
            seen[id as usize] = true;
            let mut row = g.node_json(id);
            row.insert("match".into(), json!(how));
            rows.push(Value::Object(row));
        }
    };
    let exact = g.name_range(&lower, false).collect::<Vec<_>>();
    for id in exact {
        emit(id, "exact", &mut rows);
    }
    let prefix = g.name_range(&lower, true).collect::<Vec<_>>();
    for id in prefix {
        emit(id, "prefix", &mut rows);
    }
    for id in 0..g.t.nodes.len() as u32 {
        if g.t
            .str(g.t.nodes[id as usize].name)
            .to_lowercase()
            .contains(&lower)
        {
            emit(id, "contains", &mut rows);
        }
    }
    for id in 0..g.t.nodes.len() as u32 {
        if g.key(id).to_lowercase().contains(&lower) {
            emit(id, "path", &mut rows);
        }
    }
    Page {
        total: rows.len(),
        rows,
    }
}

fn degree_counts<'a>(edges: impl Iterator<Item = &'a EdgeRec>) -> BTreeMap<&'static str, u64> {
    let mut counts = BTreeMap::new();
    for edge in edges {
        *counts.entry(edge.kind.as_str()).or_default() += 1;
    }
    counts
}

fn node(g: &Graph, id: u32) -> GraphOutput {
    let record = &g.t.nodes[id as usize];
    let mut out = g.node_json(id);
    if record.kind == NodeKind::Symbol && record.parent != NONE {
        out.insert("parent".into(), json!(g.key(record.parent)));
    }
    out.insert("out".into(), json!(degree_counts(g.t.out(id).iter())));
    out.insert("in".into(), json!(degree_counts(g.t.incoming(id))));
    let target = quote(g.key(id));
    let hints = match record.kind {
        NodeKind::File => vec![
            format!("octocode graph query symbols {target}"),
            format!("octocode graph query dependents {target}"),
            format!("octocode graph query deps {target} --depth 2"),
        ],
        NodeKind::Symbol => vec![
            format!("octocode graph query callers {target}"),
            format!("octocode graph query callees {target}"),
        ],
        NodeKind::Package => vec![format!("octocode graph query dependents {target}")],
    };
    out.insert("next".into(), json!(hints));
    GraphOutput::ok(json!({"graph": g.summary(), "op": "node", "node": out}))
}

/// Every symbol transitively contained in `root`, in source order.
fn descendants(g: &Graph, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let children =
            g.t.out(id)
                .iter()
                .filter(|edge| {
                    edge.kind == EdgeKind::Contains
                        && g.t.nodes[edge.dst as usize].kind == NodeKind::Symbol
                })
                .map(|edge| edge.dst)
                .collect::<Vec<_>>();
        for child in children.into_iter().rev() {
            out.push(child);
            stack.push(child);
        }
    }
    out.sort_by_key(|id| (g.t.nodes[*id as usize].line, *id));
    out
}

fn symbols(g: &Graph, root: u32) -> (Map<String, Value>, Page) {
    let mut rows = Vec::new();
    let mut stack = vec![(root, 0u32)];
    while let Some((id, depth)) = stack.pop() {
        let mut children =
            g.t.out(id)
                .iter()
                .filter(|edge| {
                    edge.kind == EdgeKind::Contains
                        && g.t.nodes[edge.dst as usize].kind == NodeKind::Symbol
                })
                .map(|edge| edge.dst)
                .collect::<Vec<_>>();
        children.sort_by_key(|child| (g.t.nodes[*child as usize].line, *child));
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
        if id != root {
            let mut row = g.node_json(id);
            row.remove("file");
            row.insert("depth".into(), json!(depth));
            rows.push(Value::Object(row));
        }
    }
    let mut extra = Map::new();
    extra.insert("target".into(), Value::Object(g.node_json(root)));
    (
        extra,
        Page {
            total: rows.len(),
            rows,
        },
    )
}

/// Breadth-first expansion from `seeds`; rows are in discovery order.
fn walk(
    g: &Graph,
    seeds: &[u32],
    direction: Direction,
    filter: &EdgeFilter,
    depth: u32,
    kind: Option<NodeKind>,
) -> Page {
    let mut seen = vec![false; g.t.nodes.len()];
    let mut queue = VecDeque::new();
    for seed in seeds {
        seen[*seed as usize] = true;
        queue.push_back((*seed, 0u32));
    }
    let mut rows = Vec::new();
    let mut visited = 0usize;
    while let Some((id, level)) = queue.pop_front() {
        if level >= depth {
            continue;
        }
        for (next, edge) in g.neighbors(id, direction, filter) {
            if seen[next as usize] {
                continue;
            }
            seen[next as usize] = true;
            visited += 1;
            if visited > MAX_VISITED {
                break;
            }
            queue.push_back((next, level + 1));
            if kind.is_some_and(|k| g.t.nodes[next as usize].kind != k) {
                continue;
            }
            let mut row = g.node_json(next);
            row.insert("depth".into(), json!(level + 1));
            row.insert("from".into(), json!(g.key(id)));
            g.edge_fields(&mut row, edge);
            rows.push(Value::Object(row));
        }
    }
    Page {
        total: rows.len(),
        rows,
    }
}

fn path(g: &Graph, from: u32, to: u32, direction: Direction, filter: &EdgeFilter) -> GraphOutput {
    let mut previous = vec![(NONE, None::<&EdgeRec>); g.t.nodes.len()];
    let mut seen = vec![false; g.t.nodes.len()];
    let mut queue = VecDeque::from([from]);
    seen[from as usize] = true;
    while let Some(id) = queue.pop_front() {
        if id == to {
            break;
        }
        for (next, edge) in g.neighbors(id, direction, filter) {
            if !seen[next as usize] {
                seen[next as usize] = true;
                previous[next as usize] = (id, Some(edge));
                queue.push_back(next);
            }
        }
    }
    let edges = json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    if !seen[to as usize] {
        return GraphOutput {
            value: json!({
                "graph": g.summary(), "op": "path", "found": false, "edges": edges,
                "from": g.key(from), "to": g.key(to),
                "hint": "No directed path over these edges; try --direction both or --edge imports,calls",
            }),
            exit: 1,
        };
    }
    let mut hops = Vec::new();
    let mut cursor = to;
    while cursor != from {
        let (prev, edge) = previous[cursor as usize];
        let mut row = g.node_json(cursor);
        if let Some(edge) = edge {
            g.edge_fields(&mut row, edge);
        }
        hops.push(Value::Object(row));
        cursor = prev;
    }
    hops.push(Value::Object(g.node_json(from)));
    hops.reverse();
    GraphOutput::ok(json!({
        "graph": g.summary(), "op": "path", "found": true, "edges": edges,
        "length": hops.len() - 1, "path": hops,
    }))
}

/// Iterative Tarjan over the filtered subgraph. Components are returned
/// largest first, then by their smallest key.
fn cycles(g: &Graph, filter: &EdgeFilter) -> Page {
    let n = g.t.nodes.len();
    let mut index = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut components = Vec::<Vec<u32>>::new();
    let mut counter = 0u32;
    let successors = |id: u32| {
        g.t.out(id)
            .iter()
            .filter(|edge| filter.admits(edge))
            .map(|edge| edge.dst)
            .collect::<Vec<_>>()
    };
    for start in 0..n as u32 {
        if index[start as usize] != u32::MAX {
            continue;
        }
        let mut frames = vec![(start, successors(start), 0usize)];
        index[start as usize] = counter;
        low[start as usize] = counter;
        counter += 1;
        stack.push(start);
        on_stack[start as usize] = true;
        while let Some((id, next, at)) = frames.last_mut() {
            let id = *id;
            if let Some(&w) = next.get(*at) {
                *at += 1;
                if index[w as usize] == u32::MAX {
                    index[w as usize] = counter;
                    low[w as usize] = counter;
                    counter += 1;
                    stack.push(w);
                    on_stack[w as usize] = true;
                    frames.push((w, successors(w), 0));
                } else if on_stack[w as usize] {
                    low[id as usize] = low[id as usize].min(index[w as usize]);
                }
                continue;
            }
            frames.pop();
            if let Some((parent, _, _)) = frames.last() {
                low[*parent as usize] = low[*parent as usize].min(low[id as usize]);
            }
            if low[id as usize] == index[id as usize] {
                let mut component = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w as usize] = false;
                    component.push(w);
                    if w == id {
                        break;
                    }
                }
                let self_loop = component.len() == 1 && successors(id).contains(&id);
                if component.len() > 1 || self_loop {
                    components.push(component);
                }
            }
        }
    }
    for component in &mut components {
        component.sort_by(|a, b| g.key(*a).cmp(g.key(*b)));
    }
    components.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| g.key(a[0]).cmp(g.key(b[0])))
    });
    let rows = components
        .iter()
        .map(|component| {
            let mut row = json!({
                "size": component.len(),
                "nodes": component.iter().take(MAX_CYCLE_MEMBERS).map(|id| g.key(*id)).collect::<Vec<_>>(),
            });
            if component.len() > MAX_CYCLE_MEMBERS {
                row["nodesTruncated"] = json!(true);
            }
            row
        })
        .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
    }
}

fn hubs(g: &Graph, filter: &EdgeFilter, kind: Option<NodeKind>, rank_by: Direction) -> Page {
    let default_kind = match filter.kinds.as_slice() {
        [EdgeKind::Calls] => Some(NodeKind::Symbol),
        [EdgeKind::Imports] if rank_by == Direction::Out => Some(NodeKind::File),
        _ => None,
    };
    let kind = kind.or(default_kind);
    let distinct = |edges: Vec<u32>| {
        let mut edges = edges;
        edges.sort_unstable();
        edges.dedup();
        edges.len()
    };
    let mut ranked = (0..g.t.nodes.len() as u32)
        .filter(|id| kind.is_none_or(|k| g.t.nodes[*id as usize].kind == k))
        .map(|id| {
            let fan_in = distinct(
                g.t.incoming(id)
                    .filter(|e| filter.admits(e))
                    .map(|e| e.src)
                    .collect(),
            );
            let fan_out = distinct(
                g.t.out(id)
                    .iter()
                    .filter(|e| filter.admits(e))
                    .map(|e| e.dst)
                    .collect(),
            );
            (id, fan_in, fan_out)
        })
        .filter(|(_, fan_in, fan_out)| {
            if rank_by == Direction::Out {
                *fan_out > 0
            } else {
                *fan_in > 0
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        let (ka, kb) = if rank_by == Direction::Out {
            (a.2, b.2)
        } else {
            (a.1, b.1)
        };
        kb.cmp(&ka).then_with(|| g.key(a.0).cmp(g.key(b.0)))
    });
    let rows = ranked
        .into_iter()
        .map(|(id, fan_in, fan_out)| {
            let mut row = g.node_json(id);
            row.insert("fanIn".into(), json!(fan_in));
            row.insert("fanOut".into(), json!(fan_out));
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
    }
}

fn diagnostics(g: &Graph, filter: Option<&str>) -> Page {
    let rows =
        g.t.diagnostics
            .iter()
            .filter(|diag| {
                filter.is_none_or(|f| g.t.str(diag.code) == f || g.t.str(diag.file).starts_with(f))
            })
            .map(|diag| {
                let mut row = json!({
                    "file": g.t.str(diag.file),
                    "code": g.t.str(diag.code),
                    "message": g.t.str(diag.message),
                });
                if diag.line != NONE {
                    row["line"] = json!(diag.line);
                }
                row
            })
            .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
    }
}

/// Compares recorded content digests with the files on disk. New files are
/// not detected (that needs a rescan); re-ingest when anything changed.
fn stale(g: &Graph) -> (Map<String, Value>, Page) {
    let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
    let mut rows = Vec::new();
    for (node, digest) in &g.t.digests {
        let file = g.key(*node);
        let status = match std::fs::read(root.join(file)) {
            Err(_) => "missing",
            Ok(bytes) => {
                let current = octocode_engine::index::content_digest(&bytes);
                if current == g.t.str(*digest) {
                    continue;
                } else {
                    "changed"
                }
            }
        };
        rows.push(json!({"file": file, "status": status}));
    }
    let mut extra = Map::new();
    extra.insert("checked".into(), json!(g.t.digests.len()));
    extra.insert("fresh".into(), json!(rows.is_empty()));
    if !rows.is_empty() {
        extra.insert(
            "reingest".into(),
            json!(format!(
                "octocode graph ingest {}",
                quote(&root.to_string_lossy())
            )),
        );
    }
    (
        extra,
        Page {
            total: rows.len(),
            rows,
        },
    )
}
