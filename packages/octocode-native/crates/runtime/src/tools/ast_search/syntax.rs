use crate::tools::id::ToolId;
use crate::tools::result::ToolError;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use serde_json::json;
const MAX_SOURCE: usize = super::MAX_PARSE_SOURCE_BYTES;
pub use crate::contracts::tool_types::AstSearchQuerySyntaxTree;

/// Engine-unit views over the generated `syntaxTree` query.
impl AstSearchQuerySyntaxTree {
    pub(crate) fn language(&self) -> Option<String> {
        self.language.as_ref().map(ToString::to_string)
    }
    pub(crate) fn page(&self) -> u32 {
        u32::try_from(self.page.get()).unwrap_or(u32::MAX)
    }
    pub(crate) fn page_size(&self) -> u32 {
        u32::try_from(self.page_size.get()).unwrap_or(u32::MAX)
    }
    /// The first node of this page.
    pub(crate) fn node_offset(&self) -> u32 {
        self.page()
            .saturating_sub(1)
            .saturating_mul(self.page_size())
    }
    pub(crate) fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
}

pub fn execute_syntax(
    q: &AstSearchQuerySyntaxTree,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(ToolError::cancelled)?;
    let p = paths
        .validate_read(q.path.as_str())
        .map_err(super::policy_error)?;
    super::validate_file_language(&p.canonical, q.language().as_deref())?;
    let Some(bytes) = super::read_parse_source(&p.canonical)? else {
        return Ok(super::source_limit(&p.canonical.to_string_lossy()));
    };
    let sanitized = security
        .validate_text_bytes(&bytes, Some(&p.canonical), MAX_SOURCE)
        .map_err(super::policy_error)?;
    let snapshot = crate::digest::json_sha256(&json!([
        // Node-row shape version (D2: 1-based columns).
        "rows-v2",
        p.canonical.to_string_lossy(),
        q.language,
        sanitized.content,
        q.named_only
    ]));
    if q.node_offset() > 0 && q.snapshot() != Some(snapshot.as_str()) {
        let mut restart = serde_json::to_value(q).unwrap_or_default();
        if let Some(map) = restart.as_object_mut() {
            map.remove("snapshot");
            map.remove("page");
        }
        return Ok(crate::tools::result::stale_snapshot(
            crate::tools::result::Continuation::new(ToolId::AstSearch, restart)
                .confidence("exact")
                .build(),
        ));
    }
    cancel.check().map_err(ToolError::cancelled)?;
    let r = octocode_engine::portable::inspect_syntax_tree_with_extension(
        &sanitized.content,
        &p.canonical.to_string_lossy(),
        super::cpp_header_override(&p.canonical, q.language().as_deref()).then_some("cpp"),
        Some(SyntaxTreeInspectOptions {
            named_only: Some(q.named_only),
            node_offset: Some(q.node_offset()),
            node_limit: Some(q.page_size()),
        }),
    )
    .map_err(super::native_error)?;
    // Line/column locate every node; byte offsets are verbose (core field
    // class): the verbose stage keeps `nodeBytes` for `debug`.
    let node_bytes = r
        .nodes
        .iter()
        .map(|n| format!("{}-{}", n.start_byte, n.end_byte))
        .collect::<Vec<_>>();
    let nodes = r
        .nodes
        .iter()
        .map(|n| {
            node_row(
                n.id,
                &n.kind,
                n.named,
                [n.start_line, n.start_column, n.end_line, n.end_column],
                n.parent_id,
            )
        })
        .collect::<Vec<_>>();
    let diagnostics=r.diagnostics.into_iter().map(|d|json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":p.canonical.to_string_lossy(),"recovery":d.recovery})).collect::<Vec<_>>();
    let more = r.next_offset.is_some();
    let complete = r.status == "ok" && !more;
    let display_path = p.canonical.to_string_lossy();
    let page = q.page();
    use crate::tools::num::usize_of;
    let pagination = crate::response::pages::PageFacts::open(
        usize_of(page),
        Some(usize_of(q.page_size())),
        more,
    )
    .with_total(usize_of(r.total_nodes))
    .to_value();
    let mut out = json!({"operation":"syntaxTree","path":display_path,"nodes":nodes,"nodeBytes":node_bytes,"snapshot":snapshot,"isPartial":!complete,
        "pagination":pagination});
    if more {
        out["pagination"]["nextPage"] = json!(page + 1);
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    // A failed inspection names its declared code (from the engine's first
    // diagnostic); a partial tree is not a failure: `isPartial` and the
    // diagnostics state it.
    if r.status == "error" {
        let code = diagnostics
            .first()
            .and_then(|diagnostic| diagnostic["code"].as_str())
            .map_or("executionFailed", super::engine_error_code);
        out["errorCode"] = json!(code);
    }
    if r.next_offset.is_some() {
        let mut nq = serde_json::to_value(q).unwrap_or_default();
        if let Some(map) = nq.as_object_mut() {
            map.retain(|_, value| !value.is_null());
        }
        nq["snapshot"] = json!(snapshot);
        nq["page"] = json!(page + 1);
        out["next"] = json!({"nextPage":crate::tools::result::Continuation::new(ToolId::AstSearch, nq).confidence("exact").build()})
    } else if r.status == "partial" {
        out["terminalLimit"] = json!(true)
    }
    Ok(out)
}
/// One node as `"<id> <kind> <startLine>:<startColumn>-<endLine>:<endColumn>
/// ^<parentId>"` (1-based lines and columns, end exclusive). An anonymous token's kind
/// is quoted (`"("`), as tree-sitter prints it; the root has no parent.
fn node_row(id: u32, kind: &str, named: bool, span: [u32; 4], parent: Option<u32>) -> String {
    let [start_line, start_column, end_line, end_column] = span;
    let kind = if named {
        kind.to_owned()
    } else {
        format!("\"{kind}\"")
    };
    // 1-based lines and columns (D2); the end stays exclusive.
    let (start_column, end_column) = (
        crate::tools::num::one_based_column(start_column),
        crate::tools::num::one_based_column(end_column),
    );
    let mut row = format!("{id} {kind} {start_line}:{start_column}-{end_line}:{end_column}");
    if let Some(parent) = parent {
        row.push_str(&format!(" ^{parent}"));
    }
    row
}
