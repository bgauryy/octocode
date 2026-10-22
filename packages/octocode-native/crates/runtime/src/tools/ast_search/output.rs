//! astSearch's answers to the shared response stages.

use crate::response::output::{PathAnchor, TextShape, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        // A pattern must parse as one complete node of the target grammar.
        if query["operation"] == "match" && query["pattern"].is_string() {
            crate::tools::ast_rule::PATTERN_SHAPE_HINT
        } else {
            "Broaden the syntax/name query, path, or filters."
        }
    }
    fn error_hint(&self, code: &str) -> Option<&'static str> {
        Some(match code {
            crate::tools::ast_rule::INVALID_PATTERN => crate::tools::ast_rule::INVALID_PATTERN_HINT,
            // The typed error and terminalLimit (plus a findText lead
            // when a literal was searched) are the whole answer.
            "fileTooLarge" => crate::response::output::NO_HINT,
            "languageRequired" | "languageUnsupported" => {
                "Set language to the grammar of the source files (e.g. \"typescript\", \"rust\")."
            }
            // The message names the file's own grammar and both repairs.
            "languageMismatch" => crate::response::output::NO_HINT,
            "symbolsUnsupported" => {
                "Set language (a file) or languageGlobs (a directory) to the grammar the source is written in."
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
}

#[cfg(test)]
mod tests {
    use crate::id::ToolId;
    use serde_json::json;

    #[test]
    fn empty_pattern_match_hints_at_complete_nodes() {
        let pattern = json!({"operation": "match", "pattern": "const $A = $B"});
        let hint = ToolId::AstSearch.output().fallback_hint(&pattern);
        assert!(hint.contains("whole nodes"), "{hint}");
        assert!(hint.contains("syntaxTree"), "{hint}");
        let rule = json!({"operation": "match", "rule": "id: x"});
        assert_eq!(
            ToolId::AstSearch.output().fallback_hint(&rule),
            "Broaden the syntax/name query, path, or filters."
        );
    }
}
