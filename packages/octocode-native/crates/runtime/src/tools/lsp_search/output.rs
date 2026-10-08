//! lspSearch output facts for the shared response stages.
use crate::tools::clasify::{items, resource::ResourceSource};
use crate::tools::output::{PathAnchor, TextShape, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;

impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Refresh path/symbolName/lineHint, or broaden workspaceRoot."
    }
    fn evidence_kind(&self, _query: &Value, data: &Value) -> &'static str {
        match data.pointer("/lsp/source").and_then(Value::as_str) {
            Some("native-graph-facts" | "markdown") => "syntactic",
            _ => "semantic",
        }
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::Workspace
    }
    /// documentSymbols rows print as a compact outline in YAML text; every
    /// other payload renders structured.
    fn text_shape(&self) -> TextShape {
        TextShape::Outline
    }
    fn clasify_items(&self, source: &ResourceSource, state: &Value) -> Option<Vec<items::Item>> {
        let ResourceSource::LspSearch(query) = source else {
            return None;
        };
        clasify_locations(state, query.path())
    }
}

/// Reference, caller, and definition rows as one candidate per file. Per-file
/// rows (compact references, direct callers, groupByFile summaries) already
/// are; location rows group by path, falling back to the queried file.
fn clasify_locations(state: &Value, query_path: Option<&str>) -> Option<Vec<items::Item>> {
    let data = items::page_data(state)?;
    let base = items::page_root(state);
    if let Some(files) = data.pointer("/payload/files").and_then(Value::as_array) {
        let found = files
            .iter()
            .filter_map(|file| {
                let path = items::absolute(
                    base,
                    file.get("path")?.as_str()?.trim_start_matches("file://"),
                );
                let lines = file
                    .get("matches")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .flat_map(super::locations::site_lines)
                    .collect();
                Some(items::Item {
                    state: items::narrowed(
                        state,
                        "/results/0/data/payload/files",
                        vec![file.clone()],
                    ),
                    read: Some(items::local_read(&path, lines)),
                    path: Some(path),
                    item: None,
                })
            })
            .collect();
        return Some(found);
    }
    let rows = data.pointer("/payload/matches")?.as_array()?;
    let found = items::grouped(rows, query_path)
        .into_iter()
        .map(|(path, group)| {
            let path = items::absolute(base, path.trim_start_matches("file://"));
            let lines = group
                .iter()
                .filter_map(|row| {
                    row.get("displayRange")
                        .and_then(|range| items::line(range, "startLine"))
                })
                .collect();
            items::Item {
                state: items::narrowed(state, "/results/0/data/payload/matches", group),
                read: Some(items::local_read(&path, lines)),
                path: Some(path),
                item: None,
            }
        })
        .collect();
    Some(found)
}

#[cfg(test)]
mod tests {
    use crate::tools::id::ToolId;
    use serde_json::json;

    #[test]
    fn native_language_server_results_are_semantic_evidence() {
        let lsp = ToolId::LspSearch.output();
        let kind =
            |source: &str| lsp.evidence_kind(&json!({}), &json!({"lsp": {"source": source}}));
        assert_eq!(kind("native"), "semantic");
        assert_eq!(kind("native-graph-facts"), "syntactic");
    }
}
