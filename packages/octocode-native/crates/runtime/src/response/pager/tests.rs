//! Response pager tests.

use super::*;
use serde_json::json;
mod rows;
mod structured;
mod window;

fn options(length: usize) -> ResponsePageOptions {
    ResponsePageOptions {
        response_length: Some(length),
        ..Default::default()
    }
}

fn structured_options(
    length: usize,
    offset: usize,
    snapshot: Option<String>,
) -> ResponsePageOptions {
    ResponsePageOptions {
        response_length: Some(length),
        response_offset: Some(offset),
        response_snapshot: snapshot,
        response_scope: Some(PageScope::Structured),
        saved: None,
    }
}

/// The page `input` asks for; panics on a pager error.
fn page_of(tool: &str, structured: &Value, options: &ResponsePageOptions) -> Value {
    prepare(
        ResponseInput {
            remaining: Vec::new(),
            tool: tool.into(),
            structured: structured.clone(),
            options: options.clone(),
        },
        &CancellationToken::new(),
    )
    .expect("page")
}

/// Pages one response in `scope` at `length`, following each page's own
/// `next` cursor exactly as a caller copies it, and returns every page's
/// structuredContent.
fn walk_pages(structured: &Value, scope: PageScope, length: usize) -> Vec<Value> {
    let mut options = ResponsePageOptions {
        response_scope: Some(scope),
        ..options(length)
    };
    let mut pages = Vec::new();
    loop {
        let page = page_of("localSearch", structured, &options);
        let pagination = page["responsePagination"].clone();
        pages.push(page);
        if pagination["hasMore"] != true {
            break;
        }
        assert!(
            pages.len() <= 20_000,
            "{scope:?} length {length}: the walk must terminate"
        );
        options = follow_only_the_cursor(&options, &pagination);
    }
    pages
}

/// The UTF-16 units of `page` serialized: what a caller receives.
fn rendered_units(page: &Value) -> usize {
    page.to_string().encode_utf16().count()
}

/// The options of the page `pagination.next` names, asserting that `next`
/// carries nothing but the opaque cursor.
fn follow_only_the_cursor(
    options: &ResponsePageOptions,
    pagination: &Value,
) -> ResponsePageOptions {
    let next = &pagination["next"]["query"];
    assert_eq!(
        next.as_object()
            .map(|query| query.keys().collect::<Vec<_>>()),
        Some(vec![&"responseSnapshot".to_owned()]),
        "{pagination}"
    );
    options.follow(next).expect("a decodable cursor")
}

/// The content a page's cursor names: its `next` carries the cursor while
/// more is left; the last page names its own in the block.
fn content_of(pagination: &Value) -> [u8; 32] {
    let cursor = match &pagination["next"]["query"]["responseSnapshot"] {
        Value::Null => &pagination["snapshot"],
        cursor => cursor,
    };
    Cursor::decode(cursor.as_str().expect("cursor"))
        .expect("decodable cursor")
        .digest
}
