use super::*;
use serde_json::json;

/// The cursor rides `next`: a page whose `next` carries the snapshot and
/// offset does not repeat them in its block.
#[test]
fn the_block_leaves_the_cursor_to_next() {
    let prepared = prepare(
        ResponseInput {
            remaining: Vec::new(),
            tool: "localFetch".into(),
            structured: json!({"results":[{"index":0,"data":{"content":"line1\nline2\nline3"}}]}),
            options: ResponsePageOptions {
                response_length: Some(24),
                response_scope: Some(PageScope::Structured),
                ..Default::default()
            },
        },
        &CancellationToken::new(),
    )
    .expect("page");
    let block = &prepared["responsePagination"];
    let next = &block["next"]["query"];
    let cursor = next["responseSnapshot"].as_str().expect("cursor");
    assert_eq!(next.as_object().map(Map::len), Some(1), "{block}");
    assert_eq!(cursor.len(), super::super::cursor::CURSOR_CHARS, "{block}");
    assert!(block.get("snapshot").is_none(), "{block}");
    assert!(block.get("nextOffset").is_none(), "{block}");
    assert_eq!(block["hasMore"], true, "{block}");
}
