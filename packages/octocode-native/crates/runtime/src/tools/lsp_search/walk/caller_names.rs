//! Naming callers the server reports too broadly: a whole module split into
//! its innermost functions, a whole class narrowed to the member holding the calls.

use super::HierarchyEdge;
use crate::execution::cancel::CancellationCheck;
use crate::tools::lsp_search::call_sites::{enclosing_callable, flatten_symbols};
use crate::tools::lsp_search::cancellable;
use crate::tools::lsp_search::failure::LspFailure;
use crate::tools::lsp_search::render::{decode_uri_path, uri_to_path};
use crate::tools::lsp_search::source::SourceCache;
use octocode_engine::lsp::client::NativeLspClient;
use serde_json::{Value, json};
use std::collections::HashMap;

/// LSP `SymbolKind`s of a whole file or module caller.
const MODULE_KINDS: [u64; 2] = [1, 2];

/// A caller the server reports as a whole module or file (tsserver does for
/// calls inside a top-level callback: `describe(() => …)`, `it(…)`) split
/// into one edge per innermost function symbol of `symbols` (the file's
/// `documentSymbol` answer) around its call sites, the way reference-derived
/// callers are named. Sites outside every function stay with the module.
/// Any other edge is returned unchanged.
pub(in crate::tools::lsp_search) fn split_module_callers(
    edge: &HierarchyEdge,
    symbols: &Value,
) -> Vec<HierarchyEdge> {
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
pub(super) async fn name_module_callers(
    client: &NativeLspClient,
    edges: Vec<HierarchyEdge>,
    cancel: &dyn CancellationCheck,
) -> Result<Vec<HierarchyEdge>, LspFailure> {
    let mut symbols: HashMap<String, std::sync::Arc<Value>> = HashMap::new();
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
                .unwrap_or_default();
            symbols.insert(uri.to_owned(), found);
        }
        named.extend(split_module_callers(&edge, &symbols[uri]));
    }
    Ok(named)
}

/// Largest source parsed to narrow a class caller.
const MAX_NARROW_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// LSP `SymbolKind` of a class.
const CLASS_KIND: u64 = 5;

/// Declarations of the files class callers name, per walk: each file is
/// read once through the request's policy-checked, bounded source cache,
/// and parsed off the async worker through the shared declarations cache,
/// however many class callers it holds.
#[derive(Default)]
pub(in crate::tools::lsp_search) struct ClassFacts(
    HashMap<String, Option<std::sync::Arc<octocode_engine::graph::GraphFactsDocument>>>,
);

impl ClassFacts {
    /// [`narrowed_class_caller`] of an incoming `edge`; `None` for any other
    /// edge, a file the policy refuses or cannot read, or a file over
    /// [`MAX_NARROW_SOURCE_BYTES`].
    pub(in crate::tools::lsp_search) async fn narrow(
        &mut self,
        edge: &HierarchyEdge,
        sources: &mut SourceCache<'_>,
    ) -> Option<Value> {
        if edge.node.get("kind").and_then(Value::as_u64) != Some(CLASS_KIND) {
            return None;
        }
        let path = decode_uri_path(edge.node.get("uri")?.as_str()?).ok()?;
        if !self.0.contains_key(&path) {
            let facts = match sources.get(&path).await {
                Some(source) if source.content.len() <= MAX_NARROW_SOURCE_BYTES => {
                    let file = path.clone();
                    // The parse is shared with outline pages and search hits:
                    // one parse per file version.
                    tokio::task::spawn_blocking(move || {
                        crate::tools::declarations_cache::file_document(&source.content, &file)
                    })
                    .await
                    .ok()
                    .flatten()
                }
                _ => None,
            };
            self.0.insert(path.clone(), facts);
        }
        narrowed_class_caller(&edge.node, &edge.sites, self.0.get(&path)?.as_ref()?)
    }
}

/// A caller the server reports as a whole class (tsserver does for calls in
/// a constructor or a field initializer) shown as the innermost member that
/// holds every call site, so its range and read lead fit the call. `facts`
/// are the declarations of the class's file. The walk itself keeps the
/// server's item. `None` when nothing narrower holds them.
fn narrowed_class_caller(
    node: &Value,
    sites: &[Value],
    facts: &octocode_engine::graph::GraphFactsDocument,
) -> Option<Value> {
    if node.get("kind").and_then(Value::as_u64) != Some(CLASS_KIND) {
        return None;
    }
    let site_lines = sites
        .iter()
        .filter_map(|site| site.pointer("/start/line").and_then(Value::as_u64))
        .collect::<Vec<_>>();
    let (first, last) = (*site_lines.iter().min()?, *site_lines.iter().max()?);
    let line = |value: &Value, pointer: &str| value.pointer(pointer).and_then(Value::as_u64);
    let class = (
        line(node, "/range/start/line")?,
        line(node, "/range/end/line")?,
    );
    let member = facts
        .declarations
        .iter()
        .filter_map(|declaration| {
            let kind = match declaration.kind.as_str() {
                "constructor" => 9,
                "method" => 6,
                "property" => 7,
                "function" => 12,
                _ => return None,
            };
            let span = (
                u64::from(declaration.range.start.line),
                u64::from(declaration.range.end.line),
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
        "name": declaration.name,
        "kind": kind,
        "detail": node.get("name"),
        "uri": node.get("uri"),
        "range": declaration.range,
        "selectionRange": declaration.selection_range,
    }))
}
