use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePageOptions {
    pub response_char_offset: Option<usize>,
    pub response_char_length: Option<usize>,
    pub response_snapshot: Option<String>,
    pub render_text: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct ResponseInput {
    pub tool: String,
    pub query: Value,
    pub structured: Value,
    pub rendered_text: Option<String>,
    pub is_error: bool,
    pub options: ResponsePageOptions,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub r#type: String,
    pub text: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedResponse {
    pub content: Vec<TextContent>,
    pub structured_content: Value,
    pub is_error: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePagination {
    pub scope: String,
    pub current_page: usize,
    pub total_pages: usize,
    pub has_more: bool,
    pub char_offset: usize,
    pub char_length: usize,
    pub total_chars: usize,
    pub snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_char_offset: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<ResponseContinuation>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ResponseContinuation {
    pub tool: String,
    pub query: Value,
}

#[derive(Clone, Copy, Debug)]
pub struct ResponsePagerConfig {
    pub max_rendered_bytes: usize,
}

impl Default for ResponsePagerConfig {
    fn default() -> Self {
        Self {
            max_rendered_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponseError {
    Cancelled,
    RenderedTextTooLarge,
    StructuredContentMustBeObject,
}

pub struct ResponsePager {
    config: ResponsePagerConfig,
}

impl ResponsePager {
    pub fn new(config: ResponsePagerConfig) -> Self {
        Self { config }
    }

    pub fn prepare(
        &self,
        input: ResponseInput,
        cancelled: &AtomicBool,
    ) -> Result<PreparedResponse, ResponseError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ResponseError::Cancelled);
        }
        let mut structured = input
            .structured
            .as_object()
            .cloned()
            .ok_or(ResponseError::StructuredContentMustBeObject)?;
        let Some(text) = input.rendered_text else {
            return Ok(PreparedResponse {
                content: Vec::new(),
                structured_content: Value::Object(structured),
                is_error: input.is_error,
            });
        };
        if text.len() > self.config.max_rendered_bytes {
            return Err(ResponseError::RenderedTextTooLarge);
        }
        let page = paginate_text(&text, &input.options);
        if cancelled.load(Ordering::Acquire) {
            return Err(ResponseError::Cancelled);
        }
        if let Some(mut pagination) = page.pagination {
            pagination.next =
                build_continuation(&input.tool, &input.query, &input.options, &pagination);
            structured.insert(
                "responsePagination".into(),
                serde_json::to_value(&pagination).expect("serializable pagination"),
            );
        }
        Ok(PreparedResponse {
            content: vec![TextContent {
                r#type: "text".into(),
                text: page.text,
            }],
            structured_content: Value::Object(structured),
            is_error: input.is_error,
        })
    }
}

struct Page {
    text: String,
    pagination: Option<ResponsePagination>,
}

fn paginate_text(text: &str, options: &ResponsePageOptions) -> Page {
    let Some(requested_length) = options.response_char_length else {
        return Page {
            text: text.into(),
            pagination: None,
        };
    };
    let units = text.encode_utf16().collect::<Vec<_>>();
    let total = units.len();
    let snapshot = format!(
        "response-v1:{}",
        hex::encode(Sha256::digest(text.as_bytes()))
    );
    let length = requested_length.max(1);
    let requested_offset = options.response_char_offset.unwrap_or(0);
    let offset = requested_offset.min(total);
    let changed = options.response_snapshot.as_deref() != Some(&snapshot);
    let invalid = splits_pair(&units, offset);
    if requested_offset > 0 && (changed || invalid) {
        let expected = options.response_snapshot.clone();
        let reason = if invalid && !changed {
            "The requested offset splits a Unicode code point. Restart from responseCharOffset=0 and follow the returned continuation."
        } else if expected.is_some() {
            "The full response changed since the previous page. Discard earlier pages and restart from responseCharOffset=0."
        } else {
            "Later response pages require responseSnapshot from the previous page. Restart from responseCharOffset=0."
        };
        return Page {
            text: format!("# Response pagination restart required. {reason}\n"),
            pagination: Some(ResponsePagination {
                scope: "content.text".into(),
                current_page: 1,
                total_pages: total_pages(&units, length),
                has_more: true,
                char_offset: offset,
                char_length: 0,
                total_chars: total,
                snapshot,
                expected_snapshot: expected.clone(),
                changed: Some(expected.is_some() && changed),
                restart: Some(true),
                next_char_offset: Some(0),
                next: None,
            }),
        };
    }
    let end = choose_end(&units, offset, length);
    let has_more = end < total;
    let current = page_number(&units, offset, length);
    let pages = total_pages(&units, length);
    let body = String::from_utf16(&units[offset..end]).expect("page never splits UTF-16 pairs");
    let header = if has_more {
        format!("# Response page {current}/{pages}. Next: responseCharOffset={end}\n")
    } else {
        format!("# Response page {current}/{pages}.\n")
    };
    Page {
        text: header + &body,
        pagination: Some(ResponsePagination {
            scope: "content.text".into(),
            current_page: current,
            total_pages: pages,
            has_more,
            char_offset: offset,
            char_length: end - offset,
            total_chars: total,
            snapshot,
            expected_snapshot: None,
            changed: None,
            restart: None,
            next_char_offset: has_more.then_some(end),
            next: None,
        }),
    }
}

fn build_continuation(
    tool: &str,
    query: &Value,
    request: &ResponsePageOptions,
    page: &ResponsePagination,
) -> Option<ResponseContinuation> {
    let next_offset = page.next_char_offset.filter(|_| page.has_more)?;
    let mut clean = query.as_object().cloned().unwrap_or_default();
    clean.remove("goal");
    // Emit as { queries: [q] } so continuation tokens round-trip through
    // the backward-compatible single-element path in prepare().
    let mut continuation = Map::new();
    continuation.insert("queries".into(), Value::Array(vec![Value::Object(clean)]));
    if let Some(length) = request.response_char_length {
        continuation.insert("responseCharLength".into(), json!(length));
    }
    continuation.insert("responseCharOffset".into(), json!(next_offset));
    if page.restart != Some(true) {
        continuation.insert("responseSnapshot".into(), json!(page.snapshot));
    }
    Some(ResponseContinuation {
        tool: tool.into(),
        query: Value::Object(continuation),
    })
}

fn splits_pair(units: &[u16], offset: usize) -> bool {
    offset > 0
        && offset < units.len()
        && (0xd800..=0xdbff).contains(&units[offset - 1])
        && (0xdc00..=0xdfff).contains(&units[offset])
}

fn choose_end(units: &[u16], start: usize, length: usize) -> usize {
    let raw = (start + length).min(units.len());
    if raw >= units.len() {
        return units.len();
    }
    let minimum = (length / 2).max(1);
    let mut boundary = None;
    for index in start..raw {
        if units[index] == b'\n' as u16 {
            boundary = Some(index + 1);
        }
        if index + 1 < raw && units[index] == b'\\' as u16 && units[index + 1] == b'n' as u16 {
            boundary = Some(index + 2);
        }
    }
    if let Some(value) = boundary.filter(|value| value - start >= minimum) {
        return value;
    }
    if splits_pair(units, raw) {
        if raw - start == 1 { raw + 1 } else { raw - 1 }
    } else {
        raw
    }
}

fn page_number(units: &[u16], offset: usize, length: usize) -> usize {
    let mut page = 1;
    let mut cursor = 0;
    while cursor < offset && cursor < units.len() {
        let next = choose_end(units, cursor, length);
        if next <= cursor {
            break;
        }
        cursor = next;
        page += 1;
    }
    if cursor == offset {
        page
    } else {
        offset / length + 1
    }
}

fn total_pages(units: &[u16], length: usize) -> usize {
    if units.is_empty() {
        return 1;
    }
    let mut pages = 0;
    let mut cursor = 0;
    while cursor < units.len() {
        let next = choose_end(units, cursor, length);
        if next <= cursor {
            return units.len().div_ceil(length).max(1);
        }
        cursor = next;
        pages += 1;
    }
    pages.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(length: usize) -> ResponsePageOptions {
        ResponsePageOptions {
            response_char_length: Some(length),
            ..Default::default()
        }
    }

    #[test]
    fn unicode_pages_reconstruct_exact_text_and_continuations_are_executable() {
        let text = "a😀b\n🧪研究c😀";
        for length in [1, 2, 3, 7] {
            let mut offset = 0;
            let mut snapshot = None;
            let mut joined = String::new();
            loop {
                let result = paginate_text(
                    text,
                    &ResponsePageOptions {
                        response_char_length: Some(length),
                        response_char_offset: Some(offset),
                        response_snapshot: snapshot.clone(),
                        render_text: None,
                    },
                );
                let page = result.pagination.expect("pagination");
                joined.push_str(result.text.split_once('\n').expect("header").1);
                snapshot = Some(page.snapshot.clone());
                if !page.has_more {
                    break;
                }
                offset = page.next_char_offset.expect("continuation");
            }
            assert_eq!(joined, text);
        }
    }

    #[test]
    fn snapshot_and_unicode_boundary_mismatches_restart() {
        let first = paginate_text("a😀b", &options(1));
        let snapshot = first.pagination.expect("page").snapshot;
        let split = paginate_text(
            "a😀b",
            &ResponsePageOptions {
                response_char_length: Some(1),
                response_char_offset: Some(2),
                response_snapshot: Some(snapshot),
                render_text: None,
            },
        );
        assert_eq!(split.pagination.expect("page").restart, Some(true));
        let changed = paginate_text(
            "changed",
            &ResponsePageOptions {
                response_char_length: Some(2),
                response_char_offset: Some(2),
                response_snapshot: Some("response-v1:stale".into()),
                render_text: None,
            },
        );
        assert_eq!(changed.pagination.expect("page").changed, Some(true));
    }

    #[test]
    fn prepares_both_channels_and_preserves_required_debug_state() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let result = pager
            .prepare(
                ResponseInput {
                    tool: "localFetch".into(),
                    query: json!({"path":"a", "goal":"g", "reasoning":"r", "debug":true}),
                    structured: json!({"results":[]}),
                    rendered_text: Some("line1\nline2\nline3".into()),
                    is_error: false,
                    options: options(8),
                },
                &AtomicBool::new(false),
            )
            .expect("page");
        assert_eq!(result.content.len(), 1);
        let next = &result.structured_content["responsePagination"]["next"]["query"];
        // continuation wraps in { queries: [q] } for backward compat
        assert_eq!(
            next["queries"][0],
            json!({"path":"a", "reasoning":"r", "debug":true})
        );
        assert!(
            next["responseSnapshot"]
                .as_str()
                .expect("snapshot")
                .starts_with("response-v1:")
        );
    }

    #[test]
    fn frozen_line_boundary_empty_and_last_page_metadata_match() {
        let line = paginate_text("0123456789\n0123456789abc", &options(15));
        assert_eq!(
            line.text.split_once('\n').expect("header").1,
            "0123456789\n"
        );
        assert_eq!(line.pagination.expect("page").next_char_offset, Some(11));

        let empty = paginate_text("", &options(100));
        let empty_page = empty.pagination.expect("page");
        assert_eq!(empty_page.total_chars, 0);
        assert!(!empty_page.has_more);
        assert_eq!(empty_page.next_char_offset, None);

        let short = paginate_text("short", &options(100));
        assert!(!short.pagination.expect("page").has_more);
    }

    #[test]
    fn missing_snapshot_and_changed_beyond_end_return_typed_restart() {
        let missing = paginate_text(
            "a".repeat(100).as_str(),
            &ResponsePageOptions {
                response_char_length: Some(10),
                response_char_offset: Some(10),
                ..Default::default()
            },
        );
        let missing_page = missing.pagination.expect("page");
        assert_eq!(missing_page.restart, Some(true));
        assert_eq!(missing_page.changed, Some(false));
        assert_eq!(missing_page.next_char_offset, Some(0));

        let stale = paginate_text(
            "short",
            &ResponsePageOptions {
                response_char_length: Some(5),
                response_char_offset: Some(500),
                response_snapshot: Some("response-v1:stale".into()),
                render_text: None,
            },
        );
        let stale_page = stale.pagination.expect("page");
        assert_eq!(stale_page.char_offset, 5);
        assert_eq!(stale_page.char_length, 0);
        assert_eq!(stale_page.changed, Some(true));
    }

    #[test]
    fn cancellation_and_render_budget_fail_before_cache_growth() {
        let pager = ResponsePager::new(ResponsePagerConfig {
            max_rendered_bytes: 4,
        });
        let input = ResponseInput {
            tool: "tool".into(),
            query: json!({}),
            structured: json!({}),
            rendered_text: Some("large".into()),
            is_error: false,
            options: options(2),
        };
        assert!(matches!(
            pager.prepare(input.clone(), &AtomicBool::new(false),),
            Err(ResponseError::RenderedTextTooLarge)
        ));
        assert!(matches!(
            pager.prepare(input, &AtomicBool::new(true)),
            Err(ResponseError::Cancelled)
        ));
    }
}
