use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine_core::structural::{
    StructuralDetailedMatch, StructuralDiagnostic, StructuralSearchFilesOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AstMatchQuery {
    #[serde(default = "op")]
    pub operation: String,
    pub path: String,
    pub pattern: Option<String>,
    pub rule: Option<String>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub exclude_dir: Option<Vec<String>>,
    pub hidden: Option<bool>,
    pub no_ignore: Option<bool>,
    pub max_depth: Option<u32>,
    pub max_files: Option<u32>,
    pub max_matches_per_file: Option<u32>,
    pub context_lines: Option<u32>,
    pub match_content_length: Option<u32>,
    pub sort: Option<String>,
    pub ranking_profile: Option<String>,
    pub lang_type: Option<String>,
    pub capture_text: Option<bool>,
    pub result_view: Option<String>,
    pub reverse: Option<bool>,
    pub page_size: Option<u32>,
    #[serde(default = "one")]
    pub page: u32,
    #[serde(default = "one")]
    pub match_page: u32,
    pub snapshot: Option<String>,
}
fn op() -> String {
    "match".into()
}
const fn one() -> u32 {
    1
}
pub fn execute_match(
    q: &AstMatchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    if q.pattern.is_some() == q.rule.is_some() {
        return Err(super::AstError::new(
            "structural.query.invalid",
            "match requires exactly one of pattern or rule",
        ));
    }
    let p = paths.validate(&q.path).map_err(super::AstError::from)?;
    let meta = std::fs::metadata(&p.canonical).map_err(super::io_error)?;
    if meta.is_dir() && q.lang_type.is_none() {
        return Err(super::AstError::new(
            "ast.language.required",
            "Directory matching requires langType; choose the grammar from the source files.",
        ));
    }
    let mut files = if meta.is_file() {
        let bytes = std::fs::read(&p.canonical).map_err(super::io_error)?;
        let s = security
            .validate_text_bytes(&bytes, Some(&p.canonical), 1_000_000)
            .map_err(super::AstError::from)?;
        let r = octocode_engine_core::portable::structural_search_detailed(
            &s.content,
            &p.canonical.to_string_lossy(),
            q.pattern.as_deref(),
            q.rule.as_deref(),
        )
        .map_err(super::native_error)?;
        if let Some(error) = diagnostic_error(&r.diagnostics) {
            return Err(error);
        }
        vec![(
            p.canonical
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            r.matches,
            r.diagnostics,
            r.status,
        )]
    } else {
        let r = octocode_engine_core::portable::structural_search_files_detailed_filtered(
            StructuralSearchFilesOptions {
                path: p.canonical.to_string_lossy().into_owned(),
                pattern: q.pattern.clone(),
                rule: q.rule.clone(),
                include: q
                    .include
                    .clone()
                    .or_else(|| q.lang_type.as_deref().and_then(language_include_globs)),
                exclude: q.exclude.clone(),
                exclude_dir: q.exclude_dir.clone(),
                hidden: q.hidden,
                no_ignore: q.no_ignore,
                max_depth: q.max_depth.map(|depth| depth.saturating_add(1)),
                max_files: Some(q.max_files.unwrap_or(2_000)),
                max_file_bytes: Some(1_000_000),
            },
            &|path| super::allow_discovery(path, paths, cancel),
        )
        .map_err(super::native_error)?;
        if let Some(error) = diagnostic_error(&r.diagnostics) {
            return Err(error);
        }
        r.files
            .into_iter()
            .map(|f| {
                (
                    security
                        .sanitize_text(&match_display_path(&p.canonical, &f.path), None)
                        .content,
                    f.matches,
                    f.diagnostics,
                    f.status,
                )
            })
            .collect()
    };
    cancel.check().map_err(super::cancelled)?;
    files.sort_by(|a, b| {
        let ordering = if q.sort.as_deref() == Some("matchCount") {
            b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0))
        } else {
            a.0.cmp(&b.0)
        };
        if q.reverse.unwrap_or(false) {
            ordering.reverse()
        } else {
            ordering
        }
    });
    let mut groups = vec![];
    let mut all_diagnostics = vec![];
    let mut total_matches = 0_u64;
    let file_list = matches!(q.result_view.as_deref(), Some("files" | "countMatches"));
    let matches_per_page = q.max_matches_per_file.unwrap_or(100).clamp(1, 1_000) as usize;
    let match_page = q.match_page.max(1) as usize;
    let match_start = (match_page - 1).saturating_mul(matches_per_page);
    let mut has_more_matches = false;
    let mut has_truncated_captures = false;
    for (path, matches, diagnostics, status) in files {
        let matches = matches
            .into_iter()
            .map(|value| match_value(value, q.capture_text.unwrap_or(false)))
            .collect::<Vec<_>>();
        if !matches.is_empty() {
            let total = matches.len();
            total_matches += total as u64;
            if file_list {
                if q.result_view.as_deref() == Some("countMatches") {
                    groups.push(json!({"path":path,"totalOccurrences":total}));
                } else {
                    groups.push(json!({"path":path}));
                }
                continue;
            }
            let match_end = match_start.saturating_add(matches_per_page).min(total);
            let selected = matches.get(match_start..match_end).unwrap_or(&[]).to_vec();
            has_more_matches |= match_end < total;
            has_truncated_captures |= selected
                .iter()
                .any(|value| value["capturesTruncated"] == true);
            let out_of_range = match_start >= total;
            let mut group = json!({
                "path":path,
                "totalMatchRows":total,
                "returnedMatchRows":selected.len(),
                "matches":selected
            });
            if total > matches_per_page || out_of_range {
                let total_pages = total.div_ceil(matches_per_page).max(1);
                let more = match_page < total_pages;
                group["pagination"] = json!({
                    "currentPage":match_page,
                    "totalPages":total_pages,
                    "matchesPerPage":matches_per_page,
                    "totalMatches":total,
                    "hasMore":more
                });
                if more && match_page < 1_000 {
                    group["pagination"]["nextMatchPage"] = json!(match_page + 1);
                }
                if out_of_range {
                    group["pagination"]["outOfRange"] = json!(true);
                }
            }
            groups.push(group);
        }
        if status != "ok" || !diagnostics.is_empty() {
            all_diagnostics.extend(diagnostics.into_iter().map(diag));
        }
    }
    let size = q.page_size.unwrap_or(20).clamp(1, 1_000) as usize;
    let page = q.page.max(1) as usize;
    let start = (page - 1) * size;
    let selected = groups
        .get(start..(start + size).min(groups.len()))
        .unwrap_or(&[]);
    let more = start + size < groups.len();
    let mut out =
        json!({"searchEngine":"structural","stats":{"totalStructuralMatches":total_matches}});
    if !groups.is_empty() {
        out["files"] = json!(selected);
        out["pagination"] = json!({"currentPage":page,"totalPages":groups.len().div_ceil(size).max(1),"filesPerPage":size,"totalFiles":groups.len(),"hasMore":more});
        if q.result_view.as_deref() != Some("files") {
            out["pagination"]["totalMatches"] = json!(total_matches);
        }
    }
    if !all_diagnostics.is_empty() {
        out["diagnostics"] = json!(all_diagnostics)
    }
    if groups.is_empty() && q.pattern.is_some() && all_diagnostics.is_empty() {
        let path = p
            .canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        out["diagnostics"] = json!([{
            "code":"structural.query.noMatches",
            "severity":"info",
            "stage":"match",
            "message":"0 structural matches for the requested pattern in this scope. Check the source syntax: a pattern must match a complete node, including relevant bodies (for example `$$$BODY`), return types, and decorators. For partial or relational matches, supply an explicit YAML `rule` instead of `pattern`.",
            "path":path
        }]);
    }
    if more {
        out["pagination"]["nextPage"] = json!(page + 1);
        out["next"] = json!({"nextPage":continuation(q, page + 1)})
    }
    if has_more_matches && match_page < 1_000 {
        out["next"]["nextMatchPage"] = continuation_with(
            q,
            json!({"maxMatchesPerFile":matches_per_page,"matchPage":match_page+1}),
        );
    }
    if has_truncated_captures && !q.capture_text.unwrap_or(false) {
        out["next"]["expandCaptures"] = continuation_with(q, json!({"captureText":true}));
    }
    Ok(out)
}
fn match_value(m: StructuralDetailedMatch, capture_text: bool) -> Value {
    let text = compact_match(&m.text);
    let mut ranges = serde_json::Map::new();
    let mut metavars = serde_json::Map::new();
    let mut truncated = false;
    for (name, values) in m.metavars {
        if capture_text || values.len() <= 1 {
            let budgeted = values
                .into_iter()
                .map(|value| {
                    if !capture_text && value.chars().count() > 120 {
                        truncated = true;
                    }
                    truncate_capture(value, capture_text)
                })
                .collect::<Vec<_>>();
            metavars.insert(name, json!(budgeted));
        } else {
            truncated = true;
        }
    }
    for (name, values) in m.metavar_ranges {
        let list = values.len() > 1;
        let mut budgeted = Vec::new();
        for range in values {
            if !capture_text && list && is_comment(&range.text) {
                truncated = true;
                continue;
            }
            if !capture_text && range.text.chars().count() > 120 {
                truncated = true;
            }
            budgeted.push(json!({
                "text":truncate_capture(range.text, capture_text),
                "line":range.line,
                "column":range.column,
                "endLine":range.end_line,
                "endColumn":range.end_column
            }));
        }
        if !budgeted.is_empty() {
            ranges.insert(name, Value::Array(budgeted));
        }
    }
    let mut value = json!({
        "line":m.start_line,
        "endLine":m.end_line,
        "column":m.start_col,
        "endColumn":m.end_col,
        "value":text
    });
    if !metavars.is_empty() {
        value["metavars"] = Value::Object(metavars);
    }
    if !ranges.is_empty() {
        value["metavarRanges"] = Value::Object(ranges);
    }
    if truncated {
        value["capturesTruncated"] = json!(true);
    }
    value
}

