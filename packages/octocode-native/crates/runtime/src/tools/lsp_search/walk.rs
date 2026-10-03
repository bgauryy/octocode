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
use super::locations::{items_payload, public_range};
use super::render::{as_array, decode_uri_path, symbol_kind_name, uri_to_path};
use super::source::item_uri_is_authorized;
use super::{LspSearchQuery, cancellable};
use crate::policy::path::PathPolicy;
use crate::tools::cancel::CancellationCheck;
use futures_util::StreamExt;
use octocode_engine::error::Error as EngineError;
use octocode_engine::lsp::client::NativeLspClient;
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
/// `parent` the raw item it was expanded from (`None` for a level-1 edge of
/// the anchor), and `sites` the raw call-site ranges (`fromRanges`).
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
    /// e.g. `String.prototype.toUpperCase`), skipped as call-graph noise.
    pub(super) builtin_lib: usize,
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
pub(super) async fn walk_hierarchy(
    source: &impl HierarchySource,
    roots: &[Value],
    expansion: Expansion,
    depth: u32,
    paths: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<HierarchyWalk, LspFailure> {
    let depth = depth.clamp(1, MAX_HIERARCHY_DEPTH);
    let mut keys = NodeKeys::new(paths);
    let mut walk = HierarchyWalk::default();
    let mut seen = roots
        .iter()
        .map(|root| keys.key(root))
        .collect::<HashSet<_>>();
    let mut edge_index: HashMap<(String, String), usize> = HashMap::new();
    let mut resumed = HashSet::new();
    let mut frontier = roots.to_vec();
    for level in 1..=depth {
        if frontier.is_empty() {
            break;
        }
        cancel.check().map_err(LspFailure::cancelled)?;
        let requests = frontier
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
        for (parent, response) in frontier.into_iter().zip(responses) {
            let results = match response {
                Ok(results) => as_array(&results),
                Err(error) => {
                    walk.failures.push(error);
                    continue;
                }
            };
            let parent_key = keys.key(&parent);
            // The anchor's results are all listed (the row pages them).
            let fan_out = if level == 1 {
                usize::MAX
            } else {
                MAX_HIERARCHY_FAN_OUT
            };
            if results.len() > fan_out {
                walk.fan_out_capped.push(
                    parent
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_owned(),
                );
                if resumed.insert(parent_key.clone()) {
                    walk.resumes.push(Resume {
                        node: parent.clone(),
                        depth: depth - level + 1,
                    });
                }
            }
            for result in results.into_iter().take(fan_out) {
                let Some(node) = expansion.node_of(&result).cloned() else {
                    continue;
                };
                if is_builtin_lib_declaration(&node) {
                    walk.builtin_lib += 1;
                    continue;
                }
                if !keys.authorized(&result) {
                    walk.out_of_policy += 1;
                    continue;
                }
                let node_key = keys.key(&node);
                let sites = result
                    .get("fromRanges")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if let Some(&index) = edge_index.get(&(parent_key.clone(), node_key.clone())) {
                    let edge = &mut walk.edges[index];
                    for site in sites {
                        if !edge.sites.contains(&site) {
                            edge.sites.push(site);
                        }
                    }
                    continue;
                }
                let is_new = !seen.contains(&node_key);
                let full = seen.len() >= MAX_HIERARCHY_NODES;
                if is_new && full && level > 1 {
                    walk.dropped_edges += 1;
                    if resumed.insert(parent_key.clone()) {
                        walk.resumes.push(Resume {
                            node: parent.clone(),
                            depth: depth - level + 1,
                        });
                    }
                    continue;
                }
                edge_index.insert((parent_key.clone(), node_key.clone()), walk.edges.len());
                walk.edges.push(HierarchyEdge {
                    node: node.clone(),
                    parent: (level > 1).then(|| parent.clone()),
                    level,
                    sites,
                });
                if is_new {
                    seen.insert(node_key.clone());
                    if level < depth {
                        if full {
                            // An anchor result past the node cap: listed,
                            // expanded by its own continuation.
                            walk.unexpanded_nodes += 1;
                            if resumed.insert(node_key) {
                                walk.resumes.push(Resume {
                                    node,
                                    depth: depth - level,
                                });
                            }
                        } else {
                            next.push(node);
                        }
                    }
                }
            }
        }
        frontier = next;
    }
    Ok(walk)
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
    if let Some(uri) = node.get("uri") {
        public.insert("uri".into(), uri.clone());
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
        "uri": node.get("uri"),
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
        call.insert(key.into(), public_hierarchy_node(&edge.node));
        call.insert("fromRanges".into(), json!(ranges));
        Value::Object(call)
    } else {
        public_hierarchy_node(&edge.node)
    };
    public["level"] = json!(edge.level);
    if let Some(parent) = &edge.parent {
        public["via"] = public_via(parent);
    }
    public
}

/// A page of direct callers (every edge at level 1) as per-file rows
/// `{path, calls: ["<line>:<col>[,<line>:<col>…] in <kind> <name>[ (<detail>)] <line>-<endLine>"]}`:
/// the one-based call sites in that file, then the calling declaration and
/// its range (its start line is the name line, a `lineHint` for the next
/// hop). Callers recovered from references are listed by label under
/// `recovered` with their first call line. Deeper walks keep `items`, whose
/// `via` links each edge to its parent.
pub(super) fn compact_callers(row: &mut Value) {
    let Some(items) = row.pointer("/payload/items").and_then(Value::as_array) else {
        return;
    };
    if items.is_empty()
        || items.iter().any(|item| {
            item.get("level").and_then(Value::as_u64) != Some(1)
                || item.get("via").is_some()
                || !item.get("from").is_some_and(Value::is_object)
        })
    {
        return;
    }
    let mut files: Vec<(String, serde_json::Map<String, Value>)> = Vec::new();
    for item in items {
        let from = &item["from"];
        let path = from
            .get("uri")
            .and_then(Value::as_str)
            .map(uri_to_path)
            .unwrap_or_else(|| "unknown".to_owned());
        let sites = item
            .get("fromRanges")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|range| {
                let line = range.get("startLine")?.as_u64()?;
                let column = range.get("startCharacter").and_then(Value::as_u64);
                Some((line, column))
            })
            .collect::<Vec<_>>();
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
        text.push_str("in ");
        text.push_str(from.get("kind").and_then(Value::as_str).unwrap_or("symbol"));
        text.push(' ');
        text.push_str(from.get("name").and_then(Value::as_str).unwrap_or_default());
        if let Some(detail) = from.get("detail").and_then(Value::as_str) {
            text.push_str(&format!(" ({detail})"));
        }
        if let Some(range) = from.get("displayRange")
            && let Some(start) = range.get("startLine").and_then(Value::as_u64)
        {
            match range.get("endLine").and_then(Value::as_u64) {
                Some(end) if end != start => text.push_str(&format!(" {start}-{end}")),
                _ => text.push_str(&format!(" {start}")),
            }
        }
        let index = match files.iter().position(|(seen, _)| *seen == path) {
            Some(index) => index,
            None => {
                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), json!(path));
                entry.insert("calls".into(), json!([]));
                files.push((path, entry));
                files.len() - 1
            }
        };
        let entry = &mut files[index].1;
        if let Some(calls) = entry.get_mut("calls").and_then(Value::as_array_mut) {
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
    if let Some(payload) = row.get_mut("payload").and_then(Value::as_object_mut) {
        payload.remove("items");
        payload.insert(
            "byFile".into(),
            Value::Array(
                files
                    .into_iter()
                    .map(|(_, entry)| Value::Object(entry))
                    .collect(),
            ),
        );
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
    let warnings = distinct
        .into_iter()
        .map(|(message, count)| {
            let items = if count == 1 { "item" } else { "items" };
            format!("Hierarchy expansion failed for {count} {items}: {message}")
        })
        .collect::<Vec<_>>();
    mark_partial(row, query, reason, &warnings);
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
    let out_of_policy = walks
        .iter()
        .map(|(_, walk)| walk.out_of_policy)
        .sum::<usize>();
    if out_of_policy > 0
        && let Some(warnings) = row
            .as_object_mut()
            .map(|object| object.entry("warnings").or_insert_with(|| json!([])))
            .and_then(Value::as_array_mut)
    {
        warnings.push(json!(format!(
            "{out_of_policy} hierarchy items outside the allowed read roots were omitted."
        )));
    }
    let builtin_lib = walks
        .iter()
        .map(|(_, walk)| walk.builtin_lib)
        .sum::<usize>();
    if builtin_lib > 0
        && let Some(warnings) = row
            .as_object_mut()
            .map(|object| object.entry("warnings").or_insert_with(|| json!([])))
            .and_then(Value::as_array_mut)
    {
        warnings.push(json!(format!(
            "{builtin_lib} TypeScript built-in library items (lib.*.d.ts) were omitted."
        )));
    }
    let tagged = walks.len() > 1;
    let resumes = walks
        .iter()
        .flat_map(|(expansion, walk)| walk.resumes.iter().map(move |resume| (*expansion, resume)))
        .collect::<Vec<_>>();
    let Some((_, first)) = resumes.first() else {
        return;
    };
    let dropped_edges = walks
        .iter()
        .map(|(_, walk)| walk.dropped_edges)
        .sum::<usize>();
    let unexpanded_nodes = walks
        .iter()
        .map(|(_, walk)| walk.unexpanded_nodes)
        .sum::<usize>();
    let fan_out_capped = walks
        .iter()
        .flat_map(|(_, walk)| walk.fan_out_capped.iter())
        .collect::<Vec<_>>();
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
    let mut limits = Vec::new();
    if dropped_edges > 0 || unexpanded_nodes > 0 {
        limits.push((
            "hierarchyNodeLimit",
            format!(
                "Stopped at {MAX_HIERARCHY_NODES} hierarchy nodes; {dropped_edges} edges to further nodes were not listed and {unexpanded_nodes} listed results were not expanded (payload.unexpandedParents). {follow}"
            ),
        ));
    }
    if !fan_out_capped.is_empty() {
        let names = fan_out_capped
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        limits.push((
            "hierarchyFanOutLimit",
            format!(
                "Kept the first {MAX_HIERARCHY_FAN_OUT} results of {} node(s) below the anchor ({names}); each resumes as the anchor of its continuation, which lists every result. {follow}",
                fan_out_capped.len()
            ),
        ));
    }
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
    object.insert("uri".into(), json!(uri_to_path(uri)));
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

pub(super) async fn hierarchy(
    client: &NativeLspClient,
    query: &LspSearchQuery,
    paths: &PathPolicy,
    path: &str,
    line: u32,
    character: u32,
    extra_roots: &[(String, u32, u32)],
    derived_callers: Vec<(Value, Vec<Value>)>,
    cancel: &dyn CancellationCheck,
) -> Result<Value, LspFailure> {
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
    let mut prepared = as_array(&prepared);
    // Verified importer call sites (TS/JS recovery) root the same walk; a
    // call-hierarchy item prepared there resolves in the importer's program.
    if !extra_roots.is_empty()
        && query.operation() != "supertypes"
        && query.operation() != "subtypes"
    {
        for (root_path, root_line, root_character) in extra_roots {
            if let Ok(Ok(more)) = cancellable(
                cancel,
                client.prepare_call_hierarchy(root_path.clone(), *root_line, *root_character),
            )
            .await
            {
                prepared.extend(as_array(&more));
            }
        }
    }
    let mut keys = NodeKeys::new(paths);
    let mut root_keys = HashSet::new();
    let roots = prepared
        .into_iter()
        .filter(|root| item_uri_is_authorized(root, paths))
        .filter(|root| root_keys.insert(keys.key(root)))
        .collect::<Vec<_>>();
    let depth = query.depth().unwrap_or(1);
    let mut items = Vec::new();
    let mut failures = Vec::new();
    let mut walks = Vec::new();
    for &expansion in expansions {
        let walk = walk_hierarchy(client, &roots, expansion, depth, paths, cancel).await?;
        items.extend(walk.edges.iter().map(|edge| public_edge(expansion, edge)));
        if expansion == Expansion::IncomingCalls && !derived_callers.is_empty() {
            // Reference-derived callers fill in only files the server's
            // call hierarchy did not answer for at level 1.
            let answered = walk
                .edges
                .iter()
                .filter(|edge| edge.level == 1)
                .filter_map(|edge| edge.node.get("uri").and_then(Value::as_str))
                .map(canonical_uri_path)
                .collect::<HashSet<_>>();
            for (node, sites) in &derived_callers {
                let file = node
                    .get("uri")
                    .and_then(Value::as_str)
                    .map(canonical_uri_path);
                if file.is_some_and(|file| answered.contains(&file)) {
                    continue;
                }
                let edge = HierarchyEdge {
                    node: node.clone(),
                    parent: None,
                    level: 1,
                    sites: sites.clone(),
                };
                let mut item = public_edge(expansion, &edge);
                item["source"] = json!(RECOVERED_FROM_REFERENCES);
                items.push(item);
            }
        }
        walks.push((expansion, walk));
    }
    for (_, walk) in &mut walks {
        failures.append(&mut walk.failures);
    }
    let (items, failures) = expansion_outcome(items, failures)?;
    let mut row = items_payload(query, query.operation().as_str(), json!(items));
    if query.operation() == "callers" {
        compact_callers(&mut row);
    }
    mark_partial_expansion(&mut row, query, &failures);
    let walks = walks
        .iter()
        .map(|(expansion, walk)| (*expansion, walk))
        .collect::<Vec<_>>();
    // Both directions of a `callHierarchy` are marked together: marking
    // them one after the other would let the second overwrite the first's
    // `unexpandedParents` and `continueWalk*`.
    mark_truncation(&mut row, query, &walks);
    Ok(row)
}
