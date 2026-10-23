//! The engine works in `usize`; the wire contract (generated from the core
//! Zod schema) owns the field set and its JSON integer types.
use super::LineRange;
use crate::num::{usize_of, usize_of_signed};
use octocode_contract_types::{LocalFetchQuery, MatchString, MinifyMode, ReadCaseMode, ReadRegex};

/// The one requested line span, when `ranges` holds exactly one.
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
/// Sets the read to the one line span `start..=end`.
pub(crate) fn set_line_span(query: &mut LocalFetchQuery, start: usize, end: usize) {
    set_line_spans(query, &[(start, end)]);
}
/// Sets the read to the line spans `start..=end`, in order.
pub(crate) fn set_line_spans(query: &mut LocalFetchQuery, spans: &[(usize, usize)]) {
    query.ranges = spans
        .iter()
        .filter_map(|(start, end)| format!("{start}-{end}").parse().ok())
        .collect();
}
/// `regex:"rust"` or `"pcre2"`: `matchString` is a regular expression.
pub(crate) fn is_regex(query: &LocalFetchQuery) -> bool {
    matches!(query.regex, Some(ReadRegex::Rust | ReadRegex::Pcre2))
}
/// `regex:"pcre2"`: lookaround and backreferences, under a deadline.
pub(crate) fn is_pcre2(query: &LocalFetchQuery) -> bool {
    query.regex == Some(ReadRegex::Pcre2)
}
/// Whether `pattern` matches case-sensitively. Omitted means smart, as
/// in localSearch: sensitive only when the pattern has an uppercase
/// letter.
pub(crate) fn case_sensitive_for(query: &LocalFetchQuery, pattern: &str) -> bool {
    match query.case_mode {
        Some(ReadCaseMode::Sensitive) => true,
        Some(ReadCaseMode::Smart) | None => pattern.chars().any(char::is_uppercase),
        Some(ReadCaseMode::Insensitive) => false,
    }
}
/// Context lines per match; the contract bounds the request.
pub(crate) fn context_lines(query: &LocalFetchQuery) -> Option<usize> {
    query.context_lines.map(usize_of_signed)
}
pub(crate) fn offset(query: &LocalFetchQuery) -> Option<usize> {
    query.offset.map(usize_of_signed)
}
pub(crate) fn window_length(query: &LocalFetchQuery) -> Option<usize> {
    query.length.map(|n| usize_of(n.get()))
}
/// Whether `matchString` hits must stand alone as words.
pub(crate) fn whole_word(query: &LocalFetchQuery) -> bool {
    query.whole_word == Some(true)
}

/// Every `matchString` entry (a list matches any of them).
pub(crate) fn match_strings(query: &LocalFetchQuery) -> Vec<&str> {
    match &query.match_string {
        None => Vec::new(),
        Some(MatchString::String(one)) => vec![one.as_str()],
        Some(MatchString::Array(list)) => list.iter().map(|one| one.as_str()).collect(),
    }
}
/// `ranges` parsed into 1-based inclusive line ranges, in request order.
/// The contract admits only `start-end` digits, so a malformed entry
/// (unreachable after validation) is skipped.
pub(crate) fn line_ranges(query: &LocalFetchQuery) -> Vec<LineRange> {
    query
        .ranges
        .iter()
        .filter_map(|range| {
            let (start, end) = crate::tools::line_spans::parse_span(range)?;
            Some(LineRange { start, end })
        })
        .collect()
}
pub(crate) fn has_ranges(query: &LocalFetchQuery) -> bool {
    !query.ranges.is_empty()
}
/// The file's last window as `(start, end)`, as wide as the widest
/// requested range, when every range starts past the file end. `None`
/// when any range reaches into the file (that read is served, clamped),
/// no range was requested, or the file has no line.
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
/// `block:true`: widen windows to the enclosing declaration.
pub(crate) fn block(query: &LocalFetchQuery) -> bool {
    query.block == Some(true)
}
/// Drop the multi-window selectors (`ranges`, `block`) from a derived query.
pub(crate) fn clear_block_selectors(query: &mut LocalFetchQuery) {
    query.ranges = Vec::new();
    query.block = None;
}
pub(crate) fn path(query: &LocalFetchQuery) -> &str {
    query.path.as_str()
}
/// The requested transformed view; `None` reads the file as is.
pub(crate) fn minify_mode(query: &LocalFetchQuery) -> Option<MinifyMode> {
    query.minify
}
