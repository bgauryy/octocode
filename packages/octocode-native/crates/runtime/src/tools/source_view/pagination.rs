use super::extraction::line_count;
use super::types::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

mod long_lines;
mod source_lines;
pub(super) use long_lines::LongLines;
#[cfg(test)]
pub(crate) use long_lines::placed_pieces;
use long_lines::{MatchFirst, filled_page, oversized_line_end};
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

/// How much a line page holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LineBudget {
    /// Response characters the page's lines take at most as they render: each
    /// line's JSON text (escapes, UTF-16) and its gutter.
    pub(super) fill: usize,
    /// A line longer than this is read in byte pages of about this size.
    pub(super) line: usize,
    /// Characters each line's gutter (`<line>\t`) adds when it renders; 0 for
    /// a view that is not numbered.
    pub(super) gutter: usize,
}

impl LineBudget {
    /// The characters one line of a page takes as it renders.
    pub(super) fn rendered(self, line: &str) -> usize {
        octocode_engine::json_size::string_body_units(line) + self.gutter
    }
}

#[cfg(test)]
impl LineBudget {
    /// A page of at most `bytes` bytes, whose lines may each be as long.
    pub(super) fn bytes(bytes: usize) -> Self {
        Self {
            fill: bytes,
            line: bytes,
            gutter: 0,
        }
    }
}

/// Fewest bytes a byte page defaults to. A narrow response window (a few
/// rows sharing a small `responseChars`) shrinks the line page budget, but
/// a byte page cut to the window's half would take thousands of calls to
/// walk one line; the response pager splits an oversized row losslessly.
pub(crate) const MIN_BYTE_PAGE: usize = 4_096;

/// A byte page's room, in the escaped UTF-16 units a JSON string of it
/// takes (the unit of `responseChars`): [`LINE_PAGE_BYTES`], or the line
/// page budget when that is smaller, and never under [`MIN_BYTE_PAGE`].
fn byte_page(line_page_bytes: usize) -> usize {
    line_page_bytes.clamp(MIN_BYTE_PAGE, LINE_PAGE_BYTES)
}

/// The page of `content` that `q` asks for, with a line page of at most
/// `budget`. With `source`, a
/// line window counts raw source lines from private read progress.
/// The page shows the view lines inside that window, and the
/// pagination states the raw span it covers, so the pages reach every
/// source line exactly once whatever the view dropped.
pub(super) fn page_within(
    content: &str,
    q: &LocalFetchQuery,
    budget: LineBudget,
    source: Option<&SourceLines>,
    long_lines: Option<LongLines<'_>>,
) -> Page {
    let Some(source) = source else {
        return view_page(content, q, budget, long_lines);
    };
    if local_fetch_query::unit(q) == ReadProgressUnit::Lines {
        return raw_line_page(content, q, budget, source);
    }
    // A byte page of an oversized view line resumes line paging after it, in
    // the raw lines a line page counts, never at the view line's own index.
    let mut page = view_page(content, q, budget, None);
    page.pagination.resume_line = page
        .pagination
        .resume_line
        .map(|view_line| source.raw_offset(view_line));
    page
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
    let window = local_fetch_query::line_limit(q);
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
    local_fetch_query::set_progress(
        &mut view_q,
        Some(octocode_contract_types::ReadProgress {
            unit: ReadProgressUnit::Lines,
            offset: wire_count(first),
        }),
    );
    let mut page = view_page_limited(content, &view_q, budget, count.max(1), None);
    page.out_of_range = offset > 0 && offset >= source.total;
    let pagination = &mut page.pagination;
    if pagination.unit == ReadProgressUnit::Bytes {
        // One oversized view line pages by view bytes; line paging resumes
        // at the raw line after it.
        pagination.resume_line = pagination.resume_line.map(|line| source.raw_offset(line));
        return page;
    }
    let end = page.view_lines.1.max(first);
    let next = source.raw_offset(end).max(offset);
    let has_more = end < lines.len();
    *pagination = Pagination {
        unit: ReadProgressUnit::Lines,
        offset: offset.min(source.total),
        length: next.saturating_sub(offset.min(source.total)),
        window: next.saturating_sub(offset),
        total_lines: source.total,
        total_bytes: content.len(),
        has_more,
        next_offset: has_more.then_some(next),
        resume_line: None,
    };
    page
}

/// A window of the view itself, in view lines or bytes.
fn view_page(
    content: &str,
    q: &LocalFetchQuery,
    budget: LineBudget,
    long_lines: Option<LongLines<'_>>,
) -> Page {
    view_page_limited(
        content,
        q,
        budget,
        local_fetch_query::line_limit(q),
        long_lines,
    )
}

