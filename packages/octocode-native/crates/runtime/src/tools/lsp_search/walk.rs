//! Call- and type-hierarchy walks (`callers`, `callees`, `callHierarchy`,
//! `supertypes`, `subtypes`).
//!
//! The walk is breadth-first, one level at a time. Nodes are keyed by
//! `(canonical path, selectionRange start/end)` — never by name or by the
//! whole serialized item, whose opaque `data`/`detail` vary. Every edge
//! carries its `level` and, below level 1, the parent (`via`) it connects
//! to; repeated `(parent, node)` pairs merge into one edge with the union of
//! their call sites. A node reached again (cycle or diamond) keeps its edge
//! but is not expanded twice. Depth, total nodes, and per-node fan-out are
//! capped in code; each level's requests run concurrently under a small
//! bound; per-node failures are collected as data.

use super::failure::{LspFailure, continuation, mark_partial, mark_terminal_limit, push_reason};
use super::importers::{
    Importers, callers_from_references, enclosing_callable, flatten_symbols, verified_anchors,
};
use super::locations::{items_payload, public_range};
use super::render::{as_array, decode_uri_path, symbol_kind_name, uri_to_path};
use super::scope::Scope;
use super::source::{SourceCache, item_uri_is_authorized};
use super::{LspSearchQuery, cancellable};
use crate::policy::path::PathPolicy;
use crate::tools::cancel::CancellationCheck;
use futures_util::StreamExt;
use octocode_engine::error::Error as EngineError;
use octocode_engine::lsp::client::{NativeLspClient, SnippetReadPolicy};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Hard ceiling on `depth` (the schema maximum), enforced here too.
pub(super) const MAX_HIERARCHY_DEPTH: u32 = 20;
/// Distinct nodes one walk expands toward (roots included). Past the cap,
/// edges to new nodes below the anchor are dropped and the anchor's own
/// results stay listed but unexpanded; each such parent carries a resumable
/// continuation.
pub(super) const MAX_HIERARCHY_NODES: usize = 200;
/// Results kept per expanded node below the anchor; a wider node resumes as
/// the anchor of its own continuation, where every result is kept (the
/// anchor's results are paged, never cut).
pub(super) const MAX_HIERARCHY_FAN_OUT: usize = 50;
/// Concurrent expansion requests per level.
const LEVEL_CONCURRENCY: usize = 4;

/// One hierarchy expansion request kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Expansion {
    IncomingCalls,
    OutgoingCalls,
    Supertypes,
    Subtypes,
}

impl Expansion {
    /// The hierarchy node a result leads to, expanded at the next level.
    fn node_of(self, result: &Value) -> Option<&Value> {
        match self {
            Self::IncomingCalls => result.get("from"),
            Self::OutgoingCalls => result.get("to"),
            Self::Supertypes | Self::Subtypes => Some(result),
        }
    }

    fn is_call(self) -> bool {
        matches!(self, Self::IncomingCalls | Self::OutgoingCalls)
    }

    /// Public direction label of a call expansion.
    fn direction(self) -> Option<&'static str> {
        match self {
            Self::IncomingCalls => Some("incoming"),
            Self::OutgoingCalls => Some("outgoing"),
            Self::Supertypes | Self::Subtypes => None,
        }
    }

    /// The single-direction operation that continues this expansion.
    fn operation(self) -> &'static str {
        match self {
            Self::IncomingCalls => "callers",
            Self::OutgoingCalls => "callees",
            Self::Supertypes => "supertypes",
            Self::Subtypes => "subtypes",
        }
    }
}

/// The language-server requests a hierarchy walk issues; a seam so the walk
/// is testable over a fake graph.
pub(super) trait HierarchySource {
    async fn expand(&self, expansion: Expansion, item: Value) -> Result<Value, EngineError>;
}

impl HierarchySource for NativeLspClient {
    async fn expand(&self, expansion: Expansion, item: Value) -> Result<Value, EngineError> {
        match expansion {
            Expansion::IncomingCalls => self.incoming_calls(item).await,
            Expansion::OutgoingCalls => self.outgoing_calls(item).await,
            Expansion::Supertypes => self.type_hierarchy_supertypes(item).await,
            Expansion::Subtypes => self.type_hierarchy_subtypes(item).await,
        }
    }
}

/// One discovered edge. `node` is the raw hierarchy item the edge leads to,
/// `parent` the raw item it was expanded from (`None` only for a
/// reference-derived caller), and `sites` the raw call-site ranges
/// (`fromRanges`).
#[derive(Debug)]
pub(super) struct HierarchyEdge {
    pub(super) node: Value,
    pub(super) parent: Option<Value>,
    pub(super) level: u32,
    pub(super) sites: Vec<Value>,
}

/// Where a node-capped walk can resume: re-anchor on `node` and walk the
/// remaining `depth` levels.
#[derive(Debug)]
pub(super) struct Resume {
    pub(super) node: Value,
    pub(super) depth: u32,
}

#[derive(Debug, Default)]
pub(super) struct HierarchyWalk {
    pub(super) edges: Vec<HierarchyEdge>,
    pub(super) failures: Vec<EngineError>,
    /// Edges to new nodes dropped by [`MAX_HIERARCHY_NODES`].
    pub(super) dropped_edges: usize,
    /// Every parent whose new children the node cap dropped, in walk order
    /// (each once); each gets an executable continuation.
    pub(super) resumes: Vec<Resume>,
    /// Names of nodes whose results were cut to [`MAX_HIERARCHY_FAN_OUT`];
    /// each is also a resume point.
    pub(super) fan_out_capped: Vec<String>,
    /// Anchor results listed past the node cap without being expanded; each
    /// is also a resume point.
    pub(super) unexpanded_nodes: usize,
    /// Results naming a file outside the read policy (skipped).
    pub(super) out_of_policy: usize,
    /// Results declared in a TypeScript built-in lib file (`lib.*.d.ts`,
    /// e.g. `String.toUpperCase`), kept out of the call graph and listed by
    /// name once each.
    pub(super) builtin_lib: Vec<String>,
}

/// Whether a hierarchy node is declared in a TypeScript built-in lib file
/// (`…/typescript/lib/lib.<name>.d.ts`): language built-ins, not project or
/// dependency code.
pub(super) fn is_builtin_lib_declaration(node: &Value) -> bool {
    let Some(uri) = node.get("uri").and_then(Value::as_str) else {
        return false;
    };
    let Some((dir, file)) = uri.rsplit_once('/') else {
        return false;
    };
    dir.ends_with("/typescript/lib") && file.starts_with("lib.") && file.ends_with(".d.ts")
}

