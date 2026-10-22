//! The response pager: text windows, structured windows, and row-aware
//! pages (`row_pages`) of the structured envelope, with their
//! executable continuations.

use base64::Engine as _;
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tokio_util::sync::CancellationToken;

use super::row_pages::RowSource;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponsePageOptions {
    pub(crate) response_offset: Option<usize>,
    pub(crate) response_length: Option<usize>,
    pub(crate) response_snapshot: Option<String>,
    /// Opt-in: `"structured"` windows the serialized structuredContent
    /// envelope instead of the rendered text (default `None`/`"text"`).
    pub(crate) response_scope: Option<String>,
    /// Host-set, never read from a query: render the text channel. Unset,
    /// only MCP renders it.
    #[serde(skip_deserializing)]
    pub(crate) render_text: Option<bool>,
    #[serde(skip)]
    pub(crate) saved: Option<SavedResponse>,
}

#[derive(Clone, Debug)]
pub(crate) struct SavedResponse {
    pub key: [u8; 32],
    pub expires_at: String,
    pub storage: &'static str,
    pub replayed: bool,
}

impl SavedResponse {
    fn snapshot(&self, raw: &str) -> String {
        let (kind, digest) = raw.rsplit_once(':').unwrap_or((raw, ""));
        let digest = hex::decode(digest).unwrap_or_default();
        let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!(
            "saved-v1:{}:{kind}:{}",
            base64.encode(self.key),
            base64.encode(digest)
        )
    }
}

impl ResponsePageOptions {
    pub(crate) fn structured_scope(&self) -> bool {
        self.response_scope.as_deref() == Some("structured") && self.response_length.is_some()
    }

    /// Row-aware paging of structuredContent: every page is a complete JSON
    /// envelope holding whole result rows (or whole elements of a split row).
    pub(crate) fn rows_scope(&self) -> bool {
        self.response_scope.as_deref() == Some("rows") && self.response_length.is_some()
    }

    /// The caller sized the response page: `responseLength` is set. An
    /// offset or snapshot alone continues on the automatic page, so it never
    /// lifts it (nor the read share sized from it).
    pub(crate) fn explicit(&self) -> bool {
        self.response_length.is_some()
    }

    /// The caller chose what the page windows: `responseLength` with a
    /// `responseScope`. A length alone sizes whole-row pages
    /// ([`Self::auto_paginate`]); only `responseScope:"text"` windows the
    /// rendered text.
    pub(crate) fn scoped(&self) -> bool {
        self.explicit() && self.response_scope.is_some()
    }

    /// Page whole rows at the caller's `responseLength`, else at the
    /// runtime's automatic page `auto_page` (0: never), unless the caller
    /// chose a scope ([`Self::scoped`]). Rows keep both text and
    /// structuredContent carrying the page: MCP clients that read only
    /// structuredContent would otherwise see an emptied `results`.
    /// The envelope is sized first: one past the budget pages without
    /// drawing its text; otherwise `render` draws it, and a text past the
    /// budget pages too. Returns the text the response still needs (none for
    /// row pages, which carry their own JSON).
    pub(crate) fn auto_paginate(
        &mut self,
        structured: &Value,
        auto_page: usize,
        render: impl FnOnce() -> Option<Arc<String>>,
    ) -> Option<Arc<String>> {
        if self.scoped() || auto_page == 0 {
            return render();
        }
        let budget = self.response_length.unwrap_or(auto_page);
        if octocode_engine::json_size::utf16_units(structured).unwrap_or(0) > budget {
            self.page_rows(budget);
            return None;
        }
        let text = render();
        if text
            .as_deref()
            .is_some_and(|text| text.encode_utf16().count() > budget)
        {
            self.page_rows(budget);
            return None;
        }
        // One page holds the response: nothing windows it.
        self.response_length = None;
        text
    }

