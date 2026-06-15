/// Zero-allocation UTF-8 offset helpers and content slicer.
///
/// Replaces `utils/file/byteOffset.ts` in octocode-tools-core, which used
/// `Buffer.from(content, 'utf8')` — a full copy of the content — for every
/// char↔byte conversion, and called it 4–6 times per `applyPagination` invocation.
///
/// All functions here walk the UTF-8 byte sequence in-place via `str::char_indices()`
/// with no heap allocation proportional to content length.
use crate::types::{SliceContentOptions, SliceContentResult};

// ── core offset helpers ───────────────────────────────────────────────────────

/// Number of UTF-8 bytes up to (not including) the `char_index`-th JavaScript
/// UTF-16 code unit in `s`. Clamps to `s.len()` if `char_index` exceeds the string.
pub(crate) fn char_to_byte_offset_inner(s: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    let mut utf16_units = 0usize;
    for (byte_idx, ch) in s.char_indices() {
        if utf16_units >= char_index || utf16_units + ch.len_utf16() > char_index {
            return byte_idx;
        }
        utf16_units += ch.len_utf16();
    }
    s.len() // char_index beyond string length - clamp
}

/// JavaScript UTF-16 code-unit offset corresponding to `byte_offset` bytes into `s`.
/// Clamps to the JS string length if `byte_offset` exceeds `s.len()`.
pub(crate) fn byte_to_char_offset_inner(s: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(s.len());
    // Safe: we snap to the nearest valid boundary
    let valid_offset = floor_char_boundary(s, clamped);
    utf16_len(&s[..valid_offset])
}

/// Extract a byte-range substring from `s`. Returns `""` for an out-of-range or
/// invalid range.
pub(crate) fn byte_slice_content_inner(s: &str, byte_start: usize, byte_end: usize) -> String {
    if byte_start >= byte_end || byte_start >= s.len() {
        return String::new();
    }
    let start = floor_char_boundary(s, byte_start.min(s.len()));
    let end = floor_char_boundary(s, byte_end.min(s.len()));
    if start > end {
        return String::new();
    }
    s[start..end].to_owned()
}

