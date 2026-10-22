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
    "drift",
];

#[derive(Clone, Debug, Default)]
pub struct QueryOptions {
    pub op: String,
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
        .map_err(|message| fail(ErrorCode::WorkspaceRootInvalid, message))?;
    let ids = list_snapshots(&home);
    let chosen = match options.graph.as_deref() {
        Some(spec) => choose_snapshot(&home, &ids, spec)?,
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
        fail(ErrorCode::NotFound, message)
    })
}

/// The snapshot `spec` names among `ids` (oldest first): the id itself, else
/// the newest snapshot of the one root whose scope is `spec`, else the one
/// id containing `spec`. A scope or substring that names several roots or
/// ids is ambiguous and lists them; it never picks one silently.
fn choose_snapshot(home: &Path, ids: &[String], spec: &str) -> Result<Option<String>, Failure> {
    if ids.iter().any(|id| id == spec) {
        return Ok(Some(spec.to_owned()));
    }
    let manifests = ids
        .iter()
        .filter_map(|id| Some((id, super::read_manifest(&home.join(id))?)))
        .collect::<Vec<_>>();
    let scoped = manifests
        .iter()
        .filter(|(_, manifest)| manifest["scope"] == spec)
        .collect::<Vec<_>>();
    if !scoped.is_empty() {
        // Newest snapshot of each root carrying that scope.
        let mut newest = BTreeMap::<String, &String>::new();
        for (id, manifest) in &scoped {
            let root = manifest["root"].as_str().unwrap_or_default().to_owned();
            newest.insert(root, id);
        }
        let candidates = newest.into_values().cloned().collect::<Vec<_>>();
        return match candidates.as_slice() {
            [only] => Ok(Some(only.clone())),
            _ => Err(ambiguous_graph(spec, &candidates)),
        };
    }
    let matching = ids
        .iter()
        .filter(|id| id.contains(spec))
        .cloned()
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        _ => Err(ambiguous_graph(spec, &matching)),
    }
}

fn ambiguous_graph(spec: &str, candidates: &[String]) -> Failure {
    let mut out = GraphOutput::error(
        ErrorCode::InvalidInput,
        format!(
            "{} graphs match {spec:?}; pass one exact id with --graph",
            candidates.len()
        ),
    );
    out.value["candidates"] = json!(candidates);
    Box::new(out)
}

