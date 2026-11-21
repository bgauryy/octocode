use super::super::row_pages::RowSource;
use super::{
    PreparedPage, ResponseError, ResponseInput, ResponsePageOptions, ResponsePagination,
    build_continuation, is_saved_replay, page_cursor,
};
use octocode_io::cancellation::Cancellation;
use serde_json::{Map, Value, json};

/// One row page of `source`, with its pagination block and continuation.
/// A page holding a value longer than the budget alone states top-level
/// `oversized: true` ([`super::oversized`]).
pub(super) fn cut_rows(
    source: &RowSource,
    input: &ResponseInput,
    cancelled: &Cancellation,
) -> Result<PreparedPage, ResponseError> {
    let (mut envelope, mut pagination) = source.page(&input.options, cancelled)?;
    pagination.next = continuation(input, &pagination).or(pagination.next.take());
    if pagination.should_emit() || is_saved_replay(&input.options) {
        envelope.extend(block(pagination.next.as_ref(), &input.options));
    }
    if let Some(budget) = input.options.response_chars {
        super::oversized::flag(&mut envelope, input.tool, budget, true);
    }
    Ok(PreparedPage {
        value: Value::Object(envelope),
        next: page_cursor(&input.options, &pagination),
    })
}

/// Chars the reading-on members add to a page of `pagination`
/// ([`cut_rows`] attaches it with its continuation); 0
/// when the page carries none. Every page of one plan must get the same
/// answer, whichever request builds it, so a saved read is measured as the
/// replay it becomes (a first request and its copies differ only in
/// `replayed`'s value).
pub(super) fn pagination_block_chars(
    input: &ResponseInput,
    pagination: &ResponsePagination,
) -> usize {
    let mut options = input.options.clone();
    if let Some(saved) = options.saved.as_mut() {
        saved.replayed = true;
    }
    let mut page = pagination.clone();
    page.next = build_continuation(input.tool.as_str(), &options, &page).or(page.next);
    if !(page.should_emit() || is_saved_replay(&options)) {
        return 0;
    }
    block_chars(&block(page.next.as_ref(), &options))
}

/// Chars the reading-on members add to a continuing page of `tool`
/// as [`cut_rows`] attaches it: the executable continuation, and the expiry
/// and replay facts every saved response page states, at their widest
/// (`replayed:true`). A tool that cuts its own part of a page to the
/// response window reserves this, so the pager never cuts that part again.
pub fn continuing_block_chars(tool: crate::id::ToolId) -> usize {
    let options = ResponsePageOptions {
        saved: Some(super::super::page_model::SavedResponse {
            key: [0; 32],
            expires_at: crate::civil_date::iso8601_millis_after(std::time::Duration::ZERO),
            replayed: true,
        }),
        ..ResponsePageOptions::default()
    };
    let next = crate::response::result::Continuation::cursor(
        tool.as_str(),
        super::super::cursor::issued::placeholder(),
    );
    block_chars(&block(Some(&next), &options))
}

/// The members of `block` as the envelope holds them, in response chars:
/// `,"key":` around each value.
fn block_chars(block: &Map<String, Value>) -> usize {
    block
        .iter()
        .map(|(key, value)| {
            octocode_engine::json_size::utf16_units(value).unwrap_or(0) + key.len() + 4
        })
        .sum()
}

/// The executable next-page call of `input` for `pagination`.
fn continuation(input: &ResponseInput, pagination: &ResponsePagination) -> Option<Value> {
    build_continuation(input.tool.as_str(), &input.options, pagination)
}

/// The envelope members that say a response reads on: `next`, the
/// continuation (present while more remains), with the saved response's
/// expiry, which is when that continuation stops answering, so a last page,
/// which has no continuation, states no expiry; and, on a page read back
/// from a saved response, the replay fact. That one stays on a last page: it
/// says the page was not acquired or judged again, so any usage it shows is
/// historical, which is as true of the last page as of the others.
fn block(next: Option<&Value>, options: &ResponsePageOptions) -> Map<String, Value> {
    use crate::response::continuations::{RESPONSE_EXPIRES_AT, RESPONSE_NEXT, RESPONSE_REPLAYED};
    let mut members = Map::new();
    if let Some(next) = next {
        members.insert(RESPONSE_NEXT.into(), next.clone());
    }
    if let Some(saved) = &options.saved {
        if next.is_some() {
            members.insert(RESPONSE_EXPIRES_AT.into(), json!(saved.expires_at));
        }
        if saved.replayed {
            members.insert(RESPONSE_REPLAYED.into(), json!(true));
        }
    }
    members
}

/// The chars of a page envelope that count against `responseChars`: its
/// compact rendering in UTF-16 units, less the per-call provider metadata
/// that rides outside every page measure ([`super::volatile_chars`]).
/// `None` when the page does not serialize. A batch's rejected rows also
/// ride page 1 outside its plan and measure (they join after the cut), but a
/// page cannot tell them from executed error rows, so this count includes
/// them.
pub fn budgeted_chars(page: &Value) -> Option<usize> {
    let rendered = octocode_engine::json_size::utf16_units(page)?;
    Some(rendered - page.as_object().map_or(0, super::volatile_chars))
}
