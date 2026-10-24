//! Workspaces and a fake call graph the `lspSearch` tests share.

use super::*;

/// A `graph.ts` placeholder under `root` and its file URI.
pub(super) fn graph_ts(root: &std::path::Path) -> (std::path::PathBuf, String) {
    let file = root.join("graph.ts");
    std::fs::write(&file, "// graph\n").expect("graph.ts");
    let uri = octocode_engine::lsp::uri::path_to_uri(&file.to_string_lossy()).expect("uri");
    (file, uri)
}

/// A fresh canonical workspace and its path policy; the directory is removed
/// when the returned [`Fixture`] drops.
pub(super) fn temp_workspace() -> (
    crate::tools::test_support::Fixture,
    std::path::PathBuf,
    crate::policy::path::PathPolicy,
) {
    let dir = crate::tools::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    let paths = crate::tools::test_support::workspace_policy(&root);
    (dir, root, paths)
}

/// A server request failure as a hierarchy walk records it.
pub(super) fn expansion_error(message: &str) -> LspFailure {
    LspFailure::from(octocode_engine::error::Error::new(message))
}

/// A fake call graph: `callers[name]` lists the names calling `name`. Each
/// node's selection line is derived from its name, so nodes differ by key.
pub(super) struct FakeGraph {
    pub(super) callers: std::collections::HashMap<String, Vec<String>>,
    pub(super) uri: Option<String>,
    pub(super) requests: std::cell::Cell<usize>,
}

pub(super) fn name_line(name: &str) -> u64 {
    name.bytes().fold(1469598103u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
    }) % 100_000
}

pub(super) fn node_at(name: &str, uri: Option<&str>) -> serde_json::Value {
    let line = name_line(name);
    let mut node = serde_json::json!({
        "name": name,
        "kind": 12,
        "range": {"start": {"line": line, "character": 0}, "end": {"line": line + 3, "character": 1}},
        "selectionRange": {"start": {"line": line, "character": 9}, "end": {"line": line, "character": 12}}
    });
    if let Some(uri) = uri {
        node["uri"] = serde_json::json!(uri);
    }
    node
}

pub(super) fn node(name: &str) -> serde_json::Value {
    node_at(name, None)
}

impl HierarchySource for FakeGraph {
    async fn expand(
        &self,
        _: Expansion,
        item: serde_json::Value,
    ) -> Result<serde_json::Value, LspFailure> {
        self.requests.set(self.requests.get() + 1);
        let name = item["name"].as_str().unwrap_or_default();
        if name == "broken" {
            return Err(expansion_error("expansion failed"));
        }
        // Yield once so a level's requests genuinely interleave.
        tokio::task::yield_now().await;
        Ok(serde_json::json!(
            self.callers
                .get(name)
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, caller)| serde_json::json!({
                    "from": node_at(caller, self.uri.as_deref()),
                    // The same edge read as an outgoing call, so one graph
                    // serves both call directions.
                    "to": node_at(caller, self.uri.as_deref()),
                    "fromRanges": [{"start": {"line": 2 + index, "character": 4}, "end": {"line": 2 + index, "character": 7}}]
                }))
                .collect::<Vec<_>>()
        ))
    }
}

pub(super) fn graph(edges: &[(&str, &[&str])]) -> FakeGraph {
    FakeGraph {
        callers: edges
            .iter()
            .map(|(callee, callers)| {
                (
                    (*callee).to_owned(),
                    callers.iter().map(|caller| (*caller).to_owned()).collect(),
                )
            })
            .collect(),
        uri: None,
        requests: std::cell::Cell::new(0),
    }
}

pub(super) async fn walk_from(
    graph: &FakeGraph,
    root: serde_json::Value,
    depth: u32,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<HierarchyWalk, LspFailure> {
    walk_hierarchy(
        graph,
        &[root],
        Expansion::IncomingCalls,
        depth,
        paths,
        cancel,
    )
    .await
}

pub(super) async fn walk(graph: &FakeGraph, depth: u32) -> HierarchyWalk {
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    let paths = PathPolicy::new(PathPolicyConfig::default()).expect("path policy");
    walk_from(
        graph,
        node("root"),
        depth,
        &paths,
        &crate::execution::cancel::NeverCancel,
    )
    .await
    .expect("walk")
}

pub(super) fn edge_summary(walk: &HierarchyWalk) -> Vec<(String, u64, Option<String>)> {
    walk.edges
        .iter()
        .map(|edge| public_edge(Expansion::IncomingCalls, edge, None))
        .map(|edge| {
            (
                edge["from"]["symbolName"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                edge["level"].as_u64().unwrap_or(0),
                edge["via"]["symbolName"].as_str().map(str::to_owned),
            )
        })
        .collect()
}