/// A built-in lib item's name, qualified by its container when the server
/// names one (`String.toUpperCase`).
pub(super) fn builtin_name(node: &Value) -> String {
    let name = node.get("name").and_then(Value::as_str).unwrap_or("?");
    match node
        .get("detail")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|detail| !detail.is_empty() && !detail.contains(char::is_whitespace))
    {
        Some(container) => format!("{container}.{name}"),
        None => name.to_owned(),
    }
}

/// Memoized node identity and authorization for one walk.
struct NodeKeys<'a> {
    paths: &'a PathPolicy,
    canonical: HashMap<String, String>,
    authorized: HashMap<String, bool>,
}

impl<'a> NodeKeys<'a> {
    fn new(paths: &'a PathPolicy) -> Self {
        Self {
            paths,
            canonical: HashMap::new(),
            authorized: HashMap::new(),
        }
    }

    /// `(canonical path, selectionRange start/end)`; `range` when the item
    /// has no selection range.
    fn key(&mut self, node: &Value) -> String {
        let uri = node.get("uri").and_then(Value::as_str).unwrap_or_default();
        let path = self
            .canonical
            .entry(uri.to_owned())
            .or_insert_with(|| {
                let decoded = decode_uri_path(uri).unwrap_or_else(|_| uri.to_owned());
                std::fs::canonicalize(&decoded)
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or(decoded)
            })
            .clone();
        let range = node.get("selectionRange").or_else(|| node.get("range"));
        let point = |pointer: &str| {
            range
                .and_then(|range| range.pointer(pointer))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        format!(
            "{path}\u{0}{}:{}-{}:{}",
            point("/start/line"),
            point("/start/character"),
            point("/end/line"),
            point("/end/character")
        )
    }

    fn authorized(&mut self, result: &Value) -> bool {
        let uri = result
            .pointer("/from/uri")
            .or_else(|| result.pointer("/to/uri"))
            .or_else(|| result.get("uri"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some(known) = self.authorized.get(&uri) {
            return *known;
        }
        let allowed = item_uri_is_authorized(result, self.paths);
        self.authorized.insert(uri, allowed);
        allowed
    }
}

/// Breadth-first walk from `roots` up to `depth` levels (clamped to
/// [`MAX_HIERARCHY_DEPTH`]). Cancellation is checked before every level and
/// while each level's requests are in flight.
#[cfg(test)]
pub(super) async fn walk_hierarchy(
    source: &impl HierarchySource,
    roots: &[Value],
    expansion: Expansion,
    depth: u32,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<HierarchyWalk, LspFailure> {
    Walk::new(paths, roots, expansion, depth)
        .finish(source, cancel)
        .await
}

/// A breadth-first walk in progress, one level per [`Walk::step`]. Roots
/// found after level 1 (verified importer call sites) join level 1 through
/// [`Walk::add_roots`], so the server's own level-1 answer can decide which
/// importers still need verifying.
pub(super) struct Walk<'a> {
    walker: Walker<'a>,
    expansion: Expansion,
    /// The nodes the next level expands.
    frontier: Vec<Value>,
    /// The level the next step expands.
    level: u32,
}

impl<'a> Walk<'a> {
    pub(super) fn new(
        paths: &'a PathPolicy,
        roots: &[Value],
        expansion: Expansion,
        depth: u32,
    ) -> Self {
        let depth = depth.clamp(1, MAX_HIERARCHY_DEPTH);
        Self {
            walker: Walker::new(paths, roots, depth),
            expansion,
            frontier: roots.to_vec(),
            level: 1,
        }
    }

    /// Expand `nodes` at `level`; returns the nodes the next level expands.
    async fn expand_level(
        &mut self,
        source: &impl HierarchySource,
        nodes: Vec<Value>,
        level: u32,
        cancel: &dyn CancellationCheck,
    ) -> Result<Vec<Value>, LspFailure> {
        cancel.check().map_err(LspFailure::cancelled)?;
        let expansion = self.expansion;
        let requests = nodes
            .iter()
            .map(|node| source.expand(expansion, node.clone()));
        let responses = cancellable(
            cancel,
            futures_util::stream::iter(requests)
                .buffered(LEVEL_CONCURRENCY)
                .collect::<Vec<_>>(),
        )
        .await?;
        let mut next = Vec::new();
        for (parent, response) in nodes.iter().zip(responses) {
            match response {
                Ok(results) => {
                    self.walker
                        .expand(expansion, level, parent, as_array(&results), &mut next)
                }
                Err(error) => self.walker.walk.failures.push(error),
            }
        }
        Ok(next)
    }

    /// Expand the next level; `false` once the walk is complete.
    pub(super) async fn step(
        &mut self,
        source: &impl HierarchySource,
        cancel: &dyn CancellationCheck,
    ) -> Result<bool, LspFailure> {
        if self.level > self.walker.depth || self.frontier.is_empty() {
            return Ok(false);
        }
        let nodes = std::mem::take(&mut self.frontier);
        let level = self.level;
        self.frontier = self.expand_level(source, nodes, level, cancel).await?;
        self.level += 1;
        Ok(true)
    }

    /// Expand `roots` not already in the walk at level 1, after level 1 of
    /// the original roots ran; their results join the next level.
    pub(super) async fn add_roots(
        &mut self,
        source: &impl HierarchySource,
        roots: Vec<Value>,
        cancel: &dyn CancellationCheck,
    ) -> Result<(), LspFailure> {
        let fresh = roots
            .into_iter()
            .filter(|root| {
                let key = self.walker.keys.key(root);
                self.walker.seen.insert(key)
            })
            .collect::<Vec<_>>();
        if fresh.is_empty() {
            return Ok(());
        }
        let next = self.expand_level(source, fresh, 1, cancel).await?;
        self.frontier.extend(next);
        Ok(())
    }

    /// Canonical files of the level-1 results: the files the server's own
    /// answer covers.
    pub(super) fn answered_files(&self) -> HashSet<String> {
        self.walker
            .walk
            .edges
            .iter()
            .filter(|edge| edge.level == 1)
            .filter_map(|edge| edge.node.get("uri").and_then(Value::as_str))
            .map(canonical_uri_path)
            .collect()
    }

    /// Run the remaining levels.
    pub(super) async fn finish(
        mut self,
        source: &impl HierarchySource,
        cancel: &dyn CancellationCheck,
    ) -> Result<HierarchyWalk, LspFailure> {
        while self.step(source, cancel).await? {}
        Ok(self.walker.walk)
    }
}

/// The state of one walk: discovered edges, seen nodes, and resume points.
struct Walker<'a> {
    keys: NodeKeys<'a>,
    walk: HierarchyWalk,
    seen: HashSet<String>,
    edge_index: HashMap<(String, String), usize>,
    resumed: HashSet<String>,
    depth: u32,
}

impl<'a> Walker<'a> {
    fn new(paths: &'a PathPolicy, roots: &[Value], depth: u32) -> Self {
        let mut keys = NodeKeys::new(paths);
        let seen = roots.iter().map(|root| keys.key(root)).collect();
        Self {
            keys,
            walk: HierarchyWalk::default(),
            seen,
            edge_index: HashMap::new(),
            resumed: HashSet::new(),
            depth,
        }
    }

    /// Record `parent` as a resume point, once.
    fn resume(&mut self, key: String, node: Value, depth: u32) {
        if self.resumed.insert(key) {
            self.walk.resumes.push(Resume { node, depth });
        }
    }

    /// Record one parent's `results` at `level`; new nodes to expand next go
    /// to `next`.
    fn expand(
        &mut self,
        expansion: Expansion,
        level: u32,
        parent: &Value,
        results: Vec<Value>,
        next: &mut Vec<Value>,
    ) {
        let parent_key = self.keys.key(parent);
        // The anchor's results are all listed (the row pages them).
        let fan_out = if level == 1 {
            usize::MAX
        } else {
            MAX_HIERARCHY_FAN_OUT
        };
        if results.len() > fan_out {
            self.walk.fan_out_capped.push(
                parent
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_owned(),
            );
            self.resume(parent_key.clone(), parent.clone(), self.depth - level + 1);
        }
        for result in results.into_iter().take(fan_out) {
            self.record(expansion, level, parent, &parent_key, &result, next);
        }
    }

    fn record(
        &mut self,
        expansion: Expansion,
        level: u32,
        parent: &Value,
        parent_key: &str,
        result: &Value,
        next: &mut Vec<Value>,
    ) {
        let Some(node) = expansion.node_of(result).cloned() else {
            return;
        };
        if is_builtin_lib_declaration(&node) {
            let name = builtin_name(&node);
            if !self.walk.builtin_lib.contains(&name) {
                self.walk.builtin_lib.push(name);
            }
            return;
        }
        if !self.keys.authorized(result) {
            self.walk.out_of_policy += 1;
            return;
        }
        let node_key = self.keys.key(&node);
        let sites = result
            .get("fromRanges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let pair = (parent_key.to_owned(), node_key.clone());
        if let Some(&index) = self.edge_index.get(&pair) {
            let edge = &mut self.walk.edges[index];
            for site in sites {
                if !edge.sites.contains(&site) {
                    edge.sites.push(site);
                }
            }
            return;
        }
        let is_new = !self.seen.contains(&node_key);
        let full = self.seen.len() >= MAX_HIERARCHY_NODES;
        if is_new && full && level > 1 {
            self.walk.dropped_edges += 1;
            self.resume(
                parent_key.to_owned(),
                parent.clone(),
                self.depth - level + 1,
            );
            return;
        }
        self.edge_index.insert(pair, self.walk.edges.len());
        self.walk.edges.push(HierarchyEdge {
            node: node.clone(),
            parent: Some(parent.clone()),
            level,
            sites,
        });
        if !is_new {
            return;
        }
        self.seen.insert(node_key.clone());
        if level >= self.depth {
            return;
        }
        if full {
            // An anchor result past the node cap: listed, expanded by its own
            // continuation.
            self.walk.unexpanded_nodes += 1;
            self.resume(node_key, node, self.depth - level);
        } else {
            next.push(node);
        }
    }
}

/// Public hierarchy node (`CallHierarchyItem`/`TypeHierarchyItem`): named
/// kind, `uri`, and a one-based `displayRange` that starts at the symbol
/// name (usable as `lineHint`) and ends with the full declaration. Opaque
/// server `data` and empty `detail` are dropped.
pub(super) fn public_hierarchy_node(node: &Value) -> Value {
    let mut public = serde_json::Map::new();
    if let Some(name) = node.get("name") {
        public.insert("name".into(), name.clone());
    }
    public.insert("kind".into(), json!(symbol_kind_name(node.get("kind"))));
    if let Some(detail) = node
        .get("detail")
        .and_then(Value::as_str)
        .filter(|detail| !detail.is_empty())
    {
        public.insert("detail".into(), json!(detail));
    }
    if let Some(uri) = node.get("uri").and_then(Value::as_str) {
        public.insert("path".into(), json!(uri_to_path(uri)));
    }
    let anchor = node.get("selectionRange").or_else(|| node.get("range"));
    if let Some(mut display) = anchor.and_then(public_range) {
        if let Some(end) = node.pointer("/range/end/line").and_then(Value::as_u64) {
            display["endLine"] = json!(end + 1);
        }
        public.insert("displayRange".into(), display);
    }
    Value::Object(public)
}

/// `via`: name and one-based `line`/`character` of the parent node an edge
/// connects to (its symbol name, like `displayRange.start*`).
fn public_via(node: &Value) -> Value {
    let anchor = node.get("selectionRange").or_else(|| node.get("range"));
    let point = |field: &str| {
        anchor
            .and_then(|range| range.pointer(&format!("/start/{field}")))
            .and_then(Value::as_u64)
            .map(|value| value + 1)
    };
    json!({
        "name": node.get("name"),
        "path": node.get("uri").and_then(Value::as_str).map(uri_to_path),
        "line": point("line"),
        "character": point("character")
    })
}

pub(super) fn public_edge(expansion: Expansion, edge: &HierarchyEdge) -> Value {
    let mut public = if expansion.is_call() {
        let key = if expansion == Expansion::IncomingCalls {
            "from"
        } else {
            "to"
        };
        // Call-site ranges: in the caller's file for incoming calls, in the
        // expanded (parent or anchor) node's file for outgoing calls.
        let mut ranges = edge
            .sites
            .iter()
            .filter_map(public_range)
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| {
            (
                range["startLine"].as_u64().unwrap_or(0),
                range["startCharacter"].as_u64().unwrap_or(0),
                range["endLine"].as_u64().unwrap_or(0),
            )
        });
        // Servers can report one call site as several raw ranges that differ
        // only in their end column; the public range drops that column, so
        // identical public sites are one call.
        ranges.dedup();
        let mut call = serde_json::Map::new();
        let node = (expansion == Expansion::IncomingCalls)
            .then(|| narrowed_class_caller(&edge.node, &edge.sites))
            .flatten();
        call.insert(
            key.into(),
            public_hierarchy_node(node.as_ref().unwrap_or(&edge.node)),
        );
        call.insert("fromRanges".into(), json!(ranges));
        // An outgoing call site lies in the expanded node's file; the
        // compact row is filed there.
        if expansion == Expansion::OutgoingCalls
            && let Some(uri) = edge
                .parent
                .as_ref()
                .and_then(|parent| parent.get("uri"))
                .and_then(Value::as_str)
        {
            call.insert(CALLER_PATH.into(), json!(uri_to_path(uri)));
        }
        Value::Object(call)
    } else {
        public_hierarchy_node(&edge.node)
    };
    public["level"] = json!(edge.level);
    if edge.level > 1
        && let Some(parent) = &edge.parent
    {
        public["via"] = public_via(parent);
    }
    public
}

/// LSP `SymbolKind`s of a whole file or module caller.
const MODULE_KINDS: [u64; 2] = [1, 2];

/// A caller the server reports as a whole module or file (tsserver does for
/// calls inside a top-level callback: `describe(() => …)`, `it(…)`) split
/// into one edge per innermost function symbol of `symbols` (the file's
/// `documentSymbol` answer) around its call sites, the way reference-derived
/// callers are named. Sites outside every function stay with the module.
/// Any other edge is returned unchanged.
pub(super) fn split_module_callers(edge: &HierarchyEdge, symbols: &Value) -> Vec<HierarchyEdge> {
    let unchanged = || HierarchyEdge {
        node: edge.node.clone(),
        parent: edge.parent.clone(),
        level: edge.level,
        sites: edge.sites.clone(),
    };
    if !edge
        .node
        .get("kind")
        .and_then(Value::as_u64)
        .is_some_and(|kind| MODULE_KINDS.contains(&kind))
    {
        return vec![unchanged()];
    }
    let mut flat = Vec::new();
    flatten_symbols(symbols, &mut flat);
    let mut split: Vec<HierarchyEdge> = Vec::new();
    for site in &edge.sites {
        let point = |field: &str| {
            site.pointer(&format!("/start/{field}"))
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
        };
        let node = enclosing_callable(&flat, point("line"), point("character")).map_or_else(
            || edge.node.clone(),
            |symbol| {
                // Callback names quote their call (`it("…") callback`);
                // a multi-line template keeps one line.
                let name = symbol["name"].as_str().map_or(Value::Null, |name| {
                    json!(name.split_whitespace().collect::<Vec<_>>().join(" "))
                });
                json!({
                    "name": name,
                    "kind": symbol["kind"],
                    "uri": edge.node.get("uri"),
                    "range": symbol["range"],
                    "selectionRange": symbol["selectionRange"],
                })
            },
        );
        match split.iter_mut().find(|known| {
            known.node.get("selectionRange") == node.get("selectionRange")
                && known.node.get("name") == node.get("name")
        }) {
            Some(known) => known.sites.push(site.clone()),
            None => split.push(HierarchyEdge {
                node,
                parent: edge.parent.clone(),
                level: edge.level,
                sites: vec![site.clone()],
            }),
        }
    }
    if split.is_empty() {
        split.push(unchanged());
    }
    split
}

/// [`split_module_callers`] over a walk's incoming edges, with one
/// `documentSymbol` request per module file (a failed request keeps the
/// module caller).
async fn name_module_callers(
    client: &NativeLspClient,
    edges: Vec<HierarchyEdge>,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<HierarchyEdge>, LspFailure> {
    let mut symbols: HashMap<String, Value> = HashMap::new();
    let mut named = Vec::with_capacity(edges.len());
    for edge in edges {
        let module = edge
            .node
            .get("kind")
            .and_then(Value::as_u64)
            .is_some_and(|kind| MODULE_KINDS.contains(&kind));
        let Some(uri) = edge
            .node
            .get("uri")
            .and_then(Value::as_str)
            .filter(|_| module && !edge.sites.is_empty())
        else {
            named.push(edge);
            continue;
        };
        if !symbols.contains_key(uri) {
            let found = cancellable(cancel, client.get_document_symbols(uri_to_path(uri)))
                .await?
                .unwrap_or(Value::Null);
            symbols.insert(uri.to_owned(), found);
        }
        named.extend(split_module_callers(&edge, &symbols[uri]));
    }
    Ok(named)
}

/// Largest source read to narrow a class caller.
const MAX_NARROW_SOURCE_BYTES: u64 = 4 * 1024 * 1024;

/// A caller the server reports as a whole class (tsserver does for calls in
/// a constructor or a field initializer) shown as the innermost member that
/// holds every call site, so its range and read lead fit the call. The walk
/// itself keeps the server's item. `None` when nothing narrower holds them.
pub(super) fn narrowed_class_caller(node: &Value, sites: &[Value]) -> Option<Value> {
    if node.get("kind").and_then(Value::as_u64) != Some(5) {
        return None;
    }
    let site_lines = sites
        .iter()
        .filter_map(|site| site.pointer("/start/line").and_then(Value::as_u64))
        .collect::<Vec<_>>();
    let (first, last) = (*site_lines.iter().min()?, *site_lines.iter().max()?);
    let path = decode_uri_path(node.get("uri")?.as_str()?).ok()?;
    if std::fs::metadata(&path).ok()?.len() > MAX_NARROW_SOURCE_BYTES {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let facts: Value = serde_json::from_str(&octocode_engine::portable::extract_declarations(
        &content, &path,
    )?)
    .ok()?;
    let line = |value: &Value, pointer: &str| value.pointer(pointer).and_then(Value::as_u64);
    let class = (
        line(node, "/range/start/line")?,
        line(node, "/range/end/line")?,
    );
    let member = facts["declarations"]
        .as_array()?
        .iter()
        .filter_map(|declaration| {
            let kind = match declaration["kind"].as_str()? {
                "constructor" => 9,
                "method" => 6,
                "property" => 7,
                "function" => 12,
                _ => return None,
            };
            let span = (
                line(declaration, "/range/start/line")?,
                line(declaration, "/range/end/line")?,
            );
            (span != class
                && class.0 <= span.0
                && span.1 <= class.1
                && span.0 <= first
                && last <= span.1)
                .then_some((span.1 - span.0, kind, declaration))
        })
        .min_by_key(|(size, _, _)| *size)?;
    let (_, kind, declaration) = member;
    Some(json!({
        "name": declaration["name"],
        "kind": kind,
        "detail": node.get("name"),
        "uri": node.get("uri"),
        "range": declaration["range"],
        "selectionRange": declaration.get("selectionRange").unwrap_or(&declaration["range"]),
    }))
}

/// Internal key of a compacted outgoing call: the file its sites lie in.
const CALLER_PATH: &str = "callerPath";

/// The file a hierarchy node or `via` names.
fn node_path(node: &Value) -> String {
    node.get("path")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| node.get("uri").and_then(Value::as_str).map(uri_to_path))
        .unwrap_or_else(|| "unknown".to_owned())
}

/// `target` as the response spells row paths: relative to the workspace
/// root when inside it, absolute otherwise.
fn display_path(paths: &PathPolicy, target: &str) -> String {
    paths
        .workspace_relative(target)
        .unwrap_or_else(|| target.to_owned())
}

/// Declaration words a signature detail may repeat around the name.
const DETAIL_KEYWORDS: &[&str] = &[
    "pub", "crate", "super", "async", "unsafe", "const", "extern", "static", "fn", "function",
    "def", "func", "export", "default",
];

/// Whether a call-hierarchy `detail` says more than the row already does:
/// a container name, parameters, or a return type. A detail that is only
/// the name, its kind, and declaration words (`fn walk()`) adds nothing.
fn detail_adds(detail: &str, node: &Value) -> bool {
    let name = node.get("name").and_then(Value::as_str).unwrap_or_default();
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or_default();
    detail
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|token| !token.is_empty())
        .any(|token| token != name && token != kind && !DETAIL_KEYWORDS.contains(&token))
}

/// A page of call edges (`callers`, `callees`, `callHierarchy`, any depth)
/// as per-file rows
/// `{path, matches: ["<line>:<col>[,…] in|to <kind> <name>[ (<detail>)] [<file>:]<line>-<endLine>[ via <name>@<line>]"]}`,
/// filed under the file the call sites lie in: an `in` row is a caller
/// declared in that file; a `to` row is a callee called from that file (the
/// anchor, or the `via` node), with the callee's file (workspace-relative,
/// like row paths) before its range when they differ. Incoming and outgoing rows get
/// separate entries. The declaration range starts on its name line, a
/// `lineHint` for the next hop. An edge below level 1 names its parent node
/// as `via <name>@<line>`; when another listed node shares that name and
/// line, the parent's workspace-relative file is added
/// (`via <name>@<path>:<line>`). Callers recovered from references are
/// listed by label under `recovered` with their first call line. `all` is
/// every edge of the walk (not only this page), for that disambiguation.
pub(super) fn compact_calls(row: &mut Value, all: &[Value], paths: &PathPolicy) {
    let Some(items) = row.pointer("/payload/matches").and_then(Value::as_array) else {
        return;
    };
    if items.is_empty() || items.iter().any(|item| call_node(item).is_none()) {
        return;
    }
    let declared = declaring_files(all.iter().chain(items));
    let mut files: Vec<(CallGroup, serde_json::Map<String, Value>)> = Vec::new();
    for item in items {
        let Some((key, node)) = call_node(item) else {
            continue;
        };
        let declared_in = node_path(node);
        // Call sites lie in the caller: the listed node for incoming calls,
        // the expanded node for outgoing ones.
        let path = match key {
            "to" => item
                .get(CALLER_PATH)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| item.get("via").filter(|via| via.is_object()).map(node_path))
                .unwrap_or_else(|| declared_in.clone()),
            _ => declared_in.clone(),
        };
        let sites = call_sites(item);
        let text = call_text(
            key,
            node,
            item,
            &sites,
            (&path, &declared_in),
            &declared,
            paths,
        );
        // One entry per (direction, file): a callHierarchy lists its
        // incoming rows and its outgoing rows apart.
        let group = (key, path);
        let index = match files.iter().position(|(seen, _)| *seen == group) {
            Some(index) => index,
            None => {
                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), json!(group.1));
                entry.insert("matches".into(), json!([]));
                files.push((group, entry));
                files.len() - 1
            }
        };
        push_call(&mut files[index].1, text, item, &sites);
    }
    if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
        payload.remove("matches");
        payload.insert(
            "files".into(),
            Value::Array(
                files
                    .into_iter()
                    .map(|(_, entry)| Value::Object(entry))
                    .collect(),
            ),
        );
    }
}

