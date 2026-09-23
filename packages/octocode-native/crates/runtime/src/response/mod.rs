use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePageOptions {
    pub response_char_offset: Option<usize>,
    pub response_char_length: Option<usize>,
    pub response_snapshot: Option<String>,
    /// Opt-in (R9): `"structured"` windows the serialized structuredContent
    /// envelope instead of the rendered text. Default (`None`/`"text"`)
    /// behavior is unchanged.
    pub response_scope: Option<String>,
    pub render_text: Option<bool>,
}

impl ResponsePageOptions {
    pub fn structured_scope(&self) -> bool {
        self.response_scope.as_deref() == Some("structured") && self.response_char_length.is_some()
    }

    /// Row-aware paging of structuredContent: every page is a complete JSON
    /// envelope holding whole result rows (or whole elements of a split row).
    pub fn rows_scope(&self) -> bool {
        self.response_scope.as_deref() == Some("rows") && self.response_char_length.is_some()
    }

    fn explicit(&self) -> bool {
        self.response_char_length.is_some()
            || self.response_char_offset.is_some()
            || self.response_snapshot.is_some()
    }

    /// Apply the configured `output.pagination.defaultCharLength` budget when
    /// the caller did not page explicitly. Implicit pages are whole rows so
    /// both text and structuredContent carry the page: MCP clients that read
    /// only structuredContent would otherwise see an emptied `results`.
    /// Returns whether automatic pagination was enabled.
    pub fn auto_paginate(
        &mut self,
        rendered_text: Option<&str>,
        structured: &Value,
        budget: usize,
    ) -> bool {
        if self.explicit() || budget == 0 {
            return false;
        }
        let oversized = match rendered_text {
            Some(text) => text.encode_utf16().count() > budget,
            None => structured.to_string().encode_utf16().count() > budget,
        };
        if oversized {
            self.response_scope = Some("rows".into());
            self.response_char_length = Some(budget);
        }
        oversized
    }
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
        if input.options.rows_scope() {
            let full = Value::Object(structured.clone()).to_string();
            if full.len() > self.config.max_rendered_bytes {
                return Err(ResponseError::RenderedTextTooLarge);
            }
            let (mut envelope, mut pagination) = paginate_rows(structured, &full, &input.options);
            if cancelled.load(Ordering::Acquire) {
                return Err(ResponseError::Cancelled);
            }
            pagination.next =
                build_continuation(&input.tool, &input.query, &input.options, &pagination);
            // pagination is a plain serializable struct
            #[allow(clippy::expect_used)]
            envelope.insert(
                "responsePagination".into(),
                serde_json::to_value(&pagination).expect("serializable pagination"),
            );
            let text = Value::Object(envelope.clone()).to_string();
            return Ok(PreparedResponse {
                content: vec![TextContent {
                    r#type: "text".into(),
                    text,
                }],
                structured_content: Value::Object(envelope),
                is_error: input.is_error,
            });
        }
        // Opt-in structured windowing: page the serialized envelope itself so
        // MCP clients can stream a large structuredContent. The window text
        // carries no page header — concatenating `responseWindow` pages in
        // order reconstructs the exact envelope JSON.
        if input.options.structured_scope() {
            let full = Value::Object(structured).to_string();
            if full.len() > self.config.max_rendered_bytes {
                return Err(ResponseError::RenderedTextTooLarge);
            }
            let page = paginate_units(&full, &input.options, false);
            if cancelled.load(Ordering::Acquire) {
                return Err(ResponseError::Cancelled);
            }
            let mut windowed = Map::new();
            windowed.insert("results".into(), Value::Array(Vec::new()));
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
                pagination.scope = "structuredContent".into();
                pagination.next =
                    build_continuation(&input.tool, &input.query, &input.options, &pagination);
                // pagination is a plain serializable struct
                #[allow(clippy::expect_used)]
                windowed.insert(
                    "responsePagination".into(),
                    serde_json::to_value(&pagination).expect("serializable pagination"),
                );
            }
            return Ok(PreparedResponse {
                content: vec![TextContent {
                    r#type: "text".into(),
                    text: page.text,
                }],
                structured_content: Value::Object(windowed),
                is_error: input.is_error,
            });
        }
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
            // The text page carries this window of the payload; repeating the
            // whole payload in structuredContent would defeat pagination.
            // A single page that covers everything keeps its results.
            let whole =
                pagination.char_offset == 0 && !pagination.has_more && pagination.restart.is_none();
            if !whole {
                if let Some(results) = structured.get_mut("results") {
                    *results = Value::Array(Vec::new());
                }
                structured.remove("shared");
            }
            // pagination is a plain serializable struct
            #[allow(clippy::expect_used)]
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
    paginate_units(text, options, true)
}

