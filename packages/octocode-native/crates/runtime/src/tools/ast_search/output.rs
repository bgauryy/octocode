//! astSearch's answers to the shared response stages.

use super::AstSearchQuery;
use crate::tools::clasify::{items, resource::ResourceSource};
use crate::tools::output::{PathAnchor, TextShape, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        // A pattern must parse as one complete node of the target grammar.
        if query["operation"] == "match" && query["pattern"].is_string() {
            "Patterns match whole nodes exactly (modifiers, return types, bodies); check operation:syntaxTree, or use a rule."
        } else {
            "Broaden the syntax/name query, path, or filters."
        }
    }
    fn error_hint(&self, code: &str) -> Option<&'static str> {
        Some(match code {
            crate::tools::ast_rule::INVALID_PATTERN => crate::tools::ast_rule::INVALID_PATTERN_HINT,
            "fileTooLarge" => "Target a smaller file or narrower directory scope.",
            "languageRequired" | "languageUnsupported" | "languageMismatch" => {
                "Set language to the grammar of the source files (e.g. \"typescript\", \"rust\")."
            }
            "languageFileRequired" => {
                "For a directory, drop language: each extension picks its grammar; languageGlobs overrides (e.g. {cpp:[\"**/*.h\"]})."
            }
            "languageDirectoryRequired" => {
                "For a single file, use language instead of languageGlobs."
            }
            _ => return None,
        })
    }
    fn evidence_kind(&self, query: &Value, _data: &Value) -> &'static str {
        if query["operation"] == "match" {
            "structural"
        } else {
            "syntactic"
        }
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::QueryParent
    }
    fn text_shape(&self) -> TextShape {
        TextShape::Outline
    }
    fn clasify_items(&self, source: &ResourceSource, state: &Value) -> Option<Vec<items::Item>> {
        let ResourceSource::AstSearch(query) = source else {
            return None;
        };
        let data = items::page_data(state)?;
        let base = items::page_root(state);
        match &**query {
            AstSearchQuery::MatchPattern(_) | AstSearchQuery::MatchRule(_) => {
                items::file_items(state, data, base, "matches")
            }
            AstSearchQuery::Symbols(_) if data.get("files").is_some() => {
                items::file_items(state, data, base, "symbols")
            }
            AstSearchQuery::Symbols(symbols) => {
                clasify_declarations(state, data, base, symbols.path.as_str())
            }
            AstSearchQuery::SyntaxTree(_) => None,
        }
    }
}

/// A single-file outline: one candidate per file its declarations name
/// (rows without a path belong to the outlined file).
fn clasify_declarations(
    state: &Value,
    data: &Value,
    base: Option<&str>,
    query_path: &str,
) -> Option<Vec<items::Item>> {
    let rows = data.get("symbols")?.as_array()?;
    let fallback = data
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(query_path);
    let found = items::grouped(rows, Some(fallback))
        .into_iter()
        .map(|(path, group)| {
            let path = items::absolute(base, path);
            let lines = group
                .iter()
                .filter_map(|row| items::line(row, "line"))
                .collect();
            items::Item {
                state: items::narrowed(state, "/results/0/data/symbols", group),
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
    fn empty_pattern_match_hints_at_complete_nodes() {
        let pattern = json!({"operation": "match", "pattern": "const $A = $B"});
        let hint = ToolId::AstSearch.output().fallback_hint(&pattern);
        assert!(hint.contains("whole nodes"), "{hint}");
        assert!(hint.contains("return types"), "{hint}");
        let rule = json!({"operation": "match", "rule": "id: x"});
        assert_eq!(
            ToolId::AstSearch.output().fallback_hint(&rule),
            "Broaden the syntax/name query, path, or filters."
        );
    }
}
