pub use crate::contracts::tool_types::AstSearchQuerySymbols;
use crate::policy::prune::DefaultsFlag;
use crate::{
    policy::{path::PathPolicy, prune::PruneMode},
    security::ContentSecurity,
    tools::cancel::CancellationCheck,
};
use octocode_engine::types::{GraphFactsScanOptions, GraphLanguageGlob};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn u32_of(value: std::num::NonZeroU64) -> u32 {
    u32::try_from(value.get()).unwrap_or(u32::MAX)
}

/// Engine-unit views over the generated `symbols` query.
impl AstSearchQuerySymbols {
    pub fn lang_type(&self) -> Option<String> {
        self.lang_type.as_ref().map(ToString::to_string)
    }
    pub fn language_globs(&self) -> Option<&BTreeMap<String, Vec<String>>> {
        (!self.language_globs.is_empty()).then_some(&self.language_globs)
    }
    pub fn kinds(&self) -> Option<&Vec<String>> {
        (!self.kinds.is_empty()).then_some(&self.kinds)
    }
    pub fn exclude_dir(&self) -> Option<Vec<String>> {
        (!self.exclude_dir.is_empty()).then(|| self.exclude_dir.clone())
    }
    pub fn max_files(&self) -> u32 {
        u32_of(self.max_files)
    }
    pub fn page(&self) -> u32 {
        u32_of(self.page)
    }
    pub fn page_size(&self) -> u32 {
        u32_of(self.page_size)
    }
    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref().map(String::as_str)
    }
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
    q: &AstSearchQuerySymbols,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    if let Some(unknown) = q
        .kinds
        .iter()
        .find(|kind| !DECLARATION_KINDS.contains(&kind.as_str()))
    {
        return Err(super::AstError::new(
            "ast.symbols.input.invalid",
            format!(
                "Unknown declaration kind \"{unknown}\". Use one of: {}.",
                DECLARATION_KINDS.join(", ")
            ),
        ));
    }
    let p = paths
        .validate(q.path.as_str())
        .map_err(super::AstError::from)?;
    let meta = std::fs::metadata(&p.canonical).map_err(super::io_error)?;
    if meta.is_file() && q.language_globs().is_some() {
        return Err(super::AstError::new(
            "ast.language.directoryRequired",
            "languageGlobs is for directory symbols. Use langType for a single file.",
        ));
    }
    if meta.is_file() {
        super::validate_file_language(&p.canonical, q.lang_type().as_deref())?;
    } else if q.lang_type.is_some() {
        let mut error = super::AstError::new(
            "ast.language.fileRequired",
            "langType on symbols requires a single source file.",
        );
        // Directory symbols pick each file's grammar from its extension, so
        // the same query without langType is the exact repair.
        if let Ok(mut repaired) = serde_json::to_value(q) {
            if let Some(object) = repaired.as_object_mut() {
                object.remove("langType");
            }
            error.next = Some(Box::new(json!({
                "repair": {"tool": "astSearch", "confidence": "exact", "query": repaired}
            })));
        }
        return Err(error);
    }
    let (mut entries, truncated, mut skipped, mut diagnostics) = if meta.is_file() {
        let b = std::fs::read(&p.canonical).map_err(super::io_error)?;
        if b.len() > super::MAX_PARSE_SOURCE_BYTES {
            return Ok(limit(
                &p.canonical
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
            ));
        }
        let s = security
            .validate_text_bytes(&b, Some(&p.canonical), super::MAX_PARSE_SOURCE_BYTES)
            .map_err(super::AstError::from)?;
        let source_path = p.canonical.to_string_lossy();
        let raw = if super::cpp_header_override(&p.canonical, q.lang_type().as_deref()) {
            octocode_engine::portable::extract_graph_facts_with_extension(
                &s.content,
                &source_path,
                "cpp",
            )
        } else {
            octocode_engine::portable::extract_declarations(&s.content, &source_path)
        };
        match raw {
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
                exclude_dir: Some(
                    PruneMode::SyntaxVisible
                        .directories(&q.exclude_dir, q.default_excludes.defaults()),
                ),
                max_files: Some(q.max_files()),
                max_file_bytes: u32::try_from(super::MAX_PARSE_SOURCE_BYTES).ok(),
                language_globs: q.language_globs().map(|map| {
                    map.iter()
                        .flat_map(|(language, globs)| {
                            globs.iter().map(|glob| GraphLanguageGlob {
                                language: language.clone(),
                                glob: glob.clone(),
                            })
                        })
                        .collect()
                }),
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
    let mut recovered = false;
    for (path, raw) in &entries {
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            skipped += 1;
            diagnostics.push(json!({"path":path,"message":"graph facts decode failed"}));
            continue;
        };
        if let Some(ds) = v["diagnostics"].as_array() {
            for m in ds.iter().filter_map(Value::as_str) {
                // The syntax-only caveat is static and already in the tool
                // description; repeating it costs every call.
                if is_linking_only(m) || m == SYNTAX_ONLY_NOTE {
                    continue;
                } else {
                    recovered |= m.starts_with(RECOVERED_PARSE_NOTE_PREFIX);
                    if per_row_path {
                        diagnostics.push(json!({"path":path,"message":m}));
                    } else {
                        diagnostics.push(json!({"message":m}));
                    }
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
                && q.kinds().is_none_or(|ks| ks.iter().any(|k| k == kind))
            {
                if per_row_path {
                    row["path"] = json!(path);
                }
                declarations.push(row)
            }
        }
    }
    let snapshot = super::syntax::digest(&json!([
        q.path,
        q.lang_type,
        q.language_globs(),
        q.name,
        q.kinds(),
        q.exclude_dir(),
        q.max_files(),
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
    let set = SymbolSet {
        path: super::display_name(&p.canonical),
        snapshot,
        declarations,
        diagnostics,
        files_scanned: entries.len(),
        skipped,
        truncated,
        recovered,
    };
    Ok(render_page(q, &set))
}

/// Every declaration of one symbols query, before paging.
struct SymbolSet {
    path: String,
    snapshot: String,
    declarations: Vec<Value>,
    diagnostics: Vec<Value>,
    files_scanned: usize,
    skipped: u32,
    truncated: bool,
    /// A parser recovered from syntax errors: the declarations are a partial
    /// view of the source, not a complete inventory.
    recovered: bool,
}

fn render_page(q: &AstSearchQuerySymbols, set: &SymbolSet) -> Value {
    let SymbolSet {
        path,
        snapshot,
        declarations,
        diagnostics,
        files_scanned,
        skipped,
        truncated,
        recovered,
    } = set;
    let (skipped, truncated) = (*skipped, *truncated);
    if q.page() > 1 && q.snapshot() != Some(snapshot.as_str()) {
        return json!({"status":"error","errorCode":"ast.snapshot.changed","error":"The source or query changed, or this continuation omitted its snapshot. Discard earlier pages and restart.","snapshot":snapshot,"complete":false});
    }
    let size = q.page_size().clamp(1, 1000) as usize;
    let page = q.page().max(1) as usize;
    let start = (page - 1) * size;
    let more = start + size < declarations.len();
    let incomplete = truncated || skipped > 0;
    let rows = declarations
        .get(start..(start + size).min(declarations.len()))
        .unwrap_or(&[]);
    let mut out = json!({"operation":"symbols","path":path,"totalDeclarations":declarations.len(),"filesScanned":files_scanned,"filesSkipped":skipped,"diagnostics":diagnostics,"isPartial":more||incomplete||*recovered});
    // A directory outline groups rows under their file, like `match` results,
    // so each path is written once instead of on every declaration.
    if rows.iter().any(|row| row.get("path").is_some()) {
        out["files"] = Value::Array(group_by_file(rows));
    } else {
        out["declarations"] = json!(rows);
    }
    // The snapshot only pins later pages to the same source; a single page
    // has nothing to pin.
    if more || page > 1 {
        out["snapshot"] = json!(snapshot);
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
        nq["maxFiles"] = json!(q.max_files());
        nq["snapshot"] = json!(snapshot);
        nq["page"] = json!(page + 1);
        out["next"] = json!({"nextPage":{"tool":"astSearch","query":nq,"confidence":"exact"}})
    }
    if declarations.is_empty() && !incomplete {
        out["status"] = json!("empty")
    }
    out
}
/// Consecutive rows of one file become `{path, declarations}`; rows arrive in
/// path order, so each file appears once per page.
fn group_by_file(rows: &[Value]) -> Vec<Value> {
    let mut files: Vec<Value> = Vec::new();
    for row in rows {
        let mut row = row.clone();
        let path = row
            .as_object_mut()
            .and_then(|fields| fields.remove("path"))
            .unwrap_or(Value::Null);
        match files.last_mut() {
            Some(file) if file["path"] == path => {
                if let Some(list) = file["declarations"].as_array_mut() {
                    list.push(row);
                }
            }
            _ => files.push(json!({"path":path,"declarations":[row]})),
        }
    }
    files
}

fn limit(path: &str) -> Value {
    json!({"status":"error","path":path,"errorCode":"ast.source.limit","error":"Source exceeds the native parser byte limit.","complete":false,"terminalLimit":true})
}
/// Import/module-linking caveats from graph facts. Declaration listing never
/// links imports or modules, so these only add noise to symbols output.
fn is_linking_only(message: &str) -> bool {
    message.starts_with("unsupported Rust macro expansion")
        || message.starts_with("unsupported Rust conditional or custom module attributes")
        || message.starts_with("unsupported Rust inner conditional or custom crate attributes")
}

/// Engine diagnostic for a tree-sitter parse that recovered from syntax errors.
const RECOVERED_PARSE_NOTE_PREFIX: &str = "tree-sitter recovered from parse errors";

/// Static engine caveat attached to every tree-sitter graph-facts file.
const SYNTAX_ONLY_NOTE: &str =
    "tree-sitter graph facts are syntax-only; use LSP references/callHierarchy for semantic proof";

/// Projects engine declaration facts (one file) onto compact response rows.
///
/// `line` (1-based) anchors the declaration NAME, so a row feeds lspSearch as
/// `symbolName`+`lineHint`; `character` (0-based) appears only when another
/// declaration of the same kind shares that name and line, so the column is
/// the only difference. `endLine` (1-based) ends the declaration and is omitted when
/// equal to `line`; `startLine` appears only when the declaration starts
/// before its name line (decorators, attributes); `docStartLine` is the first
/// line of the comment block directly above, when present. `parent` names the
/// containing declaration and stays meaningful when a `kinds` or `name`
/// filter drops the parent row; `parentLine` is added only when another
/// declaration in the file has the same name and kind (two `impl A` blocks). `exported` appears only when true, with
/// `exportedAs` listing public names that differ from `name`. Rows are
/// returned in input order, one per engine declaration.
fn compact_declarations(raw: &[Value]) -> Vec<Value> {
    let pos = |d: &Value, range: &str, edge: &str, field: &str| {
        d.pointer(&format!("/{range}/{edge}/{field}"))
            .and_then(Value::as_u64)
    };
    let anchors: Vec<(&str, u64, u64)> = raw
        .iter()
        .map(|d| {
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
            (d["name"].as_str().unwrap_or(""), line, character)
        })
        .collect();
    let mut per_name_kind = std::collections::HashMap::<(&str, &str), usize>::new();
    let mut per_name_line = std::collections::HashMap::<(&str, u64, &str), usize>::new();
    for (d, (name, line, _)) in raw.iter().zip(&anchors) {
        let kind = d["kind"].as_str().unwrap_or("");
        *per_name_kind.entry((name, kind)).or_default() += 1;
        *per_name_line.entry((name, *line, kind)).or_default() += 1;
    }
    let by_engine_id: std::collections::HashMap<&str, usize> = raw
        .iter()
        .enumerate()
        .filter_map(|(index, d)| Some((d["id"].as_str()?, index)))
        .collect();
    raw.iter()
        .zip(&anchors)
        .map(|(d, &(name, line, character))| {
            let kind = d["kind"].as_str().unwrap_or("");
            let mut row = json!({"name":name,"kind":kind,"line":line});
            if per_name_line.get(&(name, line, kind)).copied().unwrap_or(0) > 1 {
                row["character"] = json!(character);
            }
            if let Some(start) = pos(d, "range", "start", "line").map(|l| l + 1)
                && start < line
            {
                row["startLine"] = json!(start);
            }
            if let Some(doc) = d["docLine"].as_u64() {
                row["docStartLine"] = json!(doc + 1);
            }
            if let Some(end) = pos(d, "range", "end", "line").map(|l| l + 1)
                && end != line
            {
                row["endLine"] = json!(end);
            }
            if d["exported"].as_bool() == Some(true) {
                row["exported"] = json!(true);
                // Public names when exported under another name (`export { foo
                // as bar }`, `export default function foo`).
                if let Some(public) = d.get("exportedAs").filter(|v| v.is_array()) {
                    row["exportedAs"] = public.clone();
                }
            }
            // Parents precede children in engine preorder; an unknown parent
            // (never expected) is dropped rather than leaking an engine id.
            if let Some(&index) = d["parent"].as_str().and_then(|id| by_engine_id.get(id))
                && let Some(&(parent, parent_line, _)) = anchors.get(index)
            {
                row["parent"] = json!(parent);
                let parent_kind = raw[index]["kind"].as_str().unwrap_or("");
                if per_name_kind
                    .get(&(parent, parent_kind))
                    .copied()
                    .unwrap_or(0)
                    > 1
                {
                    row["parentLine"] = json!(parent_line);
                }
            }
            row
        })
        .collect()
}
