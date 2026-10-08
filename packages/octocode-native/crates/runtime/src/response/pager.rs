//! The response pager: text windows, structured windows, and row-aware
//! pages (`row_pages`) of the structured envelope, with their
//! executable continuations.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::row_pages::paginate_rows;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponsePageOptions {
    pub response_offset: Option<usize>,
    pub response_length: Option<usize>,
    pub response_snapshot: Option<String>,
    /// Opt-in: `"structured"` windows the serialized structuredContent
    /// envelope instead of the rendered text (default `None`/`"text"`).
    pub response_scope: Option<String>,
    /// Host-set, never read from a query: render the text channel. Unset,
    /// only MCP renders it.
    #[serde(skip_deserializing)]
    pub render_text: Option<bool>,
}

impl ResponsePageOptions {
    pub fn structured_scope(&self) -> bool {
        self.response_scope.as_deref() == Some("structured") && self.response_length.is_some()
    }

    /// Row-aware paging of structuredContent: every page is a complete JSON
    /// envelope holding whole result rows (or whole elements of a split row).
    pub fn rows_scope(&self) -> bool {
        self.response_scope.as_deref() == Some("rows") && self.response_length.is_some()
    }

    pub(crate) fn explicit(&self) -> bool {
        self.response_length.is_some()
            || self.response_offset.is_some()
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
            self.response_length = Some(budget);
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
    #[serde(rename = "offset")]
    pub char_offset: usize,
    #[serde(rename = "length")]
    pub char_length: usize,
    pub total_chars: usize,
    pub snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
    #[serde(rename = "nextOffset", skip_serializing_if = "Option::is_none")]
    pub next_char_offset: Option<usize>,
    /// Set when this page exceeds the requested length because a fragment
    /// could not be divided further (one string or scalar, or a row skeleton).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oversized: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<ResponseContinuation>,
}

impl ResponsePagination {
    /// Envelope pagination is next-call input only while something remains:
    /// another page, a restart, or a changed snapshot. A finished response
    /// carries none.
    fn is_actionable(&self) -> bool {
        self.has_more
            || self.restart == Some(true)
            || self.changed == Some(true)
            || self.next.is_some()
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ResponseContinuation {
    pub tool: String,
    pub query: Value,
}

/// The contract's `responseLength` maximum (pinned by a contracts test): a
/// continuation the pager builds must stay within it.
const MAX_RESPONSE_LENGTH: usize = 50_000;

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
    StructuredContentMustBeObject,
    Unserializable,
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
        mut input: ResponseInput,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        let Value::Object(structured) = std::mem::take(&mut input.structured) else {
            return Err(ResponseError::StructuredContentMustBeObject);
        };
        if input.options.rows_scope() {
            return self.prepare_rows(structured, &input, cancelled);
        }
        if input.options.structured_scope() {
            return self.prepare_window(structured, &input, cancelled);
        }
        match input.rendered_text.take() {
            Some(text) => self.prepare_text(structured, text, &input, cancelled),
            None => Ok(PreparedResponse {
                content: Vec::new(),
                structured_content: Value::Object(structured),
                is_error: input.is_error,
            }),
        }
    }

    /// Row-aware pages: each page is a complete envelope of whole rows (or
    /// parts of split rows), and the text channel carries the same JSON.
    fn prepare_rows(
        &self,
        mut structured: Map<String, Value>,
        input: &ResponseInput,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        strip_row_telemetry(&mut structured);
        // Row pages re-attach per-call provider facts to the row they came
        // from; the snapshot and page plan never see them.
        let volatile = take_volatile_fields(&mut structured);
        // Same bytes as the Value form, without cloning the envelope first.
        // Every page is bounded by its budget, so any envelope size pages.
        let full = serde_json::to_string(&structured).map_err(|_| ResponseError::Unserializable)?;
        let (mut envelope, mut pagination) =
            paginate_rows(structured, &full, &input.options, volatile);
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        pagination.next = continuation(input, &pagination);
        if pagination.is_actionable() {
            envelope.insert("responsePagination".into(), pagination_value(&pagination)?);
        }
        let text = Value::Object(envelope.clone()).to_string();
        Ok(text_response(text, envelope, input.is_error))
    }

