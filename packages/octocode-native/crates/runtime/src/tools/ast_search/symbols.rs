use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine::types::GraphFactsScanOptions;
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
/// Every declaration kind the native extractors emit: tree-sitter graph facts
/// (engine `signatures::nodes::declaration_kind`) and the JS/TS oxc extractor
/// (`signatures::js_oxc::symbol_kind_name`).
const DECLARATION_KINDS: &[&str] = &[
    "class",
    "constant",
    "constructor",
    "enum",
    "enumMember",
    "function",
    "impl",
    "interface",
    "label",
    "macro",
    "method",
    "module",
    "namespace",
    "property",
    "struct",
    "symbol",
    "trait",
    "type",
    "variable",
];
pub fn execute_symbols(
    q: &AstSymbolsQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    if let Some(unknown) = q
        .kinds
        .iter()
        .flatten()
        .find(|kind| !DECLARATION_KINDS.contains(&kind.as_str()))
    {
        return Err(super::AstError::new(
            "ast.symbols.invalidKind",
            format!(
                "Unknown declaration kind \"{unknown}\". Use one of: {}.",
                DECLARATION_KINDS.join(", ")
            ),
        ));
    }
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
        match octocode_engine::portable::extract_graph_facts(
            &s.content,
            &p.canonical.to_string_lossy(),
        ) {
            Some(raw) => (
                vec![(super::display_name(&p.canonical), raw)],
                false,
                0,
                vec![],
            ),
            None => {
                return Ok(
                    json!({"status":"error","errorCode":"ast.symbols.unsupported","path":super::display_name(&p.canonical),"error":"No native declaration extractor supports this source. Inspect its syntax tree or exact content."}),
                );
            }
        }
    } else {
        let r = octocode_engine::portable::scan_graph_facts_filtered(
            GraphFactsScanOptions {
                path: p.canonical.to_string_lossy().into_owned(),
                exclude_dir: q.exclude_dir.clone(),
                max_files: Some(q.max_files.unwrap_or(2000)),
                max_file_bytes: Some(1_000_000),
            },
            &|path| super::allow_discovery(path, paths, cancel),
        )
        .map_err(super::native_error)?;
        let rooted =
            |relative: &str| super::rooted_display(&p.canonical, std::path::Path::new(relative));
        let ds=r.skipped.iter().map(|d|json!({"path":rooted(&d.relative_path),"message":format!("{}: {}",d.code,d.message)})).collect();
        (
            r.entries
                .into_iter()
                .map(|e| (rooted(&e.relative_path), e.facts_json))
                .collect(),
            r.truncated,
            r.files_skipped,
            ds,
        )
    };
    cancel.check().map_err(super::cancelled)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    // A single-file query hoists `path` to the top level instead of repeating it.
    let per_row_path = !meta.is_file();
    let mut declarations = vec![];
    let mut syntax_only_note = false;
    for (path, raw) in &entries {
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            skipped += 1;
            diagnostics.push(json!({"path":path,"message":"graph facts decode failed"}));
            continue;
        };
        if let Some(ds) = v["diagnostics"].as_array() {
            for m in ds.iter().filter_map(Value::as_str) {
                if m == SYNTAX_ONLY_NOTE {
                    syntax_only_note = true;
                } else if per_row_path {
                    diagnostics.push(json!({"path":path,"message":m}));
                } else {
                    diagnostics.push(json!({"message":m}));
                }
            }
        }
        let Some(ds) = v["declarations"].as_array() else {
            continue;
        };
        for (d, mut row) in ds.iter().zip(compact_declarations(ds)) {
            let name = d["name"].as_str().unwrap_or("");
            let kind = d["kind"].as_str().unwrap_or("");
            if q.name.as_ref().is_none_or(|n| name.contains(n))
                && q.kinds
                    .as_ref()
                    .is_none_or(|ks| ks.iter().any(|k| k == kind))
            {
                if per_row_path {
                    row["path"] = json!(path);
                }
                declarations.push(row)
            }
        }
    }
    // The static syntax-only caveat is identical for every file: emit it once.
    if syntax_only_note {
        diagnostics.insert(0, json!({"message":SYNTAX_ONLY_NOTE}));
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
    for row in declarations.iter_mut().chain(diagnostics.iter_mut()) {
        if let Some(path) = row["path"].as_str() {
            row["path"] = json!(security.sanitize_text(path, None).content);
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
    let mut out = json!({"operation":"symbols","path":super::display_name(&p.canonical),"snapshot":snapshot,"declarations":declarations.get(start..(start+size).min(declarations.len())).unwrap_or(&[]),"totalDeclarations":declarations.len(),"filesScanned":entries.len(),"filesSkipped":skipped,"diagnostics":diagnostics,"isPartial":more||incomplete});
    if more || page > 1 {
        out["pagination"] = json!({"currentPage":page,"totalPages":declarations.len().div_ceil(size).max(1),"hasMore":more});
    }
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
    json!({"status":"error","path":path,"errorCode":"ast.source.limit","error":"Source exceeds the native parser byte limit.","complete":false,"terminalLimit":true})
}
/// Static engine caveat attached to every tree-sitter graph-facts file.
const SYNTAX_ONLY_NOTE: &str =
    "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof";

/// Projects engine declaration facts (one file) onto compact response rows.
///
/// Positions mirror lspSearch document symbols: `line` (1-based) and
/// `character` (0-based) anchor the declaration NAME, so they feed back as
/// `symbolName`+`lineHint` or a zero-based `position`; `endLine` (1-based) is
/// the end of the declaration and is omitted when equal to `line`;
/// `startLine` appears only when the declaration starts before its name line
/// (decorators, attributes). `id` is `name@line:character`, unique within its
/// file (`path`); `parent` uses the same scheme. `exported` appears only when
/// true. Rows are returned in input order, one per engine declaration.
fn compact_declarations(raw: &[Value]) -> Vec<Value> {
    let pos = |d: &Value, range: &str, edge: &str, field: &str| {
        d.pointer(&format!("/{range}/{edge}/{field}"))
            .and_then(Value::as_u64)
    };
    let mut seen = std::collections::HashSet::new();
    let mut ids = std::collections::HashMap::new();
    let mut rows = Vec::with_capacity(raw.len());
    for d in raw {
        let name = d["name"].as_str().unwrap_or("");
        let kind = d["kind"].as_str().unwrap_or("");
        let anchor = if d.get("selectionRange").is_some() {
            "selectionRange"
        } else {
            "range"
        };
        let line = pos(d, anchor, "start", "line")
            .map(|l| l + 1)
            .or_else(|| d["line"].as_u64())
            .unwrap_or(0);
        let character = pos(d, anchor, "start", "character").unwrap_or(0);
        let mut id = format!("{name}@{line}:{character}");
        if !seen.insert(id.clone()) {
            id = format!("{id}:{kind}");
            let base = id.clone();
            let mut n = 2;
            while !seen.insert(id.clone()) {
                id = format!("{base}#{n}");
                n += 1;
            }
        }
        if let Some(engine_id) = d["id"].as_str() {
            ids.insert(engine_id.to_owned(), id.clone());
        }
        let mut row = json!({"id":id,"name":name,"kind":kind,"line":line,"character":character});
        if let Some(start) = pos(d, "range", "start", "line").map(|l| l + 1)
            && start < line
        {
            row["startLine"] = json!(start);
        }
        if let Some(end) = pos(d, "range", "end", "line").map(|l| l + 1)
            && end != line
        {
            row["endLine"] = json!(end);
        }
        if d["exported"].as_bool() == Some(true) {
            row["exported"] = json!(true);
        }
        rows.push(row);
    }
    for (d, row) in raw.iter().zip(rows.iter_mut()) {
        if let Some(parent) = d["parent"].as_str() {
            // Parents precede children in engine preorder; an unknown parent id
            // (never expected) is dropped rather than leaking the absolute path.
            if let Some(compact) = ids.get(parent) {
                row["parent"] = json!(compact);
            }
        }
    }
    rows
}
