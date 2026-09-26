pub use crate::contracts::tool_types::{AstSearchQueryMatchPattern, AstSearchQueryMatchRule};
use crate::{
    policy::path::PathPolicy, security::ContentSecurity, tools::local_fetch::CancellationCheck,
};
use octocode_engine::structural::{
    StructuralDetailedMatch, StructuralDiagnostic, StructuralSearchFilesOptions,
};
use serde_json::{Value, json};

/// A `match` query in either of its generated forms (pattern or rule).
#[derive(Clone, Copy, Debug)]
pub enum MatchQuery<'a> {
    Pattern(&'a AstSearchQueryMatchPattern),
    Rule(&'a AstSearchQueryMatchRule),
}

/// Binds `$field` from whichever form `$query` is.
macro_rules! either_form {
    ($query:expr, $field:ident => $value:expr) => {
        match $query {
            MatchQuery::Pattern(AstSearchQueryMatchPattern { $field, .. }) => $value,
            MatchQuery::Rule(AstSearchQueryMatchRule { $field, .. }) => $value,
        }
    };
}

fn u32_of(value: std::num::NonZeroU64) -> u32 {
    u32::try_from(value.get()).unwrap_or(u32::MAX)
}

fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// Form-independent views in the engine's units.
impl MatchQuery<'_> {
    pub fn path(self) -> String {
        either_form!(self, path => path.to_string())
    }
    pub fn pattern(self) -> Option<String> {
        match self {
            Self::Pattern(query) => Some(query.pattern.to_string()),
            Self::Rule(_) => None,
        }
    }
    pub fn rule(self) -> Option<String> {
        match self {
            Self::Pattern(_) => None,
            Self::Rule(query) => Some(query.rule.to_string()),
        }
    }
    pub fn include(self) -> Option<Vec<String>> {
        either_form!(self, include => non_empty(include))
    }
    pub fn exclude(self) -> Option<Vec<String>> {
        either_form!(self, exclude => non_empty(exclude))
    }
    pub fn exclude_dir(self) -> Option<Vec<String>> {
        either_form!(self, exclude_dir => non_empty(exclude_dir))
    }
    pub fn hidden(self) -> Option<bool> {
        either_form!(self, hidden => *hidden)
    }
    pub fn no_ignore(self) -> Option<bool> {
        either_form!(self, no_ignore => *no_ignore)
    }
    pub fn reverse(self) -> Option<bool> {
        either_form!(self, reverse => *reverse)
    }
    pub fn capture_text(self) -> Option<bool> {
        either_form!(self, capture_text => *capture_text)
    }
    pub fn max_depth(self) -> Option<u32> {
        either_form!(self, max_depth => max_depth.map(|depth| u32::try_from(depth.max(0)).unwrap_or(u32::MAX)))
    }
    pub fn max_files(self) -> Option<u32> {
        either_form!(self, max_files => max_files.map(u32_of))
    }
    pub fn max_matches_per_file(self) -> Option<u32> {
        either_form!(self, max_matches_per_file => max_matches_per_file.map(u32_of))
    }
    pub fn match_content_length(self) -> Option<u32> {
        either_form!(self, match_content_length => Some(u32_of(*match_content_length)))
    }
    pub fn lang_type(self) -> Option<String> {
        either_form!(self, lang_type => lang_type.as_ref().map(ToString::to_string))
    }
    pub fn sort(self) -> Option<String> {
        either_form!(self, sort => Some(sort.to_string()))
    }
    pub fn result_view(self) -> Option<String> {
        either_form!(self, result_view => Some(result_view.to_string()))
    }
    pub fn page(self) -> u32 {
        either_form!(self, page => u32_of(*page))
    }
    pub fn match_page(self) -> u32 {
        either_form!(self, match_page => u32_of(*match_page))
    }
    pub fn page_size(self) -> Option<u32> {
        either_form!(self, page_size => page_size.map(u32_of))
    }
    pub fn snapshot(self) -> Option<String> {
        either_form!(self, snapshot => snapshot.as_ref().map(ToString::to_string))
    }
    fn to_value(self) -> Value {
        match self {
            Self::Pattern(query) => serde_json::to_value(query),
            Self::Rule(query) => serde_json::to_value(query),
        }
        .unwrap_or_else(|_| json!({}))
    }
}

