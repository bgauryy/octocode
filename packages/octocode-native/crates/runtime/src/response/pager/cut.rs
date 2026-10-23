use super::super::row_pages::RowSource;
use super::{
    ResponseError, ResponseInput, ResponsePageOptions, ResponsePagination, build_continuation,
    cursor_at, is_saved_replay, paginate_window, rows_key,
};
use serde_json::{Map, Value, json};

/// One row page of `source`, with its pagination block and continuation.
pub(super) fn cut_rows(source: &RowSource, input: &ResponseInput) -> Result<Value, ResponseError> {
    let (mut envelope, mut pagination) =
        source.page(&input.options, &input.remaining, &|pagination| {
            pagination_block_chars(input, pagination)
        });
    pagination.next = continuation(input, &pagination);
    if pagination.should_emit() || is_saved_replay(&input.options) {
        envelope.insert(
            "responsePagination".into(),
            pagination_value(&pagination, &input.options)?,
        );
    }
    Ok(Value::Object(envelope))
}

/// One structured window of the serialized envelope `full`.
pub(super) fn cut_window(full: &str, input: &ResponseInput) -> Result<Value, ResponseError> {
    let page = paginate_window(full, &input.options);
    let mut windowed = Map::new();
    windowed.insert(rows_key(&input.tool).into(), Value::Array(Vec::new()));
    // A restart page carries a diagnostic message, not envelope
    // bytes — keep it out of the concatenable window.
    let restarting = page
        .pagination
        .as_ref()
        .is_some_and(|p| p.restart == Some(true));
    windowed.insert(
        "responseWindow".into(),
        json!(if restarting { "" } else { page.text.as_str() }),
    );
    if let Some(mut pagination) = page.pagination {
        pagination.next = continuation(input, &pagination);
        if pagination.should_emit() || is_saved_replay(&input.options) {
            windowed.insert(
                "responsePagination".into(),
                pagination_value(&pagination, &input.options)?,
            );
        }
    }
    Ok(Value::Object(windowed))
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
    page.next = build_continuation(&input.tool, &options, &page);
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
    build_continuation(&input.tool, &input.options, pagination)
}

fn pagination_value(
    pagination: &ResponsePagination,
    options: &ResponsePageOptions,
) -> Result<Value, ResponseError> {
    let mut value = serde_json::to_value(pagination).map_err(|_| ResponseError::Unserializable)?;
    let Some(block) = value.as_object_mut() else {
        return Err(ResponseError::Unserializable);
    };
    // The cursor rides `next`; a page without one names its own page.
    block.remove("snapshot");
    if pagination.next.is_none()
        && let Some(cursor) = cursor_at(
            options,
            pagination,
            &pagination.snapshot,
            pagination.char_offset,
        )
    {
        block.insert("snapshot".into(), json!(cursor));
    }
    block.remove("expectedSnapshot");
    if let Some(cursor) = pagination
        .expected_snapshot
        .as_deref()
        .and_then(|expected| {
            cursor_at(
                options,
                pagination,
                expected,
                options.response_offset.unwrap_or(0),
            )
        })
    {
        block.insert("expectedSnapshot".into(), json!(cursor));
    }
    if let Some(saved) = &options.saved {
        block.insert("expiresAt".into(), json!(saved.expires_at));
        block.insert("storage".into(), json!(saved.storage));
        block.insert("replayed".into(), json!(saved.replayed));
    }
    Ok(value)
}
