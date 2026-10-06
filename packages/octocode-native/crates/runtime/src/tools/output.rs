//! Tool-owned output facts. The shared response stages ask the tool through
//! `ToolId::output` (the dispatch in `runtime::tool_output`) instead of
//! branching on the tool: each tool answers for its own recovery tips,
//! evidence kind, path anchoring and text shape.
//!
//! A tool module implements [`ToolOutput`] on a unit struct and the
//! dispatch points its `ToolId` at it. Impls not yet moved into their tool
//! module live at the end of this file.

use serde_json::Value;

/// How the envelope names the files a tool's rows point at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathAnchor {
    /// Rows keep their paths; absolute paths share their common directory
    /// as `root` (remote tools).
    Common,
    /// Local file paths are relative to the workspace root, so localFetch
    /// and localSearch resolve a copied row path as-is.
    Workspace,
    /// Like [`PathAnchor::Workspace`], but rows name files relative to the
    /// parent of the query root, so they are made absolute first.
    QueryParent,
    /// Fields stay relative to the scanned directory; one directory for the
    /// whole response becomes the row `path`, anchored on the workspace.
    ScannedDir,
}

/// How the text channel renders a tool's rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextShape {
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
pub trait ToolOutput: Sync {
    /// Recovery tip for an empty row, or an error row with no tip of its own.
    fn fallback_hint(&self, query: &Value) -> &'static str;

    /// Recovery for an `errorCode` only this tool emits. Codes every tool
    /// shares (sandbox, auth, rate limit, timeout, not found, invalid input)
    /// are answered by the response stage.
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

    /// How the text channel renders this tool's rows.
    fn text_shape(&self) -> TextShape {
        TextShape::Structured
    }

    /// The tool's own default projection of a successful row's data (never
    /// under `debug: true`): drop what only restates the caller's query.
    fn compact(&self, _data: &mut serde_json::Map<String, Value>, _query: &Value) {}

    /// The candidates clasify judges one at a time on this tool's list page:
    /// each carries its narrowed state, identity, and the read that fetches
    /// it. `source` is the parsed read that produced `state`. `None` keeps
    /// the page whole (no candidate list).
    fn clasify_items(
        &self,
        _source: &crate::tools::clasify::resource::ResourceSource,
        _state: &Value,
    ) -> Option<Vec<crate::tools::clasify::items::Item>> {
        None
    }
}

/// Roots are widened only from a trusted source, so the hint names where the
/// setting is read: a workspace `.env` or `.octocoderc` is ignored for it.
pub const SANDBOX_HINT: &str = "Outside allowed roots: add the dir to ALLOWED_PATHS (comma list) in the env or ~/.octocode/.env, not a workspace file.";

/// Recovery for a path that does not resolve: check it from its parent.
pub const VERIFY_PATH_HINT: &str =
    "Verify the path exists (structureSearch on its parent directory), then retry the exact path.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::id::ToolId;
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