fn paginate_units(text: &str, options: &ResponsePageOptions, with_header: bool) -> Page {
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
    // `choose_end` never splits a UTF-16 surrogate pair, so this is well-formed.
    #[allow(clippy::expect_used)]
    let body = String::from_utf16(&units[offset..end]).expect("page never splits UTF-16 pairs");
    let header = if !with_header {
        String::new()
    } else if has_more {
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

fn json_chars(value: &Value) -> usize {
    value.to_string().encode_utf16().count()
}

fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The largest array with at least two elements reachable from `value`
/// through objects and single-element arrays, as (JSON pointer, size).
fn largest_array(value: &Value, pointer: &str) -> Option<(String, usize)> {
    let mut best: Option<(String, usize)> = None;
    let mut consider = |candidate: Option<(String, usize)>| {
        if let Some(candidate) = candidate
            && best.as_ref().is_none_or(|current| candidate.1 > current.1)
        {
            best = Some(candidate);
        }
    };
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                consider(largest_array(
                    child,
                    &format!("{pointer}/{}", escape_pointer(key)),
                ));
            }
        }
        Value::Array(items) if items.len() >= 2 => {
            consider(Some((pointer.to_owned(), json_chars(value))));
        }
        Value::Array(items) => {
            if let Some(item) = items.first() {
                consider(largest_array(item, &format!("{pointer}/0")));
            }
        }
        _ => {}
    }
    best
}

/// Split one oversized row into row fragments that each keep every field
/// except a slice of the row's largest array. Fragments recurse into an
/// element that alone exceeds the budget; an indivisible element becomes one
/// oversized fragment rather than being cut mid-value.
fn split_row(row: &Value, budget: usize) -> Vec<Value> {
    if json_chars(row) <= budget {
        return vec![row.clone()];
    }
    let Some(data) = row.get("data") else {
        return vec![row.clone()];
    };
    let Some((relative, _)) = largest_array(data, "") else {
        return vec![row.clone()];
    };
    let pointer = format!("/data{relative}");
    // Clone the row once without its largest array; each fragment clones only
    // this skeleton, never the whole array again.
    let mut skeleton = row.clone();
    let Some(items) = skeleton
        .pointer_mut(&pointer)
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
    else {
        return vec![row.clone()];
    };
    let with_items = |chunk: Vec<Value>| {
        let mut fragment = skeleton.clone();
        if let Some(slot) = fragment.pointer_mut(&pointer) {
            *slot = Value::Array(chunk);
        }
        fragment
    };
    let base = json_chars(&with_items(Vec::new()));
    let mut fragments = Vec::new();
    let mut chunk = Vec::new();
    let mut chunk_chars = base;
    for item in items {
        let item_chars = json_chars(&item) + 1;
        if base + item_chars > budget {
            if !chunk.is_empty() {
                fragments.push(with_items(std::mem::take(&mut chunk)));
                chunk_chars = base;
            }
            fragments.extend(split_row(&with_items(vec![item]), budget));
            continue;
        }
        if !chunk.is_empty() && chunk_chars + item_chars > budget {
            fragments.push(with_items(std::mem::take(&mut chunk)));
            chunk_chars = base;
        }
        chunk_chars += item_chars;
        chunk.push(item);
    }
    if !chunk.is_empty() {
        fragments.push(with_items(chunk));
    }
    fragments
}

