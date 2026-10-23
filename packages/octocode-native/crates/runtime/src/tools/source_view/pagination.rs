use super::extraction::line_count;
use super::types::*;

mod sanitize;
mod source_lines;
pub use sanitize::{sanitize_byte_page, sanitize_line_page};
pub(super) use source_lines::SourceLines;

pub struct Page {
    pub text: String,
    pub pagination: Pagination,
    pub view_lines: (usize, usize),
    /// The requested offset is at or past the end of a non-empty view (or any
    /// positive offset into an empty one): the page is empty by construction,
    /// not because the view ended here.
    pub out_of_range: bool,
}
/// Bytes one line page may hold when the caller has no response window to
/// size it from, and a byte page's default size; a single line longer than
/// the line page is read in byte pages. The one owner of this default for
/// every reader that pages like localFetch.
pub(crate) const LINE_PAGE_BYTES: usize = 16_384;

/// How many bytes a line page holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LineBudget {
    /// Bytes of the lines one page holds at most: the row's share of the
    /// response, which the reader then fits exactly (`read_share`).
    pub(super) fill: usize,
    /// A line longer than this is read in byte pages of about this size.
    pub(super) line: usize,
}

#[cfg(test)]
impl LineBudget {
    /// A page of at most `bytes` bytes, whose lines may each be as long.
    pub(super) fn bytes(bytes: usize) -> Self {
        Self {
            fill: bytes,
            line: bytes,
        }
    }
}

/// Where a byte page that reaches past the end of an oversized line (one
/// over the line page's `line_page_bytes`, the only reason a line read
/// pages by bytes) stops instead: that line's end, and the 0-based line line
/// paging resumes at. `None` when the line fits a line page, the page ends
/// inside it, or it is the last line.
fn oversized_line_end(
    content: &str,
    line: usize,
    offset: usize,
    end: usize,
    line_page_bytes: usize,
) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    // Only a line that ends within the page matters, and only its length
    // against the line page: the scans stop at the page's end and, behind
    // the offset, a line page's length. A line start further back than that
    // makes the line oversized whatever its exact start, so it is not found.
    let behind = offset.saturating_sub(line_page_bytes);
    let line_start = bytes[behind..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(behind, |at| behind + at + 1);
    let line_end = bytes[offset..end.min(bytes.len())]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|at| offset + at + 1)?;
    // The 1-based line at `offset` is also the 0-based index of the next.
    (line_end - line_start > line_page_bytes && line_end < bytes.len()).then_some((line_end, line))
}

/// Fewest bytes a byte page defaults to. A narrow response window (a few
/// rows sharing a small `responseLength`) shrinks the line page budget, but
/// a byte page cut to the window's half would take thousands of calls to
/// walk one line; the response pager splits an oversized row losslessly.
pub(crate) const MIN_BYTE_PAGE: usize = 4_096;

/// A byte page's default size: [`LINE_PAGE_BYTES`], or the line page budget
/// when that is smaller, and never under [`MIN_BYTE_PAGE`].
fn byte_page(line_page_bytes: usize) -> usize {
    line_page_bytes.clamp(MIN_BYTE_PAGE, LINE_PAGE_BYTES)
}

/// The page of `content` that `q` asks for, with a line page of at most
/// `budget`. With `source`, a
/// line window counts raw source lines: `offset` and `length` name a span
/// of the source, the page shows the view lines inside it, and the
/// pagination states the raw span it covers, so the pages reach every
/// source line exactly once whatever the view dropped.
pub(super) fn page_within(
    content: &str,
    q: &LocalFetchQuery,
    budget: LineBudget,
    source: Option<&SourceLines>,
) -> Page {
    if let Some(source) = source
        && q.unit.unwrap_or(if q.full_content == Some(true) {
            WindowUnit::Bytes
        } else {
            WindowUnit::Lines
        }) == WindowUnit::Lines
    {
        return raw_line_page(content, q, budget, source);
    }
    view_page(content, q, budget)
}

