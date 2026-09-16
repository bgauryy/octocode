use super::types::*;
use crate::policy::discovery::{
    DISCOVERY_IGNORED_FILE_EXTENSIONS, DISCOVERY_IGNORED_FILE_NAMES, DISCOVERY_IGNORED_FOLDER_NAMES,
};
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use crate::tools::local_fetch::CancellationCheck;
use octocode_engine::{
    portable::{RipgrepPathFilter, search_ripgrep_filtered},
    types::RipgrepSearchOptions,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub fn execute_local_search(
    query: &LocalSearchRequest,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
) -> Result<LocalSearchResult, LocalSearchError> {
    cancel.check().map_err(cancelled)?;
    if query.search_text.is_empty() {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`searchText` is required".into(),
            hints: vec![],
            next: None,
        });
    }
    if query.path.is_empty() {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "Path is required for search".into(),
            hints: vec![],
            next: None,
        });
    }
    let view = query.result_view.unwrap_or_default();
    if query.match_window.is_some() && view != ResultView::MatchOnly {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`matchWindow` requires resultView:\"matchOnly\"".into(),
            hints: vec![],
            next: None,
        });
    }
    if query.unique.unwrap_or_default() != UniqueMode::Off && view != ResultView::MatchOnly {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`unique` requires resultView:\"matchOnly\"".into(),
            hints: vec![],
            next: None,
        });
    }
    let validated = paths
        .validate(&query.path)
        .map_err(|error| LocalSearchError {
            code: "fileAccessFailed",
            message: error.message,
            hints: vec![],
            next: None,
        })?;
    let case = query.case_mode.unwrap_or_default();
    let regex = query.regex.unwrap_or_default();
    let multiline = query.multiline.unwrap_or_default();
    let requested_sort = query.sort.unwrap_or_default();
    let path_sort = matches!(
        query.sort,
        Some(SortMode::Modified | SortMode::Accessed | SortMode::Created | SortMode::Path)
    );
    let options = RipgrepSearchOptions {
        path: validated.canonical.to_string_lossy().into_owned(),
        pattern: query.search_text.clone(),
        fixed_string: Some(regex == RegexMode::Literal),
        perl_regex: Some(regex == RegexMode::Pcre2),
        case_sensitive: Some(case == CaseMode::Sensitive),
        case_insensitive: Some(case == CaseMode::Insensitive),
        whole_word: query.whole_word,
        invert_match: query.invert_match,
        multiline: Some(multiline != MultilineMode::Off),
        multiline_dotall: Some(multiline == MultilineMode::Dotall),
        files_only: Some(view == ResultView::Files),
        files_without_match: Some(view == ResultView::FilesWithout),
        count_lines_per_file: Some(view == ResultView::CountLines),
        count_matches_per_file: Some(view == ResultView::CountMatches),
        context_lines: Some(
            query
                .context_lines
                .unwrap_or(if view == ResultView::Detailed { 3 } else { 2 }),
        ),
        lang_type: query.lang_type.clone(),
        include: query.include.clone(),
        exclude: Some(
            query
                .exclude
                .clone()
                .unwrap_or_default()
                .into_iter()
                .chain(DISCOVERY_IGNORED_FILE_NAMES.iter().map(|s| (*s).to_owned()))
                .chain(
                    DISCOVERY_IGNORED_FILE_EXTENSIONS
                        .iter()
                        .map(|s| format!("*{s}")),
                )
                .collect(),
        ),
        exclude_dir: Some(
            query
                .exclude_dir
                .clone()
                .unwrap_or_default()
                .into_iter()
                .chain(
                    DISCOVERY_IGNORED_FOLDER_NAMES
                        .iter()
                        .map(|s| (*s).to_owned()),
                )
                .collect(),
        ),
        no_ignore: query.no_ignore,
        hidden: query.hidden,
        max_depth: query.max_depth,
        sort: if requested_sort == SortMode::Traversal {
            Some("traversal".into())
        } else {
            Some(if path_sort {
                format!("{requested_sort:?}").to_lowercase()
            } else {
                "path".into()
            })
        },
        sort_reverse: query.reverse,
        max_snippet_chars: query.match_content_length,
        classify_matches: Some(false),
        only_matching: Some(view == ResultView::MatchOnly),
        match_window: query.match_window,
        unique: Some(matches!(
            query.unique,
            Some(UniqueMode::List | UniqueMode::Count)
        )),
        count_unique: Some(query.unique == Some(UniqueMode::Count)),
        max_collected_files: Some(10_000),
    };
    let frozen = query.no_ignore == Some(true);
    let (mut parsed, from_manifest) = if frozen
        && let Some(snapshot) = query.snapshot.as_deref()
        && let Some(stored) = super::manifest::get(snapshot)
    {
        (stored, true)
    } else {
        (
            search_ripgrep_filtered(options, Arc::new(PolicyFilter(paths.clone()))).map_err(|error| {
        let message=error.to_string(); let invalid=message.contains("regex parse error") || message.contains("PCRE2");
        let next=invalid.then(||{let mut repaired=normalized_query(query);repaired["regex"]=json!("literal");Box::new(json!({"repair":{"tool":"localSearch","query":repaired,"why":"Start a new search treating searchText as literal text, if that was intended."}}))});
        LocalSearchError{code:if invalid{"invalidRegex"}else{"toolExecutionFailed"},message,hints:if invalid{vec!["Use regex:\"literal\" for exact text, or escape metacharacters/fix searchText to keep regex matching.".into()]}else{vec![]},next}
    })?,
            false,
        )
    };
    cancel.check().map_err(cancelled)?;
    for file in &parsed.files {
        paths
            .validate_read(&file.path)
            .map_err(|_| LocalSearchError {
                code: "fileAccessFailed",
                message: "Search encountered a path denied by the active path policy".into(),
                hints: vec![],
                next: None,
            })?;
    }
    let result_identity = fingerprint(query, &parsed.files, &parsed.stats);
    if frozen && !from_manifest {
        super::manifest::put(result_identity.clone(), parsed.clone());
    }
    if !from_manifest
        && query
            .snapshot
            .as_deref()
            .is_some_and(|expected| expected != result_identity)
    {
        let mut restart = normalized_query(query);
        if let Some(object) = restart.as_object_mut() {
            object.remove("snapshot");
        }
        restart["page"] = json!(1);
        restart["matchPage"] = json!(1);
        return Err(LocalSearchError {
            code: "staleSnapshot",
            message: "Search snapshot cannot be continued (resultsChanged); restart the search."
                .into(),
            hints: vec![],
            next: Some(Box::new(json!({
                "restart": {
                    "tool": "localSearch",
                    "query": restart,
                    "why": "Start a new search against the current source.",
                    "confidence": "exact"
                }
            }))),
        });
    }
    let root = &validated.canonical;
    let output_root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root.as_path()
    };
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
        for matched in &mut file.matches {
            cancel.check().map_err(cancelled)?;
            matched.value = security
                .sanitize_text(&matched.value, Some(&output_root.join(&file.path)))
                .content;
        }
    }
    match requested_sort {
        SortMode::Path => parsed.files.sort_by(|a, b| a.path.cmp(&b.path)),
        SortMode::MatchCount => parsed.files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.path.cmp(&b.path))
        }),
        SortMode::Relevance => rank_relevance(&mut parsed.files, query, view),
        SortMode::Traversal => {}
        _ => {}
    }
    if query.reverse.unwrap_or(false)
        && matches!(query.sort, Some(SortMode::Path | SortMode::MatchCount))
    {
        parsed.files.reverse();
    }
    let page_size = query
        .page_size
        .unwrap_or(100)
        .min(query.max_files.unwrap_or(u32::MAX))
        .max(1);
    let page = query.page.unwrap_or(1).max(1);
    let total_files = parsed.files.len() as u32;
    let total_pages = total_files.div_ceil(page_size).max(1);
    let start = (page - 1).saturating_mul(page_size) as usize;
    let list = matches!(
        view,
        ResultView::Files
            | ResultView::FilesWithout
            | ResultView::Discovery
            | ResultView::CountLines
            | ResultView::CountMatches
    );
    let matches_per = query.max_matches_per_file.unwrap_or(20).max(1);
    let match_page = query.match_page.unwrap_or(1).max(1);
    let total_matches = if list {
        parsed.stats.match_count.unwrap_or(0)
    } else {
        parsed.files.iter().map(|f| f.matches.len() as u32).sum()
    };
    let empty = total_files == 0;
    let snapshot = if !empty
        && (page < total_pages
            || parsed
                .files
                .iter()
                .any(|f| f.matches.len() as u32 > matches_per))
    {
        Some(result_identity.clone())
    } else {
        None
    };
    let leftover_matches = parsed
        .files
        .iter()
        .any(|file| file.matches.len() as u32 > match_page.saturating_mul(matches_per));
    let next = build_next(
        query,
        page,
        total_pages,
        match_page,
        matches_per,
        &parsed.files,
        snapshot.as_deref(),
    );
    let files = parsed
        .files
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .map(|f| {
            let total = f.matches.len() as u32;
            let ms = (match_page - 1).saturating_mul(matches_per) as usize;
            let shown = f
                .matches
                .iter()
                .skip(ms)
                .take(matches_per as usize)
                .map(|m| SearchMatch {
                    line: m.line,
                    column: m.column,
                    value: m.value.clone(),
                    count: m.count,
                })
                .collect::<Vec<_>>();
            SearchFile {
                path: f.path,
                matches: (!list).then_some(shown.clone()),
                total_occurrences: (view == ResultView::CountMatches).then_some(f.match_count),
                total_matched_lines: (view == ResultView::CountLines).then_some(f.match_count),
                total_match_rows: (!list).then_some(total),
                returned_match_rows: (!list).then_some(shown.len() as u32),
                pagination: (!list && (total > matches_per || ms >= total as usize && total > 0))
                    .then_some(ItemPagination {
                        current_page: match_page,
                        total_pages: total.div_ceil(matches_per).max(1),
                        matches_per_page: Some(matches_per),
                        total_matches: total,
                        has_more: match_page < total.div_ceil(matches_per),
                        next_match_page: (match_page < total.div_ceil(matches_per))
                            .then_some(match_page + 1),
                        out_of_range: ms >= total as usize && total > 0,
                    }),
            }
        })
        .collect::<Vec<_>>();
    let stats = SearchStats {
        total_occurrences: parsed.stats.match_count.unwrap_or(0),
        matched_lines: parsed.stats.matched_lines.unwrap_or(0),
        files_matched: parsed.stats.files_matched.unwrap_or(total_files),
        files_searched: parsed.stats.files_searched.unwrap_or(0),
        bytes_searched: parsed.stats.bytes_searched,
        search_time: None,
        capped: parsed.stats.capped,
        cap_reason: parsed.stats.cap_reason,
        error_count: parsed.stats.error_count.filter(|n| *n > 0),
        first_error: parsed.stats.first_error,
    };
    let has_more = page < total_pages;
    let (status, terminal_limit) = classify_search(
        empty,
        stats.capped.unwrap_or(false),
        has_more,
        leftover_matches,
        stats.error_count.unwrap_or(0),
        files.len(),
        next.is_none(),
    );
    Ok(LocalSearchResult {
        status,
        search_engine: "rg".into(),
        stats,
        files,
        pagination: (!empty).then_some(FilePagination {
            snapshot: snapshot.clone(),
            current_page: page,
            total_pages,
            files_per_page: page_size,
            total_files,
            total_matches: (!matches!(
                view,
                ResultView::Files | ResultView::FilesWithout | ResultView::Discovery
            ))
            .then_some(total_matches),
            has_more,
            next_page: (page < total_pages && page < 1000).then_some(page + 1),
            out_of_range: start >= total_files as usize && total_files > 0,
        }),
        hints: if empty {
            vec![
                "No matches. Try caseMode:\"insensitive\", a shorter term, or regex:\"rust\"."
                    .into(),
            ]
        } else {
            vec![]
        },
        next,
        terminal_limit,
        warnings: vec![],
        source_snapshot: Some(result_identity),
        source_root: output_root.to_path_buf(),
    })
}