/// A compact file entry's identity: direction key and file.
type CallGroup = (&'static str, String);

/// The direction key (`from` or `to`) and node of a call item.
fn call_node(item: &Value) -> Option<(&'static str, &Value)> {
    ["from", "to"].into_iter().find_map(|key| {
        item.get(key)
            .filter(|node| node.is_object())
            .map(|node| (key, node))
    })
}

/// Files declaring each listed (name, line), over the whole walk.
fn declaring_files<'a>(
    items: impl Iterator<Item = &'a Value>,
) -> HashMap<(String, u64), HashSet<String>> {
    let mut declared: HashMap<(String, u64), HashSet<String>> = HashMap::new();
    for item in items {
        if let Some((_, node)) = call_node(item)
            && let (Some(name), Some(line)) = (
                node.get("name").and_then(Value::as_str),
                node.pointer("/displayRange/startLine")
                    .and_then(Value::as_u64),
            )
        {
            declared
                .entry((name.to_owned(), line))
                .or_default()
                .insert(node_path(node));
        }
    }
    declared
}

/// One-based `(line, column)` call sites of an item.
fn call_sites(item: &Value) -> Vec<(u64, Option<u64>)> {
    item.get("fromRanges")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|range| {
            let line = range.get("startLine")?.as_u64()?;
            let column = range.get("startCharacter").and_then(Value::as_u64);
            Some((line, column))
        })
        .collect()
}

