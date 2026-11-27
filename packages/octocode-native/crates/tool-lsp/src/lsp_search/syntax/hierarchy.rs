//! Blocking scans behind the syntax rung's call and type hierarchies: who
//! calls a name, what a declaration calls, what it extends, and the
//! declarations behind heritage-scan locations. Every relation is matched
//! by name from tree-sitter facts, never resolved.
use super::facts::{File, each_file, range_value};
use octocode_engine::lsp::types::LspCodeSnippet;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// A declaration found by a scan: its file, its id among that file's facts
/// (empty for module-level code) and its hierarchy node.
#[derive(Clone)]
pub(super) struct Found {
    pub(super) path: String,
    pub(super) id: String,
    pub(super) node: Value,
}

impl Found {
    /// Where the node is, as a walk dedupes it.
    pub(super) fn key(&self) -> (String, String) {
        (self.path.clone(), self.id.clone())
    }

    pub(super) fn name(&self) -> &str {
        self.node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

/// The name a call or heritage clause spells, without its qualifier
/// (`a.b`, `a::b`, `pkg.Type`).
fn short(name: &str) -> &str {
    name.rsplit(['.', ':']).next().unwrap_or(name)
}

/// The declarations named `name` in `files`.
pub(super) fn named(
    files: &BTreeSet<String>,
    name: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Vec<Found> {
    let (found, _) = each_file(files, stopped, |file| {
        file.outline().map_or_else(Vec::new, |outline| {
            outline
                .declarations
                .iter()
                .filter(|declaration| file.same_name(&declaration.name, name))
                .map(|declaration| Found {
                    path: file.path.clone(),
                    id: declaration.id.clone(),
                    node: file.node(declaration),
                })
                .collect()
        })
    });
    found.into_iter().flatten().collect()
}

/// The innermost declaration of `file` named `name` whose range holds the
/// zero-based `line`, among its whole facts (ids match its calls and edges).
pub(super) fn anchor_declaration(file: &File, name: &str, line: u32) -> Option<Found> {
    let facts = file.facts()?;
    facts
        .declarations
        .iter()
        .filter(|declaration| {
            file.same_name(&declaration.name, name)
                && declaration.range.start.line <= line
                && line <= declaration.range.end.line
        })
        .min_by_key(|declaration| declaration.range.end.line - declaration.range.start.line)
        .map(|declaration| Found {
            path: file.path.clone(),
            id: declaration.id.clone(),
            node: file.node(declaration),
        })
}

/// The calls in `files` of any of `names`: per called name, each calling
/// declaration (module-level code is its file) and its call sites.
pub(super) fn incoming(
    files: &BTreeSet<String>,
    names: &BTreeSet<String>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Vec<(String, Found, Vec<Value>)> {
    let (found, _) = each_file(files, stopped, |file| {
        let Some(facts) = file.facts() else {
            return Vec::new();
        };
        let names = names
            .iter()
            .map(|name| file.canonical_name(name))
            .collect::<BTreeSet<_>>();
        let mut callers: BTreeMap<(String, String), Vec<Value>> = BTreeMap::new();
        for call in &facts.calls {
            let callee = file.canonical_name(short(&call.callee));
            if names.contains(callee) {
                let caller = call.caller_id.clone().unwrap_or_default();
                callers
                    .entry((callee.to_owned(), caller))
                    .or_default()
                    .push(range_value(&call.range));
            }
        }
        callers
            .into_iter()
            .map(|((callee, id), sites)| {
                let node = facts
                    .declarations
                    .iter()
                    .find(|declaration| declaration.id == id)
                    .map_or_else(|| module_node(file), |declaration| file.node(declaration));
                let caller = Found {
                    path: file.path.clone(),
                    id,
                    node,
                };
                (callee, caller, sites)
            })
            .collect()
    });
    found.into_iter().flatten().collect()
}

/// What the declaration `id` of `path` calls: each called name and its
/// call sites.
pub(super) fn outgoing(path: &str, id: &str) -> Vec<(String, Vec<Value>)> {
    let files = BTreeSet::from([path.to_owned()]);
    let (found, _) = each_file(&files, &|| false, |file| {
        let mut callees: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for call in file.facts().iter().flat_map(|facts| facts.calls.iter()) {
            if call.caller_id.as_deref() == Some(id) {
                callees
                    .entry(file.canonical_name(short(&call.callee)).to_owned())
                    .or_default()
                    .push(range_value(&call.range));
            }
        }
        callees.into_iter().collect::<Vec<_>>()
    });
    found.into_iter().flatten().collect()
}

/// The names the declaration `id` of `path` extends or implements.
pub(super) fn bases(path: &str, id: &str) -> Vec<String> {
    let files = BTreeSet::from([path.to_owned()]);
    let (found, _) = each_file(&files, &|| false, |file| {
        file.facts()
            .iter()
            .flat_map(|facts| facts.edges.iter())
            .filter(|edge| {
                edge.from == id && matches!(edge.relation.as_str(), "extends" | "implements")
            })
            .map(|edge| short(&edge.to).to_owned())
            .collect::<BTreeSet<_>>()
    });
    found.into_iter().flatten().collect()
}

/// The declarations the heritage scan located (at their names).
pub(super) fn declarations_at(snippets: &[LspCodeSnippet]) -> Vec<Found> {
    let files = snippets
        .iter()
        .filter_map(|snippet| super::super::render::decode_uri_path(&snippet.uri).ok())
        .collect::<BTreeSet<_>>();
    let (found, _) = each_file(&files, &|| false, |file| {
        let Some(facts) = file.facts() else {
            return Vec::new();
        };
        snippets
            .iter()
            .filter(|snippet| snippet.uri == file.uri)
            .filter_map(|snippet| {
                let declaration = facts.declarations.iter().find(|declaration| {
                    declaration.selection_range.start.line == snippet.range.start.line
                        && declaration.selection_range.start.character
                            == snippet.range.start.character
                })?;
                Some(Found {
                    path: file.path.clone(),
                    id: declaration.id.clone(),
                    node: file.node(declaration),
                })
            })
            .collect()
    });
    found.into_iter().flatten().collect()
}

/// A node for a file's module-level code.
fn module_node(file: &File) -> Value {
    let name = std::path::Path::new(&file.path).file_name().map_or_else(
        || file.path.clone(),
        |name| name.to_string_lossy().into_owned(),
    );
    let start = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}});
    json!({
        "name": name,
        "kind": "module",
        "uri": file.uri,
        "range": start,
        "selectionRange": start,
    })
}