    /// Opt-in structured windowing: page the serialized envelope itself so
    /// MCP clients can stream a large structuredContent. The window text
    /// carries no page header — concatenating `responseWindow` pages in
    /// order reconstructs the exact envelope JSON.
    fn prepare_window(
        &self,
        mut structured: Map<String, Value>,
        input: &ResponseInput,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        strip_transient_telemetry(&mut structured);
        let full = Value::Object(structured).to_string();
        let page = paginate_units(&full, &input.options, false);
        if cancelled.is_cancelled() {
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
            pagination.next = continuation(input, &pagination);
            if pagination.is_actionable() {
                windowed.insert("responsePagination".into(), pagination_value(&pagination)?);
            }
        }
        Ok(text_response(page.text, windowed, input.is_error))
    }

    /// A window of the rendered text; structuredContent mirrors the window
    /// unless one page covers everything.
    fn prepare_text(
        &self,
        mut structured: Map<String, Value>,
        text: String,
        input: &ResponseInput,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        if text.len() > self.config.max_rendered_bytes {
            // Past the ceiling, serve whole-row pages (each at most the
            // requested length, else the ceiling) with a `responseScope:
            // "rows"` continuation instead of failing the call (M9).
            // A row larger than a page (rows split arrays, never a string)
            // falls back to concatenable envelope windows instead.
            drop(text);
            let length = input
                .options
                .response_length
                .unwrap_or(self.config.max_rendered_bytes)
                .min(MAX_RESPONSE_LENGTH);
            let oversized_row = structured
                .get("results")
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        serde_json::to_string(row).map_or(true, |row| row.len() > length)
                    })
                });
            let mut paged = input.clone();
            paged.options.response_length = Some(length);
            paged.options.response_scope =
                Some(if oversized_row { "structured" } else { "rows" }.into());
            return if oversized_row {
                self.prepare_window(structured, &paged, cancelled)
            } else {
                self.prepare_rows(structured, &paged, cancelled)
            };
        }
        let page = paginate_text(&text, &input.options);
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        if let Some(mut pagination) = page.pagination {
            pagination.next = continuation(input, &pagination);
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
                // MCP clients that read only structuredContent (and CLI JSON,
                // which has one channel) must still see this page's window.
                structured.insert("responseWindow".into(), json!(page.text));
            }
            if pagination.is_actionable() {
                structured.insert("responsePagination".into(), pagination_value(&pagination)?);
            }
        }
        Ok(text_response(page.text, structured, input.is_error))
    }
}

/// One text content item beside `structured`.
fn text_response(text: String, structured: Map<String, Value>, is_error: bool) -> PreparedResponse {
    PreparedResponse {
        content: vec![TextContent {
            r#type: "text".into(),
            text,
        }],
        structured_content: Value::Object(structured),
        is_error,
    }
}

/// The executable next-page call of `input` for `pagination`.
fn continuation(
    input: &ResponseInput,
    pagination: &ResponsePagination,
) -> Option<ResponseContinuation> {
    build_continuation(&input.tool, &input.query, &input.options, pagination)
}

fn pagination_value(pagination: &ResponsePagination) -> Result<Value, ResponseError> {
    serde_json::to_value(pagination).map_err(|_| ResponseError::Unserializable)
}

/// Per-row fields that describe how a call was served (e.g. cache warmth),
/// not the evidence. A paged response drops them so a copied continuation
/// served from a warmer cache keeps the same snapshot, page plan, and bytes.
const TRANSIENT_ROW_FIELDS: &[&str] = &["cache"];

