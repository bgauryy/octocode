//! Text windows of history items (bodies, comments, reviews: always GitHub's
//! verbatim text), and the `matchString` needle every content filter matches.
use super::HistoryItemRequest;
use super::remaining::BodyWindows;
use super::util::string;
use serde_json::Value;

/// One char window of `value` and its page row.
pub(super) fn paginate_text(
    value: &str,
    offset: Option<usize>,
    length: Option<usize>,
) -> (String, Value) {
    let total = value.chars().count();
    let start = offset.unwrap_or(0).min(total);
    let len = length.unwrap_or(super::DEFAULT_TEXT_WINDOW).max(1);
    let end = (start + len).min(total);
    let text = value.chars().skip(start).take(end - start).collect();
    (
        text,
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
    /// the page's body entry is the first cut window and its rest counts
    /// the chars left in all of them, not only in the first cut one.
    #[test]
    fn cut_bodies_of_one_page_count_every_remaining_char() {
        let pr = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","owner":"o","repo":"r","number":1
        }))
        .expect("pr query");
        let mut windows = BodyWindows::default();
        for body in ["a".repeat(12_015), "b".repeat(5), "c".repeat(12_030)] {
            window_body(&body, None, &pr, &mut windows);
        }
        let mut remainder = super::super::remaining::Remainder::default();
        let mut pagination = serde_json::Map::new();
        remainder.bodies(
            super::super::axis::Axis::CommentBody,
            windows,
            &mut pagination,
        );
        let page = &pagination["commentBody"];
        assert_eq!(page["nextOffset"], 12_000, "{page}");
        assert_eq!(page["total"], 12_015, "{page}");
        assert_eq!(
            remainder
                .note(super::super::axis::Axis::CommentBody, page)
                .map(|note| note.left),
            Some(format!("{} more comment body chars", 15 + 30))
        );
    }
}
