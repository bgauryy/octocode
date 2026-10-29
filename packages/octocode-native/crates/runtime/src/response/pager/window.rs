use super::super::cursor::Cursor;
use super::super::page_model::{ResponsePageOptions, ResponsePagination};
use serde_json::{Value, json};

pub(super) fn page_cursor(
    request: &ResponsePageOptions,
    page: &ResponsePagination,
) -> Option<Cursor> {
    let offset = page.next_offset.filter(|_| page.has_more)?;
    let key = request.saved.as_ref().map_or([0; 32], |saved| saved.key);
    Cursor::of_snapshot(key, &page.snapshot, request.response_chars?, offset)
}

pub(super) fn build_continuation(
    tool: &str,
    request: &ResponsePageOptions,
    page: &ResponsePagination,
) -> Option<Value> {
    page_cursor(request, page)?;
    let id = crate::id::ToolId::from_name(tool)?;
    let contract = crate::contracts::tool_contract(id).ok()?;
    let schema = &contract["inputSchema"];
    let maximum = crate::contracts::property_schemas(schema, &schema["$defs"], "cursor")
        .iter()
        .filter_map(|field| field["maxLength"].as_u64())
        .max()?;
    // Plan for the issuer's exact wire shape; maxLength is an admission bound,
    // not the size of a handle. Publication replaces only the placeholder's value.
    let cursor = super::super::cursor::issued::placeholder();
    if cursor.len() > usize::try_from(maximum).ok()? {
        return None;
    }
    Some(json!({"kind":"continue", "tool":tool, "input":{"cursor":cursor}}))
}