pub fn execute_match(
    q: MatchQuery<'_>,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &dyn CancellationCheck,
) -> super::AstResult {
    cancel.check().map_err(super::cancelled)?;
    let lang_extensions = q
        .lang_type()
        .as_deref()
        .map(|language| {
            language_extensions(language).ok_or_else(|| {
                super::AstError::new(
                    "ast.language.unsupported",
                    format!(
                        "langType \"{language}\" is not a supported structural grammar. Use a language name, id, or extension such as \"typescript\", \"rust\", or \"py\"."
                    ),
                )
            })
        })
        .transpose()?;
    let p = paths.validate(q.path()).map_err(super::AstError::from)?;
    let meta = std::fs::metadata(&p.canonical).map_err(super::io_error)?;
    if meta.is_dir() && q.lang_type().is_none() {
        return Err(super::AstError::new(
            "ast.language.required",
            "Directory matching requires langType; choose the grammar from the source files.",
        ));
    }
    // Corpus-coverage signals from the directory scan. The directory branch
    // fills these; the single-file branch leaves them at their complete defaults.
    let mut scan_truncated = false;
    let mut scan_diagnostics: Vec<StructuralDiagnostic> = Vec::new();
    let mut scan_skips = (0_u32, 0_u32, 0_u32);
    let mut skipped_by_prefilter = 0_u32;
    let mut files = if meta.is_file() {
        super::validate_file_language(&p.canonical, q.lang_type().as_deref())?;
        let bytes = std::fs::read(&p.canonical).map_err(super::io_error)?;
        let s = security
            .validate_text_bytes(&bytes, Some(&p.canonical), 1_000_000)
            .map_err(super::AstError::from)?;
        let source_path = p.canonical.to_string_lossy();
        let r = if super::cpp_header_override(&p.canonical, q.lang_type().as_deref()) {
            octocode_engine::portable::structural_search_detailed_with_extension(
                &s.content,
                &source_path,
                "cpp",
                q.pattern().as_deref(),
                q.rule().as_deref(),
            )
        } else {
            octocode_engine::portable::structural_search_detailed(
                &s.content,
                &source_path,
                q.pattern().as_deref(),
                q.rule().as_deref(),
            )
        }
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
        let r =
            octocode_engine::portable::structural_search_files_detailed_filtered_with_extension(
                StructuralSearchFilesOptions {
                    path: p.canonical.to_string_lossy().into_owned(),
                    pattern: q.pattern(),
                    rule: q.rule(),
                    include: q.include().or_else(|| {
                        lang_extensions.as_ref().map(|extensions| {
                            extensions.iter().map(|ext| format!("*.{ext}")).collect()
                        })
                    }),
                    exclude: q.exclude(),
                    exclude_dir: q.exclude_dir(),
                    hidden: q.hidden(),
                    no_ignore: q.no_ignore(),
                    max_depth: q.max_depth().map(|depth| depth.saturating_add(1)),
                    max_files: Some(q.max_files().unwrap_or(2_000)),
                    max_file_bytes: Some(1_000_000),
                },
                &|path| {
                    // Explicit include globs are intersected with langType: files
                    // outside the selected grammar are never candidates.
                    Ok(super::allow_discovery(path, paths, cancel)?
                        && (q.include().is_none()
                            || lang_extensions.as_ref().is_none_or(|extensions| {
                                !path.is_file() || has_extension_in(path, extensions)
                            })))
                },
                &|path| {
                    if super::cpp_header_override(path, q.lang_type().as_deref()) {
                        "cpp".to_owned()
                    } else {
                        path.extension()
                            .and_then(|extension| extension.to_str())
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                    }
                },
            )
            .map_err(super::native_error)?;
        if let Some(error) = diagnostic_error(&r.diagnostics) {
            return Err(error);
        }
        // Preserve the scan-level coverage signals the group projection drops.
        scan_truncated = r.scan_truncated;
        scan_skips = (r.skipped_unsupported, r.skipped_unreadable, r.skipped_large);
        skipped_by_prefilter = r.skipped_by_pre_filter;
        scan_diagnostics = r.diagnostics;
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
        let ordering = if q.sort().as_deref() == Some("matchCount") {
            b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0))
        } else {
            a.0.cmp(&b.0)
        };
        if q.reverse().unwrap_or(false) {
            ordering.reverse()
        } else {
            ordering
        }
    });
    // Snapshot fingerprint over the query shape plus the ordered result set
    // (each file and its match count). Continuation cursors (page>1 or
    // matchPage>1) that carry a stale snapshot are rejected with
    // `ast.snapshot.changed`. Fields the continuation normalizes to a default
    // are digested by their effective value so a fresh page matches.
    let ordered = files
        .iter()
        .map(|f| (f.0.clone(), f.1.len()))
        .collect::<Vec<_>>();
    let snapshot = super::syntax::digest(&json!([
        q.path(),
        q.pattern(),
        q.rule(),
        q.include(),
        q.exclude(),
        q.exclude_dir(),
        q.hidden(),
        q.no_ignore(),
        q.max_depth(),
        q.lang_type(),
        q.reverse(),
        q.sort().as_deref().unwrap_or("relevance"),
        q.result_view().as_deref().unwrap_or("content"),
        q.max_files().unwrap_or(2_000),
        ordered
    ]));
    if (q.page() > 1 || q.match_page() > 1) && q.snapshot().as_deref() != Some(&snapshot) {
        return Ok(super::snapshot_changed(&snapshot));
    }
    let mut groups = vec![];
    let mut all_diagnostics = vec![];
    let mut total_matches = 0_u64;
    let file_list = matches!(q.result_view().as_deref(), Some("files" | "countMatches"));
    let matches_per_page = q.max_matches_per_file().unwrap_or(100).clamp(1, 1_000) as usize;
    let match_page = q.match_page().max(1) as usize;
    let match_start = (match_page - 1).saturating_mul(matches_per_page);
    let content_length = q.match_content_length().unwrap_or(500).clamp(1, 100_000) as usize;
    // Per-file coverage: files that parsed, files whose query failed to
    // compile, and files cut short (execution limit, unreadable, unsupported).
    let mut parsed_files = 0_u32;
    let mut compile_error: Option<super::AstError> = None;
    let mut compile_failed_files = 0_u32;
    let mut unevaluated_files = 0_u32;
    for (path, matches, diagnostics, status) in files {
        match status.as_str() {
            "ok" => parsed_files += 1,
            "skippedByPreFilter" => {}
            _ => {
                if let Some(diagnostic) = diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic.code == "structural.query.compileFailed")
                {
                    compile_failed_files += 1;
                    compile_error.get_or_insert_with(|| {
                        super::AstError::new(diagnostic.code.clone(), diagnostic.message.clone())
                    });
                } else {
                    unevaluated_files += 1;
                }
            }
        }
        let matches = matches
            .into_iter()
            .map(|value| match_value(value, q.capture_text().unwrap_or(false), content_length))
            .collect::<Vec<_>>();
        if !matches.is_empty() {
            let total = matches.len();
            total_matches += total as u64;
            if file_list {
                if q.result_view().as_deref() == Some("countMatches") {
                    groups.push((json!({"path":path,"totalOccurrences":total}), false, false));
                } else {
                    groups.push((json!({"path":path}), false, false));
                }
                continue;
            }
            let match_end = match_start.saturating_add(matches_per_page).min(total);
            let selected = matches.get(match_start..match_end).unwrap_or(&[]).to_vec();
            let more_matches = match_end < total;
            let truncated_captures = selected
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
            groups.push((group, more_matches, truncated_captures));
        }
        if status != "ok" || !diagnostics.is_empty() {
            // A literal prefilter can skip thousands of files. Preserve the
            // coverage fact once below instead of returning one identical
            // diagnostic for every skipped path.
            all_diagnostics.extend(
                diagnostics
                    .into_iter()
                    .filter(|diagnostic| diagnostic.code != "structural.prefilter.skipped")
                    .map(diag),
            );
        }
    }
    if total_matches == 0
        && parsed_files == 0
        && compile_failed_files > 0
        && let Some(error) = compile_error
    {
        // The query compiled in no candidate file: an empty result would read
        // as proof of absence, so surface the compile failure instead.
        return Err(error);
    }
    if skipped_by_prefilter > 0 {
        all_diagnostics.push(json!({
            "code":"structural.prefilter.skipped",
            "severity":"info",
            "stage":"scan",
            "message":format!("Literal-anchor prefilter skipped AST parsing for {skipped_by_prefilter} file(s) that cannot contain the anchor."),
            "recovery":"Use a YAML rule with no safe literal anchor only when every supported candidate must be parsed."
        }));
    }
    // Surface scan-level coverage signals (skips, unsupported extensions) that
    // the group projection would otherwise discard.
    all_diagnostics.extend(scan_diagnostics.into_iter().map(diag));
    let (skipped_unsupported, skipped_unreadable, skipped_large) = scan_skips;
    if scan_truncated {
        let limit = q.max_files().unwrap_or(2_000);
        all_diagnostics.push(json!({
            "code":"structural.scan.truncated",
            "severity":"warning",
            "stage":"scan",
            "message":format!("Candidate scan hit the maxFiles limit ({limit}); files beyond it were not evaluated. Results are a bounded subset, not the full corpus."),
            "path":super::display_name(&p.canonical),
            "recovery":"Narrow the scope with include globs or excludeDir, or raise maxFiles, then re-run."
        }));
    }
    let size = q.page_size().unwrap_or(20).clamp(1, 1_000) as usize;
    let page = q.page().max(1) as usize;
    let start = (page - 1) * size;
    let page_groups = groups
        .get(start..(start + size).min(groups.len()))
        .unwrap_or(&[]);
    // Match-level pagination is page-local: only files shown on this page can
    // continue with nextMatchPage.
    let has_more_matches = page_groups.iter().any(|group| group.1);
    let has_truncated_captures = page_groups.iter().any(|group| group.2);
    let selected = page_groups
        .iter()
        .map(|group| group.0.clone())
        .collect::<Vec<_>>();
    let more = start + size < groups.len();
    let mut out = json!({"searchEngine":"structural","snapshot":snapshot,"stats":{"totalStructuralMatches":total_matches}});
    if !groups.is_empty() {
        out["files"] = json!(selected);
        out["pagination"] = json!({"currentPage":page,"totalPages":groups.len().div_ceil(size).max(1),"filesPerPage":size,"totalFiles":groups.len()});
        if q.result_view().as_deref() != Some("files") {
            out["pagination"]["totalMatches"] = json!(total_matches);
        }
        out["pagination"]["hasMore"] = json!(more);
    }
    if !all_diagnostics.is_empty() {
        out["diagnostics"] = json!(all_diagnostics)
    }
    if groups.is_empty() && q.pattern().is_some() && all_diagnostics.is_empty() {
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
            "message":"0 structural matches for the requested pattern in this scope. Patterns must be complete parseable nodes, including punctuation such as trailing semicolons and relevant bodies (`$$$BODY`), return types, or decorators. Confirm the node shape with treeKind:\"syntax\"; use an explicit YAML rule for partial or relational constraints.",
            "path":path
        }]);
    }
    let incomplete = scan_truncated
        || skipped_unsupported > 0
        || skipped_unreadable > 0
        || skipped_large > 0
        || compile_failed_files > 0
        || unevaluated_files > 0;
    out["truncated"] = json!(scan_truncated);
    out["complete"] = json!(!more && !incomplete && !has_more_matches);
    if groups.is_empty() && !more && !incomplete {
        out["status"] = json!("empty");
    }
    if scan_truncated && !more {
        out["terminalLimit"] = json!(true);
    }
    if more {
        out["pagination"]["nextPage"] = json!(page + 1);
        out["next"] = json!({"nextPage":continuation(q, page + 1, &snapshot)})
    }
    if has_more_matches && match_page < 1_000 {
        out["next"]["nextMatchPage"] = continuation_with(
            q,
            json!({"maxMatchesPerFile":matches_per_page,"matchPage":match_page+1}),
            &snapshot,
        );
    }
    if has_truncated_captures && !q.capture_text().unwrap_or(false) {
        out["next"]["expandCaptures"] =
            continuation_with(q, json!({"captureText":true}), &snapshot);
    }
    Ok(out)
}
/// One match row. Captures are emitted once, in `metavarRanges` (text plus
/// 1-based position); the parallel engine `metavars` text map is only used as a
/// fallback for a capture that carries no range. `endLine` is omitted when the
/// span is single-line (it equals `line`).
fn match_value(m: StructuralDetailedMatch, capture_text: bool, content_length: usize) -> Value {
    let text = compact_match(&m.text, content_length);
    let mut ranges = serde_json::Map::new();
    let mut metavars = serde_json::Map::new();
    let mut truncated = false;
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
            let mut row = json!({
                "text":truncate_capture(range.text, capture_text),
                "line":range.line,
                "column":range.column,
                "endColumn":range.end_column
            });
            if range.end_line != range.line {
                row["endLine"] = json!(range.end_line);
            }
            budgeted.push(row);
        }
        if !budgeted.is_empty() {
            ranges.insert(name, Value::Array(budgeted));
        }
    }
    // Capture maps are unordered; emit names sorted so output is deterministic.
    let mut captures: Vec<_> = m.metavars.into_iter().collect();
    captures.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, values) in captures {
        if ranges.contains_key(&name) || values.is_empty() {
            continue;
        }
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
    let mut value = json!({
        "line":m.start_line,
        "value":text,
        "column":m.start_col,
        "endColumn":m.end_col
    });
    if m.end_line != m.start_line {
        value["endLine"] = json!(m.end_line);
    }
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

/// Whitespace-normalized match text bounded to `limit` characters
/// (`matchContentLength`), including the trailing ellipsis when cut.
fn compact_match(text: &str, limit: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() > limit {
        let mut output = normalized
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
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
    let relative = candidate.strip_prefix(root).unwrap_or(candidate);
    super::rooted_display(root, relative)
}

fn continuation(q: MatchQuery<'_>, page: usize, snapshot: &str) -> Value {
    // A new file page restarts per-file match pagination.
    continuation_with(q, json!({"page":page,"matchPage":1}), snapshot)
}

fn continuation_with(q: MatchQuery<'_>, changes: Value, snapshot: &str) -> Value {
    let mut query = q.to_value();
    query["maxFiles"] = json!(q.max_files().unwrap_or(2_000));
    query["snapshot"] = json!(snapshot);
    if let (Some(target), Some(changes)) = (query.as_object_mut(), changes.as_object()) {
        target.extend(changes.clone());
    }
    json!({"tool":"astSearch","query":query,"confidence":"exact"})
}

pub(super) fn language_extensions(language: &str) -> Option<std::collections::BTreeSet<String>> {
    let selector = language.trim().to_ascii_lowercase();
    let capabilities = octocode_engine::portable::grammar_capabilities();
    if let Some(extension) = selector.strip_prefix('.') {
        return capabilities
            .iter()
            .any(|capability| capability.extensions.iter().any(|item| *item == extension))
            .then(|| std::collections::BTreeSet::from([extension.to_owned()]));
    }
    let mut extensions: std::collections::BTreeSet<String> = capabilities
        .into_iter()
        .filter(|capability| {
            capability.language.eq_ignore_ascii_case(&selector)
                || capability
                    .language_id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&selector))
                || capability
                    .selector_aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&selector))
                || capability
                    .extensions
                    .iter()
                    .any(|extension| extension.eq_ignore_ascii_case(&selector))
        })
        .flat_map(|capability| capability.extensions)
        .map(|extension| extension.to_ascii_lowercase())
        .collect();
    if !extensions.is_empty() && (selector == "cpp" || selector == "c++") {
        extensions.insert("h".to_owned());
    }
    (!extensions.is_empty()).then_some(extensions)
}

