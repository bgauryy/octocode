use super::super::row_pages::RowSource;
use super::{
    PreparedPage, ResponseError, ResponseInput, ResponsePageOptions, ResponsePagination,
    build_continuation, is_saved_replay, page_cursor,
};
use octocode_io::cancellation::Cancellation;
use serde_json::{Value, json};

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
        let block = pagination_value(&pagination, &input.options)?;
        envelope.insert("responsePagination".into(), block);
    }
    if let Some(budget) = input.options.response_chars {
        super::oversized::flag(&mut envelope, input.tool, budget);
    }
    Ok(PreparedPage {
        value: Value::Object(envelope),
        next: page_cursor(&input.options, &pagination),
    })
}

/// Chars the `responsePagination` entry adds to a page of `pagination`
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
    pagination_value(&page, &options).map_or(0, |value| {
        // `,"responsePagination":` around the value.
        octocode_engine::json_size::utf16_units(&value).unwrap_or(0)
            + "responsePagination".len()
            + 4
    })
}

/// The executable next-page call of `input` for `pagination`.
fn continuation(input: &ResponseInput, pagination: &ResponsePagination) -> Option<Value> {
    build_continuation(input.tool.as_str(), &input.options, pagination)
}

fn pagination_value(
    pagination: &ResponsePagination,
    options: &ResponsePageOptions,
) -> Result<Value, ResponseError> {
    let mut value = json!({
        "hasMore": pagination.has_more,
    });
    if let Some(next) = &pagination.next {
        value["next"] = next.clone();
    }
    if let Some(saved) = &options.saved {
        value["expiresAt"] = json!(saved.expires_at);
        value["replayed"] = json!(saved.replayed);
    }
    Ok(value)
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
