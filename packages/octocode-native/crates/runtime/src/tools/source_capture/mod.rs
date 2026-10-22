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
pub(crate) mod resource;
pub(crate) mod url;

use crate::config::ConfigOutput;
use crate::execution::availability::ToolAvailability;
use crate::execution::{ExecutionContext, ExecutionError, RuntimeLimits};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::outcome::DomainResult;
use crate::response::rows;
use crate::security::ContentSecurity;
use crate::tools::judgment::ClassificationError;
use crate::tools::judgment::Evaluator;
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
    /// One row of `tool` through the same domain path as a direct call.
    fn execute(
        &self,
        tool: ToolId,
        query: &Value,
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

/// Pages one selection may take; a reader that keeps paging past this is
/// treated as a failed capture, not judged in part.
const MAX_PAGES: usize = 64;

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

fn failure(code: &str, message: impl Into<String>, hint: &str) -> ClassificationError {
    ClassificationError::new(code, message, hint)
}

fn too_large(max_chars: usize) -> ClassificationError {
    failure(
        "classificationContextTooLarge",
        format!(
            "The selected source exceeds the {max_chars}-character state limit; nothing was judged."
        ),
        "Select narrower ranges, or split the selection across query rows.",
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
            failure(
                "invalidClassificationContext",
                "A clasify source reads through localFetch or ghGetFileContent.",
                "Name localFetch or ghGetFileContent, or supply the data as state.",
            )
        })?;
    if !dispatcher.availability().is_available(tool) {
        return Err(failure(
            "classificationContextUnavailable",
            format!("{tool} is not available in this runtime, so its source cannot be read."),
            "Enable the reader, or supply the data as state.",
        ));
    }
    // A captured state is the exact text: the source query admits no
    // `minify`, and an omitted one reads the file as is.
    let mut row = source["query"].clone();
    let mut text = String::new();
    // Characters captured so far, counted once per page.
    let mut chars = 0;
    let mut pin: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let page = read_page(id, &row, dispatcher, context)?;
        if pin.is_none() {
            pin = page.pin.clone();
        }
        chars += page.content.chars().count();
        text.push_str(&page.content);
        if chars > max_chars {
            return Err(too_large(max_chars));
        }
        let Some(mut next) = page.next else {
            return Ok(Captured {
                state: json!({ "source": identity(id, &source["query"], pin.as_deref()), "text": text }),
                read: read_lead(id, &source["query"], pin.as_deref()),
                source: None,
            });
        };
        // Every later page reads the commit the first page resolved.
        if id == ToolId::GhGetFileContent
            && let Some(sha) = &pin
        {
            next["ref"] = json!(sha);
        }
        row = next;
    }
    Err(too_large(max_chars))
}

/// One reader page as the capture needs it.
struct Page {
    content: String,
    /// The local file snapshot or the resolved commit SHA.
    pin: Option<String>,
    /// The reader's own row for the next content page.
    next: Option<Value>,
}

