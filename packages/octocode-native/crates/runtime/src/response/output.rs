//! Tool-owned output facts. The shared response stages ask the tool through
//! `ToolId::output` (the dispatch in `runtime::tool_output`) instead of
//! branching on the tool: each tool answers for its own recovery tips,
//! evidence kind, path anchoring and text shape.
//!
//! A tool module implements [`ToolOutput`] on a unit struct and the
//! dispatch points its `ToolId` at it.

use serde_json::Value;

/// Whether the row query sets `key` to something: not absent, null, blank or
/// an empty list. Fallback hints name the filters a query actually set.
pub(crate) fn has_value(query: &Value, key: &str) -> bool {
    match query.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(_) => true,
    }
}

/// How the envelope names the files a tool's rows point at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PathAnchor {
    /// Rows keep their paths; absolute paths share their common directory
    /// as `root` (remote tools).
    Common,
    /// Local file paths are relative to the workspace root, so localFetch
    /// and localSearch resolve a copied row path as-is.
    Workspace,
    /// Like [`PathAnchor::Workspace`], but rows name files relative to the
    /// parent of the query root, so they are made absolute first.
    QueryParent,
}

/// How the text channel renders a tool's rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextShape {
    /// Source text a caller copies from: one inline `content` per row.
    FileText,
    /// Ranked search hits (files, match rows, page cursor).
    SearchHits,
    /// History items: metadata, then each changed file's patch verbatim.
    Diff,
    /// Symbol rows as an indented declaration outline.
    Outline,
    /// Generic ordered encoding.
    Structured,
}

/// What the shared response stages ask a tool about its own output.
pub(crate) trait ToolOutput: Sync {
    /// Recovery tip for an error row with no tip of its own (an empty row
    /// carries only its own facts).
    fn fallback_hint(&self, query: &Value) -> &'static str;

    /// Recovery for an `errorCode` (or an empty row's `category`) only this
    /// tool emits; [`NO_HINT`] when the row's own facts are the whole answer.
    /// Codes every tool shares (sandbox, auth, rate limit, timeout, not
    /// found, invalid input) are answered by the response stage.
    fn error_hint(&self, _code: &str) -> Option<&'static str> {
        None
    }

    /// Kind of evidence a row is (`meta.evidence.kind`).
    fn evidence_kind(&self, query: &Value, data: &Value) -> &'static str;

    /// How the envelope anchors this tool's file paths.
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::Common
    }

    /// Whether rows read the checked-out working tree, so the response
    /// reports its commit (`shared.commitSha`).
    fn reads_worktree(&self) -> bool {
        false
    }

    /// Whether rows are resource-major matrices (`queries[]` of resources
    /// whose reads take the resource shape `{tool, query: row}`) instead of
    /// `results[]` rows. Such a tool answers for its own partiality.
    fn resource_major(&self) -> bool {
        false
    }

    /// The envelope key of this tool's rows: `queries` for resource-major
    /// matrices, else `results`.
    fn rows_key(&self) -> &'static str {
        if self.resource_major() {
            "queries"
        } else {
            "results"
        }
    }

    /// Evidence collections in outer-to-inner order. `*` visits a keyed
    /// resource or a single array element. Other values remain indivisible.
    /// Without explicit paths, the pager uses ordinary `data` arrays.
    fn collection_paths(&self) -> Option<&'static [&'static str]> {
        None
    }

    /// Shared row definitions carried by the first display fragment only.
    fn first_fragment_fields(&self) -> &'static [&'static str] {
        &[]
    }

    /// How the text channel renders this tool's rows.
    fn text_shape(&self) -> TextShape {
        TextShape::Structured
    }

    /// The tool's own default projection of a successful row's data (never
    /// without `extras: ["echo"]`): drop what only restates the caller's query.
    fn compact(&self, _data: &mut serde_json::Map<String, Value>, _query: &Value) {}

    /// Whether `clasifyContext` selects this tool's rows at the shared response
    /// boundary, over one sanitized provider page (code search), instead of
    /// before the tool's own source pagination.
    fn selects_provider_page(&self) -> bool {
        false
    }

    /// Finish a classified provider page's row `data` after selection: the
    /// tool's own leads may only name the selected evidence.
    fn finish_selection(&self, _data: &mut Value, _query: &Value) {}

    /// Settle a row of a repair lead (a corrected rejected call) that the
    /// schema admits but this tool would not serve as asked, so the lead
    /// reaches the answer in one hop.
    fn settle_repaired_row(&self, _row: &mut Value) {}
}

/// Roots are widened only from a trusted source, so the hint names where the
/// setting is read: a workspace `.env` or `.octocoderc` is ignored for it.
pub(crate) const SANDBOX_HINT: &str = "Outside allowed roots: add the dir to ALLOWED_PATHS (comma list) in the env or ~/.octocode/.env, not a workspace file.";

/// Recovery for a path that does not resolve: check it from its parent.
pub(crate) const VERIFY_PATH_HINT: &str =
    "Verify the path exists (structureSearch on its parent directory), then retry the exact path.";

/// An `error_hint` for a code whose typed error and leads are the whole
/// answer: no fallback text is added.
pub(crate) const NO_HINT: &str = "";

/// Recovery for a path a local read could not open (`fileAccessFailed`).
pub(crate) const LIST_FILES_HINT: &str =
    "Verify the path with structureSearch operation:\"files\", then retry the exact path.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::ToolId;
    use serde_json::json;

    #[test]
    fn every_tool_answers_with_a_tip_and_an_evidence_kind() {
        for tool in ToolId::ALL {
            let output = tool.output();
            assert!(!output.fallback_hint(&json!({})).is_empty(), "{tool}");
            assert!(
                !output.evidence_kind(&json!({}), &json!({})).is_empty(),
                "{tool}"
            );
            if output.text_shape() == TextShape::FileText {
                assert!(
                    output.reads_worktree() || tool.is_github(),
                    "{tool}: a source read"
                );
            }
            if output.path_anchor() != PathAnchor::Common {
                assert!(
                    tool.is_local(),
                    "{tool}: only local tools anchor local paths"
                );
            }
        }
    }
}
