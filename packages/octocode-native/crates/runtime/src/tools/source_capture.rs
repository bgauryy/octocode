//! One explicit source selection captured completely as a classification
//! state, through the same authorized readers a direct call runs: contract
//! validation, input security, the reader's own path policy and redaction,
//! and output sanitization. Nothing captured here reaches the host; the
//! caller receives the state and one read lead to exactly what was judged.
//!
//! A selection is captured whole or not at all: every content page the
//! reader returns is followed (each pinned to the first page's file snapshot
//! or commit), and a selection larger than the state limit fails instead of
//! being judged from its first part.
pub(crate) mod assets;
mod page;
pub(crate) mod resource;
pub(crate) mod url;

use crate::config::ConfigOutput;
use crate::execution::availability::ToolAvailability;
use crate::execution::{ExecutionContext, ExecutionError, RuntimeLimits};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::outcome::DomainResult;
use crate::security::{CheckedQuery, ContentSecurity, InputPolicy};
use crate::tools::judgment::ClassificationError;
use crate::tools::judgment::Evaluator;
use octocode_contract_types::ErrorCode;
use page::read_page;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The runtime services a capture reads through: the authorized readers,
/// their policies, and the shared handles. The runtime's domain dispatcher
/// implements it, so a capture (and clasify, which captures) never names the
/// dispatcher that runs it.
pub(crate) trait SourceReader: Sync {
    fn paths(&self) -> &PathPolicy;
    fn security(&self) -> &ContentSecurity;
    fn config(&self) -> &ConfigOutput;
    fn limits(&self) -> &RuntimeLimits;
    fn home(&self) -> &Path;
    fn handle(&self) -> &tokio::runtime::Handle;
    fn availability(&self) -> &ToolAvailability;
    fn evaluator(&self) -> &Evaluator;
    /// The output facts (recovery hints) of the reader `tool`.
    fn output(&self, tool: ToolId) -> &'static dyn crate::response::output::ToolOutput;
    /// One row of `tool` through the same domain path as a direct call.
    fn execute(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError>;
    /// The saved GitHub access identity, checked against the live one.
    fn retained_github_identity(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<String>, ClassificationError>;
    fn validate_retained_github_identity(
        &self,
        expected: Option<&str>,
        context: &ExecutionContext,
    ) -> Result<(), ClassificationError>;
    /// The most source text one capture reads.
    fn text_capture_limit(&self) -> usize;
    /// A local file's text, canonical path and snapshot.
    fn capture_local_text(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), ClassificationError>;
    /// GitHub source bytes through the file-read credential, cache and budget.
    fn capture_github_content(
        &self,
        query: &Value,
        context: &ExecutionContext,
        max_bytes: usize,
    ) -> Result<octocode_github::ContentResponse, ClassificationError>;
}

/// A captured selection.
pub(crate) struct Captured {
    /// What the classifier judges: the source's identity and its text.
    pub state: Value,
    /// An executable read of exactly the captured selection, at the same
    /// snapshot or commit.
    pub read: Value,
    /// Provenance a read lead cannot carry (a URL capture).
    pub source: Option<Value>,
}

/// How a source selection too large for one state is judged in parts.
pub(crate) const IN_PARTS_HINT: &str = "Judge it in parts: narrower ranges, one query row per part, or operation:\"sections\" with the source under resources.";

/// `what` exceeds the `max_chars` state limit. Its recovery: the
/// long-context limit when the row is under it, then `in_parts`.
pub(crate) fn too_large(max_chars: usize, what: &str, in_parts: &str) -> ClassificationError {
    let long = crate::id::clasify_policy::MAX_LONG_CONTEXT_STATE_CHARS;
    let mut hints = Vec::new();
    if max_chars < long {
        hints.push(format!(
            "Retry with longContext:true for a {long}-character state limit."
        ));
    }
    hints.push(in_parts.to_owned());
    ClassificationError::with_hints(
        ErrorCode::ClassificationContextTooLarge,
        format!("{what} exceeds the {max_chars}-character state limit; nothing was judged."),
        hints,
    )
}

/// Capture `source` (`{tool, query}` or `{url}`) completely, up to
/// `max_chars` characters of text.
pub(crate) fn capture(
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    max_chars: usize,
) -> Result<Captured, ClassificationError> {
    if let Some(address) = source.get("url").and_then(Value::as_str) {
        return url::capture(address, dispatcher, context, max_chars);
    }
    let tool = source["tool"].as_str().unwrap_or_default();
    let id = ToolId::from_name(tool)
        .filter(|id| matches!(id, ToolId::LocalFetch | ToolId::GhGetFileContent))
        .ok_or_else(|| {
            ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "A clasify source reads through localFetch or ghGetFileContent.",
                "Name localFetch or ghGetFileContent, or supply the data as content.",
            )
        })?;
    if !dispatcher.availability().is_available(tool) {
        return Err(ClassificationError::new(
            ErrorCode::ClassificationContextUnavailable,
            format!("{tool} is not available in this runtime, so its source cannot be read."),
            "Enable the reader, or supply the data as content.",
        ));
    }
    // A captured state is the exact text: the source query admits no
    // `minify`, and an omitted one reads the file as is.
    let (text, pin) = collect_pages(
        source["query"].clone(),
        |row| read_page(id, row, dispatcher, context),
        max_chars,
        id == ToolId::GhGetFileContent,
    )?;
    Ok(Captured {
        state: json!({ "source": identity(id, &source["query"], pin.as_deref()), "text": text }),
        read: read_lead(id, &source["query"], pin.as_deref()),
        source: None,
    })
}

/// Every reader page of a selection, from `row` through the last, as one
/// text and the pin the first page resolved. No page count bounds it: each
/// page holds text (an empty one fails the read), so `max_chars` ends any
/// selection, and a reader that offers a page it already served fails as
/// the defect it is instead of looping. `pin_ref` pins later pages to the
/// first page's commit (`ref`).
fn collect_pages(
    mut row: Value,
    mut read: impl FnMut(&Value) -> Result<page::Page, ClassificationError>,
    max_chars: usize,
    pin_ref: bool,
) -> Result<(String, Option<String>), ClassificationError> {
    let mut text = String::new();
    // Characters captured so far, counted once per page.
    let mut chars = 0;
    let mut pin: Option<String> = None;
    let mut served = std::collections::HashSet::new();
    loop {
        if !served.insert(row.to_string()) {
            return Err(ClassificationError::new(
                ErrorCode::ClassificationContextFailed,
                "The source reader offered a page it already served; nothing was judged.",
                page::RERUN_HINT,
            ));
        }
        let page = read(&row)?;
        if pin.is_none() {
            pin = page.pin.clone();
        }
        chars += page.content.chars().count();
        text.push_str(&page.content);
        if chars > max_chars {
            return Err(too_large(max_chars, "The selected source", IN_PARTS_HINT));
        }
        let Some(mut next) = page.next else {
            return Ok((text, pin));
        };
        // Every later page reads the commit the first page resolved.
        if pin_ref && let Some(sha) = &pin {
            next["ref"] = json!(sha);
        }
        row = next;
    }
}

pub(super) fn prepare_source(
    id: ToolId,
    row: &Value,
    dispatcher: &dyn SourceReader,
) -> Result<CheckedQuery, ClassificationError> {
    let tool = id.as_str();
    let prepared = crate::contracts::validate_built_query(tool, row.clone()).map_err(|error| {
        ClassificationError::new(
            ErrorCode::InvalidClassificationContext,
            format!(
                "The {tool} source does not satisfy its input contract: {}.",
                error
                    .issues
                    .first()
                    .map_or("invalid row", |issue| issue.message.as_str())
                    .trim_end_matches('.')
            ),
            "Correct the source query; it is the reader's own row.",
        )
    })?;
    // The reader's own row passes the gate every direct call passes.
    dispatcher
        .security()
        .check_input(prepared, InputPolicy::Query)
        .map_err(|_| {
            ClassificationError::new(
                ErrorCode::SecurityValidationFailed,
                "The source query is blocked by input security policy.",
                "Read an allowed path.",
            )
        })
}

/// The selection the state names: path, ranges or whole file, and pin.
fn identity(id: ToolId, query: &Value, pin: Option<&str>) -> Value {
    let mut source = json!({ "path": query["path"] });
    if id == ToolId::GhGetFileContent {
        source["repository"] = json!(format!(
            "{}/{}",
            query["owner"].as_str().unwrap_or_default(),
            query["repo"].as_str().unwrap_or_default()
        ));
        if let Some(sha) = pin {
            source["commitSha"] = json!(sha);
        }
    }
    if let Some(ranges) = query.get("ranges") {
        source["ranges"] = ranges.clone();
    }
    source
}

/// The read of exactly the captured selection: same path and ranges (or
/// whole file), exact text, pinned to the snapshot or commit.
pub(super) fn read_lead(id: ToolId, query: &Value, pin: Option<&str>) -> Value {
    let mut row = serde_json::Map::new();
    for field in ["owner", "repo", "path", "ranges"] {
        if let Some(value) = query.get(field) {
            row.insert(field.into(), value.clone());
        }
    }
    match (id, pin) {
        (ToolId::GhGetFileContent, Some(sha)) => {
            row.insert("ref".into(), json!(sha));
        }
        (ToolId::GhGetFileContent, None) => {
            if let Some(reference) = query.get("ref") {
                row.insert("ref".into(), reference.clone());
            }
        }
        (_, Some(snapshot)) => {
            row.insert("snapshot".into(), json!(snapshot));
        }
        (_, None) => {}
    }
    if pin.is_some() {
        crate::response::result::Continuation::continued(id, Value::Object(row)).build()
    } else {
        crate::response::result::Continuation::related(id, Value::Object(row)).build()
    }
}

#[cfg(test)]
mod tests;
