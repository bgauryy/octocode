//! Secret-scanning one page (plus margins) instead of the whole selected
//! view, keeping offsets in raw-view coordinates.
use super::{PAGE_SCAN_MARGIN_BYTES, Page, advance_lines};
use crate::security::scan::ContentScan;
use crate::tools::source_view::Pagination;
use crate::tools::source_view::extraction::line_count;
use octocode_contract_types::ErrorCode;
use octocode_engine::security::REDACTION_PREFIX;

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
) -> Result<Option<(Page, bool)>, (ErrorCode, String)> {
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
) -> Result<Option<(Page, bool)>, (ErrorCode, String)> {
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
            let anchor_end = clean.floor_char_boundary(anchor_end);
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
    (clean.floor_char_boundary(j), None)
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

#[cfg(test)]
mod tests;