/// Row `data` fields that differ on every execution of the same request:
/// provider request ids and rate-limit state (remaining, reset, retry-after).
const VOLATILE_DATA_FIELDS: &[&str] = &["requestId", "rateLimit", "retryAfterSeconds"];

fn rows_mut(structured: &mut Map<String, Value>) -> impl Iterator<Item = &mut Map<String, Value>> {
    structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
}

fn strip_row_telemetry(structured: &mut Map<String, Value>) {
    for row in rows_mut(structured) {
        for field in TRANSIENT_ROW_FIELDS {
            row.remove(*field);
        }
    }
}

/// Remove transient telemetry and per-call provider facts from an envelope
/// whose bytes are windowed: every page must hash and slice identical bytes.
pub(crate) fn strip_transient_telemetry(structured: &mut Map<String, Value>) {
    strip_row_telemetry(structured);
    take_volatile_fields(structured);
}

/// Move each row's per-call provider facts out of the envelope, by row.
fn take_volatile_fields(structured: &mut Map<String, Value>) -> Vec<Option<Map<String, Value>>> {
    rows_mut(structured)
        .map(|row| {
            let data = row.get_mut("data").and_then(Value::as_object_mut)?;
            let taken: Map<String, Value> = VOLATILE_DATA_FIELDS
                .iter()
                .filter_map(|field| {
                    data.remove(*field)
                        .map(|value| ((*field).to_owned(), value))
                })
                .collect();
            (!taken.is_empty()).then_some(taken)
        })
        .collect()
}

struct Page {
    text: String,
    pagination: Option<ResponsePagination>,
}

fn paginate_text(text: &str, options: &ResponsePageOptions) -> Page {
    paginate_units(text, options, true)
}

fn paginate_units(text: &str, options: &ResponsePageOptions, with_header: bool) -> Page {
    let Some(requested_length) = options.response_length else {
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
    let requested_offset = options.response_offset.unwrap_or(0);
    let offset = requested_offset.min(total);
    let changed = options.response_snapshot.as_deref() != Some(&snapshot);
    let invalid = splits_pair(&units, offset);
    if requested_offset > 0 && (changed || invalid) {
        let reason = if invalid && !changed {
            "The requested offset splits a Unicode code point. Restart from responseOffset=0 and follow the returned continuation."
        } else if options.response_snapshot.is_some() {
            "The full response changed since the previous page. Discard earlier pages and restart from responseOffset=0."
        } else {
            "Later response pages require responseSnapshot from the previous page. Restart from responseOffset=0."
        };
        return Page {
            text: format!("# Response pagination restart required. {reason}\n"),
            pagination: Some(restart_pagination(
                "content.text",
                offset,
                total_pages(&units, length),
                total,
                snapshot,
                options,
                changed,
            )),
        };
    }
    let end = choose_end(&units, offset, length);
    let has_more = end < total;
    let current = page_number(&units, offset, length);
    let pages = total_pages(&units, length);
    // `choose_end` never splits a UTF-16 surrogate pair, so this is lossless.
    let body = String::from_utf16_lossy(&units[offset..end]);
    let header = if !with_header {
        String::new()
    } else if has_more {
        format!("# Response page {current}/{pages}. Next: responseOffset={end}\n")
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
            oversized: None,
            next: None,
        }),
    }
}

/// A restart page: the request cannot continue the response it names (its
/// snapshot changed or is missing, or its offset is invalid). The caller
/// resumes from offset 0.
pub(super) fn restart_pagination(
    scope: &str,
    offset: usize,
    total_pages: usize,
    total_chars: usize,
    snapshot: String,
    options: &ResponsePageOptions,
    changed: bool,
) -> ResponsePagination {
    let expected = options.response_snapshot.clone();
    ResponsePagination {
        scope: scope.into(),
        current_page: 1,
        total_pages,
        has_more: true,
        char_offset: offset,
        char_length: 0,
        total_chars,
        snapshot,
        changed: Some(expected.is_some() && changed),
        expected_snapshot: expected,
        restart: Some(true),
        next_char_offset: Some(0),
        oversized: None,
        next: None,
    }
}