/// Snap `byte_pos` down to the nearest valid UTF-8 character boundary in `s`.
fn floor_char_boundary(s: &str, mut byte_pos: usize) -> usize {
    if byte_pos >= s.len() {
        return s.len();
    }
    // Walk back until we land on a UTF-8 leading byte
    while byte_pos > 0 && !s.is_char_boundary(byte_pos) {
        byte_pos -= 1;
    }
    byte_pos
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

// ── combined slicer ───────────────────────────────────────────────────────────

/// Paginate `content` starting at `char_offset` for up to `char_length` chars.
///
/// When `snap_to_line_boundary` is true the slice always starts at the
/// beginning of the containing line and ends at the end of the last complete
/// line within the window — equivalent to the TypeScript `sliceByCharRespectLines`
/// (dead code, 0 callers confirmed by LSP) merged with the char-mode path of
/// `applyPagination`.
pub(crate) fn slice_content_inner(
    content: &str,
    char_offset: usize,
    char_length: usize,
    options: Option<SliceContentOptions>,
) -> SliceContentResult {
    let snap = options
        .as_ref()
        .and_then(|o| o.snap_to_line_boundary)
        .unwrap_or(false);

    let total_chars = utf16_len(content);

    if total_chars == 0 || char_length == 0 {
        return SliceContentResult {
            text: String::new(),
            char_offset: 0,
            char_length: 0,
            byte_offset: 0,
            byte_length: 0,
            has_more: false,
            next_char_offset: None,
        };
    }

    let start_char = char_offset.min(total_chars);
    let raw_end_char = (start_char + char_length).min(total_chars);

    let (actual_start, actual_end) = if snap {
        snap_to_lines(content, start_char, raw_end_char)
    } else {
        (start_char, raw_end_char)
    };

    let start_byte = char_to_byte_offset_inner(content, actual_start);
    let end_byte = char_to_byte_offset_inner(content, actual_end);
    let text = content[start_byte..end_byte].to_owned();
    let actual_char_length = actual_end - actual_start;
    let has_more = actual_end < total_chars;

    SliceContentResult {
        text,
        char_offset: actual_start as u32,
        char_length: actual_char_length as u32,
        byte_offset: start_byte as u32,
        byte_length: (end_byte - start_byte) as u32,
        has_more,
        next_char_offset: if has_more {
            Some(actual_end as u32)
        } else {
            None
        },
    }
}

/// Snap `(start_char, end_char)` to line boundaries: push start back to line
/// start, extend end to line end (or next line start).
fn snap_to_lines(content: &str, start_char: usize, end_char: usize) -> (usize, usize) {
    // Build newline positions as JavaScript UTF-16 offsets (0-indexed line starts)
    let mut line_starts: Vec<usize> = vec![0];
    let mut char_idx = 0usize;
    for ch in content.chars() {
        if ch == '\n' {
            line_starts.push(char_idx + 1);
        }
        char_idx += ch.len_utf16();
    }

    // Find the line that contains start_char
    let actual_start = line_starts
        .iter()
        .rev()
        .find(|&&ls| ls <= start_char)
        .copied()
        .unwrap_or(0);

    // Find the line boundary at or after end_char
    let total_chars = utf16_len(content);
    let actual_end = line_starts
        .iter()
        .find(|&&ls| ls > end_char)
        .copied()
        .unwrap_or(total_chars);

    (actual_start, actual_end)
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── char_to_byte_offset_inner ─────────────────────────────────────────────

    #[test]
    fn char_to_byte_ascii_identity() {
        assert_eq!(char_to_byte_offset_inner("hello", 3), 3);
        assert_eq!(char_to_byte_offset_inner("hello", 0), 0);
        assert_eq!(char_to_byte_offset_inner("hello", 5), 5);
    }

    #[test]
    fn char_to_byte_multibyte() {
        // "café" → c(1) a(1) f(1) é(2) = 5 bytes for 4 chars
        let s = "café";
        assert_eq!(char_to_byte_offset_inner(s, 0), 0);
        assert_eq!(char_to_byte_offset_inner(s, 3), 3); // up to 'é'
        assert_eq!(char_to_byte_offset_inner(s, 4), 5); // after 'é'
    }

    #[test]
    fn char_to_byte_uses_javascript_utf16_indices() {
        let s = "a🌍b";
        assert_eq!(char_to_byte_offset_inner(s, 0), 0);
        assert_eq!(char_to_byte_offset_inner(s, 1), 1);
        assert_eq!(char_to_byte_offset_inner(s, 2), 1); // inside surrogate pair snaps down
        assert_eq!(char_to_byte_offset_inner(s, 3), 5); // after emoji
        assert_eq!(char_to_byte_offset_inner(s, 4), 6);
    }

    #[test]
    fn char_to_byte_clamps_beyond_length() {
        assert_eq!(char_to_byte_offset_inner("hi", 100), 2);
    }

    // ── byte_to_char_offset_inner ─────────────────────────────────────────────

    #[test]
    fn byte_to_char_ascii_identity() {
        assert_eq!(byte_to_char_offset_inner("hello", 3), 3);
        assert_eq!(byte_to_char_offset_inner("hello", 0), 0);
    }

    #[test]
    fn byte_to_char_multibyte() {
        let s = "café"; // c=0, a=1, f=2, é=3..4
        assert_eq!(byte_to_char_offset_inner(s, 0), 0);
        assert_eq!(byte_to_char_offset_inner(s, 3), 3); // at start of 'é'
        assert_eq!(byte_to_char_offset_inner(s, 5), 4); // after 'é'
    }

    #[test]
    fn byte_to_char_uses_javascript_utf16_indices() {
        let s = "a🌍b";
        assert_eq!(byte_to_char_offset_inner(s, 0), 0);
        assert_eq!(byte_to_char_offset_inner(s, 1), 1);
        assert_eq!(byte_to_char_offset_inner(s, 5), 3);
        assert_eq!(byte_to_char_offset_inner(s, 6), 4);
    }

    #[test]
    fn byte_to_char_clamps_beyond_length() {
        assert_eq!(byte_to_char_offset_inner("hi", 100), 2);
    }

    // ── byte_slice_content_inner ──────────────────────────────────────────────

    #[test]
    fn byte_slice_ascii() {
        assert_eq!(byte_slice_content_inner("hello world", 6, 11), "world");
    }

    #[test]
    fn byte_slice_multibyte() {
        let s = "café"; // bytes: 63 61 66 C3 A9
        assert_eq!(byte_slice_content_inner(s, 3, 5), "é");
    }

    #[test]
    fn byte_slice_empty_on_bad_range() {
        assert_eq!(byte_slice_content_inner("hello", 3, 2), "");
        assert_eq!(byte_slice_content_inner("hello", 10, 20), "");
    }

    // ── slice_content_inner ───────────────────────────────────────────────────

    #[test]
    fn slice_content_basic_window() {
        let content = "abcdefghij";
        let r = slice_content_inner(content, 3, 4, None);
        assert_eq!(r.text, "defg");
        assert_eq!(r.char_offset, 3);
        assert_eq!(r.char_length, 4);
        assert!(r.has_more);
    }

    #[test]
    fn slice_content_last_page_no_more() {
        let content = "abcde";
        let r = slice_content_inner(content, 3, 10, None);
        assert_eq!(r.text, "de");
        assert!(!r.has_more);
        assert!(r.next_char_offset.is_none());
    }

    #[test]
    fn slice_content_snap_to_line_start() {
        let content = "line1\nline2\nline3\n";
        // char 3 is inside "line1", should snap back to 0
        let r = slice_content_inner(
            content,
            3,
            8,
            Some(SliceContentOptions {
                snap_to_line_boundary: Some(true),
            }),
        );
        assert!(r.text.starts_with("line1"));
    }

    #[test]
    fn slice_content_snap_to_line_end() {
        let content = "line1\nline2\nline3\n";
        // start at 0 with 4 chars → raw end mid-"line1", snap extends to end of line1
        let r = slice_content_inner(
            content,
            0,
            4,
            Some(SliceContentOptions {
                snap_to_line_boundary: Some(true),
            }),
        );
        // "line1\n" = 6 chars, so snapped end should be at line2 start (char 6)
        assert_eq!(r.char_offset, 0);
        assert!(r.char_length >= 5); // at least "line1"
    }

    #[test]
    fn slice_content_empty_input() {
        let r = slice_content_inner("", 0, 100, None);
        assert_eq!(r.text, "");
        assert!(!r.has_more);
    }

    #[test]
    fn slice_content_multibyte_chars() {
        let content = "café world";
        let r = slice_content_inner(content, 0, 4, None);
        assert_eq!(r.text, "café");
        assert_eq!(r.char_length, 4);
        assert_eq!(r.byte_length, 5); // é = 2 bytes
    }

    #[test]
    fn slice_content_uses_javascript_utf16_indices() {
        let content = "a🌍b";
        let r = slice_content_inner(content, 0, 3, None);
        assert_eq!(r.text, "a🌍");
        assert_eq!(r.char_length, 3);
        assert_eq!(r.byte_length, 5);
        assert!(r.has_more);
        assert_eq!(r.next_char_offset, Some(3));
    }

    #[test]
    fn byte_offset_roundtrip() {
        let s = "hello 世界 world";
        let js_boundaries = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        for char_idx in js_boundaries {
            let byte_off = char_to_byte_offset_inner(s, char_idx);
            let char_back = byte_to_char_offset_inner(s, byte_off);
            assert_eq!(
                char_back, char_idx,
                "roundtrip failed at char_idx={char_idx}"
            );
        }
    }
}
