use super::extraction::line_count;
use super::types::*;
use crate::security::scan::ContentScan;
use crate::tools::id::ToolId;
fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}
fn records(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return vec![];
    }
    let mut o = vec![];
    let mut st = 0;
    for (i, c) in s.char_indices() {
        if c == '\n' {
            o.push(&s[st..=i]);
            st = i + 1
        }
    }
    if st < s.len() {
        o.push(&s[st..])
    }
    o
}
pub struct Page {
    pub text: String,
    pub pagination: Pagination,
    pub view_lines: (usize, usize),
    /// The requested offset is at or past the end of a non-empty view (or any
    /// positive offset into an empty one): the page is empty by construction,
    /// not because the view ended here.
    pub out_of_range: bool,
}
pub fn page(content: &str, q: &LocalFetchQuery) -> Result<Page, String> {
    let kind = q.chunk_type.unwrap_or(if q.full_content == Some(true) {
        ChunkType::Bytes
    } else {
        ChunkType::Lines
    });
    let offset = q.offset().unwrap_or(0);
    match kind {
        ChunkType::Bytes => {
            let out_of_range = offset > 0 && offset >= content.len();
            let offset = offset.min(content.len());
            if !content.is_char_boundary(offset) {
                return Err(
                    "byte offset must be a UTF-8 code-point boundary within the selected view"
                        .into(),
                );
            }
            let chunk_size = q.chunk_size().unwrap_or(if q.full_content == Some(true) {
                content.len()
            } else {
                16384
            });
            let mut end = (offset + chunk_size).min(content.len());
            while end < content.len() && !content.is_char_boundary(end) {
                end += 1
            }
            let text = content[offset..end].to_owned();
            let first = content[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
            let last = first + line_count(&text).saturating_sub(1);
            Ok(Page {
                text,
                pagination: Pagination {
                    chunk_type: kind,
                    offset,
                    length: end - offset,
                    chunk_size,
                    total_lines: line_count(content),
                    total_bytes: content.len(),
                    has_more: end < content.len(),
                    next_offset: (end < content.len()).then_some(end),
                },
                view_lines: (first, last),
                out_of_range,
            })
        }
        ChunkType::Lines => {
            let lines = records(content);
            let out_of_range = offset > 0 && offset >= lines.len();
            let offset = offset.min(lines.len());
            let selected = q.match_string.is_some()
                || q.has_ranges()
                || (q.start_line().is_some() && q.end_line().is_some());
            let requested_limit = q.chunk_size().unwrap_or(if selected {
                lines.len().clamp(1, 50_000)
            } else {
                DEFAULT_LINE_CHUNK
            });
            let mut end = (offset + requested_limit).min(lines.len());
            let mut bytes: usize = lines[offset..end].iter().map(|s| s.len()).sum();
            while bytes > 16384 && end > offset {
                end -= 1;
                bytes -= lines[end].len()
            }
            if end == offset && offset < lines.len() {
                let byte_offset: usize = lines[..offset].iter().map(|line| line.len()).sum();
                let mut byte_query = q.clone();
                byte_query.chunk_type = Some(ChunkType::Bytes);
                byte_query.offset = Some(wire_count(byte_offset));
                byte_query.chunk_size = wire_positive(16384);
                return page(content, &byte_query);
            }
            let text = lines[offset..end].concat();
            Ok(Page {
                text,
                pagination: Pagination {
                    chunk_type: kind,
                    offset,
                    length: end - offset,
                    chunk_size: q.chunk_size().unwrap_or(end.saturating_sub(offset)),
                    total_lines: lines.len(),
                    total_bytes: content.len(),
                    has_more: end < lines.len(),
                    next_offset: (end < lines.len()).then_some(end),
                },
                view_lines: (offset + 1, end),
                out_of_range,
            })
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
    let lines = records(view);
    let (first, last) = (raw.view_lines.0.saturating_sub(1), raw.view_lines.1);
    if raw.out_of_range || first >= last || last > lines.len() {
        return Ok(Some((raw, false)));
    }
    let mut start = first;
    let mut margin = 0;
    while start > 0 && margin < PAGE_SCAN_MARGIN_BYTES {
        start -= 1;
        margin += lines[start].len();
    }
    let mut end = last;
    margin = 0;
    while end < lines.len() && margin < PAGE_SCAN_MARGIN_BYTES {
        margin += lines[end].len();
        end += 1;
    }
    let window = lines[start..end].concat();
    let (clean, _) = security.sanitize(&window, source_path)?;
    if clean == window {
        return Ok(Some((raw, false)));
    }
    let clean_lines = records(&clean);
    if clean_lines.len() != end - start {
        return Ok(None);
    }
    let text = clean_lines[first - start..last - start].concat();
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
/// Offsets stay in raw-view coordinates. A redacted line cut by the page end
/// is emitted whole (clean) and the page extends to that line's end, so a
/// secret straddling the boundary is never split across two pages. Returns
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

/// A redacted line longer than this is sliced to the page instead of being
/// emitted whole.
const LONG_REDACTED_LINE_BYTES: usize = 64 * 1024;
const PLACEHOLDER_PREFIX: &str = "[REDACTED";
/// Unchanged text after a placeholder used to find where its span ends.
const ANCHOR_BYTES: usize = 32;

/// Offset in `clean` (a sanitized copy of `raw` whose secrets became
/// `[REDACTED…]` placeholders) that corresponds to byte `raw_at` of `raw`.
/// Text between placeholders is identical in both, so the walk advances in
/// lockstep there and re-synchronizes after each placeholder on the unchanged
/// text that follows it. A position inside a redacted span maps to that
/// span's placeholder, so a slice never contains secret bytes.
fn clean_offset(raw: &str, clean: &str, raw_at: usize) -> usize {
    let (mut i, mut j) = (0usize, 0usize);
    while i < raw_at && j < clean.len() {
        if clean[j..].starts_with(PLACEHOLDER_PREFIX) {
            let placeholder_end = clean[j..].find(']').map_or(clean.len(), |at| j + at + 1);
            let anchor_end = clean[placeholder_end..]
                .find(PLACEHOLDER_PREFIX)
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
                return j;
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
    j
}

/// Rebuild a raw-coordinate byte page from a sanitized window with the same
/// line count: unchanged lines keep their raw slice, redacted lines are
/// emitted whole from the clean window (extending the page end if needed).
fn map_clean_window(
    view: &str,
    from: usize,
    window: &str,
    clean: &str,
    raw: &Page,
) -> Option<Page> {
    let raw_lines = records(window);
    let clean_lines = records(clean);
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
        } else if raw_line.len() > LONG_REDACTED_LINE_BYTES {
            // A long redacted line (a minified bundle, an embedded base64
            // blob) is sliced, not emitted whole: map the page's raw range
            // onto the sanitized line so the page stays chunk-sized.
            let from = clean_offset(raw_line, clean_line, start.saturating_sub(line_start));
            let to = clean_offset(raw_line, clean_line, end.min(line_end) - line_start);
            text.push_str(&clean_line[from..to.max(from)]);
        } else {
            text.push_str(clean_line);
            end = end.max(line_end);
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
        query.offset = Some(wire_count(offset));
        query.chunk_type = Some(p.chunk_type);
        query.chunk_size = wire_positive(p.chunk_size);
        NextCalls {
            r#continue: Some(Continuation {
                tool: ToolId::LocalFetch.as_str().into(),
                query,
                confidence: "exact".into(),
                reason: None,
            }),
            read_bounded_lines: None,
            restart: None,
            ..NextCalls::default()
        }
    })
}
pub fn result_counts(s: &str) -> (usize, usize, usize) {
    (utf16(s), s.len(), line_count(s))
}

#[cfg(test)]
mod long_redacted_line_tests {
    use super::clean_offset;

    #[test]
    fn offsets_track_unchanged_text_and_snap_into_placeholders() {
        let raw = "aaaa SECRETSECRET bbbb SECRET2 cccc";
        let clean = "aaaa [REDACTED-X] bbbb [REDACTED-Y] cccc";
        assert_eq!(clean_offset(raw, clean, 0), 0);
        assert_eq!(clean_offset(raw, clean, 4), 4);
        // Inside the first secret: the placeholder start, never secret bytes.
        assert_eq!(clean_offset(raw, clean, 8), 5);
        // After it, raw and clean realign on " bbbb".
        let raw_b = raw.find("bbbb").expect("b");
        assert_eq!(&clean[clean_offset(raw, clean, raw_b)..][..4], "bbbb");
        let raw_c = raw.find("cccc").expect("c");
        assert_eq!(&clean[clean_offset(raw, clean, raw_c)..], "cccc");
        assert_eq!(clean_offset(raw, clean, raw.len()), clean.len());
    }
}
