//! Text windows of history items (bodies, comments, reviews: always GitHub's
//! verbatim text), and the `matchString` needle every content filter matches.
use super::HistoryItemRequest;
use super::remaining::BodyWindows;
use super::util::string;
use crate::response::cut::clean_cut;
use serde_json::Value;

/// The byte index of char `chars` in `value` (its end when past it).
fn byte_at(value: &str, chars: usize) -> usize {
    value
        .char_indices()
        .nth(chars)
        .map_or(value.len(), |(byte, _)| byte)
}

/// One char window of `value` (already redacted: offsets count that view) and
/// its page row. The row states the window's nominal edges: `length` chars
/// from `offset`, `nextOffset` where the next one starts. Each edge of the
/// text itself moves back off a redaction marker or CRLF it would split
/// ([`clean_cut`]). Every body of a page shares one `offset`, so each body's
/// windows tile it exactly: walking `nextOffset` rejoins `value` once, with
/// no gap and no repeat, whichever bodies share the page. A window narrower
/// than one marker may show no text; its `nextOffset` still advances.
pub(super) fn paginate_text(
    value: &str,
    offset: Option<usize>,
    length: Option<usize>,
) -> (String, Value) {
    let total = value.chars().count();
    let start = offset.unwrap_or(0).min(total);
    let len = length.unwrap_or(super::DEFAULT_TEXT_WINDOW).max(1);
    let end = (start + len).min(total);
    let edge = |chars: usize| match chars {
        0 => 0,
        chars if chars >= total => value.len(),
        chars => clean_cut(value, byte_at(value, chars)),
    };
    // Both cuts are ordered character boundaries of the same sealed text.
    let (start_byte, end_byte) = (edge(start), edge(end));
    let text = value
        .split_at(start_byte)
        .1
        .split_at(end_byte - start_byte)
        .0;
    (
        text.to_owned(),
        crate::response::pages::SpanFacts::window("chars", start, end - start, total).to_value(),
    )
}

/// Lower-cased `matchString`, the needle every content filter matches.
pub(super) fn needle(
    query: &HistoryItemRequest,
) -> Option<std::sync::Arc<super::matcher::Matcher>> {
    query.matcher.clone()
}
pub(super) fn body_matches(value: &Value, matcher: Option<&super::matcher::Matcher>) -> bool {
    matcher.is_none_or(|matcher| matcher.matches(&string(value.get("body"))))
}

/// A body window a continuation hop (`offset` > 0) finds empty: an earlier
/// window of the same page delivered the whole body (an empty one
/// included), so the hop does not list its item again.
pub(super) fn delivered_earlier(offset: Option<usize>, body_page: &Value) -> bool {
    offset.is_some_and(|offset| offset > 0)
        && body_page["length"].as_u64() == Some(0)
        && body_page["hasMore"] != true
}

/// Window one item body, verbatim (GitHub's own text and character
/// offsets: HTML comments, image links and emoji are evidence), noting its
/// window on the page's `windows` (one shared `offset` continues every cut
/// body).
pub(super) fn window_body(
    body: &str,
    offset: Option<usize>,
    _query: &HistoryItemRequest,
    windows: &mut BodyWindows,
) -> (String, Value) {
    let (text, page) = paginate_text(body, offset, None);
    windows.add(&page);
    (text, page)
}