/// The compact text of one call row filed under `path`.
fn call_text(
    key: &str,
    node: &Value,
    item: &Value,
    sites: &[(u64, Option<u64>)],
    (path, declared_in): (&str, &str),
    declared: &HashMap<(String, u64), HashSet<String>>,
    paths: &PathPolicy,
) -> String {
    let mut text = sites
        .iter()
        .map(|(line, column)| match column {
            Some(column) => format!("{line}:{column}"),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(if key == "from" { "in " } else { "to " });
    text.push_str(node.get("kind").and_then(Value::as_str).unwrap_or("symbol"));
    text.push(' ');
    text.push_str(node.get("name").and_then(Value::as_str).unwrap_or_default());
    // A container name (`App`) always helps; a signature only tells apart
    // listed nodes that share a name (the read lead shows it otherwise).
    let name = node.get("name").and_then(Value::as_str).unwrap_or_default();
    let shared_name = declared
        .iter()
        .filter(|((listed, _), _)| listed == name)
        .map(|(_, files)| files.len())
        .sum::<usize>()
        > 1;
    if let Some(detail) = node
        .get("detail")
        .and_then(Value::as_str)
        .filter(|detail| detail_adds(detail, node))
        .filter(|detail| shared_name || !detail.contains('('))
    {
        // One line per call row: a multi-line signature's whitespace runs
        // collapse to single spaces.
        let detail = detail.split_whitespace().collect::<Vec<_>>().join(" ");
        text.push_str(&format!(" ({detail})"));
    }
    if let Some(range) = node.get("displayRange")
        && let Some(start) = range.get("startLine").and_then(Value::as_u64)
    {
        // A callee declared in another file names it, so its range is a
        // `lineHint` for the next hop.
        let file = if declared_in == path {
            String::new()
        } else {
            format!("{}:", display_path(paths, declared_in))
        };
        match range.get("endLine").and_then(Value::as_u64) {
            Some(end) if end != start => text.push_str(&format!(" {file}{start}-{end}")),
            _ => text.push_str(&format!(" {file}{start}")),
        }
    }
    if let Some(via) = item.get("via").filter(|via| via.is_object()) {
        let name = via.get("name").and_then(Value::as_str).unwrap_or_default();
        let line = via.get("line").and_then(Value::as_u64).unwrap_or(0);
        let ambiguous = declared
            .get(&(name.to_owned(), line))
            .is_some_and(|files| files.len() > 1);
        if ambiguous {
            let file = display_path(paths, &node_path(via));
            text.push_str(&format!(" via {name}@{file}:{line}"));
        } else {
            text.push_str(&format!(" via {name}@{line}"));
        }
    }
    text
}

/// Append a call row to its file entry; a recovered caller is also listed
/// under its label with its first call line.
fn push_call(
    entry: &mut serde_json::Map<String, Value>,
    text: String,
    item: &Value,
    sites: &[(u64, Option<u64>)],
) {
    if let Some(calls) = entry.get_mut("matches").and_then(Value::as_array_mut) {
        calls.push(json!(text));
    }
    if let Some(label) = item.get("source").and_then(Value::as_str)
        && let Some(lines) = entry
            .entry("recovered")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .and_then(|recovered| {
                recovered
                    .entry(label)
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
            })
    {
        lines.push(json!(sites.first().map_or(0, |site| site.0)));
    }
}

/// Combine what a hierarchy expansion found with the provider failures it hit.
/// A failure with nothing found is an error (never a silent empty answer); a
/// failure after some results is a partial answer the caller must mark.
pub(super) fn expansion_outcome(
    items: Vec<Value>,
    failures: Vec<EngineError>,
) -> Result<(Vec<Value>, Vec<EngineError>), LspFailure> {
    if items.is_empty()
        && let Some(first) = failures.first()
    {
        return Err(LspFailure::from_engine(first));
    }
    Ok((items, failures))
}

pub(super) fn mark_partial_expansion(
    row: &mut Value,
    query: &LspSearchQuery,
    failures: &[EngineError],
) {
    if failures.is_empty() {
        return;
    }
    let reason = if matches!(query.operation().as_str(), "supertypes" | "subtypes") {
        "typeHierarchyExpansionFailed"
    } else {
        "callHierarchyExpansionFailed"
    };
    // Every distinct failure, once, with how many items it hit.
    let mut distinct: Vec<(String, usize)> = Vec::new();
    for failure in failures {
        let message = failure.to_string();
        match distinct.iter_mut().find(|(seen, _)| *seen == message) {
            Some((_, count)) => *count += 1,
            None => distinct.push((message, 1)),
        }
    }
    // The disclosure is one count; the error list is verbose (core field
    // class `expansionFailures`), listed whole under `debug: true`.
    let total = failures.len();
    let items = if total == 1 { "item" } else { "items" };
    let warning = match distinct.as_slice() {
        [(message, _)] => {
            format!(
                "Hierarchy expansion failed for {total} {items}: {message}. next.retry reruns the walk."
            )
        }
        _ => {
            let (message, count) =
                distinct.iter().fold(
                    &distinct[0],
                    |top, entry| if entry.1 > top.1 { entry } else { top },
                );
            format!(
                "Hierarchy expansion failed for {total} {items} ({} distinct errors; most frequent, {count}×: {message}). next.retry reruns the walk; debug:true lists every error.",
                distinct.len()
            )
        }
    };
    mark_partial(row, query, reason, &[warning]);
    if let Some(object) = row.as_object_mut() {
        object.insert(
            "expansionFailures".into(),
            json!(
                distinct
                    .iter()
                    .map(|(message, count)| format!("{count}× {message}"))
                    .collect::<Vec<_>>()
            ),
        );
    }
}

/// Truncation diagnostics over every direction a hierarchy request walked
/// (`callHierarchy` walks incoming and outgoing calls; the other operations
/// walk one direction). A node-capped walk gets `payload.truncated`,
/// `payload.unexpandedParents`, an executable `next.continueWalk` that
/// re-anchors on the first parent whose children were dropped, and
/// `next.continueWalk2…N` for every other such parent — one combined list
/// across directions, so neither direction's frontier is lost. With two
/// directions each unexpanded parent carries `direction` (`incoming` or
/// `outgoing`) and its continuation walks only that direction (`callers` or
/// `callees`). A node whose results were cut to [`MAX_HIERARCHY_FAN_OUT`]
/// resumes the same way: re-anchored, it lists every result.
pub(super) fn mark_truncation(
    row: &mut Value,
    query: &LspSearchQuery,
    walks: &[(Expansion, &HierarchyWalk)],
) {
    if row.get("status").and_then(Value::as_str) == Some("error") {
        return;
    }
    warn_omitted(row, walks);
    let tagged = walks.len() > 1;
    let resumes = walks
        .iter()
        .flat_map(|(expansion, walk)| walk.resumes.iter().map(move |resume| (*expansion, resume)))
        .collect::<Vec<_>>();
    let Some((_, first)) = resumes.first() else {
        return;
    };
    let fan_out_capped = walks
        .iter()
        .map(|(_, walk)| walk.fan_out_capped.len())
        .sum::<usize>();
    row["payload"]["truncated"] = json!(true);
    // Every unexpanded or capped parent is listed and resumable.
    row["payload"]["unexpandedParents"] = json!(
        resumes
            .iter()
            .map(|(expansion, resume)| {
                let mut parent = public_via(&resume.node);
                parent["remainingDepth"] = json!(resume.depth);
                if tagged && let Some(direction) = expansion.direction() {
                    parent["direction"] = json!(direction);
                }
                parent
            })
            .collect::<Vec<_>>()
    );
    let queries = resumes
        .iter()
        .map(|(expansion, resume)| {
            resume_query(query, resume, tagged.then(|| expansion.operation()))
        })
        .collect::<Option<Vec<_>>>();
    let follow = format!(
        "Run next.continueWalk to walk on from {}{}.",
        first
            .node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("the first unexpanded node"),
        if resumes.len() > 1 {
            ", then next.continueWalk2… for the rest"
        } else {
            ""
        }
    );
    let limits = limit_warnings(walks, fan_out_capped, &follow);
    match queries {
        Some(queries) => {
            for (reason, warning) in limits {
                push_reason(row, reason, &[warning]);
            }
            // One flat continuation per parent, so the whole frontier is
            // executable: `continueWalk`, `continueWalk2`, ….
            for (index, query) in queries.into_iter().enumerate() {
                let key = match index {
                    0 => "continueWalk".to_owned(),
                    _ => format!("continueWalk{}", index + 1),
                };
                row["next"][key] = continuation(query);
            }
        }
        None => {
            for (reason, warning) in limits {
                mark_terminal_limit(row, reason, &[warning]);
            }
        }
    }
}

/// A warning for results outside the read policy; TypeScript built-in
/// library callees are listed by name under `payload.builtinLib` (tiny, and
/// never worth a second call).
fn warn_omitted(row: &mut Value, walks: &[(Expansion, &HierarchyWalk)]) {
    let out_of_policy = walks
        .iter()
        .map(|(_, walk)| walk.out_of_policy)
        .sum::<usize>();
    let mut builtin_lib: Vec<&String> = Vec::new();
    for name in walks.iter().flat_map(|(_, walk)| &walk.builtin_lib) {
        if !builtin_lib.contains(&name) {
            builtin_lib.push(name);
        }
    }
    let mut push = |warning: String| {
        if let Some(warnings) = row
            .as_object_mut()
            .map(|object| object.entry("warnings").or_insert_with(|| json!([])))
            .and_then(Value::as_array_mut)
        {
            warnings.push(json!(warning));
        }
    };
    if out_of_policy > 0 {
        push(format!(
            "{out_of_policy} hierarchy items outside the allowed read roots were omitted."
        ));
    }
    if !builtin_lib.is_empty()
        && let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut)
    {
        payload.insert("builtinLib".into(), json!(builtin_lib));
    }
}

/// `(partial reason, warning)` for each cap a walk hit.
fn limit_warnings(
    walks: &[(Expansion, &HierarchyWalk)],
    fan_out_capped: usize,
    follow: &str,
) -> Vec<(&'static str, String)> {
    let dropped_edges = walks
        .iter()
        .map(|(_, walk)| walk.dropped_edges)
        .sum::<usize>();
    let unexpanded_nodes = walks
        .iter()
        .map(|(_, walk)| walk.unexpanded_nodes)
        .sum::<usize>();
    let mut limits = Vec::new();
    if dropped_edges > 0 || unexpanded_nodes > 0 {
        limits.push((
            "hierarchyNodeLimit",
            format!(
                "Stopped at {MAX_HIERARCHY_NODES} hierarchy nodes; {dropped_edges} edges to further nodes were not listed and {unexpanded_nodes} listed results were not expanded (payload.unexpandedParents). {follow}"
            ),
        ));
    }
    // `payload.unexpandedParents` names every capped node once; the warning
    // carries the count.
    if fan_out_capped > 0 {
        limits.push((
            "hierarchyFanOutLimit",
            format!(
                "Kept the first {MAX_HIERARCHY_FAN_OUT} results of {fan_out_capped} node(s) below the anchor (payload.unexpandedParents); each resumes as the anchor of its continuation, which lists every result. {follow}"
            ),
        ));
    }
    limits
}

/// An executable query that walks `resume.depth` levels from `resume.node`:
/// the same operation (or `operation`, to walk one direction only)
/// re-anchored at the node's zero-based selection start.
fn resume_query(query: &LspSearchQuery, resume: &Resume, operation: Option<&str>) -> Option<Value> {
    let uri = resume.node.get("uri").and_then(Value::as_str)?;
    let anchor = resume
        .node
        .get("selectionRange")
        .or_else(|| resume.node.get("range"))?;
    let line = u32::try_from(anchor.pointer("/start/line")?.as_u64()?).ok()?;
    let character = u32::try_from(anchor.pointer("/start/character")?.as_u64()?).ok()?;
    // Re-anchoring changes the query's shape (symbol anchor to position),
    // so the continuation is edited as a row.
    let mut next = query.to_row();
    let object = next.as_object_mut()?;
    if let Some(operation) = operation {
        object.insert("operation".into(), json!(operation));
    }
    object.insert("path".into(), json!(uri_to_path(uri)));
    object.insert(
        "position".into(),
        json!({"line": line, "character": character}),
    );
    for field in ["symbolName", "lineHint", "orderHint", "page", "snapshot"] {
        object.remove(field);
    }
    object.insert("depth".into(), json!(resume.depth));
    Some(next)
}

/// Label for caller edges derived from verified importer references.
const RECOVERED_FROM_REFERENCES: &str = "recoveredFromReferences";

fn canonical_uri_path(uri: &str) -> String {
    let decoded = decode_uri_path(uri).unwrap_or_else(|_| uri.to_owned());
    std::fs::canonicalize(&decoded)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or(decoded)
}

/// The walk's authorized, deduplicated roots, prepared at the anchor, and
/// the directions to expand.
async fn hierarchy_roots(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    paths: &PathPolicy,
    (path, line, character): (&str, u32, u32),
    cancel: &dyn CancellationCheck,
) -> Result<(Vec<Value>, &'static [Expansion]), LspFailure> {
    let (prepared, expansions): (Value, &[Expansion]) = match query.operation().as_str() {
        "callers" | "callees" | "callHierarchy" => (
            cancellable(
                cancel,
                client.prepare_call_hierarchy(path.to_owned(), line, character),
            )
            .await??,
            match query.operation().as_str() {
                "callers" => &[Expansion::IncomingCalls],
                "callees" => &[Expansion::OutgoingCalls],
                _ => &[Expansion::IncomingCalls, Expansion::OutgoingCalls],
            },
        ),
        _ => (
            cancellable(
                cancel,
                client.prepare_type_hierarchy(path.to_owned(), line, character),
            )
            .await??,
            if query.operation() == "supertypes" {
                &[Expansion::Supertypes]
            } else {
                &[Expansion::Subtypes]
            },
        ),
    };
    let mut keys = NodeKeys::new(paths);
    let mut root_keys = HashSet::new();
    let roots = as_array(&prepared)
        .into_iter()
        .filter(|root| item_uri_is_authorized(root, paths))
        .filter(|root| root_keys.insert(keys.key(root)))
        .collect::<Vec<_>>();
    Ok((roots, expansions))
}

/// Call-hierarchy items prepared at verified importer call sites (TS/JS
/// recovery); an item prepared there resolves in the importer's program.
async fn importer_roots(
    client: &NativeLspClient,
    paths: &PathPolicy,
    importers: &Importers,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<Value>, LspFailure> {
    let mut roots = Vec::new();
    for anchor in importers.call_sites() {
        if let Ok(more) = cancellable(
            cancel,
            client.prepare_call_hierarchy(anchor.path.clone(), anchor.line, anchor.character),
        )
        .await?
        {
            roots.extend(
                as_array(&more)
                    .into_iter()
                    .filter(|root| item_uri_is_authorized(root, paths)),
            );
        }
    }
    Ok(roots)
}

/// Importer recovery for an incoming walk: the symbol whose importers are
/// verified, and the policy for the snippets the checks read.
pub(super) struct ImporterRecovery<'a> {
    pub(super) symbol: String,
    pub(super) snippet_policy: &'a SnippetReadPolicy,
}

/// A call or type hierarchy walk from the anchor. With `recovery`, an
/// incoming walk first takes the server's level-1 answer, then verifies
/// only the importer files that answer does not cover (the same contract as
/// `references`), roots the walk at their call sites too, and derives
/// callers from their references where the call hierarchy cannot cross the
/// importer's binding. The files the answer covers are recorded on `scope`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn hierarchy(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    sources: &mut SourceCache<'_>,
    scope: &Scope,
    (path, line, character): (&str, u32, u32),
    recovery: Option<ImporterRecovery<'_>>,
    cancel: &dyn CancellationCheck,
) -> Result<(Value, Option<Importers>), LspFailure> {
    let paths = sources.policy();
    let (roots, expansions) =
        hierarchy_roots(client, query, paths, (path, line, character), cancel).await?;
    let depth = query.depth().unwrap_or(1);
    let mut items = Vec::new();
    let mut failures = Vec::new();
    let mut walks = Vec::new();
    let mut verified = None;
    for &expansion in expansions {
        let mut walk = Walk::new(paths, &roots, expansion, depth);
        walk.step(client, cancel).await?;
        let mut derived_items = Vec::new();
        if expansion == Expansion::IncomingCalls
            && let Some(recovery) = &recovery
        {
            let mut answered = walk.answered_files();
            answered.insert(canonical_uri_path(path));
            let importers = verified_anchors(
                client,
                sources,
                recovery.snippet_policy,
                cancel,
                &recovery.symbol,
                scope,
                path,
                line,
                character,
                &answered,
            )
            .await?;
            let extra = importer_roots(client, paths, &importers, cancel).await?;
            walk.add_roots(client, extra, cancel).await?;
            // Reference-derived callers fill in only files the server's
            // call hierarchy did not answer for at level 1.
            let derived = callers_from_references(
                client,
                sources,
                cancel,
                &importers,
                &walk.answered_files(),
            )
            .await?;
            for (node, sites) in derived {
                let edge = HierarchyEdge {
                    node,
                    parent: None,
                    level: 1,
                    sites,
                };
                let mut item = public_edge(expansion, &edge);
                item["source"] = json!(RECOVERED_FROM_REFERENCES);
                derived_items.push(item);
            }
            verified = Some(importers);
        }
        let mut walk = walk.finish(client, cancel).await?;
        if expansion == Expansion::IncomingCalls {
            walk.edges =
                name_module_callers(client, std::mem::take(&mut walk.edges), cancel).await?;
        }
        items.extend(walk.edges.iter().map(|edge| public_edge(expansion, edge)));
        items.extend(derived_items);
        walks.push((expansion, walk));
    }
    scope.answer(answer_files(path, &items, verified.as_ref()));
    for (_, walk) in &mut walks {
        failures.append(&mut walk.failures);
    }
    let (items, failures) = expansion_outcome(items, failures)?;
    let mut row = items_payload(query, query.operation().as_str(), json!(items.clone()));
    compact_calls(&mut row, &items, paths);
    mark_partial_expansion(&mut row, query, &failures);
    let walks = walks
        .iter()
        .map(|(expansion, walk)| (*expansion, walk))
        .collect::<Vec<_>>();
    // Both directions of a `callHierarchy` are marked together: marking
    // them one after the other would let the second overwrite the first's
    // `unexpandedParents` and `continueWalk*`.
    mark_truncation(&mut row, query, &walks);
    Ok((row, verified))
}

/// Every file a walk's answer covers: the anchor, each listed node and call
/// site file, the verified importers, and the candidates the server showed
/// to name another declaration.
fn answer_files(anchor: &str, items: &[Value], importers: Option<&Importers>) -> Vec<String> {
    let mut files = vec![anchor.to_owned()];
    for item in items {
        for pointer in [
            "/from/path",
            "/to/path",
            "/path",
            &format!("/{CALLER_PATH}"),
        ] {
            if let Some(path) = item.pointer(pointer).and_then(Value::as_str) {
                files.push(path.to_owned());
            }
        }
    }
    if let Some(importers) = importers {
        files.extend(importers.per_file().map(|anchor| anchor.path.clone()));
        files.extend(importers.rejected.iter().cloned());
    }
    files
}
