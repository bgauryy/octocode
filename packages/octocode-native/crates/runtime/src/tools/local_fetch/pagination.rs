use super::extraction::line_count;
use super::types::*;
use crate::security::scan::ContentScan;
use octocode_engine::security::REDACTION_PREFIX;
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
pub(super) const MIN_BYTE_PAGE: usize = 4_096;

/// A byte page's default size: [`LINE_PAGE_BYTES`], or the line page budget
/// when that is smaller, and never under [`MIN_BYTE_PAGE`].
fn byte_page(line_page_bytes: usize) -> usize {
    line_page_bytes.clamp(MIN_BYTE_PAGE, LINE_PAGE_BYTES)
}

/// The 1-based line holding byte `offset` of `content`.
fn line_at(content: &str, offset: usize) -> usize {
    1 + content.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

/// The lines behind a transformed view (the standard view drops blank and
/// comment lines) in the unit a raw read of the same selection pages in:
/// each view line's 1-based line of the selected text, ascending, and that
/// text's line count. A whole-file view's selected text is the source.
pub(super) struct SourceLines {
    pub(super) lines: Vec<usize>,
    pub(super) total: usize,
}

impl SourceLines {
    /// The source line of every line of a gutter-numbered view (`N\t…`);
    /// `None` when a line carries no number or the numbers do not ascend.
    pub(super) fn of_gutter(view: &str, total: usize) -> Option<Self> {
        let mut lines = Vec::new();
        for record in view.split_inclusive('\n') {
            let (number, _) = record.split_once(crate::response::numbered::SEPARATOR)?;
            let line = number.parse::<usize>().ok()?;
            if lines.last().is_some_and(|last| *last >= line) || line > total {
                return None;
            }
            lines.push(line);
        }
        Some(Self { lines, total })
    }

    /// [`Self::of_gutter`] for a view of an extraction (ranges): `selected`
    /// maps each line of the extracted text to its source line
    /// ([`super::extraction::OMISSION_LINE`] for an omission marker), so a
    /// gutter line counts as its line of the extracted text and a gap line
    /// ([`crate::response::numbered::GAP`]) as its marker. `None` when a view
    /// line maps onto no extracted line.
    pub(super) fn of_selected_gutter(view: &str, selected: &[usize]) -> Option<Self> {
        let mut lines = Vec::new();
        let mut next = 0;
        for record in view.split_inclusive('\n') {
            let wanted = if record == crate::response::numbered::GAP {
                super::extraction::OMISSION_LINE
            } else {
                let (number, _) = record.split_once(crate::response::numbered::SEPARATOR)?;
                number.parse::<usize>().ok()?
            };
            let at = next
                + selected
                    .get(next..)?
                    .iter()
                    .position(|line| *line == wanted)?;
            lines.push(at + 1);
            next = at + 1;
        }
        Some(Self {
            lines,
            total: selected.len(),
        })
    }
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
            let mut offset = offset.min(content.len());
            while !content.is_char_boundary(offset) {
                offset -= 1;
            }
            let chunk_size =
                local_fetch_query::window_length(q).unwrap_or(if q.full_content == Some(true) {
                    content.len()
                } else {
                    byte_page(budget.line)
                });
            let mut end = (offset + chunk_size).min(content.len());
            while end < content.len() && !content.is_char_boundary(end) {
                end += 1
            }
            let first = line_at(content, offset);
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
                return view_page(content, &byte_query, budget);
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
/// Context kept on each side of a line page when it is scanned alone, so a
/// multi-line secret straddling the page boundary is still matched whole. Same
/// bound as the engine's chunk overlap (bounded token patterns top out at a few
/// hundred chars; PEM blocks are redacted over the full file before selection).
pub(super) const PAGE_SCAN_MARGIN_BYTES: usize = 8_192;

/// Secret-scan only a line page (plus [`PAGE_SCAN_MARGIN_BYTES`] of
/// whole lines on each side) instead of the whole selected view, so paging a
/// large file costs one page scan rather than one full-file scan per page.
///
/// Sound because redaction preserves line structure: the redacted window has
/// the same line count, so the page's lines are cut from it by index. Returns
/// `None` when a redaction changed the line count (callers fall back to the
/// whole-view scan). The boolean reports whether the returned page changed.
pub fn sanitize_line_page(
    view: &str,
    raw: Page,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> Result<Option<(Page, bool)>, (String, String)> {
    let (first, last) = (raw.view_lines.0.saturating_sub(1), raw.view_lines.1);
    if raw.out_of_range || first >= last {
        return Ok(Some((raw, false)));
    }
    // Walk the view's newlines only as far as the page and its margins:
    // a page's cost follows the page, not the whole view.
    let Some((page_start, page_end)) = advance_lines(view, 0, first)
        .and_then(|start| Some((start, advance_lines(view, start, last - first)?)))
    else {
        return Ok(Some((raw, false)));
    };
    let mut start = page_start;
    while start > 0 && page_start - start < PAGE_SCAN_MARGIN_BYTES {
        start = view[..start - 1].rfind('\n').map_or(0, |at| at + 1);
    }
    let mut end = page_end;
    while end < view.len() && end - page_end < PAGE_SCAN_MARGIN_BYTES {
        end = advance_lines(view, end, 1).unwrap_or(view.len());
    }
    let window = &view[start..end];
    let (clean, _) = security.sanitize(window, source_path)?;
    if clean == window {
        return Ok(Some((raw, false)));
    }
    let clean_lines = clean.split_inclusive('\n').collect::<Vec<_>>();
    if clean_lines.len() != window.split_inclusive('\n').count() {
        return Ok(None);
    }
    let before = view[start..page_start].matches('\n').count();
    let text = clean_lines[before..before + (last - first)].concat();
    let changed = text != raw.text;
    Ok(Some((Page { text, ..raw }, changed)))
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
/// LOC-byte: secret-scan only a byte page's window instead of the whole
/// selected view. The window is the page's whole lines plus at least
/// [`PAGE_SCAN_MARGIN_BYTES`] of whole lines on each side, so every
/// single-line secret touching the page is scanned whole (token patterns never
/// span a newline, whatever their length), multi-line patterns get the same
/// margin as line pages and the engine's chunk overlap, and PEM/key blocks
/// were already redacted over the full file before selection.
///
/// Offsets stay in raw-view coordinates. A redacted line is sliced to the
/// page's raw range on its sanitized text, never raw bytes: a secret the
/// page's end cuts extends the page to the secret's end, so its placeholder
/// shows on the page it starts on and `next_offset` resumes after it, and an
/// offset inside a secret starts the page at its placeholder. Returns
/// `Ok(None)` when redaction changes the line structure even over the whole
/// view; the caller then falls back to the whole-view scan.
pub fn sanitize_byte_page(
    view: &str,
    raw: Page,
    source_path: &std::path::Path,
    security: &impl ContentScan,
) -> Result<Option<(Page, bool)>, (String, String)> {
    if raw.out_of_range || raw.text.is_empty() {
        return Ok(Some((raw, false)));
    }
    let bytes = view.as_bytes();
    let start = raw.pagination.offset;
    let end = start + raw.pagination.length;
    let line_start = |at: usize| {
        bytes[..at]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1)
    };
    let line_end = |at: usize| {
        bytes[at..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |index| at + index + 1)
    };
    let window_start = line_start(start.saturating_sub(PAGE_SCAN_MARGIN_BYTES));
    let window_end = line_end((end + PAGE_SCAN_MARGIN_BYTES).min(bytes.len()));
    for (from, to) in [(window_start, window_end), (0, bytes.len())] {
        let window = &view[from..to];
        let (clean, _) = security.sanitize(window, source_path)?;
        if clean == window {
            return Ok(Some((raw, false)));
        }
        if let Some(page) = map_clean_window(view, from, window, &clean, &raw) {
            return Ok(Some((page, true)));
        }
    }
    Ok(None)
}

/// Unchanged text after a placeholder used to find where its span ends.
const ANCHOR_BYTES: usize = 32;

/// Offset in `clean` (a sanitized copy of `raw` whose secrets became
/// `[REDACTED…]` placeholders) that corresponds to byte `raw_at` of `raw`.
/// Text between placeholders is identical in both, so the walk advances in
/// lockstep there and re-synchronizes after each placeholder on the unchanged
/// text that follows it. A position inside a redacted span maps to that
/// span's placeholder start, so a slice never contains secret bytes, and
/// also names where the span ends: `(raw_end, clean_end)`, the byte after
/// the span in `raw` and after its placeholder in `clean`.
fn clean_offset(raw: &str, clean: &str, raw_at: usize) -> (usize, Option<(usize, usize)>) {
    let (mut i, mut j) = (0usize, 0usize);
    while i < raw_at && j < clean.len() {
        if clean[j..].starts_with(REDACTION_PREFIX) {
            let placeholder_end = clean[j..].find(']').map_or(clean.len(), |at| j + at + 1);
            let anchor_end = clean[placeholder_end..]
                .find(REDACTION_PREFIX)
                .map_or(clean.len(), |at| placeholder_end + at)
                .min(placeholder_end + ANCHOR_BYTES);
            let mut anchor_end = anchor_end;
            while !clean.is_char_boundary(anchor_end) {
                anchor_end -= 1;
            }
            let anchor = &clean[placeholder_end..anchor_end];
            let resumes = if anchor.is_empty() {
                raw.len()
            } else {
                raw[i..].find(anchor).map_or(raw.len(), |at| i + at)
            };
            if resumes > raw_at {
                return (j, Some((resumes, placeholder_end)));
            }
            i = resumes;
            j = placeholder_end;
            continue;
        }
        let step = clean[j..].chars().next().map_or(1, char::len_utf8);
        i += step;
        j += step;
    }
    let mut j = j.min(clean.len());
    while !clean.is_char_boundary(j) {
        j -= 1;
    }
    (j, None)
}

/// Rebuild a raw-coordinate byte page from a sanitized window with the same
/// line count: unchanged lines keep their raw slice, redacted lines give the
/// slice of their sanitized text that maps onto the page's raw range.
fn map_clean_window(
    view: &str,
    from: usize,
    window: &str,
    clean: &str,
    raw: &Page,
) -> Option<Page> {
    let raw_lines = window.split_inclusive('\n').collect::<Vec<_>>();
    let clean_lines = clean.split_inclusive('\n').collect::<Vec<_>>();
    if raw_lines.len() != clean_lines.len() {
        return None;
    }
    let start = raw.pagination.offset;
    let mut end = start + raw.pagination.length;
    let mut text = String::with_capacity(raw.text.len());
    let mut position = from;
    for (raw_line, clean_line) in raw_lines.iter().zip(&clean_lines) {
        let line_start = position;
        let line_end = position + raw_line.len();
        position = line_end;
        if line_end <= start || line_start >= end {
            continue;
        }
        if raw_line == clean_line {
            text.push_str(&view[line_start.max(start)..line_end.min(end)]);
        } else {
            // Map the page's raw range onto the sanitized line. An end inside
            // a secret moves to the secret's end (within this line, the
            // page's last), so the whole placeholder shows on this page.
            let (from, _) = clean_offset(raw_line, clean_line, start.saturating_sub(line_start));
            let to = if end >= line_end {
                clean_line.len()
            } else {
                match clean_offset(raw_line, clean_line, end - line_start) {
                    (_, Some((raw_end, clean_end))) => {
                        end = line_start + raw_end;
                        clean_end
                    }
                    (to, None) => to,
                }
            };
            text.push_str(&clean_line[from..to.max(from)]);
        }
    }
    let total = view.len();
    let first = raw.view_lines.0;
    let last = first + line_count(&view[start..end]).saturating_sub(1);
    Some(Page {
        text,
        pagination: Pagination {
            length: end - start,
            has_more: end < total,
            next_offset: (end < total).then_some(end),
            ..raw.pagination.clone()
        },
        view_lines: (first, last),
        out_of_range: false,
    })
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
mod long_redacted_line_tests {
    use super::clean_offset;

    #[test]
    fn offsets_track_unchanged_text_and_snap_into_placeholders() {
        let raw = "aaaa SECRETSECRET bbbb SECRET2 cccc";
        let clean = "aaaa [REDACTED-X] bbbb [REDACTED-Y] cccc";
        assert_eq!(clean_offset(raw, clean, 0), (0, None));
        assert_eq!(clean_offset(raw, clean, 4), (4, None));
        // A secret's first byte is a boundary, not inside it.
        assert_eq!(clean_offset(raw, clean, 5), (5, None));
        // Inside the first secret: the placeholder start, never secret
        // bytes, and where the secret and its placeholder end.
        let secret_end = raw.find(" bbbb").expect("end");
        let placeholder_end = clean.find(" bbbb").expect("end");
        assert_eq!(
            clean_offset(raw, clean, 8),
            (5, Some((secret_end, placeholder_end)))
        );
        // After it, raw and clean realign on " bbbb".
        let raw_b = raw.find("bbbb").expect("b");
        assert_eq!(&clean[clean_offset(raw, clean, raw_b).0..][..4], "bbbb");
        let raw_c = raw.find("cccc").expect("c");
        assert_eq!(&clean[clean_offset(raw, clean, raw_c).0..], "cccc");
        assert_eq!(clean_offset(raw, clean, raw.len()), (clean.len(), None));
    }

    /// Redacts every `SECRET…` run of word bytes to one placeholder.
    struct Scan;
    impl crate::security::scan::ContentScan for Scan {
        fn sanitize(
            &self,
            text: &str,
            _: &std::path::Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            let pattern = regex::Regex::new(r"SECRET\w*").expect("pattern");
            Ok((
                pattern.replace_all(text, "[REDACTED-X]").into_owned(),
                vec![],
            ))
        }
    }

    /// Byte pages through a line whose secret spans several pages: the page
    /// the secret starts on shows its whole placeholder and ends after the
    /// secret, so no page is left empty and the walk reaches every byte
    /// once; an offset inside the secret returns the placeholder.
    #[test]
    fn a_secret_across_byte_pages_shows_on_the_page_it_starts_on() {
        use super::*;
        let secret = format!("SECRET{}", "s".repeat(70));
        let view = format!("{}{secret} {}\n", "a".repeat(25), "b".repeat(25));
        let secret_at = view.find("SECRET").expect("secret");
        let path = std::path::Path::new("v.txt");
        let query = |offset: usize| -> LocalFetchQuery {
            serde_json::from_value(serde_json::json!({
                "path": "v.txt", "unit": "bytes", "offset": offset, "length": 20
            }))
            .expect("query")
        };
        let mut offset = 0;
        let mut pages = Vec::new();
        loop {
            let raw = page_within(
                &view,
                &query(offset),
                LineBudget::bytes(LINE_PAGE_BYTES),
                None,
            );
            let (page, _) = sanitize_byte_page(&view, raw, path, &Scan)
                .expect("scan")
                .expect("page");
            assert_eq!(page.pagination.offset, offset);
            assert!(!page.text.is_empty(), "{offset}: an empty page");
            assert!(!page.text.contains("SECRET"), "{offset}: {}", page.text);
            pages.push((offset, page.text.clone()));
            match page.pagination.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        // The secret starts on the second page, which ends after it.
        assert_eq!(pages[1].0, 20);
        assert_eq!(
            pages[1].1,
            format!("{}[REDACTED-X]", "a".repeat(secret_at - 20))
        );
        assert_eq!(pages[2].0, secret_at + secret.len());
        assert_eq!(
            pages
                .iter()
                .map(|(_, text)| text.as_str())
                .collect::<String>(),
            Scan.sanitize(&view, path).expect("scan").0
        );
        // An offset inside the secret starts at its placeholder.
        let raw = page_within(
            &view,
            &query(secret_at + 30),
            LineBudget::bytes(LINE_PAGE_BYTES),
            None,
        );
        let (inside, _) = sanitize_byte_page(&view, raw, path, &Scan)
            .expect("scan")
            .expect("page");
        assert_eq!(inside.text, "[REDACTED-X]");
        assert_eq!(
            inside.pagination.next_offset,
            Some(secret_at + secret.len())
        );
    }
}

#[cfg(test)]
mod adaptive_line_tests {
    use super::*;

    #[test]
    fn default_line_pages_fill_the_byte_window_and_preserve_explicit_counts() {
        let source = "x\n".repeat(12_000);
        let mut query: LocalFetchQuery = serde_json::from_value(serde_json::json!({
            "path":"short.txt", "unit":"lines", "fullContent":true
        }))
        .expect("query");
        let first = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
        assert_eq!(first.pagination.length, LINE_PAGE_BYTES / 2);
        assert_eq!(first.text, source[..LINE_PAGE_BYTES]);
        query.offset = first.pagination.next_offset.map(wire_count);
        let last = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
        assert_eq!(first.text + &last.text, source);
        assert!(!last.pagination.has_more);
        query.offset = None;
        query.length = wire_positive(7);
        let explicit = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
        assert_eq!(explicit.pagination.length, 7);
        assert_eq!(explicit.pagination.next_offset, Some(7));
    }

    /// A line longer than the configured line page but shorter than the
    /// default one pages by bytes only through itself: line paging resumes
    /// right after it, and the walk reaches every byte exactly once.
    #[test]
    fn a_line_over_the_configured_page_resumes_line_paging_after_it() {
        let long = format!("{}\n", "y".repeat(3_000));
        let content = format!("{}{long}{}", "short\n".repeat(10), "tail\n".repeat(300));
        let budget = 1_000;
        let mut query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":"long.txt","unit":"lines"}))
                .expect("query");
        let mut walked = String::new();
        let mut units = Vec::new();
        for _ in 0..20 {
            let page = page_within(&content, &query, LineBudget::bytes(budget), None);
            if page.pagination.unit == WindowUnit::Bytes {
                assert!(
                    long.contains(&page.text),
                    "a byte page stays inside the long line"
                );
            }
            units.push(page.pagination.unit);
            walked.push_str(&page.text);
            let Some(next) = continuation(&query, &page.pagination) else {
                break;
            };
            query = next.continue_window.expect("next").query;
        }
        assert_eq!(walked, content);
        assert_eq!(units.last(), Some(&WindowUnit::Lines), "{units:?}");
    }

    #[test]
    fn default_raw_line_windows_keep_source_coordinates_after_transformations() {
        let source = SourceLines {
            lines: (1..=5_000).filter(|line| line % 2 == 0).collect(),
            total: 5_000,
        };
        let content = "x\n".repeat(source.lines.len());
        let query: LocalFetchQuery = serde_json::from_value(serde_json::json!({
            "path":"compact.txt", "unit":"lines"
        }))
        .expect("query");
        let result = page_within(
            &content,
            &query,
            LineBudget::bytes(LINE_PAGE_BYTES),
            Some(&source),
        );
        assert_eq!(result.text, content);
        assert_eq!(result.pagination.length, source.total);
        assert!(!result.pagination.has_more);
    }
}

#[cfg(test)]
mod line_page_scan_tests {
    use super::*;
    use crate::security::scan::ContentScan;

    /// Redacts `SECRET` and counts the bytes it scanned.
    struct Scan(std::cell::Cell<usize>);
    impl ContentScan for Scan {
        fn sanitize(
            &self,
            text: &str,
            _: &std::path::Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            self.0.set(self.0.get() + text.len());
            Ok((text.replace("SECRET", "[R]"), vec![]))
        }
    }

    /// The scan of a line page before it was made proportional: the whole
    /// view split into lines, the page's lines plus margins scanned.
    fn reference(view: &str, raw: &Page) -> Option<String> {
        let lines = view.split_inclusive('\n').collect::<Vec<_>>();
        let (first, last) = (raw.view_lines.0.saturating_sub(1), raw.view_lines.1);
        if raw.out_of_range || first >= last || last > lines.len() {
            return Some(raw.text.clone());
        }
        let (mut start, mut margin) = (first, 0);
        while start > 0 && margin < PAGE_SCAN_MARGIN_BYTES {
            start -= 1;
            margin += lines[start].len();
        }
        let (mut end, mut margin) = (last, 0);
        while end < lines.len() && margin < PAGE_SCAN_MARGIN_BYTES {
            margin += lines[end].len();
            end += 1;
        }
        let clean = lines[start..end].concat().replace("SECRET", "[R]");
        let clean_lines = clean.split_inclusive('\n').collect::<Vec<_>>();
        (clean_lines.len() == end - start)
            .then(|| clean_lines[first - start..last - start].concat())
    }

    /// Every page of a view scans what the whole-view split scanned, and
    /// returns the same text, whatever the line lengths; a page deep in a
    /// large view scans only its page and margins.
    #[test]
    fn a_line_page_scan_matches_the_whole_view_split() {
        let mut view = String::new();
        for line in 0..3_000 {
            let text = match line % 7 {
                0 => "SECRET here".to_owned(),
                1 => String::new(),
                _ => "z".repeat(line % 300),
            };
            view.push_str(&text);
            view.push('\n');
        }
        view.push_str("tail SECRET without newline");
        let path = std::path::Path::new("v.txt");
        let mut query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path": "v.txt"})).expect("query");
        loop {
            let raw = page_within(&view, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
            let scan = Scan(std::cell::Cell::new(0));
            let again = page_within(&view, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
            let page = sanitize_line_page(&view, again, path, &scan).expect("scan");
            assert_eq!(
                page.map(|(page, _)| page.text),
                reference(&view, &raw),
                "{:?}",
                raw.view_lines
            );
            assert!(scan.0.get() <= raw.text.len() + 2 * (PAGE_SCAN_MARGIN_BYTES + 400));
            match raw.pagination.next_offset {
                Some(next) => query.offset = Some(wire_count(next)),
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod oversized_line_end_tests {
    use super::oversized_line_end;

    /// The whole-line scan: the line holding `offset` measured from its real
    /// start to its real end, whatever the page's end.
    fn reference(
        content: &str,
        line: usize,
        offset: usize,
        end: usize,
        line_page_bytes: usize,
    ) -> Option<(usize, usize)> {
        let bytes = content.as_bytes();
        let line_start = bytes[..offset]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |at| at + 1);
        let line_end = bytes[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |at| offset + at + 1);
        (line_end - line_start > line_page_bytes && end >= line_end && line_end < bytes.len())
            .then_some((line_end, line))
    }

    /// Lines of every length around the line page, read at every offset by
    /// pages of several sizes, give the answer of the whole-line scan.
    #[test]
    fn a_bounded_scan_answers_as_the_whole_line_scan() {
        let page = 8;
        let content: String = [0, 1, 7, 8, 9, 10, 20, 3, 8, 9, 0]
            .iter()
            .map(|len| format!("{}\n", "x".repeat(*len)))
            .collect::<String>()
            + "tail without newline";
        let mut oversized = 0;
        for offset in 0..=content.len() {
            let line = 1 + content.as_bytes()[..offset]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count();
            for length in [1, 4, 8, 9, 16, 30, content.len()] {
                let end = (offset + length).min(content.len());
                let expected = reference(&content, line, offset, end, page);
                oversized += usize::from(expected.is_some());
                assert_eq!(
                    oversized_line_end(&content, line, offset, end, page),
                    expected,
                    "offset {offset} end {end}"
                );
            }
        }
        assert!(oversized > 20, "the corpus reaches oversized lines");
    }
}