/// A window of the view, at most `limit` lines of it. A line too long for
/// one page is read in byte pages: from its first byte, or, when matches sit
/// in it, their windows first ([`MatchFirst`]); either way the required
/// continuations reach every byte of the line once.
fn view_page_limited(
    content: &str,
    q: &LocalFetchQuery,
    budget: LineBudget,
    limit: usize,
    long_lines: Option<LongLines<'_>>,
) -> Page {
    let kind = local_fetch_query::unit(q);
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    match kind {
        ReadProgressUnit::Bytes => {
            let out_of_range = offset > 0 && offset >= content.len();
            // The reader refused an offset no window emits; a streamed
            // window's raw offset starts at its character's first byte.
            let offset = content.floor_char_boundary(offset.min(content.len()));
            let chunk_size = byte_page(budget.line);
            if !out_of_range
                && let Some(walk) = MatchFirst::of(content, long_lines, offset, chunk_size)
            {
                return walk.page(content, offset, chunk_size);
            }
            // The window that fits the room as serialized text, ending at
            // a line end when one fits, never inside a character, a CRLF
            // pair or a redaction marker.
            let mut end = crate::response::cut::window_end(content, offset, chunk_size);
            let first = octocode_engine::text::line_containing(content, offset);
            let resume = oversized_line_end(content, first, offset, end, budget.line);
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
        ReadProgressUnit::Lines => {
            let total = line_count(content);
            let out_of_range = offset > 0 && offset >= total;
            let offset = offset.min(total);
            let requested_limit = limit;
            // Only the page's own lines are cut: the longest run from the
            // offset within the requested count and the rendered fill, before
            // any line too long for a line page (it is read in byte pages).
            // The first line within the byte budget is taken even when it
            // renders over the fill: a line page of it alone, which the
            // response pager splits without loss, so short lines after it
            // keep paging by lines.
            let byte_offset = advance_lines(content, 0, offset).unwrap_or(content.len());
            let mut rendered = 0;
            // Window separators are no rows: `limit` counts the lines
            // between them, and the next page never starts on one.
            let mut rows = 0;
            let mut page = 0;
            // The page stopped at a line too long for a line page.
            let mut long = false;
            for (at, line) in content[byte_offset..].split_inclusive('\n').enumerate() {
                let separator = crate::response::numbered::is_separator(line);
                if !separator && rows == requested_limit {
                    break;
                }
                rendered += budget.rendered(line);
                long = line.len() > budget.line;
                if (at != 0 && rendered > budget.fill) || long {
                    break;
                }
                rows += usize::from(!separator);
                page = at + 1;
            }
            let end = offset + page;
            if end == offset && offset < total {
                // Matches in the line lead its byte walk.
                let matched =
                    MatchFirst::of(content, long_lines, byte_offset, byte_page(budget.line))
                        .map(|walk| walk.first());
                let mut byte_query = q.clone();
                local_fetch_query::set_progress(
                    &mut byte_query,
                    Some(octocode_contract_types::ReadProgress {
                        unit: ReadProgressUnit::Bytes,
                        offset: wire_count(matched.unwrap_or(byte_offset)),
                    }),
                );
                let page = view_page(content, &byte_query, budget, long_lines);
                return match matched {
                    Some(_) => page,
                    None => whole_line_page(page, offset),
                };
            }
            let end_byte = advance_lines(content, byte_offset, page).unwrap_or(content.len());
            if long
                && let Some(filled) = filled_page(
                    content,
                    byte_offset..end_byte,
                    (offset, page),
                    budget,
                    long_lines,
                )
            {
                return filled;
            }
            let text = content[byte_offset..end_byte].to_owned();
            Page {
                text,
                pagination: Pagination {
                    unit: kind,
                    offset,
                    length: end - offset,
                    window: end.saturating_sub(offset),
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
fn whole_line_page(mut page: Page, offset: usize) -> Page {
    let pagination = &page.pagination;
    let Some(next) = pagination.resume_line.or_else(|| {
        (!pagination.has_more && page.view_lines.0 == offset + 1).then_some(page.view_lines.1)
    }) else {
        return page;
    };
    let total = pagination.total_lines;
    page.pagination = Pagination {
        unit: ReadProgressUnit::Lines,
        offset,
        length: next - offset,
        window: next - offset,
        total_lines: total,
        total_bytes: pagination.total_bytes,
        has_more: next < total,
        next_offset: (next < total).then_some(next),
        resume_line: None,
    };
    page
}

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
        let progress = p.resume_line.map_or(
            octocode_contract_types::ReadProgress {
                unit: p.unit,
                offset: wire_count(offset),
            },
            |line| octocode_contract_types::ReadProgress {
                unit: ReadProgressUnit::Lines,
                offset: wire_count(line),
            },
        );
        local_fetch_query::set_progress(&mut query, Some(progress));
        NextCalls {
            continue_window: Some(FetchCall::continued(query)),
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