/// Row-aware pages: pack whole rows (or fragments of one oversized row) into
/// complete envelopes. `responseCharOffset` addresses the zero-based page.
fn paginate_rows(
    mut structured: Map<String, Value>,
    full: &str,
    options: &ResponsePageOptions,
) -> (Map<String, Value>, ResponsePagination) {
    let budget = options.response_char_length.unwrap_or(1).max(1);
    let rows = match structured.remove("results") {
        Some(Value::Array(rows)) => rows,
        _ => Vec::new(),
    };
    let overhead = json_chars(&Value::Object(structured.clone())) + "\"results\":[],".len();
    let row_budget = budget.saturating_sub(overhead).max(1);
    let mut pages: Vec<Vec<Value>> = Vec::new();
    let mut page: Vec<Value> = Vec::new();
    let mut page_chars = 0usize;
    for row in &rows {
        let mut fragments = split_row(row, row_budget);
        let parts = fragments.len();
        if parts > 1 {
            for (index, fragment) in fragments.iter_mut().enumerate() {
                fragment["rowPart"] = json!({"part": index + 1, "of": parts});
            }
        }
        for fragment in fragments {
            let chars = json_chars(&fragment) + 1;
            if !page.is_empty() && page_chars + chars > row_budget {
                pages.push(std::mem::take(&mut page));
                page_chars = 0;
            }
            page_chars += chars;
            page.push(fragment);
        }
    }
    if !page.is_empty() || pages.is_empty() {
        pages.push(page);
    }
    let total = full.encode_utf16().count();
    let snapshot = format!(
        "response-rows-v1:{}",
        hex::encode(Sha256::digest(full.as_bytes()))
    );
    let requested = options.response_char_offset.unwrap_or(0);
    let changed = options.response_snapshot.as_deref() != Some(&snapshot);
    if requested > 0 && (changed || requested >= pages.len()) {
        let expected = options.response_snapshot.clone();
        structured.insert("results".into(), Value::Array(Vec::new()));
        return (
            structured,
            ResponsePagination {
                scope: "rows".into(),
                current_page: 1,
                total_pages: pages.len(),
                has_more: true,
                char_offset: requested,
                char_length: 0,
                total_chars: total,
                snapshot,
                expected_snapshot: expected.clone(),
                changed: Some(expected.is_some() && changed),
                restart: Some(true),
                next_char_offset: Some(0),
                next: None,
            },
        );
    }
    let total_pages = pages.len();
    let selected = pages.swap_remove(requested);
    let has_more = requested + 1 < total_pages;
    structured.insert("results".into(), Value::Array(selected));
    let char_length = json_chars(&Value::Object(structured.clone()));
    (
        structured,
        ResponsePagination {
            scope: "rows".into(),
            current_page: requested + 1,
            total_pages,
            has_more,
            char_offset: requested,
            char_length,
            total_chars: total,
            snapshot,
            expected_snapshot: None,
            changed: None,
            restart: None,
            next_char_offset: has_more.then_some(requested + 1),
            next: None,
        },
    )
}

