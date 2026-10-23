//! Fitting a page to the row's share of the response window.
use super::super::types::*;
use crate::id::ToolId;

/// Cuts a fitted page may try while it grows toward its share.
pub(super) const FIT_STEPS: usize = 4;

/// Response chars of `page`'s published row ([`published_row`],
/// [`crate::response::read_share::row_chars`]).
pub(super) fn page_chars(page: &LocalFetchResult, q: &LocalFetchQuery) -> usize {
    published_row(page, q).map_or(usize::MAX, |row| {
        crate::response::read_share::row_chars(&row)
    })
}

/// `page`'s row as the response stage ships it: shaped, its default extras
/// pruned and minimized. Its share holds this row, not the raw one.
fn published_row(page: &LocalFetchResult, q: &LocalFetchQuery) -> Option<serde_json::Value> {
    let query = crate::response::result::query_value(q);
    let data = serde_json::to_value(page).ok()?;
    let mut row = crate::response::rows::finish_row(
        ToolId::LocalFetch,
        0,
        &query,
        data,
        page.status.row_status(),
        crate::response::rows::RowParts::default(),
    );
    Some(row["data"].take())
}

/// The lines of `page`'s text that fit `share` beside the rest of `fitted`
/// (a shorter cut of it): `fitted`'s envelope prices the cut.
pub(super) fn lines_fitting(
    page: &LocalFetchResult,
    fitted: &LocalFetchResult,
    q: &LocalFetchQuery,
    share: usize,
) -> usize {
    let total = page.total_lines.unwrap_or(0);
    let text_chars = |text: Option<&str>| {
        crate::response::read_share::text_chars(text.unwrap_or_default(), total)
    };
    let envelope = page_chars(fitted, q).saturating_sub(text_chars(fitted.content.as_deref()));
    let mut room = share.saturating_sub(envelope);
    page.content
        .as_deref()
        .unwrap_or_default()
        .split_inclusive('\n')
        .take_while(|line| {
            let chars = text_chars(Some(line));
            let fits = chars <= room;
            room = room.saturating_sub(chars);
            fits
        })
        .count()
}

/// The lines of `page` that fit `share` response chars, when the page is a
/// line page over it ([`crate::response::read_share::fitted_lines`]).
pub(crate) fn fitted_lines(
    page: &LocalFetchResult,
    q: &LocalFetchQuery,
    share: usize,
) -> Option<usize> {
    if page.status != ReadStatus::Success {
        return None;
    }
    let row = published_row(page, q)?;
    crate::response::read_share::fitted_lines(&row, &crate::response::result::query_value(q), share)
}

/// `q` as a line page of `lines` from where it starts: a whole-file read
/// becomes the same view's first line page.
pub(super) fn shorter_page(q: &LocalFetchQuery, lines: usize) -> LocalFetchQuery {
    let mut query = q.clone();
    if query.full_content == Some(true) {
        query.full_content = None;
        query.offset = Some(0);
    }
    query.unit = Some(WindowUnit::Lines);
    query.length = wire_positive(lines);
    query
}

/// A page shortened to its share pages on at the size `asked` named
/// ([`page_on_as_asked`]); its `hasMore` and `next.continueWindow` state the
/// rest, so it is not partial.
pub(super) fn keep_asked_page(page: &mut LocalFetchResult, asked: &LocalFetchQuery) {
    if let Some(next) = page.next.as_mut() {
        page_on_as_asked(next, asked);
    }
}

/// Reads that page on from a page cut for the response share keep the
/// size `asked` named, not the share: a line read takes `asked`'s `length`
/// (and, when it is no window of the read, its `unit` and `fullContent`).
pub(super) fn page_on_as_asked(next: &mut NextCalls, asked: &LocalFetchQuery) {
    for read in next.reads_mut() {
        let query = &mut read.query;
        if query.unit == Some(WindowUnit::Bytes) {
            continue;
        }
        query.length = asked.length;
        if query.offset.is_none() && !local_fetch_query::has_ranges(query) {
            query.unit = asked.unit;
            query.full_content = asked.full_content;
        }
        if query.full_content.is_some() {
            query.length = None;
        }
    }
}