fn compact_match(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() > 300 {
        let mut output = normalized.chars().take(299).collect::<String>();
        output.push('…');
        output
    } else {
        normalized
    }
}

fn truncate_capture(text: String, verbatim: bool) -> String {
    if verbatim || text.chars().count() <= 120 {
        text
    } else {
        let mut out = text.chars().take(120).collect::<String>();
        out.push('…');
        out
    }
}

fn is_comment(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
}

fn match_display_path(root: &std::path::Path, path: &str) -> String {
    let candidate = std::path::Path::new(path);
    candidate
        .strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(candidate)
        .to_string_lossy()
        .into_owned()
}

fn continuation(q: &AstMatchQuery, page: usize) -> Value {
    continuation_with(q, json!({"page":page}))
}

fn continuation_with(q: &AstMatchQuery, changes: Value) -> Value {
    let mut query = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    if let Some(map) = query.as_object_mut() {
        map.retain(|_, value| !value.is_null());
    }
    query["matchContentLength"] = json!(q.match_content_length.unwrap_or(500));
    query["sort"] = json!(q.sort.as_deref().unwrap_or("relevance"));
    query["rankingProfile"] = json!(q.ranking_profile.as_deref().unwrap_or("auto"));
    query["resultView"] = json!(q.result_view.as_deref().unwrap_or("content"));
    query["maxFiles"] = json!(q.max_files.unwrap_or(2_000));
    if let (Some(target), Some(changes)) = (query.as_object_mut(), changes.as_object()) {
        target.extend(changes.clone());
    }
    json!({"tool":"astSearch","query":query,"confidence":"exact"})
}

