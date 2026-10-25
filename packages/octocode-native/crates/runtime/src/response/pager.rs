//! The response pager: row-aware pages (`row_pages`) and concatenable
//! windows of the one JSON envelope, with their executable continuations.
//! Every surface (CLI JSON, CLI views, MCP) reads the same pages.

mod cut;
mod telemetry;
mod window;
pub(crate) use cut::measure_page;
use cut::{cut_rows, pagination_block_chars};
use serde_json::{Map, Value};
use std::sync::OnceLock;
use telemetry::take_volatile_fields;
use tokio_util::sync::CancellationToken;
use window::{build_continuation, page_cursor};

use super::page_model::{ResponsePageOptions, ResponsePagination};
use super::row_pages::RowSource;

#[derive(Clone, Debug)]
pub(crate) struct ResponseInput {
    pub tool: String,
    pub structured: Value,
    pub options: ResponsePageOptions,
    /// Each result row's remaining-page notes (by position), so a split
    /// row's parts re-render their disclosure
    /// ([`super::pages::take_remaining_notes`]).
    pub remaining: Vec<Vec<super::result::Remaining>>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct PreparedPage {
    pub value: Value,
    pub next: Option<super::cursor::Cursor>,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResponseError {
    Cancelled,
    StructuredContentMustBeObject,
    Unserializable,
    InvalidPage,
}

/// The page of `input` its options ask for: whole-row pages (`rows`), a
/// concatenable window of the serialized envelope (`structured`), else the
/// envelope unchanged.
pub(crate) fn prepare(
    mut input: ResponseInput,
    cancelled: &CancellationToken,
) -> Result<PreparedPage, ResponseError> {
    if cancelled.is_cancelled() {
        return Err(ResponseError::Cancelled);
    }
    let Value::Object(structured) = std::mem::take(&mut input.structured) else {
        return Err(ResponseError::StructuredContentMustBeObject);
    };
    if input.options.paging {
        let source = row_source(structured, &input)?;
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        return cut_rows(&source, &input);
    }
    Ok(PreparedPage {
        value: Value::Object(structured),
        next: None,
    })
}

/// The page of a saved response `input` asks for, cut from `pages` (built
/// from `envelope` on the first read).
pub(crate) fn prepare_saved(
    input: &ResponseInput,
    envelope: &Value,
    pages: &SavedPages,
    cancelled: &CancellationToken,
) -> Result<PreparedPage, ResponseError> {
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
    }
}

/// The row plan of `structured` at `input`'s page length, without the
/// per-call telemetry a page never carries.
fn row_source(
    mut structured: Map<String, Value>,
    input: &ResponseInput,
) -> Result<RowSource, ResponseError> {
    // Row pages re-attach per-call provider facts to the row they came
    // from; the snapshot and page plan never see them.
    let volatile = take_volatile_fields(&mut structured);
    // One streaming pass over the envelope: its length and snapshot digest,
    // with no rendering built. Every page is bounded by its budget, so any
    // envelope size pages.
    let envelope = octocode_engine::digest::fingerprint_json(&structured)
        .ok_or(ResponseError::Unserializable)?;
    Ok(RowSource::new(
        structured,
        &envelope,
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
    Ok(SavedSource::Rows(Box::new(row_source(
        structured.clone(),
        input,
    )?)))
}

fn is_saved_replay(options: &ResponsePageOptions) -> bool {
    options.saved.as_ref().is_some_and(|saved| saved.replayed)
}

#[cfg(test)]
mod tests;