/// A line page over raw source lines (see [`page_within`]): the view page
/// of the view lines whose source lines fall in the requested raw window,
/// restated as the raw span from the requested offset to the next unshown
/// view line's source line (or the source end).
fn raw_line_page(
    content: &str,
    q: &LocalFetchQuery,
    budget: LineBudget,
    source: &SourceLines,
) -> Page {
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    let window = local_fetch_query::window_length(q).unwrap_or(DEFAULT_LINE_CHUNK);
    let lines = &source.lines;
    // View lines with source line <= raw offset are before the page.
    let first = lines.partition_point(|line| *line <= offset);
    let inside = lines.partition_point(|line| *line <= offset.saturating_add(window));
    // A raw window holding no kept line still shows the next kept one, so
    // every page advances.
    let count = inside
        .saturating_sub(first)
        .max(usize::from(first < lines.len()));
    let mut view_q = q.clone();
    view_q.full_content = None;
    view_q.unit = Some(WindowUnit::Lines);
    view_q.offset = Some(wire_count(first));
    view_q.length = wire_positive(count.max(1));
    let mut page = view_page(content, &view_q, budget);
    page.out_of_range = offset > 0 && offset >= source.total;
    let raw_of = |view_index: usize| lines.get(view_index).map(|line| line - 1);
    let pagination = &mut page.pagination;
    if pagination.unit == WindowUnit::Bytes {
        // One oversized view line pages by view bytes; line paging resumes
        // at the raw line after it.
        pagination.resume_line = pagination.resume_line.and_then(raw_of);
        return page;
    }
    let end = page.view_lines.1.max(first);
    let next = raw_of(end).unwrap_or(source.total).max(offset);
    let has_more = end < lines.len();
    *pagination = Pagination {
        unit: WindowUnit::Lines,
        offset: offset.min(source.total),
        length: next.saturating_sub(offset.min(source.total)),
        window: local_fetch_query::window_length(q).unwrap_or(next.saturating_sub(offset)),
        total_lines: source.total,
        total_bytes: content.len(),
        has_more,
        next_offset: has_more.then_some(next),
        resume_line: None,
    };
    page
}

/// A window of the view itself, in view lines or bytes.
fn view_page(content: &str, q: &LocalFetchQuery, budget: LineBudget) -> Page {
    let kind = q.unit.unwrap_or(if q.full_content == Some(true) {
        WindowUnit::Bytes
    } else {
        WindowUnit::Lines
    });
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    match kind {
        WindowUnit::Bytes => {
            let out_of_range = offset > 0 && offset >= content.len();
            // An offset inside a character starts the page at that
            // character's first byte (the end snaps forward the same way),
            // so no byte is skipped; the row states the page's offset.
            let offset = content.floor_char_boundary(offset);
            let chunk_size =
                local_fetch_query::window_length(q).unwrap_or(if q.full_content == Some(true) {
                    content.len()
                } else {
                    byte_page(budget.line)
                });
            let mut end = content.ceil_char_boundary((offset + chunk_size).min(content.len()));
            let first = octocode_engine::text::line_containing(content, offset);
            let resume = (q.full_content != Some(true))
                .then(|| oversized_line_end(content, first, offset, end, budget.line))
                .flatten();
            if let Some((line_end, _)) = resume {
                end = line_end;
            }
            let text = content[offset..end].to_owned();
            let last = first + line_count(&text).saturating_sub(1);
            Page {
                text,
                pagination: Pagination {
                    unit: kind,
                    offset,
                    length: end - offset,
                    window: chunk_size,
                    total_lines: line_count(content),
                    total_bytes: content.len(),
                    has_more: end < content.len(),
                    next_offset: (end < content.len()).then_some(end),
                    resume_line: resume.map(|(_, line)| line),
                },
                view_lines: (first, last),
                out_of_range,
            }
        }
        WindowUnit::Lines => {
            let total = line_count(content);
            let out_of_range = offset > 0 && offset >= total;
            let offset = offset.min(total);
            let requested_limit = local_fetch_query::window_length(q).unwrap_or(DEFAULT_LINE_CHUNK);
            // Only the page's own lines are cut: the longest run from the
            // offset within the requested count and the byte budget, before
            // any line too long for a line page (it is read in byte pages).
            let byte_offset = advance_lines(content, 0, offset).unwrap_or(content.len());
            let mut bytes = 0;
            let page = content[byte_offset..]
                .split_inclusive('\n')
                .take(requested_limit)
                .take_while(|line| {
                    bytes += line.len();
                    bytes <= budget.fill && line.len() <= budget.line
                })
                .count();
            let end = offset + page;
            if end == offset && offset < total {
                let mut byte_query = q.clone();
                byte_query.unit = Some(WindowUnit::Bytes);
                byte_query.offset = Some(wire_count(byte_offset));
                byte_query.length = wire_positive(byte_page(budget.line));
                return whole_line_page(view_page(content, &byte_query, budget), q, offset);
            }
            let end_byte = advance_lines(content, byte_offset, page).unwrap_or(content.len());
            let text = content[byte_offset..end_byte].to_owned();
            Page {
                text,
                pagination: Pagination {
                    unit: kind,
                    offset,
                    length: end - offset,
                    window: local_fetch_query::window_length(q)
                        .unwrap_or(end.saturating_sub(offset)),
                    total_lines: total,
                    total_bytes: content.len(),
                    has_more: end < total,
                    next_offset: (end < total).then_some(end),
                    resume_line: None,
                },
                view_lines: (offset + 1, end),
                out_of_range,
            }
        }
    }
}
/// A byte page a line read fell back to, restated as the line page it is
/// when it holds whole lines: it starts at line `offset` (0-based) and ends
/// at a line end (the oversized line's, or the view's). Its unit is then
/// the unit its continuation pages in. A page that ends inside a line stays
/// a byte page.
fn whole_line_page(mut page: Page, q: &LocalFetchQuery, offset: usize) -> Page {
    let pagination = &page.pagination;
    let Some(next) = pagination.resume_line.or_else(|| {
        (!pagination.has_more && page.view_lines.0 == offset + 1).then_some(page.view_lines.1)
    }) else {
        return page;
    };
    let total = pagination.total_lines;
    page.pagination = Pagination {
        unit: WindowUnit::Lines,
        offset,
        length: next - offset,
        window: local_fetch_query::window_length(q).unwrap_or(DEFAULT_LINE_CHUNK),
        total_lines: total,
        total_bytes: pagination.total_bytes,
        has_more: next < total,
        next_offset: (next < total).then_some(next),
        resume_line: None,
    };
    page
}

