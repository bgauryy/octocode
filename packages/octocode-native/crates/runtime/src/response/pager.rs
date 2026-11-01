//! The response pager: row-aware pages (`row_pages`) and concatenable
//! windows of the one JSON envelope, with their executable continuations.
//! Every surface (CLI JSON, CLI views, MCP) reads the same pages.

mod cut;
mod handles;
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
    pub tool: crate::id::ToolId,
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
    InvalidEvidence(String),
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
        let source = row_source(structured, &input, cancelled)?;
        if cancelled.is_cancelled() {
            return Err(ResponseError::Cancelled);
        }
        return cut_rows(&source, &input, cancelled);
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
            let built = saved_source(envelope, input, cancelled)?;
            pages.source.get_or_init(|| built)
        }
    };
    if cancelled.is_cancelled() {
        return Err(ResponseError::Cancelled);
    }
    match source {
        SavedSource::Rows(rows) => cut_rows(rows, input, cancelled),
    }
}

/// The row plan of `structured` at `input`'s page length, without the
/// per-call telemetry a page never carries.
fn row_source(
    mut structured: Map<String, Value>,
    input: &ResponseInput,
    cancelled: &CancellationToken,
) -> Result<RowSource, ResponseError> {
    // Row pages re-attach per-call provider facts to the row they came
    // from; the snapshot and page plan never see them.
    let volatile = take_volatile_fields(&mut structured);
    let envelope = fingerprint(&mut structured).ok_or(ResponseError::Unserializable)?;
    RowSource::new(
        structured,
        &envelope,
        &input.options,
        volatile,
        input.tool,
        &|pagination| pagination_block_chars(input, pagination),
        cancelled,
    )
}

/// One streaming pass over the envelope: its length and snapshot digest,
/// with no rendering built (every page is bounded by its budget, so any
/// envelope size pages), and issued handles read as their placeholder
/// ([`handles`]).
fn fingerprint(
    structured: &mut Map<String, Value>,
) -> Option<octocode_engine::digest::JsonFingerprint> {
    handles::masked(structured, |structured| {
        octocode_engine::digest::fingerprint_json(structured)
    })
}

/// The content snapshot the pages of `structured` at `budget` chars per
/// page carry: what a re-execution must reproduce to serve a page of an
/// earlier response ([`super::cursor::Cursor::content_snapshot`]).
pub(crate) fn content_snapshot(structured: &Value, budget: usize) -> Option<String> {
    let Value::Object(structured) = structured else {
        return None;
    };
    let mut structured = structured.clone();
    take_volatile_fields(&mut structured);
    let envelope = fingerprint(&mut structured)?;
    Some(super::cursor::rows_snapshot(
        budget.max(1),
        &envelope.digest,
    ))
}

/// What a saved read's pages cut from, built from a copy of the saved
/// envelope: its row plan, else its serialized bytes.
fn saved_source(
    envelope: &Value,
    input: &ResponseInput,
    cancelled: &CancellationToken,
) -> Result<SavedSource, ResponseError> {
    let Value::Object(structured) = envelope else {
        return Err(ResponseError::StructuredContentMustBeObject);
    };
    Ok(SavedSource::Rows(Box::new(row_source(
        structured.clone(),
        input,
        cancelled,
    )?)))
}

fn is_saved_replay(options: &ResponsePageOptions) -> bool {
    options.saved.as_ref().is_some_and(|saved| saved.replayed)
}

#[cfg(test)]
mod tests;