struct PolicyFilter(PathPolicy);
impl RipgrepPathFilter for PolicyFilter {
    fn allows(&self, path: &std::path::Path, is_dir: bool) -> bool {
        if is_dir {
            self.0.validate(path).is_ok()
        } else {
            self.0.validate_read(path).is_ok()
        }
    }
}

fn cancelled(message: String) -> LocalSearchError {
    LocalSearchError {
        code: "cancelled",
        message,
        hints: vec![],
        next: None,
    }
}

fn rank_relevance(
    files: &mut [octocode_engine::types::RipgrepFile],
    q: &LocalSearchRequest,
    view: ResultView,
) {
    if !q
        .search_text
        .chars()
        .any(|c| c.is_alphanumeric() || c == '_')
    {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        return;
    }
    if matches!(
        view,
        ResultView::Files | ResultView::FilesWithout | ResultView::Discovery
    ) {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        return;
    }
    if matches!(view, ResultView::CountLines | ResultView::CountMatches) {
        files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.path.cmp(&b.path))
        });
        return;
    }
    let profile = q.ranking_profile.as_deref().unwrap_or("auto");
    files.sort_by(|a, b| {
        score(
            b,
            profile,
            view,
            q.unique.unwrap_or_default() != UniqueMode::Off,
        )
        .cmp(&score(
            a,
            profile,
            view,
            q.unique.unwrap_or_default() != UniqueMode::Off,
        ))
        .then_with(|| b.match_count.cmp(&a.match_count))
        .then_with(|| a.path.cmp(&b.path))
    });
}
fn score(
    file: &octocode_engine::types::RipgrepFile,
    profile: &str,
    view: ResultView,
    unique: bool,
) -> i32 {
    let p = file.path.replace('\\', "/").to_lowercase();
    let mut s = file.match_count as i32 * 10;
    if p.contains("/test") || p.contains(".test.") {
        s -= 100
    }
    if p.contains("/docs/") || p.ends_with(".md") {
        s += 5
    }
    if p.contains("/src/") || p.starts_with("src/") {
        s += 30
    }
    if p.contains("main.") {
        s += 25
    }
    if p.starts_with('.') {
        s += 20
    }
    let language = if profile == "auto" {
        if p.ends_with(".rs") {
            "rust"
        } else if p.ends_with(".ts") || p.ends_with(".tsx") {
            "typescript"
        } else if p.ends_with(".js") || p.ends_with(".jsx") {
            "javascript"
        } else if p.ends_with(".py") {
            "python"
        } else {
            profile
        }
    } else {
        profile
    };
    let boost = match language {
        "rust" => p.ends_with(".rs"),
        "typescript" => p.ends_with(".ts") || p.ends_with(".tsx"),
        "javascript" => p.ends_with(".js") || p.ends_with(".jsx") || p.ends_with(".mjs"),
        "python" => p.ends_with(".py"),
        "go" => p.ends_with(".go"),
        "java" => p.ends_with(".java"),
        "markdown" => p.ends_with(".md"),
        _ => false,
    };
    if boost {
        s += 100
    } else if matches!(
        language,
        "rust" | "typescript" | "javascript" | "python" | "go" | "java"
    ) {
        s -= 10
    }
    if view == ResultView::MatchOnly {
        if p.ends_with(".md") {
            s += 100
        }
        if p.contains("unicode") {
            s -= 50
        }
        if p.contains("/test") || p.contains(".test.") {
            s += 70
        }
        if unique && p.contains("unicode") {
            s += 90
        }
    }
    s
}