/// Context kept on each side of a line page when it is scanned alone, so a
/// multi-line secret straddling the page boundary is still matched whole. Same
/// bound as the engine's chunk overlap (bounded token patterns top out at a few
/// hundred chars; PEM blocks are redacted over the full file before selection).
pub(super) const PAGE_SCAN_MARGIN_BYTES: usize = 8_192;

/// The byte where the line `count` lines after the one starting at `at`
/// starts (the view's length after its last line), or `None` past that.
fn advance_lines(view: &str, mut at: usize, count: usize) -> Option<usize> {
    for _ in 0..count {
        if at >= view.len() {
            return None;
        }
        at = view[at..].find('\n').map_or(view.len(), |end| at + end + 1);
    }
    Some(at)
}

pub fn continuation(q: &LocalFetchQuery, p: &Pagination) -> Option<NextCalls> {
    p.next_offset.map(|offset| {
        let mut query = q.clone();
        // Line pages are the default unit; a line page the caller did not
        // size takes the line page budget again.
        let default_lines =
            |unit: WindowUnit| unit == WindowUnit::Lines && q.full_content != Some(true);
        if let Some(line) = p.resume_line {
            // Past the oversized line, paging is by lines again.
            query.offset = Some(wire_count(line));
            query.unit = (!default_lines(WindowUnit::Lines)).then_some(WindowUnit::Lines);
            query.length = None;
        } else {
            query.offset = Some(wire_count(offset));
            query.unit = (!default_lines(p.unit)).then_some(p.unit);
            query.length = if default_lines(p.unit) {
                q.length
            } else {
                wire_positive(p.window)
            };
        }
        NextCalls {
            continue_window: Some(FetchCall { query }),
            read_bounded_lines: None,
            restart: None,
            ..NextCalls::default()
        }
    })
}
pub fn result_counts(s: &str) -> (usize, usize, usize) {
    (s.encode_utf16().count(), s.len(), line_count(s))
}

#[cfg(test)]
mod tests;
