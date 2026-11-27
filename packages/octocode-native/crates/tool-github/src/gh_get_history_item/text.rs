//! Text windows of history items (bodies, comments, reviews: always GitHub's
//! verbatim text), and the `matchString` needle every content filter matches.
use super::HistoryItemRequest;
use super::remaining::BodyWindows;
use super::util::string;
use octocode_engine::security::{CONTENT_REDACTION_PREFIX, REDACTION_PREFIX};
use octocode_runtime_base::response::cut::clean_cut;
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
/// than one marker may show no text; its `nextOffset` still advances. A
/// window whose edge a marker moved shows text other than its nominal
/// `length` (the whole marker from before `offset`, or text cut short of
/// the marker that the next window shows), so it states `oversized:true`;
/// a CRLF pair moves an edge by its `\r` alone and states nothing.
pub(super) fn paginate_text(
    value: &str,
    offset: Option<usize>,
    length: Option<usize>,
) -> (String, Value) {
    let total = value.chars().count();
    let start = offset.unwrap_or(0).min(total);
    let len = length.unwrap_or(super::DEFAULT_TEXT_WINDOW).max(1);
    let end = (start + len).min(total);
    // Each edge's byte, and whether a redaction marker moved it: the cut
    // then lands on the marker's start (a CRLF move lands on its CR).
    let edge = |chars: usize| match chars {
        0 => (0, false),
        chars if chars >= total => (value.len(), false),
        chars => {
            let nominal = byte_at(value, chars);
            let cut = clean_cut(value, nominal);
            let marker = value.split_at(cut).1;
            let moved = cut < nominal
                && (marker.starts_with(REDACTION_PREFIX)
                    || marker.starts_with(CONTENT_REDACTION_PREFIX));
            (cut, moved)
        }
    };
    // Both cuts are ordered character boundaries of the same sealed text.
    let ((start_byte, start_moved), (end_byte, end_moved)) = (edge(start), edge(end));
    let text = value
        .split_at(start_byte)
        .1
        .split_at(end_byte - start_byte)
        .0;
    let mut page = octocode_runtime_base::response::pages::SpanFacts::window(
        "chars",
        start,
        end - start,
        total,
    )
    .to_value();
    if start_moved || end_moved {
        page["oversized"] = Value::Bool(true);
    }
    (text.to_owned(), page)
}

/// Lower-cased `matchString`, the needle every content filter matches.
pub(super) fn needle(
    query: &HistoryItemRequest,
) -> Option<std::sync::Arc<super::matcher::Matcher>> {
    query.matcher.clone()
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
/// the row's fields in order, then after the body `fullBodyChars` when a
/// `matchString` narrowed it to its hit lines
/// ([`super::body_match::matched_view`]) and `bodyPagination` when the
/// window is cut or continued. Returns the row and its window.
pub(super) fn shape_comment(
    comment: &Value,
    offset: Option<usize>,
    query: &HistoryItemRequest,
    windows: &mut BodyWindows,
) -> (Value, Value) {
    let raw = string(comment.get("body"));
    let (view, full_chars) = super::body_match::matched_view(&raw, query);
    let (body, page) = window_body(&view, offset, query, windows);
    // A whole body restates no window.
    let windowed = page["offset"] != 0 || page["hasMore"] == true;
    let mut row = serde_json::Map::new();
    for (key, value) in comment.as_object().into_iter().flatten() {
        if key != "body" {
            row.insert(key.clone(), value.clone());
            continue;
        }
        row.insert("body".into(), Value::String(body.clone()));
        if let Some(full) = full_chars {
            row.insert("fullBodyChars".into(), Value::from(full));
        }
        if windowed {
            row.insert("bodyPagination".into(), page.clone());
        }
    }
    (Value::Object(row), page)
}

#[cfg(test)]
mod tests;
