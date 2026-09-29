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
/// Distinct nodes one walk may record (roots included). Edges to new nodes
/// past the cap are dropped and the row carries a resumable continuation.
pub(super) const MAX_HIERARCHY_NODES: usize = 200;
/// Results kept per expanded node; the rest are dropped with a terminal
/// diagnostic (re-anchoring on that node would hit the same cap).
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
    /// Names of nodes whose results were cut to [`MAX_HIERARCHY_FAN_OUT`].
    pub(super) fan_out_capped: Vec<String>,
    /// Results naming a file outside the read policy (skipped).
    pub(super) out_of_policy: usize,
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
            if results.len() > MAX_HIERARCHY_FAN_OUT {
                walk.fan_out_capped.push(
                    parent
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_owned(),
                );
            }
            for result in results.into_iter().take(MAX_HIERARCHY_FAN_OUT) {
                let Some(node) = expansion.node_of(&result).cloned() else {
                    continue;
                };
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
                if is_new && seen.len() >= MAX_HIERARCHY_NODES {
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
                    seen.insert(node_key);
                    if level < depth {
                        next.push(node);
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
            )
        });
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
    let warnings = failures
        .iter()
        .take(5)
        .map(|failure| format!("Hierarchy expansion failed for some items: {failure}"))
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
/// `callees`). A fan-out cap is a typed terminal limit.
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
    let tagged = walks.len() > 1;
    let resumes = walks
        .iter()
        .flat_map(|(expansion, walk)| walk.resumes.iter().map(move |resume| (*expansion, resume)))
        .collect::<Vec<_>>();
    let dropped_edges = walks
        .iter()
        .map(|(_, walk)| walk.dropped_edges)
        .sum::<usize>();
    if let Some((_, first)) = resumes.first() {
        row["payload"]["truncated"] = json!(true);
        // Every dropped parent is listed and resumable, not only the first.
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
        let warning = format!(
            "Stopped at {MAX_HIERARCHY_NODES} hierarchy nodes; {dropped_edges} edges to further nodes under {} parent(s) (payload.unexpandedParents) were not listed. Run next.continueWalk to walk on from {}{}.",
            resumes.len(),
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
        match queries {
            Some(queries) => {
                push_reason(row, "hierarchyNodeLimit", &[warning]);
                // One flat continuation per dropped parent, so the whole
                // frontier is executable: `continueWalk`, `continueWalk2`, ….
                for (index, query) in queries.into_iter().enumerate() {
                    let key = match index {
                        0 => "continueWalk".to_owned(),
                        _ => format!("continueWalk{}", index + 1),
                    };
                    row["next"][key] = continuation(query);
                }
            }
            None => mark_terminal_limit(row, "hierarchyNodeLimit", &[warning]),
        }
    }
    let fan_out_capped = walks
        .iter()
        .flat_map(|(_, walk)| walk.fan_out_capped.iter())
        .collect::<Vec<_>>();
    if !fan_out_capped.is_empty() {
        row["payload"]["truncated"] = json!(true);
        let names = fan_out_capped
            .iter()
            .take(5)
            .map(|name| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        mark_terminal_limit(
            row,
            "hierarchyFanOutLimit",
            &[format!(
                "Kept the first {MAX_HIERARCHY_FAN_OUT} results of {} node(s) ({names}); use references with pagination for the complete list.",
                fan_out_capped.len()
            )],
        );
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