fn normalized_query(q: &LocalSearchRequest) -> Value {
    let mut value = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    let o = value.as_object_mut().expect("request object");
    o.retain(|_, v| !v.is_null());
    o.entry("regex").or_insert(json!("rust"));
    o.entry("caseMode").or_insert(json!("smart"));
    o.entry("contextLines").or_insert(json!(2));
    o.entry("matchContentLength").or_insert(json!(500));
    o.entry("multiline").or_insert(json!("off"));
    o.entry("sort").or_insert(json!("relevance"));
    o.entry("rankingProfile").or_insert(json!("auto"));
    o.entry("unique").or_insert(json!("off"));
    o.entry("matchPage").or_insert(json!(1));
    o.entry("page").or_insert(json!(1));
    o.entry("resultView").or_insert(json!("paginated"));
    o.entry("pageSize").or_insert(json!(100));
    value
}
pub(crate) fn classify_search(
    empty: bool,
    capped: bool,
    has_more: bool,
    leftover_matches: bool,
    error_count: u32,
    files_returned: usize,
    next_missing: bool,
) -> (SearchStatus, bool) {
    if empty {
        return (SearchStatus::Empty, false);
    }
    let partial = capped || has_more || leftover_matches || (error_count > 0 && files_returned > 0);
    let status = if partial {
        SearchStatus::Partial
    } else {
        SearchStatus::Success
    };
    let terminal = (partial || has_more) && next_missing;
    (status, terminal)
}

