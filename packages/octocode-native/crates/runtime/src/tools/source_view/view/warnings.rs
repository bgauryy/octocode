//! Notes on a read's scope: a reversed range, a byte offset inside a
//! character, and a long-line byte walk.
use super::super::types::*;

/// Notes for a read's scope: a reversed range and a long-line byte walk.
/// A large file's head needs none: the description states the head size and
/// the row carries `totalLines` and `continueWindow`.
pub(super) fn read_scope_warnings(
    q: &LocalFetchQuery,
    pg: &super::super::pagination::Page,
    warnings: &mut Vec<String>,
) {
    warnings.extend(reversed_range_warnings(q));
    let page = &pg.pagination;
    if let Some(requested) = local_fetch_query::offset(q)
        && q.unit == Some(WindowUnit::Bytes)
        && page.unit == WindowUnit::Bytes
        && !pg.out_of_range
        && page.offset < requested
    {
        warnings.push(format!(
            "Byte offset {requested} falls inside a UTF-8 character; the page starts at its first byte, {}.",
            page.offset
        ));
    }
    if let Some(pages) = long_line_pages(q, pg) {
        warnings.push(long_line_warning(pages));
    }
}

/// A line read fell back to byte pages inside one line this long (a
/// minified bundle, a one-line JSON dump) takes this many pages or more
/// before its first page names the faster routes.
const LONG_LINE_PAGES: usize = 8;

/// The byte pages a file's first page opens, when a line read had to page by
/// bytes (one line over a line page) and there are [`LONG_LINE_PAGES`] or
/// more of them.
fn long_line_pages(q: &LocalFetchQuery, pg: &super::super::pagination::Page) -> Option<usize> {
    let asked_bytes = q.unit == Some(WindowUnit::Bytes) || q.full_content == Some(true);
    let page = &pg.pagination;
    let pages = page.total_bytes.checked_div(page.length)?;
    (!asked_bytes
        && page.unit == WindowUnit::Bytes
        && page.offset == 0
        && page.has_more
        && pages >= LONG_LINE_PAGES)
        .then(|| page.total_bytes.div_ceil(page.length))
}

fn long_line_warning(pages: usize) -> String {
    use crate::contracts::query_limits::local_fetch::LENGTH_MAXIMUM;
    format!(
        "One line fills this file: {pages} byte pages to read it through. Faster: matchString returns byte windows around each hit \
         (not with `contextLines:0`), or unit:\"bytes\" with length:{LENGTH_MAXIMUM} walks it in fewer, larger pages."
    )
}

/// A reversed range (`"50-10"`) reads the same span low end first; the row
/// says so, so the caller sees how its range was read.
fn reversed_range_warnings(q: &LocalFetchQuery) -> Vec<String> {
    q.ranges
        .iter()
        .filter_map(|range| {
            let (start, end) = crate::tools::line_spans::parse_span_raw::<u64>(range)?;
            (end < start)
                .then(|| format!("Range \"{range}\" is reversed: read as lines {end}-{start}."))
        })
        .collect()
}