    fn page_rows(&mut self, budget: usize) {
        self.response_scope = Some("rows".into());
        self.response_length = Some(budget);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResponseInput {
    pub tool: String,
    pub query: Value,
    pub structured: Value,
    pub(crate) rendered_text: Option<Arc<String>>,
    pub is_error: bool,
    pub options: ResponsePageOptions,
    /// Each result row's remaining-page notes (by position), so a split
    /// row's parts re-render their disclosure
    /// ([`super::pages::take_remaining_notes`]).
    pub remaining: Vec<Vec<super::pages::Remaining>>,
}

/// A saved response's page memo: the text drawn for it, and the source its
/// pages cut from, built from the saved envelope on the first read and shared
/// by every later one, so no page read copies or re-plans the envelope.
#[derive(Default)]
pub(crate) struct SavedPages {
    text: Mutex<Option<(bool, Arc<String>)>>,
    source: OnceLock<SavedSource>,
}

enum SavedSource {
    Rows(Box<RowSource>),
    /// The serialized envelope a structured window slices.
    Window(String),
    /// The text a text window slices, and the envelope it renders.
    Text {
        envelope: Map<String, Value>,
        text: Arc<String>,
    },
}

impl SavedPages {
    /// The envelope's text with or without transient telemetry
    /// (`stripped`), drawn by `render` only the first time.
    pub(crate) fn text(&self, stripped: bool, render: impl FnOnce() -> String) -> Arc<String> {
        let mut cached = self.text.lock().unwrap_or_else(PoisonError::into_inner);
        match cached.as_ref() {
            Some((was_stripped, text)) if *was_stripped == stripped => Arc::clone(text),
            _ => {
                let text = Arc::new(render());
                *cached = Some((stripped, Arc::clone(&text)));
                text
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn has_text(&self) -> bool {
        self.text
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub r#type: String,
    pub text: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedResponse {
    pub content: Vec<TextContent>,
    pub structured_content: Value,
    pub is_error: bool,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponsePagination {
    pub scope: String,
    pub(crate) current_page: usize,
    pub(crate) total_pages: usize,
    pub(crate) has_more: bool,
    #[serde(rename = "offset")]
    pub(crate) char_offset: usize,
    #[serde(rename = "length")]
    pub char_length: usize,
    pub total_chars: usize,
    pub snapshot: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
    #[serde(rename = "nextOffset", skip_serializing_if = "Option::is_none")]
    pub(crate) next_char_offset: Option<usize>,
    /// Set when this page exceeds the requested length because a fragment
    /// could not be divided further (one string or scalar, or a row skeleton).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oversized: Option<bool>,
    /// The next page's call, built by [`crate::response::result::Continuation`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Value>,
}

impl ResponsePagination {
    /// Keep remaining-page input and indivisible-size disclosures visible.
    /// A finished response that fits its window carries neither.
    fn should_emit(&self) -> bool {
        self.has_more
            || self.restart == Some(true)
            || self.changed == Some(true)
            || self.next.is_some()
            || self.oversized == Some(true)
    }
}

/// `tool`'s contract `responseLength` maximum: a continuation the pager
/// builds must stay within it. A tool without the envelope field has no
/// bound to keep.
fn max_response_length(tool: &str) -> usize {
    crate::contracts::tool_contract_named(tool)
        .and_then(Result::ok)
        .and_then(|contract| {
            let input = &contract["inputSchema"];
            crate::contracts::property_schemas(input, &input["$defs"], "responseLength")
                .into_iter()
                .filter_map(|schema| schema["maximum"].as_u64())
                .min()
        })
        .and_then(|maximum| usize::try_from(maximum).ok())
        .unwrap_or(usize::MAX)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResponsePagerConfig {
    pub(crate) max_rendered_bytes: usize,
}

impl Default for ResponsePagerConfig {
    fn default() -> Self {
        Self {
            max_rendered_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResponseError {
    Cancelled,
    StructuredContentMustBeObject,
    Unserializable,
}

pub(crate) struct ResponsePager {
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

    /// The page of a saved response `input` asks for, cut from `pages`
    /// (built from `envelope` on the first read). `render` draws the text a
    /// text window slices, unless the first page already drew it.
    pub(crate) fn prepare_saved(
        &self,
        input: &ResponseInput,
        envelope: &Value,
        pages: &SavedPages,
        render: impl FnOnce(&Value) -> String,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        let source = match pages.source.get() {
            Some(source) => source,
            None => {
                let built = saved_source(envelope, input, pages, render)?;
                pages.source.get_or_init(|| built)
            }
        };
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        match source {
            SavedSource::Rows(rows) => cut_rows(rows, input),
            SavedSource::Window(full) => cut_window(full, input),
            SavedSource::Text { envelope, text } => {
                if text.len() > self.config.max_rendered_bytes {
                    return self.prepare_text(envelope.clone(), Arc::clone(text), input, cancelled);
                }
                let rows_key = rows_key(&input.tool);
                text_page(paginate_text(text, &input.options), input, |whole| {
                    if whole {
                        return envelope.clone();
                    }
                    // A window's envelope drops what it windows: copy the rest.
                    envelope
                        .iter()
                        .filter(|(key, _)| key.as_str() != "shared")
                        .map(|(key, value)| {
                            let value = if key == rows_key {
                                Value::Array(Vec::new())
                            } else {
                                value.clone()
                            };
                            (key.clone(), value)
                        })
                        .collect()
                })
            }
        }
    }

    /// Row-aware pages: each page is a complete envelope of whole rows (or
    /// parts of split rows), and the text channel carries the same JSON.
    fn prepare_rows(
        &self,
        structured: Map<String, Value>,
        input: &ResponseInput,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        let source = row_source(structured, input)?;
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        cut_rows(&source, input)
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
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        cut_window(&full, input)
    }

    /// A window of the rendered text; structuredContent mirrors the window
    /// unless one page covers everything.
    fn prepare_text(
        &self,
        structured: Map<String, Value>,
        text: Arc<String>,
        input: &ResponseInput,
        cancelled: &CancellationToken,
    ) -> Result<PreparedResponse, ResponseError> {
        if text.len() > self.config.max_rendered_bytes {
            // Past the ceiling, serve whole-row pages (each at most the
            // requested length, else the ceiling) with a `responseScope:
            // "rows"` continuation instead of failing the call.
            // A row larger than a page (rows split arrays, never a string)
            // falls back to concatenable envelope windows instead.
            drop(text);
            let length = input
                .options
                .response_length
                .unwrap_or(self.config.max_rendered_bytes)
                .min(max_response_length(&input.tool));
            let oversized_row = structured
                .get(rows_key(&input.tool))
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        octocode_engine::json_size::utf16_units(row).unwrap_or(0) > length
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
        if input.options.response_length.is_none() {
            // Unpaged: the text is the whole response, moved, not copied.
            return Ok(text_response(
                Arc::unwrap_or_clone(text),
                structured,
                input.is_error,
            ));
        }
        let page = paginate_text(&text, &input.options);
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        text_page(page, input, |_| structured)
    }
}

/// The row plan of `structured` at `input`'s page length, without the
/// per-call telemetry a page never carries.
fn row_source(
    mut structured: Map<String, Value>,
    input: &ResponseInput,
) -> Result<RowSource, ResponseError> {
    strip_row_telemetry(&mut structured);
    // Row pages re-attach per-call provider facts to the row they came
    // from; the snapshot and page plan never see them.
    let volatile = take_volatile_fields(&mut structured);
    // Same bytes as the Value form, without cloning the envelope first.
    // Every page is bounded by its budget, so any envelope size pages.
    let full = serde_json::to_string(&structured).map_err(|_| ResponseError::Unserializable)?;
    Ok(RowSource::new(
        structured,
        &full,
        &input.options,
        volatile,
        crate::id::ToolId::from_name(&input.tool),
        &|pagination| pagination_block_chars(input, pagination),
    ))
}

/// What a saved read's pages cut from, built from a copy of the saved
/// envelope: its row plan, its serialized bytes, or its rendered text.
fn saved_source(
    envelope: &Value,
    input: &ResponseInput,
    pages: &SavedPages,
    render: impl FnOnce(&Value) -> String,
) -> Result<SavedSource, ResponseError> {
    let Value::Object(structured) = envelope else {
        return Err(ResponseError::StructuredContentMustBeObject);
    };
    let mut structured = structured.clone();
    if input.options.rows_scope() {
        return Ok(SavedSource::Rows(Box::new(row_source(structured, input)?)));
    }
    strip_transient_telemetry(&mut structured);
    let structured = Value::Object(structured);
    if input.options.structured_scope() {
        return Ok(SavedSource::Window(structured.to_string()));
    }
    let text = pages.text(true, || render(&structured));
    let Value::Object(envelope) = structured else {
        unreachable!("the envelope was built as an object above");
    };
    Ok(SavedSource::Text { envelope, text })
}

/// One row page of `source`, with its pagination block and continuation.
fn cut_rows(source: &RowSource, input: &ResponseInput) -> Result<PreparedResponse, ResponseError> {
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
    let text = serde_json::to_string(&envelope).map_err(|_| ResponseError::Unserializable)?;
    Ok(text_response(text, envelope, input.is_error))
}

/// One structured window of the serialized envelope `full`.
fn cut_window(full: &str, input: &ResponseInput) -> Result<PreparedResponse, ResponseError> {
    let page = paginate_units(full, &input.options, false);
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
        pagination.scope = "structuredContent".into();
        pagination.next = continuation(input, &pagination);
        if pagination.should_emit() || is_saved_replay(&input.options) {
            windowed.insert(
                "responsePagination".into(),
                pagination_value(&pagination, &input.options)?,
            );
        }
    }
    Ok(text_response(page.text, windowed, input.is_error))
}

/// A text window's response. The text page carries this window of the
/// payload; repeating the whole payload in structuredContent would defeat
/// pagination, so `structured(whole)` gives the envelope, and only a single
/// page that covers everything keeps its results.
fn text_page(
    page: Page,
    input: &ResponseInput,
    structured: impl FnOnce(bool) -> Map<String, Value>,
) -> Result<PreparedResponse, ResponseError> {
    let Some(mut pagination) = page.pagination else {
        return Ok(text_response(page.text, structured(true), input.is_error));
    };
    pagination.next = continuation(input, &pagination);
    let whole = pagination.char_offset == 0 && !pagination.has_more && pagination.restart.is_none();
    let mut structured = structured(whole);
    if !whole {
        if let Some(results) = structured.get_mut(rows_key(&input.tool)) {
            *results = Value::Array(Vec::new());
        }
        structured.remove("shared");
        // MCP clients that read only structuredContent (and CLI JSON,
        // which has one channel) must still see this page's window.
        structured.insert("responseWindow".into(), json!(page.text));
    }
    if pagination.should_emit() || is_saved_replay(&input.options) {
        structured.insert(
            "responsePagination".into(),
            pagination_value(&pagination, &input.options)?,
        );
    }
    Ok(text_response(page.text, structured, input.is_error))
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

/// Chars the `responsePagination` entry adds to a page of `pagination`
/// ([`ResponsePager::prepare_rows`] attaches it with its continuation); 0
/// when the page carries none. Every page of one plan must get the same
/// answer, whichever request builds it, so a saved read is measured as the
/// replay it becomes (a first request and its copies differ only in
/// `replayed`'s value).
fn pagination_block_chars(input: &ResponseInput, pagination: &ResponsePagination) -> usize {
    let mut options = input.options.clone();
    if let Some(saved) = options.saved.as_mut() {
        saved.replayed = true;
    }
    let mut page = pagination.clone();
    page.next = build_continuation(&input.tool, &input.query, &options, &page);
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
    build_continuation(&input.tool, &input.query, &input.options, pagination)
}

fn pagination_value(
    pagination: &ResponsePagination,
    options: &ResponsePageOptions,
) -> Result<Value, ResponseError> {
    let mut value = serde_json::to_value(pagination).map_err(|_| ResponseError::Unserializable)?;
    if let Some(saved) = &options.saved {
        value["snapshot"] = json!(saved.snapshot(&pagination.snapshot));
        if let Some(expected) = &pagination.expected_snapshot {
            value["expectedSnapshot"] = json!(saved.snapshot(expected));
        }
        value["expiresAt"] = json!(saved.expires_at);
        value["storage"] = json!(saved.storage);
        value["replayed"] = json!(saved.replayed);
    }
    Ok(value)
}

fn is_saved_replay(options: &ResponsePageOptions) -> bool {
    options.saved.as_ref().is_some_and(|saved| saved.replayed)
}

fn rows_key(tool: &str) -> &'static str {
    crate::id::ToolId::from_name(tool).map_or("results", |tool| tool.output().rows_key())
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
        octocode_engine::digest::sha256(text.as_bytes())
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
                page_number(&units, offset, length),
                offset,
                total_pages(&units, length),
                total,
                snapshot,
                options,
                changed,
                true,
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
/// snapshot changed or is missing, or its offset is invalid). Its `next`
/// resumes from offset 0. `has_more` is false only for a page past the end
/// of an unchanged response: nothing follows the offset asked for, so a
/// caller walking `next` while `hasMore` stops instead of reading page 1
/// again.
pub(super) fn restart_pagination(
    scope: &str,
    current_page: usize,
    offset: usize,
    total_pages: usize,
    total_chars: usize,
    snapshot: String,
    options: &ResponsePageOptions,
    changed: bool,
    has_more: bool,
) -> ResponsePagination {
    let expected = options.response_snapshot.clone();
    ResponsePagination {
        scope: scope.into(),
        current_page,
        total_pages,
        has_more,
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
) -> Option<Value> {
    let tool = crate::id::ToolId::from_name(tool)?;
    let next_offset = page
        .next_char_offset
        .filter(|_| page.has_more || page.restart == Some(true))?;
    let mut continuation = Map::new();
    if request.saved.is_none() && page.restart == Some(true) {
        let queries = query
            .get("queries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![query.clone()]);
        continuation.insert("queries".into(), Value::Array(queries));
    }
    if let Some(length) = request.response_length {
        continuation.insert("responseLength".into(), json!(length));
    }
    if let Some(scope) = &request.response_scope {
        continuation.insert("responseScope".into(), json!(scope));
    }
    continuation.insert("responseOffset".into(), json!(next_offset));
    if let Some(saved) = &request.saved {
        continuation.insert(
            "responseSnapshot".into(),
            json!(saved.snapshot(&page.snapshot)),
        );
    } else if page.restart != Some(true) {
        continuation.insert("responseSnapshot".into(), json!(page.snapshot));
    }
    Some(crate::response::result::Continuation::input(tool, Value::Object(continuation)).build())
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
mod tests;
