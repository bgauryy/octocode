//! The response pager: row-aware pages (`row_pages`) and concatenable
//! windows of the one JSON envelope, with their executable continuations.
//! Every surface (CLI JSON, CLI views, MCP) reads the same pages.

mod cut;
mod telemetry;
mod window;
use cut::{cut_rows, cut_window, pagination_block_chars};
use serde_json::{Map, Value};
use std::sync::OnceLock;
pub(crate) use telemetry::strip_transient_telemetry;
use telemetry::{rows_key, strip_row_telemetry, take_volatile_fields};
use tokio_util::sync::CancellationToken;
pub(super) use window::restart_pagination;
use window::{build_continuation, cursor_at, paginate_window};

use super::cursor::{Cursor, PageScope};
use super::row_pages::RowSource;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResponsePageOptions {
    /// The page a saved read serves, from its cursor; never read from a
    /// query (the cursor carries it).
    #[serde(skip)]
    pub(crate) response_offset: Option<usize>,
    pub(crate) response_length: Option<usize>,
    /// On a query, the cursor ([`Cursor`]); once opened, the content
    /// snapshot it names.
    pub(crate) response_snapshot: Option<String>,
    /// `"rows"` (the default) pages whole result rows; `"structured"`
    /// windows the serialized envelope into concatenable `responseWindow`s.
    pub(crate) response_scope: Option<PageScope>,
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

impl ResponsePageOptions {
    /// Serve the page `cursor` names: its content snapshot, scope, length
    /// and offset.
    pub(crate) fn open(&mut self, cursor: &Cursor) {
        self.response_snapshot = Some(cursor.content_snapshot());
        self.response_length = Some(cursor.length);
        self.response_scope = Some(cursor.scope);
        self.response_offset = Some(cursor.offset);
    }

    /// These options at the page a `responsePagination.next.query` names
    /// (outside the runtime, which opens it from its saved response).
    #[cfg(test)]
    pub(crate) fn follow(&self, next: &Value) -> Option<Self> {
        let cursor = Cursor::decode(next["responseSnapshot"].as_str()?)?;
        let mut options = self.clone();
        options.open(&cursor);
        Some(options)
    }

    pub(crate) fn structured_scope(&self) -> bool {
        self.response_scope == Some(PageScope::Structured) && self.response_length.is_some()
    }

    /// Row-aware paging of structuredContent: every page is a complete JSON
    /// envelope holding whole result rows (or whole elements of a split row).
    pub(crate) fn rows_scope(&self) -> bool {
        self.response_scope == Some(PageScope::Rows) && self.response_length.is_some()
    }

    /// The caller sized the response page: `responseLength` is set. A
    /// cursor alone continues on the automatic page, so it never lifts it
    /// (nor the read share sized from it).
    pub(crate) fn explicit(&self) -> bool {
        self.response_length.is_some()
    }

    /// The caller chose what the page windows: `responseLength` with a
    /// `responseScope`. A length alone sizes whole-row pages
    /// ([`Self::auto_paginate`]).
    pub(crate) fn scoped(&self) -> bool {
        self.explicit() && self.response_scope.is_some()
    }

    /// Whether `structured` is cut into pages: the caller chose a scope, or
    /// the envelope is past the caller's `responseLength`, else past the
    /// runtime's automatic page `auto_page` (0: never).
    pub(crate) fn pages(&self, structured: &Value, auto_page: usize) -> bool {
        if self.scoped() {
            return true;
        }
        auto_page != 0
            && octocode_engine::json_size::utf16_units(structured).unwrap_or(0)
                > self.response_length.unwrap_or(auto_page)
    }

    /// Page whole rows at the caller's `responseLength`, else at `auto_page`,
    /// unless the caller chose a scope ([`Self::scoped`]). An envelope within
    /// the budget is one page: nothing windows it.
    pub(crate) fn auto_paginate(&mut self, structured: &Value, auto_page: usize) {
        if self.scoped() || auto_page == 0 {
            return;
        }
        if self.pages(structured, auto_page) {
            self.page_rows(self.response_length.unwrap_or(auto_page));
        } else {
            self.response_length = None;
        }
    }

    fn page_rows(&mut self, budget: usize) {
        self.response_scope = Some(PageScope::Rows);
        self.response_length = Some(budget);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResponseInput {
    pub tool: String,
    pub structured: Value,
    pub options: ResponsePageOptions,
    /// Each result row's remaining-page notes (by position), so a split
    /// row's parts re-render their disclosure
    /// ([`super::pages::take_remaining_notes`]).
    pub remaining: Vec<Vec<super::pages::Remaining>>,
}

/// A saved response's page memo: the source its pages cut from, built from
/// the saved envelope on the first read and shared by every later one, so no
/// page read copies or re-plans the envelope.
#[derive(Default)]
pub(crate) struct SavedPages {
    source: OnceLock<SavedSource>,
}

enum SavedSource {
    Rows(Box<RowSource>),
    /// The serialized envelope a structured window slices.
    Window(String),
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
    /// The next page's offset; the cursor in `next` carries it.
    #[serde(skip)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResponseError {
    Cancelled,
    StructuredContentMustBeObject,
    Unserializable,
}

/// The page of `input` its options ask for: whole-row pages (`rows`), a
/// concatenable window of the serialized envelope (`structured`), else the
/// envelope unchanged.
pub(crate) fn prepare(
    mut input: ResponseInput,
    cancelled: &CancellationToken,
) -> Result<Value, ResponseError> {
    if cancelled.is_cancelled() {
        return Err(ResponseError::Cancelled);
    }
    let Value::Object(mut structured) = std::mem::take(&mut input.structured) else {
        return Err(ResponseError::StructuredContentMustBeObject);
    };
    if input.options.rows_scope() {
        let source = row_source(structured, &input)?;
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        return cut_rows(&source, &input);
    }
    if input.options.structured_scope() {
        // Concatenating the `responseWindow` pages in order reconstructs
        // the exact envelope JSON.
        strip_transient_telemetry(&mut structured);
        let full = Value::Object(structured).to_string();
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        return cut_window(&full, &input);
    }
    Ok(Value::Object(structured))
}

/// The page of a saved response `input` asks for, cut from `pages` (built
/// from `envelope` on the first read).
pub(crate) fn prepare_saved(
    input: &ResponseInput,
    envelope: &Value,
    pages: &SavedPages,
    cancelled: &CancellationToken,
) -> Result<Value, ResponseError> {
    if cancelled.is_cancelled() {
        return Err(ResponseError::Cancelled);
    }
    let source = match pages.source.get() {
        Some(source) => source,
        None => {
            let built = saved_source(envelope, input)?;
            pages.source.get_or_init(|| built)
        }
    };
    if cancelled.is_cancelled() {
        return Err(ResponseError::Cancelled);
    }
    match source {
        SavedSource::Rows(rows) => cut_rows(rows, input),
        SavedSource::Window(full) => cut_window(full, input),
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
/// envelope: its row plan, else its serialized bytes.
fn saved_source(envelope: &Value, input: &ResponseInput) -> Result<SavedSource, ResponseError> {
    let Value::Object(structured) = envelope else {
        return Err(ResponseError::StructuredContentMustBeObject);
    };
    let mut structured = structured.clone();
    if input.options.rows_scope() {
        return Ok(SavedSource::Rows(Box::new(row_source(structured, input)?)));
    }
    strip_transient_telemetry(&mut structured);
    Ok(SavedSource::Window(Value::Object(structured).to_string()))
}

fn is_saved_replay(options: &ResponsePageOptions) -> bool {
    options.saved.as_ref().is_some_and(|saved| saved.replayed)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cursor_tests;
