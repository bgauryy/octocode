//! Typed views of the exact generated reader alternatives.
use super::LineRange;
use crate::num::usize_of_signed;
use crate::tools::variant_field::variant_field;
use octocode_contract_types::{
    CaseMode, LocalFetchQuery, LocalFetchQueryContent0, LocalFetchQueryContent0View,
    LocalFetchQueryContentMatchString, LocalFetchQueryContentRanges,
    LocalFetchQueryContentRangesView, LocalFetchQuerySymbols, LocalFetchQuerySymbolsView,
    ReadProgress, ReadProgressUnit, ReadView, RegexEngine, SourceLineRange,
};
use octocode_engine::num::saturating_usize;
use serde_json::{Value, json};

/// Binds `$q` to the query struct of every view.
macro_rules! each_shape {
    ($query:expr, $q:ident => $body:expr) => {
        variant_field!($query, each [
            LocalFetchQuery::Content0, LocalFetchQuery::ContentRanges,
            LocalFetchQuery::ContentMatchString, LocalFetchQuery::Compact3,
            LocalFetchQuery::CompactRanges, LocalFetchQuery::Symbols,
        ] $q => $body)
    };
}

/// Binds `$q` to the query struct of a `content` view; `$other` answers for
/// `compact` and `symbols`.
macro_rules! content_view {
    ($query:expr, $q:ident => $body:expr, else => $other:expr) => {
        variant_field!($query, each [
            LocalFetchQuery::Content0, LocalFetchQuery::ContentRanges,
            LocalFetchQuery::ContentMatchString,
        ] $q => $body, else => $other)
    };
}

/// Binds `$q` to the query struct of a view with `ranges`; `$other` answers
/// for the rest.
macro_rules! ranged_view {
    ($query:expr, $q:ident => $body:expr, else => $other:expr) => {
        variant_field!($query, each [
            LocalFetchQuery::ContentRanges, LocalFetchQuery::ContentMatchString,
            LocalFetchQuery::CompactRanges,
        ] $q => $body, else => $other)
    };
}

pub(crate) fn path(query: &LocalFetchQuery) -> &str {
    each_shape!(query, q => q.path.as_str())
}

pub(crate) fn matched(query: &LocalFetchQuery) -> Option<&LocalFetchQueryContentMatchString> {
    variant_field!(query, each [LocalFetchQuery::ContentMatchString] q => Some(q), else => None)
}

pub(crate) fn matched_mut(
    query: &mut LocalFetchQuery,
) -> Option<&mut LocalFetchQueryContentMatchString> {
    variant_field!(query, each [LocalFetchQuery::ContentMatchString] q => Some(q), else => None)
}

pub(crate) fn has_matches(query: &LocalFetchQuery) -> bool {
    matched(query).is_some()
}

pub(crate) fn is_regex(query: &LocalFetchQuery) -> bool {
    matched(query).is_some_and(|q| q.regex_engine != RegexEngine::Literal)
}

pub(crate) fn is_pcre2(query: &LocalFetchQuery) -> bool {
    matched(query).is_some_and(|q| q.regex_engine == RegexEngine::Pcre2)
}

pub(crate) fn case_sensitive_for(query: &LocalFetchQuery, pattern: &str) -> bool {
    matched(query).is_some_and(|q| match q.case_mode {
        CaseMode::Sensitive => true,
        CaseMode::Smart => pattern.chars().any(char::is_uppercase),
        CaseMode::Insensitive => false,
    })
}

pub(crate) fn context_lines(query: &LocalFetchQuery) -> octocode_engine::types::SearchContext {
    matched(query)
        .and_then(|q| q.context_lines.as_ref())
        .map(|context| octocode_engine::types::SearchContext {
            before: crate::num::u32_of_signed(context.before),
            after: crate::num::u32_of_signed(context.after),
        })
        .unwrap_or_default()
}

pub(crate) fn whole_word(query: &LocalFetchQuery) -> bool {
    matched(query).is_some_and(|q| q.whole_word)
}

pub(crate) fn match_strings(query: &LocalFetchQuery) -> Vec<&str> {
    matched(query).map_or_else(Vec::new, |q| {
        q.match_string.0.iter().map(|term| term.as_str()).collect()
    })
}

fn ranges(query: &LocalFetchQuery) -> &[SourceLineRange] {
    ranged_view!(query, q => &q.ranges, else => &[])
}

pub(crate) fn line_ranges(query: &LocalFetchQuery) -> Vec<LineRange> {
    ranges(query)
        .iter()
        .map(|range| LineRange {
            start: saturating_usize(range.start_line.get()),
            end: saturating_usize(range.end_line.get()),
        })
        .collect()
}

pub(crate) fn has_ranges(query: &LocalFetchQuery) -> bool {
    !ranges(query).is_empty()
}

