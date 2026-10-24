//! Typed views of the exact generated reader alternatives.
use super::LineRange;
use crate::num::{usize_of, usize_of_signed};
use octocode_contract_types::{
    CaseMode, LocalFetchQuery, LocalFetchQueryContentMatchString, ReadProgress, ReadProgressUnit,
    ReadView, RegexEngine, SourceLineRange,
};
use serde_json::{Value, json};

macro_rules! each_shape {
    ($query:expr, $q:ident => $body:expr) => {
        match $query {
            LocalFetchQuery::Content0($q) => $body,
            LocalFetchQuery::ContentRanges($q) => $body,
            LocalFetchQuery::ContentMatchString($q) => $body,
            LocalFetchQuery::Compact3($q) => $body,
            LocalFetchQuery::CompactRanges($q) => $body,
            LocalFetchQuery::Symbols($q) => $body,
        }
    };
}

pub(crate) fn path(query: &LocalFetchQuery) -> &str {
    each_shape!(query, q => q.path.as_str())
}

pub(crate) fn matched(query: &LocalFetchQuery) -> Option<&LocalFetchQueryContentMatchString> {
    match query {
        LocalFetchQuery::ContentMatchString(q) => Some(q),
        _ => None,
    }
}

pub(crate) fn matched_mut(
    query: &mut LocalFetchQuery,
) -> Option<&mut LocalFetchQueryContentMatchString> {
    match query {
        LocalFetchQuery::ContentMatchString(q) => Some(q),
        _ => None,
    }
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
    match query {
        LocalFetchQuery::ContentRanges(q) => &q.ranges,
        LocalFetchQuery::ContentMatchString(q) => &q.ranges,
        LocalFetchQuery::CompactRanges(q) => &q.ranges,
        _ => &[],
    }
}

pub(crate) fn line_ranges(query: &LocalFetchQuery) -> Vec<LineRange> {
    ranges(query)
        .iter()
        .map(|range| LineRange {
            start: usize_of(range.start_line.get()),
            end: usize_of(range.end_line.get()),
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
    match query {
        LocalFetchQuery::ContentRanges(q) => q.block == Some(true),
        LocalFetchQuery::ContentMatchString(q) => q.block == Some(true),
        LocalFetchQuery::CompactRanges(q) => q.block == Some(true),
        _ => false,
    }
}

pub(crate) fn view(query: &LocalFetchQuery) -> ReadView {
    match query {
        LocalFetchQuery::Compact3(_) | LocalFetchQuery::CompactRanges(_) => ReadView::Compact,
        LocalFetchQuery::Symbols(_) => ReadView::Symbols,
        _ => ReadView::Content,
    }
}

pub(crate) fn has_classification(query: &LocalFetchQuery) -> bool {
    match query {
        LocalFetchQuery::Content0(q) => q.classification.is_some(),
        LocalFetchQuery::ContentRanges(q) => q.classification.is_some(),
        LocalFetchQuery::ContentMatchString(q) => q.classification.is_some(),
        _ => false,
    }
}

pub(crate) fn selection(query: &LocalFetchQuery) -> Option<&str> {
    match query {
        LocalFetchQuery::Content0(q) => q.selection.as_ref().map(|token| token.as_str()),
        LocalFetchQuery::ContentRanges(q) => q.selection.as_ref().map(|token| token.as_str()),
        LocalFetchQuery::ContentMatchString(q) => q.selection.as_ref().map(|token| token.as_str()),
        _ => None,
    }
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
    match query {
        LocalFetchQuery::Content0(q) => q.selection = selection.map(str::parse).transpose()?,
        LocalFetchQuery::ContentRanges(q) => q.selection = selection.map(str::parse).transpose()?,
        LocalFetchQuery::ContentMatchString(q) => {
            q.selection = selection.map(str::parse).transpose()?;
        }
        _ if selection.is_some() => return Err("selection requires content view".into()),
        _ => {}
    }
    Ok(())
}
