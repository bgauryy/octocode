//! One explicit source selection captured completely as a classification
//! state, through the same authorized readers a direct call runs: contract
//! validation, input security, the reader's own path policy and redaction,
//! and output sanitization. Nothing captured here reaches the host; the
//! caller receives the state and one read lead to exactly what was judged.
//!
//! A selection is captured whole or not at all: every content page the
//! reader returns is followed (each pinned to the first page's file snapshot
//! or commit), and a selection larger than a capture holds fails instead of
//! being judged from its first part.
pub(crate) mod assets;
pub(crate) mod glob;
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
    /// The policy a capture reads under: what goes to a provider, so credential
    /// file names stay withheld whatever the setting.
    fn paths(&self) -> &PathPolicy;
    /// The user's own policy, which a direct read runs under: it decides only
    /// the route a refused capture names, never what a capture reads.
    fn read_paths(&self) -> &PathPolicy;
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
    /// Characters of the judged source text. The read returns that text
    /// with its line-number gutter (`N\t` on each line a line page holds),
    /// so its pages run longer by the gutter.
    pub read_chars: usize,
    /// The read is a fresh one ([`fresh_read`]) of more characters than one
    /// response of this runtime holds: it pages, and paging needs saved
    /// continuations, which this runtime keeps for no later call. Raw
    /// characters are a floor of the rendered read, so this is never a false
    /// alarm; a read just under it meets the runtime's own refusal, which
    /// names the same fix.
    pub read_pages: bool,
    /// Provenance a read lead cannot carry (a URL capture).
    pub source: Option<Value>,
}

/// How a source selection too large for one state is judged in parts.
pub(crate) const IN_PARTS_HINT: &str = "Judge it in parts: narrower ranges, one query row per part, or operation:\"sections\" with the source under resources.";

/// `what` is more text than one capture holds in memory, whose limit stands
/// whatever the provider would judge.
pub(crate) fn too_large(max_chars: usize, what: &str, in_parts: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextTooLarge,
        format!(
            "{what} exceeds the {max_chars}-character limit of one capture; nothing was judged."
        ),
        in_parts,
    )
}

/// The refusal of a GitHub source whose name the classification egress
/// withholds: the code a local capture meets, before any fetch, so no copy is
/// cached. The egress rule stays strict whatever the setting, since redaction
/// recognizes known secret formats only and a provider must never receive a
/// credential file (graph snapshots and local classification hold the same
/// line). Its route follows `reads`, the user's own policy a direct read runs
/// under: only where that opens the file is the plain read offered, as a lead.
pub(crate) fn withheld_github_source(
    query: &Value,
    reads: &PathPolicy,
) -> Option<ClassificationError> {
    let path = query["path"].as_str()?;
    if !crate::policy::discovery::is_sensitive_repository_file(path) {
        return None;
    }
    let message = format!(
        "Path '{path}' is withheld from classification by the security path policy (a credential file name or directory): nothing was fetched or sent to the provider, and no flag or config setting lifts it."
    );
    if !reads.withholds_repository_file(path) {
        return Some(ClassificationError {
            leads: vec![(
                "read".to_owned(),
                read_lead(ToolId::GhGetFileContent, query, None),
            )],
            ..ClassificationError::new(
                ErrorCode::PathPolicyDenied,
                message,
                "A ghGetFileContent read without classification returns this file with its values redacted, keeps it in memory only and sends nothing to a provider.",
            )
        });
    }
    let hint = if crate::policy::discovery::is_in_sensitive_directory(
        &crate::policy::discovery::repository_path(path),
    ) {
        "No read returns it: a file in a sensitive directory is withheld from every read, whatever the configuration."
    } else {
        "A plain ghGetFileContent read is refused too; only the user's home .octocoderc setting security.credentialFiles: allow opens it, redacted and in memory."
    };
    Some(ClassificationError::new(
        ErrorCode::PathPolicyDenied,
        message,
        hint,
    ))
}

/// Capture `source` (`{tool, query}` or `{url}`) completely, up to
/// `max_chars` characters of text. `resumable`: this runtime keeps its saved
/// work for a later call, so the read lead is the pinned handle; otherwise
/// it is a [`fresh_read`]. A URL capture needs persistent storage either way.
pub(crate) fn capture(
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    max_chars: usize,
    resumable: bool,
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
    let reader = reader_context(context, max_chars);
    let (text, pin) = collect_pages(
        source["query"].clone(),
        |row| read_page(id, row, dispatcher, &reader),
        max_chars,
        id == ToolId::GhGetFileContent,
    )?;
    let read_chars = text.chars().count();
    Ok(Captured {
        read_chars,
        read_pages: !resumable && read_chars > dispatcher.limits().auto_page_chars,
        state: json!({ "source": identity(id, &source["query"], pin.as_deref()), "text": text }),
        read: if resumable {
            read_lead(id, &source["query"], pin.as_deref())
        } else {
            fresh_read(id, &source["query"], pin.as_deref())
        },
        source: None,
    })
}

/// The context a capture reads its source pages under: the call's deadline
/// and cancellation, with a reader window holding the capture's whole
/// budget. Every reader page reads, scans and digests the whole source, and
/// the capture keeps every page, so a page sized to the caller's response
/// share (a single long line: 16 KiB byte pages) cost one full source pass
/// per 16 KiB: 640 passes and 844 G instructions for a 10 MiB line.
pub(crate) fn reader_context(context: &ExecutionContext, max_chars: usize) -> ExecutionContext {
    ExecutionContext {
        response_window: Some(crate::response::page_windows::window_holding(max_chars)),
        ..context.clone()
    }
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
    let row = read_row(id, query, pin, true);
    if pin.is_some() {
        crate::response::result::Continuation::continued(id, row).build()
    } else {
        crate::response::result::Continuation::related(id, row).build()
    }
}

/// The read a runtime that keeps nothing for a later call offers for its
/// capture: a related call any later runtime runs, never a handle into this
/// one. A GitHub read stays at the commit it captured; a local read rereads
/// the path, since its snapshot ends with this runtime.
pub(crate) fn fresh_read(id: ToolId, query: &Value, pin: Option<&str>) -> Value {
    crate::response::result::Continuation::related(id, read_row(id, query, pin, false)).build()
}

/// The reader row of the captured selection: its path and ranges, and the
/// commit a GitHub capture resolved. A local `snapshot` pin is added only
/// when `snapshot` holds (the runtime keeps it).
fn read_row(id: ToolId, query: &Value, pin: Option<&str>, snapshot: bool) -> Value {
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
        (_, Some(pin)) if snapshot => {
            row.insert("snapshot".into(), json!(pin));
        }
        (_, _) => {}
    }
    Value::Object(row)
}

#[cfg(test)]
mod tests;
