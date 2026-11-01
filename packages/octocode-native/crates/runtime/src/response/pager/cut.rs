use super::super::row_pages::RowSource;
use super::{
    PreparedPage, ResponseError, ResponseInput, ResponsePageOptions, ResponsePagination,
    build_continuation, is_saved_replay, page_cursor,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

/// One row page of `source`, with its pagination block and continuation.
pub(super) fn cut_rows(
    source: &RowSource,
    input: &ResponseInput,
    cancelled: &CancellationToken,
) -> Result<PreparedPage, ResponseError> {
    let (mut envelope, mut pagination) = source.page(
        &input.options,
        &input.remaining,
        &|pagination| pagination_block_chars(input, pagination),
        cancelled,
    )?;
    pagination.next = continuation(input, &pagination);
    if pagination.should_emit() || is_saved_replay(&input.options) {
        // The page is measured once ([`RowSource::page`]): the rendered
        // page is those chars plus the block's entry, so the oversized flag
        // ([`measure_page`]'s rule) follows without serializing the page again.
        pagination.oversized = None;
        let mut block = pagination_value(&pagination, &input.options)?;
        let rendered = pagination.returned_chars + entry_chars(&envelope, &block)?;
        if input
            .options
            .response_chars
            .is_some_and(|budget| rendered > budget)
        {
            block["oversized"] = json!(true);
        }
        envelope.insert("responsePagination".into(), block);
    }
    Ok(PreparedPage {
        value: Value::Object(envelope),
        next: page_cursor(&input.options, &pagination),
    })
}

/// Chars `,"responsePagination":<block>` adds to `envelope` (no comma when
/// it is the envelope's only entry).
fn entry_chars(
    envelope: &serde_json::Map<String, Value>,
    block: &Value,
) -> Result<usize, ResponseError> {
    let value =
        octocode_engine::json_size::utf16_units(block).ok_or(ResponseError::Unserializable)?;
    // `"responsePagination":` and the separating comma.
    Ok(value + "responsePagination".len() + 3 + usize::from(!envelope.is_empty()))
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
    page.next = build_continuation(input.tool.as_str(), &options, &page);
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
        "page": pagination.page,
        "totalPages": pagination.total_pages,
        "hasMore": pagination.has_more,
    });
    if let Some(next) = &pagination.next {
        value["next"] = next.clone();
    }
    if let Some(oversized) = pagination.oversized {
        value["oversized"] = json!(oversized);
    }
    if let Some(saved) = &options.saved {
        value["expiresAt"] = json!(saved.expires_at);
        value["replayed"] = json!(saved.replayed);
    }
    Ok(value)
}

pub(crate) fn measure_page(output: &mut Value, budget: Option<usize>) -> Result<(), ResponseError> {
    if output.get("responsePagination").is_some() {
        if let Some(page) = output["responsePagination"].as_object_mut() {
            page.remove("oversized");
        }
        let rendered =
            octocode_engine::json_size::utf16_units(output).ok_or(ResponseError::Unserializable)?;
        if budget.is_some_and(|budget| rendered > budget) {
            output["responsePagination"]["oversized"] = json!(true);
        }
    }
    Ok(())
}
