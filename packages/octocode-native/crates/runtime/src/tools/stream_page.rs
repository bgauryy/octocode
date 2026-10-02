//! Streamed page sizing shared by the local listing tools. A default-layout
//! page is cut so the whole response row fits the configured response window
//! (`output.pagination.defaultCharLength`); otherwise the response pager
//! splits the page into row parts and every page walk costs extra calls.
//! Sizes are serialized JSON chars in UTF-16 units, the unit the response
//! pager measures, so escaping and non-ASCII text count as they render.

/// Upper bound on the rows one streamed page carries, whatever the window:
/// a larger window means fewer hops, not ever-larger pages.
pub const MAX_PAGE_CHARS: usize = 24_000;

/// Room for everything a response row holds besides its listed entries:
/// the envelope, stats, pagination, warnings, and the fixed text of row
/// continuations and handoffs.
const ROW_SKELETON_CHARS: usize = 2_000;

/// Room for the caller's `goal` and `reasoning`, which row continuations
/// copy. They are budgeted at a fixed size rather than measured, so a walk
/// whose continuation rewords them still cuts every page at the same rows.
const FREE_TEXT_CHARS: usize = 1_200;

/// Query fields that never change which rows a page holds.
const UNSIZED_QUERY_FIELDS: [&str; 5] = ["goal", "reasoning", "page", "matchPage", "snapshot"];

/// Counts the UTF-16 units of UTF-8 written to it.
struct Utf16Counter(usize);

impl std::io::Write for Utf16Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &byte in bytes {
            // One unit per scalar value (each non-continuation byte); a
            // four-byte sequence is a surrogate pair, two units.
            if byte & 0xC0 != 0x80 {
                self.0 += 1;
            }
            if byte >= 0xF0 {
                self.0 += 1;
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Serialized JSON length of `value` in UTF-16 units, without building the
/// string.
pub fn json_chars(value: &impl serde::Serialize) -> usize {
    let mut counter = Utf16Counter(0);
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => counter.0,
        Err(_) => 0,
    }
}

/// Serialized length of `text` as a JSON string body: escaped, unquoted.
pub fn json_text_chars(text: &str) -> usize {
    json_chars(&text).saturating_sub(2)
}

/// Chars a streamed page's entries may take. With a response window, the
/// page leaves room for the row around it ([`reserve_chars`]); a tiny window
/// still gets half of itself, so a walk keeps moving (its pages then split
/// into row parts). Without one, pages take [`MAX_PAGE_CHARS`].
pub fn page_chars(window: Option<usize>, reserve: usize) -> usize {
    match window {
        Some(window) => window
            .saturating_sub(reserve)
            .max(window / 2)
            .clamp(1, MAX_PAGE_CHARS),
        None => MAX_PAGE_CHARS,
    }
}

/// What a response row needs besides its entries, for a row whose
/// continuations copy `query` `copies` times. Only fields that select rows
/// are measured, so every page of one walk gets the same budget.
pub fn reserve_chars(query: &serde_json::Value, copies: usize) -> usize {
    let measured = match query.as_object() {
        Some(fields) => fields
            .iter()
            .filter(|(name, _)| !UNSIZED_QUERY_FIELDS.contains(&name.as_str()))
            .map(|(name, value)| json_text_chars(name) + json_chars(value) + 4)
            .sum::<usize>(),
        None => json_chars(query),
    };
    ROW_SKELETON_CHARS + FREE_TEXT_CHARS + copies.saturating_mul(measured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_chars_count_escapes_and_utf16_units_as_rendered() {
        for value in [
            json!("plain"),
            json!("quote \" backslash \\ tab \t newline \n bell \u{7}"),
            json!("é 中文 \u{1F600}"),
            json!({"line": 12, "value": "a\"b", "nested": [1, null, true]}),
        ] {
            let rendered = value.to_string().encode_utf16().count();
            assert_eq!(json_chars(&value), rendered, "{value}");
        }
        assert_eq!(json_text_chars("a\"b\u{1F600}"), 6);
    }

    #[test]
    fn page_chars_leave_the_row_room_inside_the_window() {
        assert_eq!(page_chars(None, 5_000), MAX_PAGE_CHARS);
        assert_eq!(page_chars(Some(50_000), 4_000), MAX_PAGE_CHARS);
        assert_eq!(page_chars(Some(20_000), 4_000), 16_000);
        assert_eq!(page_chars(Some(1_000), 4_000), 500);
    }

    #[test]
    fn reserve_ignores_free_text_and_cursor_fields() {
        let first = json!({"goal": "g", "reasoning": "r", "path": "src", "searchText": "x"});
        let later = json!({"goal": "a much longer goal", "reasoning": "reworded",
            "path": "src", "searchText": "x", "page": 7, "snapshot": "lexical-live-v1:abc"});
        assert_eq!(reserve_chars(&first, 2), reserve_chars(&later, 2));
        let wider = json!({"path": "src", "searchText": "a longer search"});
        assert!(reserve_chars(&wider, 2) > reserve_chars(&first, 2));
    }
}