fn build_continuation(
    tool: &str,
    query: &Value,
    request: &ResponsePageOptions,
    page: &ResponsePagination,
) -> Option<ResponseContinuation> {
    let next_offset = page.next_char_offset.filter(|_| page.has_more)?;
    let mut queries = query
        .get("queries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![query.clone()]);
    for query in &mut queries {
        if let Some(object) = query.as_object_mut() {
            object.remove("goal");
        }
    }
    // The runtime passes a flat query for one row and an envelope for a batch.
    // Keep the rows flat so the continuation can be submitted unchanged.
    let mut continuation = Map::new();
    continuation.insert("queries".into(), Value::Array(queries));
    if let Some(length) = request.response_char_length {
        continuation.insert("responseCharLength".into(), json!(length));
    }
    if let Some(scope) = &request.response_scope {
        continuation.insert("responseScope".into(), json!(scope));
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
                        response_scope: None,
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
                response_scope: None,
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
                response_scope: None,
                render_text: None,
            },
        );
        assert_eq!(changed.pagination.expect("page").changed, Some(true));
    }

    fn structured_options(
        length: usize,
        offset: usize,
        snapshot: Option<String>,
    ) -> ResponsePageOptions {
        ResponsePageOptions {
            response_char_length: Some(length),
            response_char_offset: Some(offset),
            response_snapshot: snapshot,
            response_scope: Some("structured".into()),
            render_text: None,
        }
    }

    #[test]
    fn structured_scope_windows_the_envelope_and_pages_reassemble_exactly() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let envelope = json!({"results":[
            {"index":0,"data":{"content":"alpha beta gamma delta epsilon"},"status":"empty"},
            {"index":1,"data":{"content":"zeta eta theta iota kappa lambda"}}
        ]});
        let full = envelope.to_string();
        let mut offset = 0;
        let mut snapshot = None;
        let mut joined = String::new();
        loop {
            let prepared = pager
                .prepare(
                    ResponseInput {
                        tool: "localFetch".into(),
                        query: json!({"path":"a","reasoning":"r","debug":false}),
                        structured: envelope.clone(),
                        rendered_text: None,
                        is_error: false,
                        options: structured_options(40, offset, snapshot.clone()),
                    },
                    &AtomicBool::new(false),
                )
                .expect("page");
            let out = prepared.structured_content;
            // The windowed envelope stays schema-valid: results present.
            assert_eq!(out["results"], json!([]));
            let window = out["responseWindow"].as_str().expect("window");
            let pagination = &out["responsePagination"];
            assert_eq!(pagination["scope"], "structuredContent");
            // Window text carries no page header; content mirrors it.
            assert_eq!(prepared.content[0].text, window);
            joined.push_str(window);
            snapshot = Some(
                pagination["snapshot"]
                    .as_str()
                    .expect("snapshot")
                    .to_owned(),
            );
            if pagination["hasMore"] != json!(true) {
                break;
            }
            // The continuation is executable and keeps the opt-in scope.
            let next = &pagination["next"]["query"];
            assert_eq!(next["responseScope"], "structured");
            offset = pagination["nextCharOffset"].as_u64().expect("offset") as usize;
        }
        assert_eq!(
            joined, full,
            "concatenated windows must equal the envelope JSON"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&joined).expect("json"),
            envelope
        );
    }

    #[test]
    fn structured_scope_restart_keeps_the_window_empty() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let prepared = pager
            .prepare(
                ResponseInput {
                    tool: "localFetch".into(),
                    query: json!({"path":"a","reasoning":"r"}),
                    structured: json!({"results":[{"index":0,"data":{"content":"body"}}]}),
                    rendered_text: None,
                    is_error: false,
                    options: structured_options(10, 5, Some("response-v1:stale".into())),
                },
                &AtomicBool::new(false),
            )
            .expect("restart page");
        let out = prepared.structured_content;
        assert_eq!(out["responsePagination"]["restart"], json!(true));
        assert_eq!(
            out["responseWindow"], "",
            "restart pages carry no envelope bytes"
        );
    }

    #[test]
    fn default_scope_is_unchanged_by_the_new_field() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
        let prepared = pager
            .prepare(
                ResponseInput {
                    tool: "localFetch".into(),
                    query: json!({"path":"a","reasoning":"r"}),
                    structured: envelope.clone(),
                    rendered_text: None,
                    is_error: false,
                    options: ResponsePageOptions::default(),
                },
                &AtomicBool::new(false),
            )
            .expect("default");
        assert_eq!(prepared.structured_content, envelope);
        assert!(prepared.structured_content.get("responseWindow").is_none());
    }

    #[test]
    fn a_text_page_covering_everything_keeps_structured_results() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
        let prepared = pager
            .prepare(
                ResponseInput {
                    tool: "localFetch".into(),
                    query: json!({"path":"a","reasoning":"r"}),
                    structured: envelope.clone(),
                    rendered_text: Some("body".into()),
                    is_error: false,
                    options: ResponsePageOptions {
                        response_char_length: Some(1_000),
                        ..Default::default()
                    },
                },
                &AtomicBool::new(false),
            )
            .expect("page");
        assert_eq!(prepared.structured_content["results"], envelope["results"]);
        assert_eq!(
            prepared.structured_content["responsePagination"]["hasMore"],
            false
        );
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
    fn bulk_continuations_preserve_queries_and_pass_output_contract() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let queries = json!([
            {"path":"/repo/a.ts", "reasoning":"read first", "debug":true, "fullContent":true},
            {"path":"/repo/b.ts", "reasoning":"read second", "debug":false, "startLine":2, "endLine":5}
        ]);
        for options in [
            options(8),
            ResponsePageOptions {
                response_char_length: Some(8),
                response_char_offset: Some(8),
                response_snapshot: Some("response-v1:stale".into()),
                response_scope: None,
                render_text: None,
            },
        ] {
            let restarting = options.response_char_offset.is_some();
            let result = pager
                .prepare(
                    ResponseInput {
                        tool: "localFetch".into(),
                        query: json!({"queries":queries}),
                        structured: json!({"results":[]}),
                        rendered_text: Some("line1\nline2\nline3".into()),
                        is_error: false,
                        options,
                    },
                    &AtomicBool::new(false),
                )
                .expect("page");
            crate::contracts::validate_output("localFetch", &result.structured_content)
                .expect("continuation must satisfy the public output contract");
            let next = &result.structured_content["responsePagination"]["next"]["query"];
            assert_eq!(next["queries"], queries);
            let prepared = crate::contracts::prepare_many_and_validate(
                "localFetch",
                next.clone(),
                crate::contracts::PrepareOptions::default(),
            )
            .expect("continuation must be accepted as a new tool call");
            assert_eq!(prepared.len(), 2);
            for (prepared, original) in prepared.iter().zip(queries.as_array().expect("queries")) {
                for (field, value) in original.as_object().expect("query") {
                    assert_eq!(&prepared[field], value);
                }
            }
            assert_eq!(next["responseCharLength"], 8);
            assert_eq!(next["responseSnapshot"].is_null(), restarting);
            if restarting {
                assert_eq!(next["responseCharOffset"], 0);
            }
        }
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
                response_scope: None,
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

    #[test]
    fn rows_scope_splits_nested_arrays_into_complete_json_pages() {
        let matches = (0..200)
            .map(|line| json!({"line":line,"value":"x".repeat(40)}))
            .collect::<Vec<_>>();
        let structured = json!({"results":[{"index":0,"data":{
            "stats":{"total":400},
            "files":[{"path":"a","matches":matches.clone()},{"path":"b","matches":matches}]
        }}]});
        let options = ResponsePageOptions {
            response_char_length: Some(2000),
            response_scope: Some("rows".into()),
            ..Default::default()
        };
        let full = structured.to_string();
        let (first, pagination) =
            paginate_rows(structured.as_object().unwrap().clone(), &full, &options);
        assert!(pagination.total_pages > 2 && pagination.has_more);
        let row = &first["results"][0];
        assert_eq!(
            row["data"]["stats"]["total"], 400,
            "non-split fields are kept"
        );
        assert_eq!(row["data"]["files"].as_array().unwrap().len(), 1);
        assert!(json_chars(&Value::Object(first.clone())) <= 2000 + 200);
        let mut seen = 0;
        for page in 0..pagination.total_pages {
            let options = ResponsePageOptions {
                response_char_offset: Some(page),
                response_snapshot: Some(pagination.snapshot.clone()),
                ..options.clone()
            };
            let (envelope, _) =
                paginate_rows(structured.as_object().unwrap().clone(), &full, &options);
            for row in envelope["results"].as_array().unwrap() {
                for file in row["data"]["files"].as_array().unwrap() {
                    seen += file["matches"].as_array().unwrap().len();
                }
            }
        }
        assert_eq!(seen, 400, "every match appears exactly once");
        let stale = ResponsePageOptions {
            response_char_offset: Some(1),
            response_snapshot: Some("response-rows-v1:stale".into()),
            ..options
        };
        let (_, restart) = paginate_rows(structured.as_object().unwrap().clone(), &full, &stale);
        assert_eq!(restart.restart, Some(true));
        assert_eq!(restart.next_char_offset, Some(0));
    }
}