pub(super) fn prepare_source(
    id: ToolId,
    row: &Value,
    dispatcher: &dyn SourceReader,
) -> Result<Value, ClassificationError> {
    let tool = id.as_str();
    let mut prepared = crate::contracts::prepare_many_and_validate(tool, json!({"queries":[row]}))
        .map_err(|error| {
            failure(
                "invalidClassificationContext",
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
    let prepared = prepared.pop().unwrap_or(Value::Null);
    let checked = dispatcher.security().validate_input_parameters(&prepared);
    if !checked.is_valid {
        return Err(failure(
            "securityValidationFailed",
            "The source query is blocked by input security policy.",
            "Read an allowed path.",
        ));
    }
    Ok(Value::Object(checked.sanitized_params))
}

fn read_page(
    id: ToolId,
    row: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<Page, ClassificationError> {
    let prepared = prepare_source(id, row, dispatcher)?;
    let tool = id.as_str();
    context.check().map_err(|stop| {
        let (code, hint) = match stop {
            crate::execution::ExecutionError::Timeout => (
                "timeout",
                "Retry with more time, or supply the source text as state.",
            ),
            _ => ("cancelled", crate::response::rows::CANCELLED_HINT),
        };
        failure(
            code,
            "The call stopped before its source was captured.",
            hint,
        )
    })?;
    let result = dispatcher.execute(id, &prepared, context).map_err(|stop| {
        failure(
            match stop {
                crate::execution::ExecutionError::Timeout => "timeout",
                crate::execution::ExecutionError::Cancelled => "cancelled",
                _ => "classificationContextFailed",
            },
            format!("The {tool} source read could not complete."),
            RERUN_HINT,
        )
    })?;
    let source_digest = result.source_digest.clone();
    let row_value = rows::result_row(id, 0, &prepared, result.data, result.status);
    let mut state = rows::envelope_in(vec![row_value], id, &[Some(&prepared)], dispatcher.paths());
    rows::sanitize_output(
        &mut state,
        id,
        dispatcher.security(),
        context,
        dispatcher.config().resolved.output.redact_emails,
    )
    .map_err(|_| {
        failure(
            "classificationContextFailed",
            "The source read's output could not be sanitized.",
            RERUN_HINT,
        )
    })?;
    let data = &state["results"][0]["data"];
    if state["results"][0]["status"] == "error" || data.get("errorCode").is_some() {
        return Err(reader_failure(id, data));
    }
    let content = data["content"].as_str().unwrap_or_default();
    if content.is_empty() {
        return Err(failure(
            "classificationContextEmpty",
            format!("The {tool} source selected no text; nothing was judged."),
            "Check the ranges and the path.",
        ));
    }
    let pin = match id {
        ToolId::GhGetFileContent => data["commitSha"]
            .as_str()
            .or_else(|| state["shared"]["commitSha"].as_str())
            .map(str::to_owned),
        _ => source_digest,
    };
    let next = data["next"]["continueWindow"]["query"]["queries"][0]
        .as_object()
        .map(|next| Value::Object(next.clone()));
    Ok(Page {
        content: content.to_owned(),
        pin,
        next,
    })
}

/// Re-running a read that failed for a reason repeating it cannot change
/// (the reader's own hints and leads are the recovery then).
const RERUN_HINT: &str = "Run the same read directly to inspect it.";

/// The reader's error row as the capture's failure: its code (when
/// declared), its message, and its own recovery. The reader's leads
/// (`findFile`, `viewTree`, `repair`, ...) are kept by name. A missing path,
/// a refusal or an invalid input fails the same way on a re-run, so its own
/// hints say what to do (else the code's shared recovery); any other failure
/// also offers the re-run.
fn reader_failure(tool: ToolId, data: &Value) -> ClassificationError {
    use crate::id::error_codes::{ErrorClass, class};
    let declared = data["errorCode"]
        .as_str()
        .and_then(|code| Some((code, class(code)?)));
    let leads: Vec<(String, Value)> = ["next", "hints"]
        .iter()
        .filter_map(|channel| data[*channel].as_object())
        .flatten()
        .filter(|(_, lead)| lead.get("tool").is_some() && lead.get("query").is_some())
        .map(|(name, lead)| (name.clone(), lead.clone()))
        .collect();
    let own = match &data["hints"] {
        Value::Array(items) => items.as_slice(),
        hints => hints["text"].as_array().map_or(&[][..], Vec::as_slice),
    };
    let mut hints: Vec<String> = own
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let final_failure = declared.is_some_and(|(_, class)| {
        matches!(
            class,
            ErrorClass::NotFound | ErrorClass::Permission | ErrorClass::InvalidInput
        )
    });
    if !final_failure {
        hints.push(RERUN_HINT.into());
    } else if hints.is_empty() && leads.is_empty() {
        // Re-running a final failure fails the same way: say the code's fix.
        let hint = declared
            .and_then(|(code, _)| crate::response::rows::error_code_hint(tool, code))
            .unwrap_or(RERUN_HINT);
        hints.push(hint.into());
    }
    ClassificationError {
        code: declared
            .map_or("classificationContextFailed", |(code, _)| code)
            .to_owned(),
        message: data["error"]
            .as_str()
            .unwrap_or("The source read failed.")
            .to_owned(),
        hints,
        leads,
        ..ClassificationError::default()
    }
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
    for field in ["owner", "repo", "path", "ranges", "fullContent"] {
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
    crate::response::result::Continuation::new(id, Value::Object(row)).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing file's capture failure carries the reader's own leads and
    /// hints, not a re-run of a read that cannot succeed.
    #[test]
    fn a_final_reader_failure_keeps_its_own_leads_and_hints() {
        let lead = |tool: &str| json!({"tool": tool, "query": {"queries": [{"path": "src"}]}});
        let data = json!({
            "path": "src/exector.rs",
            "errorCode": "pathNotFound",
            "error": "Path does not exist: src/exector.rs",
            "hints": ["Check the spelling."],
            "next": {"viewTree": lead("structureSearch"), "findFile": lead("structureSearch")},
        });
        let failure = reader_failure(ToolId::LocalFetch, &data);
        assert_eq!(failure.code, "pathNotFound");
        assert_eq!(failure.hints, vec!["Check the spelling.".to_owned()]);
        let mut names: Vec<&str> = failure
            .leads
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["findFile", "viewTree"]);
        // A refusal with no recovery of its own says the code's fix, never
        // a re-run that fails the same way.
        let bare = reader_failure(
            ToolId::LocalFetch,
            &json!({"errorCode": "outsideAllowedRoots", "error": "no"}),
        );
        assert_eq!(bare.hints.len(), 1, "{:?}", bare.hints);
        assert_ne!(bare.hints[0], RERUN_HINT);
        // A failure a re-run can change keeps the re-run beside its hints.
        let transient = reader_failure(
            ToolId::LocalFetch,
            &json!({
                "errorCode": "fileReadFailed", "error": "io", "hints": {"text": ["Disk busy."]}
            }),
        );
        assert_eq!(
            transient.hints,
            vec!["Disk busy.".to_owned(), RERUN_HINT.to_owned()]
        );
        let undeclared = reader_failure(
            ToolId::LocalFetch,
            &json!({"errorCode": "madeUp", "error": "x"}),
        );
        assert_eq!(undeclared.code, "classificationContextFailed");
    }
}
