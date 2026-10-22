//! JSON size measurement shared by the paging owners and the response
//! pager. Sizes are serialized JSON chars in UTF-16 units, the unit the
//! response pager measures, so escaping and non-ASCII text count as they
//! render.

/// Serialized length of `text` as a JSON string body: escaped, unquoted.
pub(crate) fn json_text_chars(text: &str) -> usize {
    octocode_engine::json_size::utf16_units(&text)
        .unwrap_or(0)
        .saturating_sub(2)
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
            assert_eq!(
                octocode_engine::json_size::utf16_units(&value).unwrap_or(0),
                rendered,
                "{value}"
            );
        }
        assert_eq!(json_text_chars("a\"b\u{1F600}"), 6);
    }
}
