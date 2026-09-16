use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine_core::types::GraphFactsScanOptions;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AstSymbolsQuery {
    pub goal: Option<String>,
    pub reasoning: Option<String>,
    #[serde(default = "op")]
    pub operation: String,
    pub path: String,
    pub name: Option<String>,
    pub kinds: Option<Vec<String>>,
    pub exclude_dir: Option<Vec<String>>,
    pub max_files: Option<u32>,
    #[serde(default = "one")]
    pub page: u32,
    #[serde(default = "hundred")]
    pub page_size: u32,
    pub snapshot: Option<String>,
}
fn op() -> String {
    "symbols".into()
}
const fn one() -> u32 {
    1
}
const fn hundred() -> u32 {
    100
}
pub fn execute_symbols(
    q: &AstSymbolsQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    let p = paths.validate(&q.path).map_err(super::AstError::from)?;
    let meta = std::fs::metadata(&p.canonical).map_err(super::io_error)?;
    let (mut entries, truncated, mut skipped, mut diagnostics) = if meta.is_file() {
        let b = std::fs::read(&p.canonical).map_err(super::io_error)?;
        if b.len() > 1_000_000 {
            return Ok(limit(
                &p.canonical
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
            ));
        }
        let s = security
            .validate_text_bytes(&b, Some(&p.canonical), 1_000_000)
            .map_err(super::AstError::from)?;
        match octocode_engine_core::portable::extract_graph_facts(
            &s.content,
            &p.canonical.to_string_lossy(),
        ) {
            Some(raw) => (vec![(p.display.clone(), raw)], false, 0, vec![]),
            None => {
                return Ok(
                    json!({"status":"error","errorCode":"ast.symbols.unsupported","path":super::display_name(&p.canonical),"error":"No native declaration extractor supports this source. Inspect its syntax tree or exact content."}),
                );
            }
        }
    } else {
        let r = octocode_engine_core::portable::scan_graph_facts_filtered(
            GraphFactsScanOptions {
                path: p.canonical.to_string_lossy().into_owned(),
                exclude_dir: q.exclude_dir.clone(),
                max_files: Some(q.max_files.unwrap_or(2000)),
                max_file_bytes: Some(1_000_000),
            },
            &|path| super::allow_discovery(path, paths, cancel),
        )
        .map_err(super::native_error)?;
        let ds=r.skipped.iter().map(|d|json!({"path":p.canonical.join(&d.relative_path).to_string_lossy(),"message":format!("{}: {}",d.code,d.message)})).collect();
        (
            r.entries
                .into_iter()
                .map(|e| {
                    (
                        p.canonical
                            .join(&e.relative_path)
                            .to_string_lossy()
                            .into_owned(),
                        e.facts_json,
                    )
                })
                .collect(),
            r.truncated,
            r.files_skipped,
            ds,
        )
    };
    cancel.check().map_err(super::cancelled)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut declarations = vec![];
    for (path, raw) in &entries {
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            skipped += 1;
            diagnostics.push(json!({"path":path,"message":"graph facts decode failed"}));
            continue;
        };
        if let Some(ds) = v["diagnostics"].as_array() {
            diagnostics.extend(
                ds.iter()
                    .filter_map(Value::as_str)
                    .map(|m| json!({"path":path,"message":m})),
            )
        }
        if let Some(ds) = v["declarations"].as_array() {
            for d in ds {
                let name = d["name"].as_str().unwrap_or("");
                let kind = d["kind"].as_str().unwrap_or("");
                if q.name.as_ref().is_none_or(|n| name.contains(n))
                    && q.kinds
                        .as_ref()
                        .is_none_or(|ks| ks.iter().any(|k| k == kind))
                {
                    let mut d = d.clone();
                    d["path"] = json!(path);
                    declarations.push(d)
                }
            }
        }
    }
    let snapshot = super::syntax::digest(&json!([
        q.path,
        q.name,
        q.kinds,
        q.exclude_dir,
        q.max_files.unwrap_or(2000),
        &declarations,
        &diagnostics,
        truncated,
        skipped
    ]));
    for declaration in &mut declarations {
        if let Some(path) = declaration["path"].as_str() {
            declaration["path"] = json!(security.sanitize_text(&paths.redact(path), None).content);
        }
    }
    for diagnostic in &mut diagnostics {
        if let Some(path) = diagnostic["path"].as_str() {
            diagnostic["path"] = json!(security.sanitize_text(&paths.redact(path), None).content);
        }
    }
    if q.page > 1 && q.snapshot.as_deref() != Some(&snapshot) {
        return Ok(
            json!({"status":"error","errorCode":"ast.snapshot.changed","error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.","snapshot":snapshot,"complete":false}),
        );
    }
    let size = q.page_size.clamp(1, 1000) as usize;
    let page = q.page.max(1) as usize;
    let start = (page - 1) * size;
    let more = start + size < declarations.len();
    let incomplete = truncated || skipped > 0;
    let mut out = json!({"operation":"symbols","path":super::display_name(&p.canonical),"snapshot":snapshot,"declarations":declarations.get(start..(start+size).min(declarations.len())).unwrap_or(&[]),"totalDeclarations":declarations.len(),"filesScanned":entries.len(),"filesSkipped":skipped,"diagnostics":diagnostics,"complete":!more&&!incomplete,"isPartial":more||incomplete,"pagination":{"currentPage":page,"totalPages":declarations.len().div_ceil(size).max(1),"hasMore":more}});
    if incomplete {
        out["terminalLimit"] = json!(true)
    }
    if more {
        let mut nq = serde_json::to_value(q).unwrap_or_default();
        if let Some(map) = nq.as_object_mut() {
            map.retain(|_, value| !value.is_null());
        }
        nq["goal"] = json!("Execute astSearch via octocode");
        nq["reasoning"] = json!("Executed via octocode tool command");
        nq["maxFiles"] = json!(q.max_files.unwrap_or(2000));
        nq["snapshot"] = json!(snapshot);
        nq["page"] = json!(page + 1);
        out["next"] = json!({"nextPage":{"tool":"astSearch","query":nq}})
    }
    if declarations.is_empty() && !incomplete {
        out["status"] = json!("empty")
    }
    Ok(out)
}
fn limit(path: &str) -> Value {
    json!({"path":path,"errorCode":"ast.source.limit","error":"Source exceeds the native parser byte limit.","complete":false,"terminalLimit":true})
}