fn load(options: &QueryOptions, paths: &PathPolicy) -> Result<Graph, Failure> {
    let dir = locate(options)?;
    let dir = paths
        .validate(&dir)
        .map_err(|error| {
            fail(
                error.local_error_code(ErrorCode::PathPolicyDenied),
                error.message,
            )
        })?
        .canonical;
    let manifest = std::fs::read(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| {
            fail(
                ErrorCode::Decode,
                format!("{} has no readable manifest", dir.display()),
            )
        })?;
    let bytes = read_graph_file(&dir.join(GRAPH_FILE)).map_err(|error| {
        fail(
            ErrorCode::Decode,
            format!("cannot read {GRAPH_FILE}: {error}"),
        )
    })?;
    let (t, digest) = decode(&bytes).map_err(|message| fail(ErrorCode::Decode, message))?;
    if manifest["graph"]["sha256"].as_str() != Some(digest.as_str()) {
        return Err(fail(
            ErrorCode::Decode,
            "graph.bin does not match its manifest digest; re-run `octocode graph ingest`",
        ));
    }
    // Every later read, hash, mention scan and `git -C` runs under the
    // manifest's root, which its digest does not cover: it must be the
    // canonical spelling of a directory inside the allowed roots.
    let root = manifest["root"].as_str().unwrap_or_default();
    let allowed = paths.validate_output(root).map_err(|error| {
        fail(
            error.local_error_code(ErrorCode::PathPolicyDenied),
            error.message,
        )
    })?;
    if allowed.canonical != Path::new(root) {
        return Err(fail(
            ErrorCode::Decode,
            format!(
                "{MANIFEST_FILE} root {root:?} is not a canonical path; re-run `octocode graph ingest`"
            ),
        ));
    }
    let id = manifest["id"].as_str().unwrap_or_default().to_owned();
    Ok(Graph { id, manifest, t })
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

    /// The snapshot every answer names. A scan cut (the `maxFiles` bound,
    /// the edge cap, files skipped as unreadable or oversized) leaves every
    /// answer partial: `partialReasons` names the cuts, and `reingest` the
    /// widened ingest that lifts a file-scan cut (the others it cannot).
    fn summary(&self) -> Value {
        let mut out = json!({"id": self.id, "root": self.manifest["root"], "createdAt": self.manifest["createdAt"]});
        let scan = &self.manifest["scan"];
        let truncated = scan["truncated"] == true;
        let reasons = [
            (truncated, "maxFiles"),
            (scan["edgesCapped"] == true, "edgeCap"),
            (
                scan["filesSkipped"].as_u64().unwrap_or(0) > 0,
                "filesSkipped",
            ),
        ]
        .into_iter()
        .filter_map(|(cut, reason)| cut.then_some(reason))
        .collect::<Vec<_>>();
        if !reasons.is_empty() {
            out["partialReasons"] = json!(reasons);
        }
        if truncated {
            out["reingest"] = json!(reingest_command(&self.manifest));
        }
        out
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
            .map(octocode_engine::text::slash_path);
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
                    ErrorCode::NotFound,
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
                    ErrorCode::InvalidInput,
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

impl Graph {
    /// The files that use an export of `file` through a re-exporting
    /// module: `uses` edges resolved through a re-export into a symbol
    /// `file` declares, from another file.
    fn reexport_users(&self, file: u32) -> impl Iterator<Item = (u32, &EdgeRec)> + '_ {
        contained_symbols(self, file)
            .into_iter()
            .flat_map(move |symbol| {
                self.t
                    .incoming(symbol)
                    .filter(move |edge| {
                        edge.kind == EdgeKind::Uses
                            && self.t.str(edge.detail) == "reexport"
                            && edge.src != file
                            && self.t.nodes[edge.src as usize].kind == NodeKind::File
                    })
                    .map(|edge| (edge.src, edge))
            })
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

const LIST_PREVIEW: usize = 100;

fn select_list(
    options: &QueryOptions,
    extra: &mut Map<String, Value>,
) -> Result<Option<Page>, GraphOutput> {
    let Some(list) = options.list.as_deref() else {
        return Ok(None);
    };
    let Some((section, field)) = list.split_once('.') else {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            "--list needs summary.<field> or baseline.<field>",
        ));
    };
    if !matches!(section, "summary" | "baseline") {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            "--list names a summary or baseline array",
        ));
    }
    let Some(Value::Array(rows)) = extra
        .get_mut(section)
        .and_then(Value::as_object_mut)
        .and_then(|object| object.remove(field))
    else {
        return Err(GraphOutput::error(
            ErrorCode::InvalidInput,
            format!("unknown evidence list {list:?}"),
        ));
    };
    let total = rows.len();
    extra.insert("list".into(), json!(list));
    Ok(Some(Page { rows, total }))
}

fn preview_extra_lists(options: &QueryOptions, id: &str, extra: &mut Map<String, Value>) {
    let mut continuations = Map::new();
    for section in ["summary", "baseline"] {
        let Some(object) = extra.get_mut(section).and_then(Value::as_object_mut) else {
            continue;
        };
        for (field, value) in object {
            let Some(rows) = value.as_array_mut() else {
                continue;
            };
            if rows.len() <= LIST_PREVIEW {
                continue;
            }
            let list = format!("{section}.{field}");
            let mut next = options.clone();
            next.list = Some(list.clone());
            rows.truncate(LIST_PREVIEW);
            continuations.insert(list, json!(next_command(&next, id, 0)));
        }
    }
    if !continuations.is_empty() {
        extra.insert("nextLists".into(), Value::Object(continuations));
    }
}

