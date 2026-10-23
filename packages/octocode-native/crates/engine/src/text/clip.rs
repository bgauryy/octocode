//! The one rule for clipping a long line around a hit, shared by localSearch
//! and ghSearchCode: a clipped line starts with `…` when the window opens
//! after the line's start, ends with `...` when the line goes on, and never
//! exceeds the char budget.

/// Chars kept after the match start when a snippet must be clipped.
pub(crate) const MATCH_TAIL_CHARS: usize = 20;

/// Whether a match at char `match_char` gets a window cut around it rather
/// than the line's head.
fn clips_to_window(match_char: usize, max_chars: usize) -> bool {
    max_chars > 3 && match_char + MATCH_TAIL_CHARS.min(max_chars / 2) >= max_chars
}

/// Clip `line` to `max_chars`, keeping the match at char index `match_char`
/// visible. A line whose match fits the head is truncated from the start;
/// otherwise the window starts a quarter-budget before the match and is
/// marked with `…`. A window near the line's end starts earlier instead, so
/// it still spends `max_chars` (the tail it ends on is the line's own,
/// unmarked). A limit too short for a marker shows the line's own chars from
/// the match.
pub fn clip_around(line: &str, match_char: usize, max_chars: usize) -> String {
    let line_chars = line.chars().count();
    if line_chars <= max_chars {
        return line.to_owned();
    }
    if max_chars <= 3 {
        let start = match_char.min(line_chars - max_chars);
        return line.chars().skip(start).take(max_chars).collect();
    }
    if !clips_to_window(match_char, max_chars) {
        return truncate_unicode(line, max_chars);
    }
    // `…` takes one char; the rest of the window is the line from its start.
    let start_char = match_char
        .saturating_sub(max_chars / 4)
        .min(line_chars - (max_chars - 1));
    let start_byte = line
        .char_indices()
        .nth(start_char)
        .map_or(line.len(), |(byte, _)| byte);
    format!("…{}", truncate_unicode(&line[start_byte..], max_chars - 1))
}

/// Truncates a string to at most `max_chars` Unicode scalar values, appending
/// `...` when truncated; a limit too short for the marker keeps the leading
/// chars themselves.
pub(crate) fn truncate_unicode(s: &str, max_chars: usize) -> String {
    if max_chars <= 3 {
        return s.chars().take(max_chars).collect();
    }

    let head_chars = max_chars - 3;
    let mut head_byte_end = 0;
    for (char_idx, (byte_idx, _ch)) in s.char_indices().enumerate() {
        if char_idx == head_chars {
            head_byte_end = byte_idx;
        }
        if char_idx == max_chars {
            return format!("{}...", &s[..head_byte_end]);
        }
    }
    s.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_unicode_counts_chars_not_bytes() {
        // "café" is 4 chars but 5 bytes (é = 2 bytes)
        let r = truncate_unicode("café world", 4);
        // limit 4 → head at max_chars-3=1 chars + "..."
        assert_eq!(r, "c...");
    }

    #[test]
    fn truncate_unicode_zero_limit_returns_empty() {
        assert_eq!(truncate_unicode("hello", 0), "");
    }

    /// A limit too short for the `...` marker returns real chars, never
    /// only dots.
    #[test]
    fn truncate_unicode_tiny_limits_return_real_chars() {
        assert_eq!(truncate_unicode("hello", 1), "h");
        assert_eq!(truncate_unicode("hello", 2), "he");
        assert_eq!(truncate_unicode("hello", 3), "hel");
        assert_eq!(truncate_unicode("héllo", 2), "hé");
    }

    #[test]
    fn truncate_unicode_tiny_limits_preserve_short_input() {
        assert_eq!(truncate_unicode("é", 1), "é");
        assert_eq!(truncate_unicode("é", 2), "é");
    }

    /// A tiny limit shows the match's own chars, ending at the line's end.
    #[test]
    fn a_tiny_clip_shows_the_match() {
        assert_eq!(clip_around("xxxxx needle", 6, 3), "nee");
        assert_eq!(clip_around("xxxxx needle", 11, 3), "dle");
        assert_eq!(clip_around("needle xxxxx", 0, 2), "ne");
    }

    #[test]
    fn a_line_that_fits_is_kept_whole() {
        assert_eq!(clip_around("short", 4, 5), "short");
    }

    /// One marker convention: a hit in the head keeps the head and ends in
    /// `...`; a later hit opens the window with `…` and ends in `...`.
    #[test]
    fn markers_are_the_same_wherever_the_hit_is() {
        let line = format!("needle{}needle{}", "a".repeat(300), "b".repeat(300));
        let head = clip_around(&line, 0, 40);
        assert!(
            head.starts_with("needle") && head.ends_with("..."),
            "{head}"
        );
        let mid = clip_around(&line, 306, 40);
        assert!(mid.starts_with('…') && mid.contains("needle"), "{mid}");
        assert!(mid.ends_with("..."), "{mid}");
        for clipped in [head, mid] {
            assert_eq!(clipped.chars().count(), 40, "{clipped}");
        }
    }

    /// A hit near the end of a long line gets a window that spends the whole
    /// budget: the window shifts left, ends on the line's own last char, and
    /// keeps the match visible.
    #[test]
    fn a_window_near_the_line_end_fills_the_budget() {
        let line = format!("{}cursor:pointer}}", "a".repeat(230));
        let at = line.find("cursor").expect("hit");
        for max in [40, 200] {
            let value = clip_around(&line, at, max);
            assert_eq!(value.chars().count(), max, "{value}");
            assert!(
                value.starts_with('…') && value.ends_with("cursor:pointer}"),
                "{value}"
            );
        }
        // A hit mid-line keeps its quarter-budget lead.
        let mid = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
        let value = clip_around(&mid, 300, 40);
        assert!(
            value.starts_with(&format!("…{}needle", "a".repeat(10))),
            "{value}"
        );
        assert!(value.ends_with("..."), "{value}");
        assert_eq!(value.chars().count(), 40, "{value}");
    }
}
