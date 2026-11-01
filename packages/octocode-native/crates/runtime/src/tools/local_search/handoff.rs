//! Read leads out of a search page: the declaration or hits a localFetch
//! call reads next.

use super::enclosing::*;
use super::local_search_query;
use super::types::*;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::id::ToolId;
use crate::response::result::Continuation;
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::types::SearchContext;
use serde_json::{Value, json};

/// The searched symbol's declaration among the shown hits.
pub(super) struct Definition {
    /// Absolute path of the declaring file.
    pub(super) source: String,
    /// The declaration's name line.
    pub(super) line: u32,
    pub(super) kind: String,
    /// Its first and last line.
    pub(super) start: u32,
    pub(super) end: u32,
}

/// localFetch read of a shown declaration whole, from its first line to its
/// last; localFetch pages a long one.
pub(super) fn declaration_read(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
    start: u32,
    end: u32,
) -> Value {
    Continuation::related(
        ToolId::LocalFetch,
        json!({"path": joined_path(query, root, file), "ranges": [{"startLine":start,"endLine":end}]}),
    )
    .build()
}

/// A shown file's path joined to the caller's own `path`, so a read
/// resolves wherever the search did.
pub(super) fn joined_path(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
) -> String {
    if root.is_file() {
        local_search_query::path(query).to_string()
    } else {
        // A leading `.` (a search of `.`) is no part of the path: the read
        // names the file as its row does, not `./src/a.rs`.
        std::path::Path::new(local_search_query::path(query))
            .join(&file.path)
            .components()
            .filter(|part| *part != std::path::Component::CurDir)
            .collect::<std::path::PathBuf>()
            .to_string_lossy()
            .into_owned()
    }
}

/// Hits a handed-off read may cover (each opens a ±6-line window).
pub(super) const READ_HANDOFF_MAX_HITS: usize = 20;

/// Lines a handed-off read shows on each side of a hit.
pub(super) const READ_HANDOFF_CONTEXT: u32 = 6;

/// `(start, end)` windows of `context` lines around each line, merged where
/// they overlap or touch, in line order; none ends past `last`, the file's
/// last line when known.
pub(super) fn hit_windows(
    lines: impl IntoIterator<Item = u32>,
    context: SearchContext,
    last: Option<u32>,
) -> Vec<(u32, u32)> {
    crate::tools::line_spans::merge_spans(lines.into_iter().map(|line| {
        let end = line.saturating_add(context.after);
        (
            line.saturating_sub(context.before).max(1),
            last.map_or(end, |last| end.min(last.max(line))),
        )
    }))
}

/// localFetch read of one file's hits in context (the file declaring the
/// searched symbol, else the top file): ±6-line `ranges`
/// around exactly the shown hits, or (when they need more windows than one
/// read holds) the search text as `matchString`. Paths join the caller's
/// own `path`, so the read resolves wherever the search did.
pub(super) fn read_handoff(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    top: &SearchFile,
) -> Option<Value> {
    let hits = top.matches.as_ref()?;
    if hits.is_empty() || hits.len() > READ_HANDOFF_MAX_HITS {
        return None;
    }
    let path = joined_path(query, root, top);
    let source = if root.is_file() {
        root.to_path_buf()
    } else {
        root.join(&top.path)
    };
    let windows = hit_windows(
        hits.iter().flat_map(row_lines),
        SearchContext::symmetric(READ_HANDOFF_CONTEXT),
        crate::tools::line_spans::file_lines(&source),
    );
    if windows.len() <= RANGES_MAX_ITEMS {
        let ranges = windows
            .iter()
            .map(|(start, end)| json!({"startLine":start,"endLine":end}))
            .collect::<Vec<_>>();
        return Some(
            Continuation::related(ToolId::LocalFetch, json!({"path": path, "ranges": ranges}))
                .build(),
        );
    }
    let mut read = json!({
        "path": path,
        "matchString": [local_search_query::pattern(query)],
        "contextLines": {"before":READ_HANDOFF_CONTEXT,"after":READ_HANDOFF_CONTEXT},
    });
    read["regexEngine"] = json!(local_search_query::regex_mode(query));
    read["caseMode"] = json!(local_search_query::case_mode(query));
    read["wholeWord"] = json!(local_search_query::whole_word(query).unwrap_or(false));
    Some(Continuation::related(ToolId::LocalFetch, read).build())
}
