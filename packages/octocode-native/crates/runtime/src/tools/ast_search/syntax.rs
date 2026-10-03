use crate::tools::id::ToolId;
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::cancel::CancellationCheck,
};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const MAX_SOURCE: usize = super::MAX_PARSE_SOURCE_BYTES;
pub use crate::contracts::tool_types::AstSearchQuerySyntaxTree;

/// Engine-unit views over the generated `syntaxTree` query.
impl AstSearchQuerySyntaxTree {
    pub fn lang_type(&self) -> Option<String> {
        self.lang_type.as_ref().map(ToString::to_string)
    }
    pub fn node_offset(&self) -> u32 {
        u32::try_from(self.node_offset.max(0)).unwrap_or(u32::MAX)
    }
    pub fn node_limit(&self) -> u32 {
        u32::try_from(self.node_limit.get()).unwrap_or(u32::MAX)
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
    super::validate_file_language(&p.canonical, q.lang_type().as_deref())?;
    let bytes = std::fs::read(&p.canonical).map_err(super::io_error)?;
    if bytes.len() > MAX_SOURCE {
        return Ok(
            json!({"status":"error","path":p.canonical.file_name().unwrap_or_default().to_string_lossy(),"errorCode":"ast.source.limit","error":"Source exceeds the native parser byte limit.","complete":false,"terminalLimit":true}),
        );
    }
    let sanitized = security
        .validate_text_bytes(&bytes, Some(&p.canonical), MAX_SOURCE)
        .map_err(super::AstError::from)?;
    let snapshot = digest(&json!([
        p.canonical.to_string_lossy(),
        q.lang_type,
        sanitized.content,
        q.named_only
    ]));
    if q.node_offset() > 0 && q.snapshot() != Some(snapshot.as_str()) {
        let mut restart = serde_json::to_value(q).unwrap_or_default();
        restart["nodeOffset"] = json!(0);
        restart.as_object_mut().map(|m| m.remove("snapshot"));
        return Ok(
            json!({"status":"error","errorCode":"ast.snapshot.changed","error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.","snapshot":snapshot,"complete":false,"next":{"restart":{"tool":ToolId::AstSearch.as_str(),"query":restart}}}),
        );
    }
    cancel.check().map_err(super::cancelled)?;
    let r = octocode_engine::portable::inspect_syntax_tree_with_extension(
        &sanitized.content,
        &p.canonical.to_string_lossy(),
        super::cpp_header_override(&p.canonical, q.lang_type().as_deref()).then_some("cpp"),
        Some(SyntaxTreeInspectOptions {
            named_only: Some(q.named_only),
            node_offset: Some(q.node_offset()),
            node_limit: Some(q.node_limit()),
        }),
    )
    .map_err(super::native_error)?;
    // Line/column locate every node; byte offsets double each node's size and
    // are diagnostics, so they ride only on `debug`.
    let nodes = r
        .nodes
        .into_iter()
        .map(|n| {
            let mut v = json!({"id":n.id,"kind":n.kind,"named":n.named,"startLine":n.start_line,"startColumn":n.start_column,"endLine":n.end_line,"endColumn":n.end_column});
            if q.debug {
                v["startByte"] = json!(n.start_byte);
                v["endByte"] = json!(n.end_byte);
            }
            if let Some(parent) = n.parent_id {
                v["parentId"] = json!(parent)
            }
            v
        })
        .collect::<Vec<_>>();
    let diagnostics=r.diagnostics.into_iter().map(|d|json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":super::display_name(&p.canonical),"recovery":d.recovery})).collect::<Vec<_>>();
    let more = r.next_offset.is_some();
    let complete = r.status == "ok" && !more;
    let display_path = p
        .canonical
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let mut out = json!({"operation":"syntaxTree","path":display_path,"nodes":nodes,"totalNodes":r.total_nodes,"snapshot":snapshot,"isPartial":!complete});
    if !diagnostics.is_empty() {
        out["diagnostics"] = json!(diagnostics);
    }
    if r.status != "ok" {
        out["errorCode"] = json!(format!("ast.syntax.{}", r.status))
    }
    if let Some(next) = r.next_offset {
        out["nextOffset"] = json!(next);
        let mut nq = serde_json::to_value(q).unwrap_or_default();
        if let Some(map) = nq.as_object_mut() {
            map.retain(|_, value| !value.is_null());
        }
        nq["snapshot"] = json!(snapshot);
        nq["nodeOffset"] = json!(next);
        out["next"] =
            json!({"nextPage":{"tool":ToolId::AstSearch.as_str(),"query":nq,"confidence":"exact"}})
    } else if r.status == "partial" {
        out["terminalLimit"] = json!(true)
    }
    Ok(out)
}
pub(super) fn digest(v: &Value) -> String {
    let mut h = Sha256::new();
    h.update(serde_json::to_vec(v).unwrap_or_default());
    hex::encode(h.finalize())
}