fn build_continuation(
    tool: &str,
    query: &Value,
    request: &ResponsePageOptions,
    page: &ResponsePagination,
) -> Option<ResponseContinuation> {
    let next_offset = page.next_char_offset.filter(|_| page.has_more)?;
    let queries = query
        .get("queries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![query.clone()]);
    // The runtime passes a flat query for one row and an envelope for a batch.
    // Keep the rows flat so the continuation can be submitted unchanged.
    let mut continuation = Map::new();
    continuation.insert("queries".into(), Value::Array(queries));
    if let Some(length) = request.response_length {
        continuation.insert("responseLength".into(), json!(length));
    }
    if let Some(scope) = &request.response_scope {
        continuation.insert("responseScope".into(), json!(scope));
    }
    continuation.insert("responseOffset".into(), json!(next_offset));
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
            response_length: Some(length),
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
                        response_length: Some(length),
                        response_offset: Some(offset),
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
                response_length: Some(1),
                response_offset: Some(2),
                response_snapshot: Some(snapshot),
                response_scope: None,
                render_text: None,
            },
        );
        assert_eq!(split.pagination.expect("page").restart, Some(true));
        let changed = paginate_text(
            "changed",
            &ResponsePageOptions {
                response_length: Some(2),
                response_offset: Some(2),
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
            response_length: Some(length),
            response_offset: Some(offset),
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
                        query: json!({"path":"a","mainGoal": "test", "reasoning":"r","debug":false}),
                        structured: envelope.clone(),
                        rendered_text: None,
                        is_error: false,
                        options: structured_options(40, offset, snapshot.clone()),
                    },
                    &CancellationToken::new(),
                )
                .expect("page");
            let out = prepared.structured_content;
            // The windowed envelope stays schema-valid: results present.
            assert_eq!(out["results"], json!([]));
            let window = out["responseWindow"].as_str().expect("window");
            // Window text carries no page header; content mirrors it.
            assert_eq!(prepared.content[0].text, window);
            joined.push_str(window);
            // The final window carries no pagination: nothing remains.
            let Some(pagination) = out.get("responsePagination") else {
                break;
            };
            assert_eq!(pagination["scope"], "structuredContent");
            assert_eq!(pagination["hasMore"], true, "{pagination}");
            snapshot = Some(
                pagination["snapshot"]
                    .as_str()
                    .expect("snapshot")
                    .to_owned(),
            );
            // The continuation is executable and keeps the opt-in scope.
            let next = &pagination["next"]["query"];
            assert_eq!(next["responseScope"], "structured");
            offset = pagination["nextOffset"].as_u64().expect("offset") as usize;
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
                    query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                    structured: json!({"results":[{"index":0,"data":{"content":"body"}}]}),
                    rendered_text: None,
                    is_error: false,
                    options: structured_options(10, 5, Some("response-v1:stale".into())),
                },
                &CancellationToken::new(),
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
                    query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                    structured: envelope.clone(),
                    rendered_text: None,
                    is_error: false,
                    options: ResponsePageOptions::default(),
                },
                &CancellationToken::new(),
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
                    query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                    structured: envelope.clone(),
                    rendered_text: Some("body".into()),
                    is_error: false,
                    options: ResponsePageOptions {
                        response_length: Some(1_000),
                        ..Default::default()
                    },
                },
                &CancellationToken::new(),
            )
            .expect("page");
        assert_eq!(prepared.structured_content["results"], envelope["results"]);
        // A finished response carries no envelope pagination.
        assert!(
            prepared
                .structured_content
                .get("responsePagination")
                .is_none(),
            "{}",
            prepared.structured_content
        );
    }

    /// MCP hosts that surface only structuredContent must still receive the
    /// page: a split text page carries its window in `responseWindow`.
    #[test]
    fn a_split_text_page_carries_its_window_in_structured_content() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
        let text = "line1\nline2\nline3\nline4\n";
        let mut options = options(12);
        let mut joined = String::new();
        loop {
            let prepared = pager
                .prepare(
                    ResponseInput {
                        tool: "localFetch".into(),
                        query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                        structured: envelope.clone(),
                        rendered_text: Some(text.into()),
                        is_error: false,
                        options: options.clone(),
                    },
                    &CancellationToken::new(),
                )
                .expect("page");
            let structured = &prepared.structured_content;
            assert_eq!(structured["results"], json!([]));
            let window = structured["responseWindow"].as_str().expect("window");
            assert_eq!(window, prepared.content[0].text);
            joined.push_str(window.split_once('\n').map_or("", |(_, body)| body));
            let pagination = &structured["responsePagination"];
            if pagination["hasMore"] != true {
                break;
            }
            options.response_offset = pagination["nextOffset"].as_u64().map(|v| v as usize);
            options.response_snapshot = pagination["snapshot"].as_str().map(str::to_owned);
        }
        assert_eq!(joined, text);
    }

    #[test]
    fn prepares_both_channels_and_preserves_required_debug_state() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let result = pager
            .prepare(
                ResponseInput {
                    tool: "localFetch".into(),
                    query: json!({"path":"a", "mainGoal":"g", "reasoning":"r", "debug":true}),
                    structured: json!({"results":[]}),
                    rendered_text: Some("line1\nline2\nline3".into()),
                    is_error: false,
                    options: options(8),
                },
                &CancellationToken::new(),
            )
            .expect("page");
        assert_eq!(result.content.len(), 1);
        let next = &result.structured_content["responsePagination"]["next"]["query"];
        // A continuation is the complete input: `{queries:[row]}`.
        assert_eq!(
            next["queries"][0],
            json!({"path":"a", "mainGoal":"g", "reasoning":"r", "debug":true})
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
            {"path":"/repo/a.ts", "mainGoal": "test", "reasoning":"read both", "debug":true, "fullContent":true},
            {"path":"/repo/b.ts", "mainGoal": "test", "reasoning":"read both", "debug":false, "ranges":["2-5"]}
        ]);
        for options in [
            options(8),
            ResponsePageOptions {
                response_length: Some(8),
                response_offset: Some(8),
                response_snapshot: Some("response-v1:stale".into()),
                response_scope: None,
                render_text: None,
            },
        ] {
            let restarting = options.response_offset.is_some();
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
                    &CancellationToken::new(),
                )
                .expect("page");
            crate::contracts::validate_output("localFetch", &result.structured_content)
                .expect("continuation must satisfy the public output contract");
            let next = &result.structured_content["responsePagination"]["next"]["query"];
            assert_eq!(next["queries"], queries);
            let prepared = crate::contracts::prepare_many_and_validate("localFetch", next.clone())
                .expect("continuation must be accepted as a new tool call");
            assert_eq!(prepared.len(), 2);
            for (prepared, original) in prepared.iter().zip(queries.as_array().expect("queries")) {
                for (field, value) in original.as_object().expect("query") {
                    assert_eq!(&prepared[field], value);
                }
            }
            assert_eq!(next["responseLength"], 8);
            assert_eq!(next["responseSnapshot"].is_null(), restarting);
            if restarting {
                assert_eq!(next["responseOffset"], 0);
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
                response_length: Some(10),
                response_offset: Some(10),
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
                response_length: Some(5),
                response_offset: Some(500),
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
    fn cancellation_fails_before_cache_growth() {
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
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(
            pager.prepare(input, &cancelled),
            Err(ResponseError::Cancelled)
        ));
    }

    /// M9: rendered text past the byte ceiling is served as row pages with an
    /// executable continuation, never a whole-call failure; walking the pages
    /// returns every row exactly once.
    #[test]
    fn oversized_rendered_text_falls_back_to_row_pages() {
        let pager = ResponsePager::new(ResponsePagerConfig {
            max_rendered_bytes: 600,
        });
        let rows: Vec<Value> = (0..6)
            .map(|index| json!({"index": index, "data": {"content": format!("{index}:{}", "x".repeat(200))}}))
            .collect();
        let structured = json!({"results": rows});
        let text = "y".repeat(2_000);
        let mut request = ResponsePageOptions::default();
        let mut seen = Vec::new();
        for _ in 0..20 {
            let prepared = pager
                .prepare(
                    ResponseInput {
                        tool: "localFetch".into(),
                        query: json!({"queries": [{"path": "a"}]}),
                        structured: structured.clone(),
                        rendered_text: Some(text.clone()),
                        is_error: false,
                        options: request.clone(),
                    },
                    &CancellationToken::new(),
                )
                .expect("an oversized response pages instead of failing");
            let page = prepared.structured_content;
            for row in page["results"].as_array().expect("rows") {
                seen.push(row["index"].as_u64().expect("index"));
            }
            let text_len = prepared.content.iter().map(|c| c.text.len()).sum::<usize>();
            assert!(text_len <= 2 * 600, "page text stays bounded: {text_len}");
            let Some(next) = page["responsePagination"]["next"]["query"].as_object() else {
                break;
            };
            assert_eq!(next["responseScope"], "rows", "{next:?}");
            request = serde_json::from_value(Value::Object(next.clone())).expect("options");
        }
        assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
    }

    /// M9: one row too large for any row page falls back to envelope
    /// windows that concatenate back to the exact envelope JSON.
    #[test]
    fn oversized_single_row_falls_back_to_concatenable_windows() {
        let pager = ResponsePager::new(ResponsePagerConfig {
            max_rendered_bytes: 600,
        });
        let structured = json!({"results": [{"index": 0, "data": {"content": "z".repeat(2_000)}}]});
        let mut request = ResponsePageOptions::default();
        let mut joined = String::new();
        for _ in 0..50 {
            let prepared = pager
                .prepare(
                    ResponseInput {
                        tool: "localFetch".into(),
                        query: json!({"queries": [{"path": "a"}]}),
                        structured: structured.clone(),
                        rendered_text: Some("y".repeat(2_000)),
                        is_error: false,
                        options: request.clone(),
                    },
                    &CancellationToken::new(),
                )
                .expect("pages");
            let page = prepared.structured_content;
            joined.push_str(page["responseWindow"].as_str().expect("window"));
            let Some(next) = page["responsePagination"]["next"]["query"].as_object() else {
                break;
            };
            assert_eq!(next["responseScope"], "structured", "{next:?}");
            request = serde_json::from_value(Value::Object(next.clone())).expect("options");
        }
        assert_eq!(
            serde_json::from_str::<Value>(&joined).expect("windows join"),
            structured
        );
    }

    /// Cold call, then the copied continuation served from a warm cache: the
    /// per-row `cache` telemetry must not read as a source change.
    #[test]
    fn cache_warmth_does_not_restart_a_copied_continuation() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let envelope = |warm: bool| {
            let rows = (0..3)
                .map(|index| {
                    let mut row = json!({"index":index,"data":{"content":"x".repeat(300)}});
                    if warm {
                        row["cache"] = json!(1);
                    }
                    row
                })
                .collect::<Vec<_>>();
            json!({"shared":{"commitSha":"abc"},"results":rows})
        };
        for scope in ["rows", "structured"] {
            let prepare = |warm: bool, offset: Option<usize>, snapshot: Option<String>| {
                pager
                    .prepare(
                        ResponseInput {
                            tool: "ghGetFileContent".into(),
                            query: json!({"queries":[{"path":"a"}]}),
                            structured: envelope(warm),
                            rendered_text: None,
                            is_error: false,
                            options: ResponsePageOptions {
                                response_length: Some(400),
                                response_offset: offset,
                                response_snapshot: snapshot,
                                response_scope: Some(scope.into()),
                                render_text: None,
                            },
                        },
                        &CancellationToken::new(),
                    )
                    .expect("page")
                    .structured_content
            };
            let cold = prepare(false, None, None);
            let pagination = &cold["responsePagination"];
            assert_eq!(pagination["hasMore"], true, "{scope}: {cold}");
            let next = pagination["next"]["query"].clone();
            let warm = prepare(
                true,
                next["responseOffset"].as_u64().map(|x| x as usize),
                next["responseSnapshot"].as_str().map(str::to_owned),
            );
            let page = &warm["responsePagination"];
            assert!(page.get("restart").is_none(), "{scope}: {warm}");
            assert!(page.get("changed").is_none(), "{scope}: {warm}");
            assert_eq!(page["snapshot"], pagination["snapshot"], "{scope}");
            assert!(!warm.to_string().contains("\"cache\""), "{scope}: {warm}");
        }
    }

    /// B1: two executions of a batch with a GitHub error row differ only in
    /// the provider's per-call facts; the copied continuation must page on.
    #[test]
    fn provider_request_ids_do_not_restart_a_copied_continuation() {
        let pager = ResponsePager::new(ResponsePagerConfig::default());
        let envelope = |call: usize| {
            let mut rows = (0..3)
                .map(|index| json!({"index":index,"data":{"content":"x".repeat(300)}}))
                .collect::<Vec<_>>();
            rows.push(json!({"index":3,"status":"error","data":{
                "error":"Repository, resource, or path not found","errorCode":"notFound",
                "httpStatus":404,
                "requestId":format!("{call:X}F4:376635:800A3C:A22BF9:6ABD2D60"),
                "rateLimit":{"remaining":100_000 + call,"resetEpochSeconds":1_700_000_000 + call,"retryAfterSeconds":call},
                "retryAfterSeconds":call}}));
            json!({"results":rows})
        };
        for scope in ["rows", "structured"] {
            let prepare = |call: usize, offset: Option<usize>, snapshot: Option<String>| {
                pager
                    .prepare(
                        ResponseInput {
                            tool: "ghGetFileContent".into(),
                            query: json!({"queries":[{"path":"a"}]}),
                            structured: envelope(call),
                            rendered_text: None,
                            is_error: false,
                            options: ResponsePageOptions {
                                response_length: Some(400),
                                response_offset: offset,
                                response_snapshot: snapshot,
                                response_scope: Some(scope.into()),
                                render_text: None,
                            },
                        },
                        &CancellationToken::new(),
                    )
                    .expect("page")
                    .structured_content
            };
            let mut offset = None;
            let mut snapshot = None;
            let mut error_rows = Vec::new();
            for call in 1..20 {
                let page = prepare(call * 4096, offset, snapshot.clone());
                let pagination = &page["responsePagination"];
                assert!(
                    pagination.get("restart").is_none(),
                    "{scope} call {call}: {page}"
                );
                error_rows.extend(
                    page["results"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|row| row["status"] == "error")
                        .cloned(),
                );
                if pagination["hasMore"] != true {
                    break;
                }
                let next = &pagination["next"]["query"];
                offset = next["responseOffset"].as_u64().map(|x| x as usize);
                snapshot = next["responseSnapshot"].as_str().map(str::to_owned);
            }
            if scope == "rows" {
                // Row pages keep the facts of the execution that served them.
                assert_eq!(error_rows.len(), 1, "{error_rows:?}");
                assert!(error_rows[0]["data"]["requestId"].is_string());
                assert!(error_rows[0]["data"]["rateLimit"]["remaining"].is_number());
            }
        }
    }
}