/// One discussion or review comment (a mapped comment row) with its body
/// windowed at `offset`, for an issue page and a pull-request page alike:
/// the row's fields in order, `bodyPagination` right after the body when
/// the window is cut or continued. Returns the row and its window.
pub(super) fn shape_comment(
    comment: &Value,
    offset: Option<usize>,
    query: &HistoryItemRequest,
    windows: &mut BodyWindows,
) -> (Value, Value) {
    let (body, page) = window_body(&string(comment.get("body")), offset, query, windows);
    // A whole body restates no window.
    let windowed = page["offset"] != 0 || page["hasMore"] == true;
    let mut row = serde_json::Map::new();
    for (key, value) in comment.as_object().into_iter().flatten() {
        if key != "body" {
            row.insert(key.clone(), value.clone());
            continue;
        }
        row.insert("body".into(), Value::String(body.clone()));
        if windowed {
            row.insert("bodyPagination".into(), page.clone());
        }
    }
    (Value::Object(row), page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An issue comment and a pull-request discussion comment are one row
    /// shape: the mapped fields in order, the window after the body only
    /// when it is cut.
    #[test]
    fn a_comment_row_keeps_its_fields_and_windows_its_body() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"issue","owner":"o","repo":"r","number":1
        }))
        .expect("query");
        let comment = json!({"id":"7","author":"a","body":"x".repeat(super::super::DEFAULT_TEXT_WINDOW + 3),"createdAt":"t","path":"p"});
        let (row, page) = shape_comment(&comment, None, &query, &mut BodyWindows::default());
        assert_eq!(
            row.as_object()
                .map(|row| row.keys().cloned().collect::<Vec<_>>()),
            Some(
                [
                    "id",
                    "author",
                    "body",
                    "bodyPagination",
                    "createdAt",
                    "path"
                ]
                .map(String::from)
                .to_vec()
            )
        );
        assert_eq!(row["body"], "x".repeat(super::super::DEFAULT_TEXT_WINDOW));
        assert_eq!(page["hasMore"], true);
        let whole = json!({"id":"8","author":"a","body":"ab","createdAt":"t"});
        let (row, _) = shape_comment(&whole, None, &query, &mut BodyWindows::default());
        assert_eq!(row, whole);
    }

    /// Every window of `value` at `length` chars, walked by `nextOffset`.
    fn walk(value: &str, length: usize) -> Vec<String> {
        let mut windows = Vec::new();
        let mut offset = 0;
        loop {
            let (text, page) = paginate_text(value, Some(offset), Some(length));
            windows.push(text);
            if page["hasMore"] != true {
                return windows;
            }
            let next = usize::try_from(page["nextOffset"].as_u64().expect("next")).expect("fits");
            assert!(next > offset, "advances: {page}");
            offset = next;
        }
    }

    /// A window never ends inside a redaction marker, a CRLF or a char, at
    /// any window length, and the windows rejoin the text exactly once.
    #[test]
    fn windows_never_split_a_marker_or_crlf_and_rejoin_the_text() {
        let marker = "[REDACTED-GITHUBTOKENS]";
        let content = "[CONTENT-REDACTED-SANITIZER-FAILURE]";
        let text = format!("é🦀 a\r\nb {marker}{marker} c\r\n{content}\r\n🦀{marker}");
        for length in 1..=text.chars().count() + 1 {
            let windows = walk(&text, length);
            assert_eq!(windows.concat(), text, "length {length}");
            for window in &windows {
                assert!(!window.ends_with('\r'), "length {length}: {window:?}");
                // A marker is whole in a window, or absent from it.
                let opens = window.matches("[REDACTED").count()
                    + window.matches("[CONTENT-REDACTED").count();
                assert_eq!(
                    opens,
                    window.matches(']').count(),
                    "length {length}: {window:?}"
                );
            }
        }
    }

    /// A marker wider than the window is shown whole in one window, the
    /// windows before it empty; the offsets still advance.
    #[test]
    fn a_marker_wider_than_the_window_is_shown_whole() {
        let windows = walk("ab[REDACTED-GITHUBTOKENS]cd", 3);
        assert_eq!(windows.concat(), "ab[REDACTED-GITHUBTOKENS]cd");
        assert!(windows.contains(&"ab".to_owned()), "{windows:?}");
        assert!(
            windows.iter().any(|w| w == "[REDACTED-GITHUBTOKENS]cd"),
            "{windows:?}"
        );
    }

    /// Two bodies cut at one shared `offset` tile each exactly once even when
    /// a marker moves one body's edge.
    #[test]
    fn bodies_sharing_an_offset_each_tile_exactly_once() {
        let marked = format!("{}[REDACTED-GITHUBTOKENS] end", "a".repeat(8));
        let plain = "b".repeat(30);
        for length in 1..=12 {
            for body in [marked.as_str(), plain.as_str()] {
                assert_eq!(walk(body, length).concat(), body, "length {length}");
            }
        }
        let (first, page) = paginate_text(&marked, None, Some(12));
        let (second, _) = paginate_text(&plain, None, Some(12));
        assert_eq!(first, "aaaaaaaa");
        assert_eq!(second, "b".repeat(12));
        assert_eq!(page["nextOffset"], 12, "{page}");
    }

    #[test]
    fn text_windows_are_unicode_safe() {
        let (value, page) = paginate_text("a🦀b", Some(1), Some(1));
        assert_eq!(value, "🦀");
        assert_eq!(page["nextOffset"], 2);
    }

    #[test]
    fn default_text_window_uses_the_core_policy_and_advances() {
        let body = "x".repeat(super::super::DEFAULT_TEXT_WINDOW + 3);
        let (first, _page) = paginate_text(&body, None, None);
        assert_eq!(first.len(), super::super::DEFAULT_TEXT_WINDOW);
        let (last, page) = paginate_text(&body, Some(first.len()), None);
        assert_eq!(last, "xxx");
        assert_eq!(page["hasMore"], false);
    }

    /// Every pull-request text surface is the raw GitHub text by default:
    /// an HTML comment, an image attachment, a warning line and emoji are
    /// evidence a view must not drop (react#30774's review discussion).
    #[test]
    fn pull_request_text_is_verbatim_by_default() {
        let raw = "Fixes #1\n<!-- danger: header -->\n![image](https://github.com/user-attachments/assets/05746ddc)\n\u{1f6a8} WARNING \u{1f6a8}<br/>\n[![ci](https://x/badge.svg)](https://x)\nBody";
        let pr = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1
        }))
        .expect("pr query");
        let mut windows = BodyWindows::default();
        let (text, page) = window_body(raw, None, &pr, &mut windows);
        assert_eq!(text, raw);
        assert_eq!(page["total"], raw.chars().count());
    }

    /// (microsoft/TypeScript#57465: seven 26–37k-char comments on one
    /// page): one shared `offset` continues every cut body of a page, so
    /// the page's body entry is the first cut window.
    #[test]
    fn the_first_cut_body_is_the_pages_entry() {
        let pr = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1
        }))
        .expect("pr query");
        let mut windows = BodyWindows::default();
        for body in ["a".repeat(12_015), "b".repeat(5), "c".repeat(12_030)] {
            window_body(&body, None, &pr, &mut windows);
        }
        let mut pagination = serde_json::Map::new();
        windows.attach(super::super::axis::Axis::CommentBody, &mut pagination);
        let page = &pagination["commentBody"];
        assert_eq!(page["nextOffset"], 12_000, "{page}");
        assert_eq!(page["total"], 12_015, "{page}");
    }
}