pub(super) fn has_extension_in(
    path: &std::path::Path,
    extensions: &std::collections::BTreeSet<String>,
) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.contains(&ext.to_ascii_lowercase()))
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

#[cfg(test)]
mod language_glob_tests {
    fn language_include_globs(language: &str) -> Option<Vec<String>> {
        super::language_extensions(language).map(|extensions| {
            extensions
                .into_iter()
                .map(|extension| format!("*.{extension}"))
                .collect()
        })
    }

    #[test]
    fn directory_language_globs_derive_from_the_canonical_grammar_registry() {
        let cuda_enabled = octocode_engine::portable::supported_structural_extensions()
            .iter()
            .any(|extension| extension == "cu");
        assert_eq!(
            language_include_globs("cuda"),
            cuda_enabled.then(|| vec!["*.cu".to_owned(), "*.cuh".to_owned()])
        );
        assert_eq!(
            language_include_globs("assembly"),
            Some(vec![
                "*.asm".to_owned(),
                "*.assembly".to_owned(),
                "*.s".to_owned()
            ])
        );
        assert_eq!(
            language_include_globs("typescript"),
            Some(vec![
                "*.cts".to_owned(),
                "*.mts".to_owned(),
                "*.ts".to_owned(),
                "*.tsx".to_owned()
            ])
        );
        assert_eq!(language_include_globs("unsupported"), None);
    }
}