fn language_include_globs(language: &str) -> Option<Vec<String>> {
    let extensions: &[&str] = match language
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "rust" | "rs" => &["rs"],
        "typescript" | "ts" => &["ts", "tsx", "mts", "cts"],
        "typescriptreact" | "tsx" => &["tsx"],
        "javascript" | "js" => &["js", "jsx", "mjs", "cjs"],
        "python" | "py" => &["py", "pyi"],
        "go" => &["go"],
        "java" => &["java"],
        "c" => &["c", "h"],
        "ruby" | "rb" => &["rb", "rake", "gemspec", "ru"],
        "php" => &["php"],
        "kotlin" | "kt" => &["kt", "kts"],
        "sql" => &["sql"],
        "html" => &["html", "htm"],
        "css" => &["css"],
        "scss" => &["scss"],
        "scala" => &["scala", "sc", "sbt"],
        "json" => &["json", "jsonc"],
        "yaml" | "yml" => &["yaml", "yml"],
        "cpp" | "c++" => &["cpp", "hpp", "cc", "cxx", "hh", "hxx"],
        "csharp" | "c#" | "cs" => &["cs"],
        "swift" => &["swift"],
        _ => return None,
    };
    Some(
        extensions
            .iter()
            .map(|extension| format!("*.{extension}"))
            .collect(),
    )
}
fn diag(d: StructuralDiagnostic) -> Value {
    json!({"code":d.code,"severity":d.severity,"stage":d.stage,"message":d.message,"path":d.path,"recovery":d.recovery})
}

fn diagnostic_error(diagnostics: &[StructuralDiagnostic]) -> Option<super::AstError> {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == "error")
        .map(|diagnostic| super::AstError::new(diagnostic.code.clone(), diagnostic.message.clone()))
}