fn run_page(
    options: &QueryOptions,
    graph: &Graph,
    mut extra: Map<String, Value>,
    mut page: Page,
) -> GraphOutput {
    if options.list.as_deref() != Some("cycleNodes") {
        match select_list(options, &mut extra) {
            Ok(Some(selected)) => page = selected,
            Ok(None) => {}
            Err(error) => return error,
        }
    }
    preview_extra_lists(options, &graph.id, &mut extra);
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

    if offset + shown < total {
        out.insert("truncated".into(), json!(true));
        out.insert(
            "next".into(),
            json!(next_command(options, &graph.id, offset + shown)),
        );
    }
    if total == 0 {
        GraphOutput::empty(Value::Object(out))
    } else {
        GraphOutput::ok(Value::Object(out))
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
    if let Some(workspace) = &options.workspace {
        parts.push(format!(
            "--workspace {}",
            quote(&workspace.to_string_lossy())
        ));
    }
    if !options.edges.is_empty() {
        parts.push(format!("--edge {}", quote(&options.edges.join(","))));
    }
    for (flag, value) in [
        ("--kind", options.kind.clone()),
        ("--direction", options.direction.clone()),
        ("--depth", options.depth.map(|d| d.to_string())),
        ("--confidence", options.confidence.clone()),
        ("--list", options.list.clone()),
        ("--limit", options.limit.map(|l| l.to_string())),
        (
            "--detector",
            (!options.detectors.is_empty()).then(|| options.detectors.join(",")),
        ),
        (
            "--entry",
            (!options.entries.is_empty()).then(|| options.entries.join(",")),
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
        let mut command = std::process::Command::new("git");
        // The repository's own config must not run programs: an fsmonitor
        // hook executes on every index read. No prompts, no index rewrites.
        command
            .arg("-C")
            .arg(root)
            .args(["-c", "core.fsmonitor=false"])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0");
        let output = crate::tools::bounded_process::run_bounded(
            command,
            "git",
            std::time::Duration::from_secs(30),
            32 * 1024 * 1024,
            &crate::execution::cancel::NeverCancel,
        )?;
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
    match options.op.as_str() {
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
        "node" => Ok(node(g, query.target("a node reference")?)),
        "symbols" => {
            let root = query.target("a file or symbol")?;
            let (extra, page) = symbols(g, root);
            Ok(run_page(options, g, extra, page))
        }
        "deps" | "dependents" | "callers" | "callees" | "walk" => query.traversal(),
        "path" => query.path(),
        "cycles" => {
            let filter = query.filter(&[EdgeKind::Imports]);
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
    }
}

/// The parsed query flags.
struct Flags {
    kind: Option<NodeKind>,
    edge_kinds: Vec<EdgeKind>,
    direction: Option<Direction>,
    weakest: Confidence,
    depth: Option<u32>,
}

impl Flags {
    fn parse(options: &QueryOptions) -> Result<Self, GraphOutput> {
        let invalid = |message: String| GraphOutput::error(ErrorCode::InvalidInput, message);
        let kind = match options.kind.as_deref() {
            None => None,
            Some(value) => Some(NodeKind::parse(value).ok_or_else(|| {
                invalid(format!(
                    "unknown --kind {value:?}; expected file, symbol, or package"
                ))
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
                invalid(format!(
                    "unknown --edge {value:?}; expected contains, imports, or calls"
                ))
            })?);
        }
        let direction = match options.direction.as_deref() {
            None => None,
            Some("out") => Some(Direction::Out),
            Some("in") => Some(Direction::In),
            Some("both") => Some(Direction::Both),
            Some(other) => {
                return Err(invalid(format!(
                    "unknown --direction {other:?}; expected out, in, or both"
                )));
            }
        };
        let weakest = match options.confidence.as_deref() {
            None | Some("low") => Confidence::Low,
            Some("medium") => Confidence::Medium,
            Some("high") => Confidence::High,
            Some(other) => {
                return Err(invalid(format!(
                    "unknown --confidence {other:?}; expected high, medium, or low"
                )));
            }
        };
        Ok(Self {
            kind,
            edge_kinds,
            direction,
            weakest,
            depth: options.depth.map(|d| d.clamp(1, MAX_DEPTH)),
        })
    }
}

/// `extra.edges`: the edge kinds a page followed.
fn edges_extra(filter: &EdgeFilter) -> Map<String, Value> {
    let mut extra = Map::new();
    extra.insert(
        "edges".into(),
        json!(filter.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
    );
    extra
}

/// One query against one loaded graph.
struct Query<'q> {
    options: &'q QueryOptions,
    g: &'q Graph,
    flags: Flags,
}

impl Query<'_> {
    fn target(&self, required: &str) -> Result<u32, GraphOutput> {
        let reference = self.options.target.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("`{}` needs {required}", self.options.op),
            )
        })?;
        self.g
            .resolve(reference, self.flags.kind)
            .map_err(|failure| *failure)
    }

    fn filter(&self, default: &[EdgeKind]) -> EdgeFilter {
        EdgeFilter {
            kinds: if self.flags.edge_kinds.is_empty() {
                default.to_vec()
            } else {
                self.flags.edge_kinds.clone()
            },
            weakest: self.flags.weakest,
        }
    }

    /// `deps`, `dependents`, `callers`, `callees`, `walk`.
    fn traversal(&self) -> Result<GraphOutput, GraphOutput> {
        let (g, op) = (self.g, self.options.op.as_str());
        let root = self.target("a node reference")?;
        let node_kind = g.t.nodes[root as usize].kind;
        let natural = if node_kind == NodeKind::Symbol {
            EdgeKind::Calls
        } else {
            EdgeKind::Imports
        };
        let (default_edges, default_direction, default_depth) = match op {
            "deps" => (vec![natural], Direction::Out, 1),
            "dependents" => (vec![natural], Direction::In, 1),
            "callers" => (vec![EdgeKind::Calls], Direction::In, 1),
            "callees" => (vec![EdgeKind::Calls], Direction::Out, 1),
            _ => (EdgeKind::ALL.to_vec(), Direction::Out, 2),
        };
        // A file's callers/callees are those of every symbol it declares.
        let seeds = if matches!(op, "callers" | "callees") && node_kind == NodeKind::File {
            descendants(g, root)
        } else {
            vec![root]
        };
        let filter = self.filter(&default_edges);
        // A file's default dependents include the files that use its exports
        // through a re-exporting module; an explicit --edge walks only those
        // edges.
        let reexports =
            op == "dependents" && node_kind == NodeKind::File && self.flags.edge_kinds.is_empty();
        let page = walk(
            g,
            &seeds,
            &Walk {
                direction: self.flags.direction.unwrap_or(default_direction),
                filter: &filter,
                depth: self.flags.depth.unwrap_or(default_depth),
                kind: self.flags.kind,
                reexports,
            },
        );
        let mut extra = Map::new();
        extra.insert("target".into(), Value::Object(g.node_json(root)));
        extra.extend(edges_extra(&filter));
        if op == "dependents" && filter.kinds.contains(&EdgeKind::Imports) {
            let unlinked = unlinked_path_imports(g, root);
            if !unlinked.is_empty() {
                extra.insert("summary".into(), json!({"unlinkedPathImports": unlinked}));
            }
        }
        if filter.kinds.contains(&EdgeKind::Calls) {
            extra.insert("coverage".into(), g.coverage(root));
        }
        Ok(run_page(self.options, g, extra, page))
    }

    fn path(&self) -> Result<GraphOutput, GraphOutput> {
        let g = self.g;
        let from = self.target("a source reference")?;
        let reference = self.options.to.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                "`path` needs a target: path <from> <to>",
            )
        })?;
        let to = g
            .resolve(reference, self.flags.kind)
            .map_err(|failure| *failure)?;
        let both_symbols = g.t.nodes[from as usize].kind == NodeKind::Symbol
            && g.t.nodes[to as usize].kind == NodeKind::Symbol;
        let default = if both_symbols {
            vec![EdgeKind::Calls]
        } else {
            vec![EdgeKind::Imports]
        };
        let filter = self.filter(&default);
        let direction = self.flags.direction.unwrap_or(Direction::Out);
        let mut out = path(g, from, to, direction, &filter);
        if filter.kinds.contains(&EdgeKind::Calls) {
            out.value["coverage"] = g.coverage(from);
        }
        Ok(out)
    }

    fn hubs(&self) -> Result<GraphOutput, GraphOutput> {
        let filter = self.filter(&[EdgeKind::Imports]);
        let rank_by = self.flags.direction.unwrap_or(Direction::In);
        let page = hubs(self.g, &filter, self.flags.kind, rank_by);
        let mut extra = edges_extra(&filter);
        extra.insert(
            "rankBy".into(),
            json!(if rank_by == Direction::Out {
                "out"
            } else {
                "in"
            }),
        );
        Ok(run_page(self.options, self.g, extra, page))
    }

    fn impact(&self) -> Result<GraphOutput, GraphOutput> {
        let (g, options) = (self.g, self.options);
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
                .map_err(|message| GraphOutput::error(ErrorCode::GitFailed, message))?;
            for path in changed {
                if super::super::freshness::is_global_config(&path) {
                    global.push((parent_of(&path).to_owned(), path));
                } else if g.t.by_key(&path).is_some() {
                    refs.push(path);
                } else {
                    unmapped.push(path);
                }
            }
        }
        if refs.is_empty() && global.is_empty() {
            let Some(rev) = &options.since else {
                return Err(GraphOutput::error(
                    ErrorCode::InvalidInput,
                    "`impact` needs a reference, --changed, or --since <rev>",
                ));
            };
            // A change set with nothing in the graph is an empty answer.
            let mut summary = json!({
                "since": rev,
                "reason": if unmapped.is_empty() {
                    format!("no files changed since {rev}")
                } else {
                    format!("no file changed since {rev} is in the graph")
                },
            });
            if !unmapped.is_empty() {
                summary["unmapped"] = json!(unmapped);
            }
            let mut extra = Map::new();
            extra.insert("summary".into(), summary);
            return Ok(run_page(
                options,
                g,
                extra,
                Page {
                    total: 0,
                    rows: Vec::new(),
                },
            ));
        }
        let mut seeds = Vec::new();
        for reference in &refs {
            seeds.push(
                g.resolve(reference, self.flags.kind)
                    .map_err(|failure| *failure)?,
            );
        }
        seeds.sort_unstable();
        seeds.dedup();
        let max_depth = self.flags.depth.unwrap_or(DEFAULT_IMPACT_DEPTH);
        let impact = super::impact::run(&g.t, &seeds, &global, max_depth);
        let mut extra = Map::new();
        let mut summary = impact.summary;
        summary["maxDepth"] = json!(max_depth);
        if let Some(first) = seeds.first() {
            extra.insert("coverage".into(), g.coverage(*first));
        }
        if !unmapped.is_empty() {
            summary["unmapped"] = json!(unmapped);
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

    fn issues(&self, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
        let (g, options) = (self.g, self.options);
        let detectors = options
            .detectors
            .iter()
            .flat_map(|d| d.split(','))
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let invalid = |message: String| GraphOutput::error(ErrorCode::InvalidInput, message);
        let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
        let scan_root = root.is_dir().then_some(root.as_path());
        let mut entries = Vec::new();
        for reference in options
            .entries
            .iter()
            .flat_map(|e| e.split(','))
            .map(str::trim)
            .filter(|e| !e.is_empty())
        {
            let file = g
                .resolve(reference, Some(NodeKind::File))
                .map_err(|failure| *failure)?;
            entries.push(g.key(file).to_owned());
        }
        let entries = (!entries.is_empty()).then_some(entries.as_slice());
        let report = super::detect::run(&g.t, &detectors, scan_root, entries).map_err(invalid)?;
        let min_tier = options.min_tier.unwrap_or(DEFAULT_MIN_TIER);
        let keep = |f: &&super::detect::Finding| {
            options.min_score.is_none_or(|min| f.score >= min) && f.tier >= min_tier
        };
        let total_before = report.findings.len();
        let findings = report
            .findings
            .iter()
            .filter(keep)
            .cloned()
            .collect::<Vec<_>>();
        let mut extra = Map::new();
        let mut summary = report.summary.clone();
        summary["minTier"] = json!(min_tier);
        summary["hiddenBelowTier"] = json!(total_before - findings.len());
        extra.insert("summary".into(), summary);
        let mut status = BTreeMap::<String, &str>::new();
        if let Some(base) = &options.baseline {
            let baseline =
                self.baseline(paths, base, &detectors, entries, &findings, &mut status)?;
            extra.insert("baseline".into(), baseline);
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

    /// The structural drift from the `--baseline` snapshot to this one.
    fn drift(&self, paths: &PathPolicy) -> Result<GraphOutput, GraphOutput> {
        let base = self.options.baseline.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                "`drift` needs --baseline <snapshot>",
            )
        })?;
        let base_options = QueryOptions {
            graph: Some(base.to_owned()),
            workspace: self.options.workspace.clone(),
            ..Default::default()
        };
        let base_graph = load(&base_options, paths).map_err(|failure| *failure)?;
        let drift = super::drift::diff(&base_graph.t, &self.g.t);
        let mut extra = Map::new();
        extra.insert("baseline".into(), base_graph.summary());
        extra.insert("summary".into(), drift.summary);
        let rows = drift.rows;
        Ok(run_page(
            self.options,
            self.g,
            extra,
            Page {
                total: rows.len(),
                rows,
            },
        ))
    }

    /// Findings compared with a baseline graph: each current one is `new`
    /// or `existing` (written to `status`), and the resolved ones are listed.
    fn baseline(
        &self,
        paths: &PathPolicy,
        base: &str,
        detectors: &[String],
        entries: Option<&[String]>,
        findings: &[super::detect::Finding],
        status: &mut BTreeMap<String, &str>,
    ) -> Result<Value, GraphOutput> {
        let base_options = QueryOptions {
            graph: Some(base.to_owned()),
            workspace: self.options.workspace.clone(),
            ..Default::default()
        };
        let base_graph = load(&base_options, paths).map_err(|failure| *failure)?;
        let base_root = PathBuf::from(base_graph.manifest["root"].as_str().unwrap_or_default());
        let before = super::detect::run(
            &base_graph.t,
            detectors,
            base_root.is_dir().then_some(base_root.as_path()),
            entries,
        )
        .map_err(|message| GraphOutput::error(ErrorCode::InvalidInput, message))?;
        let old = before
            .findings
            .iter()
            .map(|f| f.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let now = findings
            .iter()
            .map(|f| f.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for finding in findings {
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
        Ok(json!({
            "graph": base_graph.id,
            "new": status.values().filter(|s| **s == "new").count(),
            "existing": status.values().filter(|s| **s == "existing").count(),
            "resolved": resolved.len(),
            "resolvedFindings": resolved,
        }))
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

/// The symbols `id` directly contains, in edge order.
fn contained_symbols(g: &Graph, id: u32) -> Vec<u32> {
    g.t.out(id)
        .iter()
        .filter(|edge| {
            edge.kind == EdgeKind::Contains && g.t.nodes[edge.dst as usize].kind == NodeKind::Symbol
        })
        .map(|edge| edge.dst)
        .collect()
}

/// Every symbol transitively contained in `root`, in source order.
fn descendants(g: &Graph, root: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let children = contained_symbols(g, id);
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
        let mut children = contained_symbols(g, id);
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

/// How a [`walk`] expands.
struct Walk<'f> {
    direction: Direction,
    filter: &'f EdgeFilter,
    depth: u32,
    kind: Option<NodeKind>,
    /// Inward from a file, also step to the files that use its exports
    /// through a re-exporting module.
    reexports: bool,
}

/// Breadth-first expansion from `seeds`; rows are in discovery order.
fn walk(g: &Graph, seeds: &[u32], how: &Walk) -> Page {
    let Walk {
        direction,
        filter,
        depth,
        kind,
        reexports,
    } = *how;
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
        let through = (reexports
            && direction != Direction::Out
            && g.t.nodes[id as usize].kind == NodeKind::File)
            .then(|| g.reexport_users(id))
            .into_iter()
            .flatten();
        for (next, edge) in g.neighbors(id, direction, filter).chain(through) {
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
        return GraphOutput::empty(json!({
            "graph": g.summary(), "op": "path", "found": false, "edges": edges,
            "from": g.key(from), "to": g.key(to),
            "hint": "No directed path over these edges; try --direction both or --edge imports,calls",
        }));
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
fn cycle_preview_row(options: &QueryOptions, id: &str, keys: &[String]) -> Value {
    let mut row = json!({
        "size": keys.len(),
        "nodes": keys.iter().take(MAX_CYCLE_MEMBERS).collect::<Vec<_>>(),
    });
    if keys.len() > MAX_CYCLE_MEMBERS {
        let mut detail = options.clone();
        detail.target = keys.first().cloned();
        detail.to = None;
        detail.list = Some("cycleNodes".into());
        row["nodesTruncated"] = json!(true);
        // The preview shows the first members: the listing reads on after them.
        row["nextNodes"] = json!(next_command(&detail, id, MAX_CYCLE_MEMBERS));
    }
    row
}

/// Cycle groups as member keys, each with whether a runtime (module-load)
/// loop runs inside it. Over import edges they are the one cycle engine's
/// groups ([`crate::tools::ast_graph::algorithms::Cycles`], as the `cycle`
/// detector reports them); over other edge kinds (calls),
/// the strongly connected groups of those edges, with no runtime verdict.
fn cycle_groups(g: &Graph, filter: &EdgeFilter) -> Vec<(Vec<String>, Option<bool>)> {
    if filter.kinds == [EdgeKind::Imports] {
        let graph = super::detect::file_graph(&g.t, &|_| true);
        let cycles = crate::tools::ast_graph::algorithms::Cycles::of(&graph);
        return cycles
            .groups()
            .map(|(_, files)| {
                let members = files
                    .iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>();
                (
                    files.clone(),
                    Some(!cycles.runtime_within(&members).is_empty()),
                )
            })
            .collect();
    }
    let n = g.t.nodes.len();
    let successors = |id: u32| {
        g.t.out(id)
            .iter()
            .filter(|edge| filter.admits(edge))
            .map(|edge| edge.dst)
            .collect::<Vec<_>>()
    };
    let nodes = (0..n as u32).collect::<Vec<_>>();
    super::detect::tarjan(&nodes, &successors, n, &|component| {
        component.len() > 1 || successors(component[0]).contains(&component[0])
    })
    .into_iter()
    .map(|component| {
        let keys = component.iter().map(|id| g.key(*id).to_owned()).collect();
        (keys, None)
    })
    .collect()
}

fn cycles(g: &Graph, filter: &EdgeFilter, options: &QueryOptions) -> Result<Page, GraphOutput> {
    let mut components = cycle_groups(g, filter);
    for (component, _) in &mut components {
        component.sort();
    }
    components.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    if options.list.as_deref() == Some("cycleNodes") {
        let Some(target) = options.target.as_deref() else {
            return Err(GraphOutput::error(
                ErrorCode::InvalidInput,
                "cycleNodes needs a cycle member",
            ));
        };
        let Some((component, _)) = components
            .iter()
            .find(|(component, _)| component.iter().any(|key| key == target))
        else {
            return Err(GraphOutput::error(
                ErrorCode::InvalidInput,
                format!("no cycle contains {target:?}"),
            ));
        };
        let rows = component.iter().map(|key| json!(key)).collect::<Vec<_>>();
        return Ok(Page {
            total: rows.len(),
            rows,
        });
    }
    let rows = components
        .iter()
        .map(|(keys, runtime)| {
            let mut row = cycle_preview_row(options, &g.id, keys);
            if let Some(runtime) = runtime {
                row["runtime"] = json!(runtime);
            }
            row
        })
        .collect::<Vec<_>>();
    Ok(Page {
        total: rows.len(),
        rows,
    })
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
                if diag.target != NONE {
                    row["target"] = json!(g.key(diag.target));
                }
                row
            })
            .collect::<Vec<_>>();
    Page {
        total: rows.len(),
        rows,
    }
}

/// The unlinked path imports that name `file` (see the ingest's
/// `unlinked-path-import` diagnostics): importers no edge records.
fn unlinked_path_imports(g: &Graph, file: u32) -> Vec<Value> {
    g.t.diagnostics
        .iter()
        .filter(|diag| {
            diag.target == file && g.t.str(diag.code) == DiagnosticCode::UnlinkedPathImport.as_str()
        })
        .map(|diag| {
            json!({
                "file": g.t.str(diag.file),
                "line": diag.line,
                "message": g.t.str(diag.message),
            })
        })
        .collect()
}

/// The ingest that rebuilds the snapshot of `manifest` with its own scan
/// options; a scan cut at maxFiles re-ingests with twice the bound, the only
/// limit a re-ingest can lift.
fn reingest_command(manifest: &Value) -> String {
    let scan = &manifest["scan"];
    let root = manifest["root"].as_str().unwrap_or_default();
    let mut command = format!("octocode graph ingest {}", quote(root));
    let max_files = match scan["maxFiles"].as_u64() {
        Some(max) if scan["truncated"] == true => {
            Some(max.saturating_mul(2).min(u64::from(u32::MAX)))
        }
        max => max.filter(|max| *max != u64::from(super::DEFAULT_MAX_FILES)),
    };
    if let Some(max) = max_files {
        command.push_str(&format!(" --max-files {max}"));
    }
    for dir in scan["excludeDir"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        command.push_str(&format!(" --exclude {}", quote(dir)));
    }
    for (language, globs) in scan["languageGlobs"].as_object().into_iter().flatten() {
        for glob in globs
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            command.push_str(&format!(
                " --language-glob {}",
                quote(&format!("{language}={glob}"))
            ));
        }
    }
    command
}

/// What changed under the snapshot's root since ingest: sources edited,
/// removed or added, and linking configs (manifests, tsconfig, ...) edited,
/// removed or added; the same check an ingest runs before reusing it.
fn stale(g: &Graph, paths: &PathPolicy) -> (Map<String, Value>, Page) {
    let root = PathBuf::from(g.manifest["root"].as_str().unwrap_or_default());
    let freshness = super::snapshot_changes(paths, &root, &g.t, &g.manifest, false);
    let rows = freshness
        .changes
        .into_iter()
        .map(|change| {
            let mut row = json!({"file": change.file, "status": change.status});
            if change.config {
                row["config"] = json!(true);
            }
            row
        })
        .collect::<Vec<_>>();
    let mut extra = Map::new();
    extra.insert("checked".into(), json!(g.t.digests.len()));
    extra.insert(
        "fresh".into(),
        json!(rows.is_empty() && freshness.added_checked),
    );
    let scan = &g.manifest["scan"];
    let max_files = scan["maxFiles"].as_u64();
    if !freshness.added_checked {
        extra.insert(
            "unchecked".into(),
            json!(format!(
                "files added since ingest: the snapshot's scan stopped at maxFiles ({})",
                max_files.map_or_else(|| "?".to_owned(), |max| max.to_string())
            )),
        );
    }
    // A scan cut at maxFiles names its widened re-ingest in the graph block.
    if !rows.is_empty() && freshness.added_checked {
        extra.insert("reingest".into(), json!(reingest_command(&g.manifest)));
    }
    (
        extra,
        Page {
            total: rows.len(),
            rows,
        },
    )
}

#[cfg(test)]
mod evidence_list_tests {
    use super::*;

    #[test]
    fn nested_summary_list_has_a_lossless_paged_route() {
        let mut extra = Map::new();
        let values = (0..125)
            .map(|i| json!(format!("test-{i}")))
            .collect::<Vec<_>>();
        extra.insert(
            "summary".into(),
            json!({"testsToRun":values,"testCount":125}),
        );
        let options = QueryOptions {
            op: "impact".into(),
            ..Default::default()
        };
        preview_extra_lists(&options, "snapshot-id", &mut extra);
        assert_eq!(
            extra["summary"]["testsToRun"].as_array().unwrap().len(),
            100
        );
        assert!(
            extra["nextLists"]["summary.testsToRun"]
                .as_str()
                .unwrap()
                .contains("--list summary.testsToRun")
        );

        let mut complete = Map::new();
        complete.insert(
            "summary".into(),
            json!({"testsToRun":(0..125).map(|i| format!("test-{i}")).collect::<Vec<_>>() }),
        );
        let list_options = QueryOptions {
            list: Some("summary.testsToRun".into()),
            ..options
        };
        let page = select_list(&list_options, &mut complete)
            .expect("list")
            .expect("rows");
        assert_eq!(page.total, 125);
        assert_eq!(page.rows[124], "test-124");
    }

    #[test]
    fn large_cycle_preview_links_to_all_members() {
        let keys = (0..51).map(|i| format!("node-{i}")).collect::<Vec<_>>();
        let row = cycle_preview_row(
            &QueryOptions {
                op: "cycles".into(),
                ..Default::default()
            },
            "snapshot-id",
            &keys,
        );
        assert_eq!(row["nodes"].as_array().unwrap().len(), 50);
        assert!(
            row["nextNodes"]
                .as_str()
                .unwrap()
                .contains("--list cycleNodes")
        );
        assert!(row["nextNodes"].as_str().unwrap().contains("node-0"));
        assert!(
            row["nextNodes"].as_str().unwrap().ends_with("--offset 50"),
            "{row}"
        );
    }

    #[test]
    fn continuation_keeps_the_explicit_workspace() {
        let options = QueryOptions {
            op: "impact".into(),
            workspace: Some(PathBuf::from("/tmp/graph-fixture")),
            list: Some("summary.testsToRun".into()),
            ..Default::default()
        };
        let next = next_command(&options, "snapshot-id", 50);
        assert!(next.contains("--workspace /tmp/graph-fixture"), "{next}");
    }

    #[test]
    fn resolved_findings_are_reachable_after_the_preview() {
        let mut extra = Map::new();
        extra.insert(
            "baseline".into(),
            json!({"resolvedFindings":(0..125).map(|i| json!({"id":format!("finding-{i}")})).collect::<Vec<_>>() }),
        );
        let options = QueryOptions {
            op: "issues".into(),
            ..Default::default()
        };
        preview_extra_lists(&options, "snapshot-id", &mut extra);
        assert_eq!(
            extra["baseline"]["resolvedFindings"]
                .as_array()
                .unwrap()
                .len(),
            100
        );
        assert!(
            extra["nextLists"]["baseline.resolvedFindings"]
                .as_str()
                .unwrap()
                .contains("--list baseline.resolvedFindings")
        );
        let list = QueryOptions {
            list: Some("baseline.resolvedFindings".into()),
            ..options
        };
        let mut complete = Map::new();
        complete.insert("baseline".into(), json!({"resolvedFindings":(0..125).map(|i| json!({"id":format!("finding-{i}")})).collect::<Vec<_>>() }));
        let page = select_list(&list, &mut complete)
            .expect("list")
            .expect("page");
        assert_eq!(page.total, 125);
        assert_eq!(page.rows[124]["id"], "finding-124");
    }
}
