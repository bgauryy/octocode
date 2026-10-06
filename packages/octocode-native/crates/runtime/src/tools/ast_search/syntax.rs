use crate::tools::id::ToolId;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use serde_json::json;
const MAX_SOURCE: usize = super::MAX_PARSE_SOURCE_BYTES;
pub use crate::contracts::tool_types::AstSearchQuerySyntaxTree;

/// Engine-unit views over the generated `syntaxTree` query.
impl AstSearchQuerySyntaxTree {
    pub fn language(&self) -> Option<String> {
        self.language.as_ref().map(ToString::to_string)
    }
    pub fn page(&self) -> u32 {
        u32::try_from(self.page.get()).unwrap_or(u32::MAX)
    }
    pub fn page_size(&self) -> u32 {
        u32::try_from(self.page_size.get()).unwrap_or(u32::MAX)
    }
    /// The first node of this page.
    pub fn node_offset(&self) -> u32 {
        self.page()
            .saturating_sub(1)
            .saturating_mul(self.page_size())
    }
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
}

pub fn execute_syntax(
    q: &AstSearchQuerySyntaxTree,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    let p = paths
        .validate_read(q.path.as_str())
        .map_err(super::AstError::from)?;
    super::validate_file_language(&p.canonical, q.language().as_deref())?;
    let Some(bytes) = super::read_parse_source(&p.canonical)? else {
        return Ok(super::source_limit(&super::display_name(&p.canonical)));
    };
    let sanitized = security
        .validate_text_bytes(&bytes, Some(&p.canonical), MAX_SOURCE)
        .map_err(super::AstError::from)?;
    let snapshot = crate::digest::json_sha256(&json!([
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
    cancel.check().map_err(super::cancelled)?;
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
    let diagnostics=r.diagnostics.into_iter().map(|d|json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":super::display_name(&p.canonical),"recovery":d.recovery})).collect::<Vec<_>>();
    let more = r.next_offset.is_some();
    let complete = r.status == "ok" && !more;
    let display_path = super::display_name(&p.canonical);
    let page = q.page();
    let total_pages = r.total_nodes.div_ceil(q.page_size().max(1)).max(1);
    let mut out = json!({"operation":"syntaxTree","path":display_path,"nodes":nodes,"nodeBytes":node_bytes,"snapshot":snapshot,"isPartial":!complete,
        "pagination":{"currentPage":page,"totalPages":total_pages,"totalItems":r.total_nodes,"hasMore":more}});
    if more {
        out["pagination"]["nextPage"] = json!(page + 1);
    }
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    if r.status != "ok" {
        out["errorCode"] = json!(format!("ast.syntax.{}", r.status))
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
/// ^<parentId>"` (1-based lines, 0-based columns). An anonymous token's kind
/// is quoted (`"("`), as tree-sitter prints it; the root has no parent.
fn node_row(id: u32, kind: &str, named: bool, span: [u32; 4], parent: Option<u32>) -> String {
    let [start_line, start_column, end_line, end_column] = span;
    let kind = if named {
        kind.to_owned()
    } else {
        format!("\"{kind}\"")
    };
    let mut row = format!("{id} {kind} {start_line}:{start_column}-{end_line}:{end_column}");
    if let Some(parent) = parent {
        row.push_str(&format!(" ^{parent}"));
    }
    row
}