fn build_next(
    q: &LocalSearchRequest,
    page: u32,
    total_pages: u32,
    match_page: u32,
    matches_per: u32,
    files: &[octocode_engine::types::RipgrepFile],
    snapshot: Option<&str>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();
    let base = normalized_query(q);
    if page < total_pages && page < 1000 {
        let mut n = base.clone();
        n["page"] = json!(page + 1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextPage".into(),
            json!({"tool":"localSearch","query":n,"confidence":"exact"}),
        );
    }
    if files
        .iter()
        .any(|f| f.matches.len() as u32 > match_page * matches_per)
    {
        let mut n = base;
        n["matchPage"] = json!(match_page + 1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextMatchPage".into(),
            json!({"tool":"localSearch","query":n,"confidence":"exact"}),
        );
    }
    (!map.is_empty()).then_some(Value::Object(map))
}
fn fingerprint(
    q: &LocalSearchRequest,
    files: &[octocode_engine::types::RipgrepFile],
    stats: &octocode_engine::types::RipgrepStats,
) -> String {
    let mut legacy = serde_json::Map::new();
    legacy.insert("searchText".into(), json!(q.search_text));
    legacy.insert(
        "mode".into(),
        json!(match q.result_view.unwrap_or_default() {
            ResultView::Discovery => "discovery",
            ResultView::Detailed => "detailed",
            _ => "paginated",
        }),
    );
    legacy.insert(
        "regex".into(),
        json!(match q.regex.unwrap_or_default() {
            RegexMode::Literal => "fixed",
            RegexMode::Pcre2 => "perl",
            RegexMode::Rust => "smart",
        }),
    );
    legacy.insert(
        "caseMode".into(),
        json!(match q.case_mode.unwrap_or_default() {
            CaseMode::Sensitive => "sensitive",
            CaseMode::Insensitive => "insensitive",
            CaseMode::Smart => "smart",
        }),
    );
    legacy.insert(
        "contextLines".into(),
        json!(
            q.context_lines
                .unwrap_or(if q.result_view == Some(ResultView::Detailed) {
                    3
                } else {
                    2
                })
        ),
    );
    legacy.insert(
        "matchContentLength".into(),
        json!(q.match_content_length.unwrap_or(500)),
    );
    legacy.insert(
        "multiline".into(),
        json!(match q.multiline.unwrap_or_default() {
            MultilineMode::Off => "off",
            MultilineMode::On => "on",
            MultilineMode::Dotall => "dotall",
        }),
    );
    legacy.insert(
        "sort".into(),
        json!(match q.sort.unwrap_or_default() {
            SortMode::Relevance => "relevance",
            SortMode::Traversal => "traversal",
            SortMode::MatchCount => "matchCount",
            SortMode::Path => "path",
            SortMode::Modified => "modified",
            SortMode::Accessed => "accessed",
            SortMode::Created => "created",
        }),
    );
    legacy.insert(
        "rankingProfile".into(),
        json!(q.ranking_profile.as_deref().unwrap_or("auto")),
    );
    legacy.insert(
        "output".into(),
        json!(match q.result_view.unwrap_or_default() {
            ResultView::MatchOnly => "matchOnly",
            ResultView::Files => "files",
            ResultView::FilesWithout => "filesWithout",
            ResultView::CountLines => "countLines",
            ResultView::CountMatches => "countMatches",
            _ => "content",
        }),
    );
    legacy.insert(
        "unique".into(),
        json!(match q.unique.unwrap_or_default() {
            UniqueMode::Off => "off",
            UniqueMode::List => "list",
            UniqueMode::Count => "count",
        }),
    );
    macro_rules! opt {
        ($name:literal,$value:expr) => {
            if let Some(v) = $value {
                legacy.insert($name.into(), json!(v));
            }
        };
    }
    opt!("wholeWord", q.whole_word);
    opt!("invertMatch", q.invert_match);
    opt!("include", q.include.as_ref());
    opt!("exclude", q.exclude.as_ref());
    opt!("excludeDir", q.exclude_dir.as_ref());
    opt!("noIgnore", q.no_ignore);
    opt!("hidden", q.hidden);
    opt!("maxDepth", q.max_depth);
    opt!("langType", q.lang_type.as_ref());
    opt!("matchWindow", q.match_window);
    opt!("sortReverse", q.reverse);
    let mut entries = legacy.into_iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let query_key = hex::encode(Sha256::digest(
        serde_json::to_vec(&json!([q.path, entries])).unwrap_or_default(),
    ));
    let file_values = files
        .iter()
        .map(|f| {
            let matches = f
                .matches
                .iter()
                .map(|m| {
                    let mut o = serde_json::Map::new();
                    o.insert("line".into(), json!(m.line));
                    o.insert("column".into(), json!(m.column));
                    o.insert("value".into(), json!(m.value));
                    if let Some(c) = m.count {
                        o.insert("count".into(), json!(c));
                    }
                    Value::Object(o)
                })
                .collect::<Vec<_>>();
            json!({"path":f.path,"matchCount":f.match_count,"matches":matches})
        })
        .collect::<Vec<_>>();
    let canonical = canonicalize(
        json!([query_key,file_values,{"totalOccurrences":stats.match_count.unwrap_or(0),"matchedLines":stats.matched_lines.unwrap_or(0),"filesMatched":stats.files_matched.unwrap_or(files.len() as u32),"filesSearched":stats.files_searched.unwrap_or(0),"capped":stats.capped.unwrap_or(false),"capReason":stats.cap_reason,"errorCount":stats.error_count.filter(|n|*n>0),"firstError":stats.first_error} ]),
    );
    format!(
        "lexical-live-v1:{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(&canonical).unwrap_or_default()
        ))
    )
}
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map
                .into_iter()
                .filter(|(_, v)| !v.is_null())
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonicalize(v)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.into_iter().map(canonicalize).collect()),
        v => v,
    }
}