fn single_range(query: &LocalFetchQuery) -> Option<LineRange> {
    match line_ranges(query).as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

pub(crate) fn start_line(query: &LocalFetchQuery) -> Option<usize> {
    single_range(query).map(|range| range.start)
}

pub(crate) fn end_line(query: &LocalFetchQuery) -> Option<usize> {
    single_range(query).map(|range| range.end)
}

pub(crate) fn block(query: &LocalFetchQuery) -> bool {
    ranged_view!(query, q => q.block == Some(true), else => false)
}

pub(crate) fn view(query: &LocalFetchQuery) -> ReadView {
    match query {
        LocalFetchQuery::Compact3(_) | LocalFetchQuery::CompactRanges(_) => ReadView::Compact,
        LocalFetchQuery::Symbols(_) => ReadView::Symbols,
        _ => ReadView::Content,
    }
}

pub(crate) fn has_classification(query: &LocalFetchQuery) -> bool {
    content_view!(query, q => q.classification.is_some(), else => false)
}

pub(crate) fn selection(query: &LocalFetchQuery) -> Option<&str> {
    content_view!(query, q => q.selection.as_ref().map(|token| token.as_str()), else => None)
}

pub(crate) fn snapshot(query: &LocalFetchQuery) -> Option<&str> {
    each_shape!(query, q => q.snapshot.as_ref().map(|value| value.as_str()))
}

pub(crate) fn read_progress(query: &LocalFetchQuery) -> Option<&ReadProgress> {
    each_shape!(query, q => q.read_progress.as_ref())
}

pub(crate) fn offset(query: &LocalFetchQuery) -> Option<usize> {
    read_progress(query).map(|progress| usize_of_signed(progress.offset))
}

/// Lines one page holds at most: the caller's `limit`; omitted, the page
/// budget alone cuts it.
pub(crate) fn line_limit(query: &LocalFetchQuery) -> usize {
    each_shape!(query, q => q.limit.map_or(usize::MAX, |limit| saturating_usize(limit.get())))
}

pub(crate) fn unit(query: &LocalFetchQuery) -> ReadProgressUnit {
    read_progress(query).map_or(ReadProgressUnit::Lines, |progress| progress.unit)
}

pub(crate) fn set_progress(query: &mut LocalFetchQuery, progress: Option<ReadProgress>) {
    each_shape!(query, q => q.read_progress = progress);
}

/// Revalidate an internally derived selection through the generated exact alternatives.
pub(crate) fn updated(
    query: &LocalFetchQuery,
    changes: Value,
) -> Result<LocalFetchQuery, serde_json::Error> {
    let mut value = serde_json::to_value(query)?;
    if let (Some(row), Some(changes)) = (value.as_object_mut(), changes.as_object()) {
        for (key, value) in changes {
            if value.is_null() {
                row.remove(key);
            } else {
                row.insert(key.clone(), value.clone());
            }
        }
    }
    serde_json::from_value(value)
}

pub(crate) fn set_line_span(
    query: &mut LocalFetchQuery,
    start: usize,
    end: usize,
) -> Result<(), serde_json::Error> {
    set_line_spans(query, &[(start, end)])
}

pub(crate) fn set_line_spans(
    query: &mut LocalFetchQuery,
    spans: &[(usize, usize)],
) -> Result<(), serde_json::Error> {
    *query = updated(
        query,
        json!({"ranges": spans.iter().map(|(start, end)| json!({"startLine":start,"endLine":end})).collect::<Vec<_>>()}),
    )?;
    Ok(())
}

pub(crate) fn last_window_span(
    query: &LocalFetchQuery,
    total_lines: usize,
) -> Option<(usize, usize)> {
    let requested = line_ranges(query);
    if total_lines == 0
        || requested.is_empty()
        || requested.iter().any(|range| range.start <= total_lines)
    {
        return None;
    }
    let width = requested
        .iter()
        .map(|range| range.end.saturating_sub(range.start) + 1)
        .max()?;
    Some((total_lines.saturating_sub(width - 1).max(1), total_lines))
}

pub(crate) fn set_path(
    query: &mut LocalFetchQuery,
    path: &str,
) -> Result<(), octocode_contract_types::error::ConversionError> {
    each_shape!(query, q => { q.path = path.parse()?; Ok(()) })
}

pub(crate) fn set_snapshot(
    query: &mut LocalFetchQuery,
    snapshot: Option<&str>,
) -> Result<(), octocode_contract_types::error::ConversionError> {
    each_shape!(query, q => { q.snapshot = snapshot.map(str::parse).transpose()?; Ok(()) })
}

pub(crate) fn set_selection(
    query: &mut LocalFetchQuery,
    selection: Option<&str>,
) -> Result<(), octocode_contract_types::error::ConversionError> {
    content_view!(query, q => q.selection = selection.map(str::parse).transpose()?, else => {
        if selection.is_some() {
            return Err("selection requires content view".into());
        }
    });
    Ok(())
}

/// A whole-file `content` read of `path`: the lead that opens a file.
pub(crate) fn read_lead(path: &str) -> Option<LocalFetchQuery> {
    Some(
        LocalFetchQueryContent0 {
            classification: None,
            limit: None,
            main_goal: None,
            path: path.try_into().ok()?,
            read_progress: None,
            reasoning: None,
            selection: None,
            snapshot: None,
            view: LocalFetchQueryContent0View::Content,
        }
        .into(),
    )
}

/// A `content` read of `path` over the inclusive `(start, end)` line
/// windows; `None` when a window starts or ends before line 1.
pub(crate) fn ranges_lead(path: &str, windows: &[(u64, u64)]) -> Option<LocalFetchQuery> {
    let ranges = windows
        .iter()
        .map(|&(start, end)| {
            Some(SourceLineRange {
                start_line: std::num::NonZeroU64::new(start)?,
                end_line: std::num::NonZeroU64::new(end)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(
        LocalFetchQueryContentRanges {
            block: None,
            classification: None,
            limit: None,
            main_goal: None,
            path: path.try_into().ok()?,
            ranges,
            read_progress: None,
            reasoning: None,
            selection: None,
            snapshot: None,
            view: LocalFetchQueryContentRangesView::Content,
        }
        .into(),
    )
}

/// The `symbols` outline of `path`.
pub(crate) fn outline_lead(path: &str) -> Option<LocalFetchQuery> {
    Some(
        LocalFetchQuerySymbols {
            limit: None,
            main_goal: None,
            path: path.try_into().ok()?,
            read_progress: None,
            reasoning: None,
            snapshot: None,
            view: LocalFetchQuerySymbolsView::Symbols,
        }
        .into(),
    )
}
