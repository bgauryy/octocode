use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine::structural::SyntaxTreeInspectOptions;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const MAX_SOURCE: usize = 1_000_000;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AstSyntaxQuery {
    pub goal: Option<String>,
    pub reasoning: Option<String>,
    #[serde(default = "tree")]
    pub operation: String,
    #[serde(default = "syntax")]
    pub tree_kind: String,
    pub path: String,
    #[serde(default = "yes")]
    pub named_only: bool,
    #[serde(default)]
    pub node_offset: u32,
    #[serde(default = "thousand")]
    pub node_limit: u32,
    pub snapshot: Option<String>,
}
fn tree() -> String {
    "tree".into()
}
fn syntax() -> String {
    "syntax".into()
}
const fn yes() -> bool {
    true
}
const fn thousand() -> u32 {
    100
}
pub fn execute_syntax(
    q: &AstSyntaxQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    let p = paths
        .validate_read(&q.path)
        .map_err(super::AstError::from)?;
    let bytes = std::fs::read(&p.canonical).map_err(super::io_error)?;
    if bytes.len() > MAX_SOURCE {
        return Ok(
            json!({"path":p.canonical.file_name().unwrap_or_default().to_string_lossy(),"errorCode":"ast.source.limit","error":"Source exceeds the native parser byte limit.","complete":false,"terminalLimit":true}),
        );
    }
    let sanitized = security
        .validate_text_bytes(&bytes, Some(&p.canonical), MAX_SOURCE)
        .map_err(super::AstError::from)?;
    let snapshot = digest(&json!([q.path, sanitized.content, q.named_only]));
    if q.node_offset > 0 && q.snapshot.as_deref() != Some(&snapshot) {
        let mut restart = serde_json::to_value(q).unwrap_or_default();
        restart["nodeOffset"] = json!(0);
        restart.as_object_mut().map(|m| m.remove("snapshot"));
        return Ok(
            json!({"status":"error","errorCode":"ast.snapshot.changed","error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.","snapshot":snapshot,"complete":false,"next":{"restart":{"tool":"astSearch","query":restart}}}),
        );
    }
    cancel.check().map_err(super::cancelled)?;
    let r = octocode_engine::portable::inspect_syntax_tree(
        &sanitized.content,
        &p.canonical.to_string_lossy(),
        Some(SyntaxTreeInspectOptions {
            named_only: Some(q.named_only),
            node_offset: Some(q.node_offset),
            node_limit: Some(q.node_limit),
        }),
    )
    .map_err(super::native_error)?;
    let nodes=r.nodes.into_iter().map(|n|{let mut v=json!({"id":n.id,"kind":n.kind,"named":n.named,"startLine":n.start_line,"startColumn":n.start_column,"endLine":n.end_line,"endColumn":n.end_column,"startByte":n.start_byte,"endByte":n.end_byte});if let Some(parent)=n.parent_id{v["parentId"]=json!(parent)}v}).collect::<Vec<_>>();
    let diagnostics=r.diagnostics.into_iter().map(|d|json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":super::display_name(&p.canonical),"recovery":d.recovery})).collect::<Vec<_>>();
    let more = r.next_offset.is_some();
    let complete = r.status == "ok" && !more;
    let display_path = p
        .canonical
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let mut out = json!({"operation":"tree","treeKind":"syntax","path":display_path,"nodes":nodes,"totalNodes":r.total_nodes,"snapshot":snapshot,"complete":complete,"isPartial":!complete});
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
        nq["goal"] = json!("Execute astSearch via octocode");
        nq["reasoning"] = json!("Executed via octocode tool command");
        nq["snapshot"] = json!(snapshot);
        nq["nodeOffset"] = json!(next);
        out["next"] = json!({"nextPage":{"tool":"astSearch","query":nq}})
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
