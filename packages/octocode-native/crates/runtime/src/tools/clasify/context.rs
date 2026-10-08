//! Validated, bounded tool context; never re-enters public request admission.
use crate::response::rows;
use crate::runtime::{ExecutionContext, domain_dispatch::DomainDispatcher};
use crate::tools::id::ToolId;
use crate::{
    contracts,
    tools::clasify::{is_context_tool, transport::ClassificationError},
};
use serde_json::{Map, Value, json};
use std::path::Path;

const MAX_RECEIPT_BYTES: usize = 16 * 1024;
/// Receipt limitation for a page whose continuation was not followed.
pub(super) const PAGE_ONLY_LIMITATION: &str =
    "Only the returned tool page was evaluated; continue explicitly for additional evidence.";

fn error(code: &str, message: impl Into<String>) -> ClassificationError {
    ClassificationError {
        code: code.into(),
        message: message.into(),
        hints: vec!["Run the ordinary context tool to inspect or correct its request.".into()],
        ..Default::default()
    }
}

fn checked(context: &ExecutionContext) -> Result<(), ClassificationError> {
    context.check().map_err(|failure| {
        error(
            match failure {
                crate::runtime::ExecutionError::Timeout => "timeout",
                _ => "cancelled",
            },
            "Context execution stopped before classification.",
        )
    })
}

/// A context tool that failed to run at all. The execution kind stays visible:
/// a deadline spent elsewhere (e.g. slow client construction) otherwise reads
/// like a broken query.
fn dispatch_failure(tool: &str, failure: crate::runtime::ExecutionError) -> ClassificationError {
    match failure {
        crate::runtime::ExecutionError::Timeout => error(
            "timeout",
            format!("Context tool {tool} timed out before classification."),
        ),
        crate::runtime::ExecutionError::Cancelled => error(
            "cancelled",
            format!("Context tool {tool} was cancelled before classification."),
        ),
        other => {
            let kind = serde_json::to_value(other)
                .ok()
                .and_then(|kind| kind.as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{other:?}"));
            error(
                "classificationContextFailed",
                format!("Context tool {tool} could not complete ({kind})."),
            )
        }
    }
}

pub(super) struct ContextFailure {
    pub error: ClassificationError,
    pub receipt: Option<Value>,
}

impl From<ClassificationError> for ContextFailure {
    fn from(error: ClassificationError) -> Self {
        Self {
            error,
            receipt: None,
        }
    }
}

/// One context query as its tool runs it: a single row, contract-validated.
pub(super) fn prepare(tool: &str, query: &Value) -> Result<Value, ClassificationError> {
    let object = query.as_object().ok_or_else(|| {
        error(
            "invalidClassificationContext",
            "Context query must be one ordinary query object.",
        )
    })?;
    if [
        "queries",
        "renderText",
        "responseOffset",
        "responseLength",
        "responseSnapshot",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
    {
        return Err(error(
            "invalidClassificationContext",
            "Context query cannot contain a bulk envelope or response paging options.",
        ));
    }
    if tool == ToolId::GhStructure.as_str() && query.get("materialize") == Some(&Value::Bool(true))
    {
        return Err(error(
            "invalidClassificationContext",
            "Clasify context cannot materialize a repository tree; use ghStructure directly when files are needed locally.",
        ));
    }
    let mut queries = contracts::prepare_many_and_validate(tool, json!({"queries":[query]}))
        .map_err(|validation_error| {
            let detail = validation_error
                .issues
                .iter()
                .map(|issue| {
                    if issue.path.is_empty() {
                        issue.message.clone()
                    } else {
                        format!("{}: {}", issue.path.join("."), issue.message)
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            error(
                "invalidClassificationContext",
                if detail.is_empty() {
                    format!("Context query does not satisfy the {tool} input contract.")
                } else {
                    format!(
                        "Context query does not satisfy the {tool} input contract: {}.",
                        detail.trim_end_matches('.')
                    )
                },
            )
        })?;
    if queries.len() != 1 {
        return Err(error(
            "invalidClassificationContext",
            "Context must contain exactly one ordinary query.",
        ));
    }
    queries
        .pop()
        .ok_or_else(|| error("invalidClassificationContext", "Context query is missing."))
}

/// Why a clasify context tool is unavailable: the gate that enables it (from
/// the config contract's env bindings) and the context tools that are enabled.
pub(crate) fn unavailable_context_message(tool: &str, available: impl Fn(&str) -> bool) -> String {
    let id = ToolId::from_name(tool);
    let gate = id.map(ToolId::availability_env_vars).unwrap_or_default();
    let enable = match id.and_then(ToolId::availability_config_path) {
        Some(path) if !gate.is_empty() => format!(
            "Enable it with {} (config {path}); tools.enabled/tools.disabled can also exclude it.",
            gate.iter()
                .map(|var| format!("{var}=true"))
                .collect::<Vec<_>>()
                .join(" or ")
        ),
        _ => "Check tools.enabled/tools.disabled.".to_owned(),
    };
    let enabled = crate::tools::id::clasify_policy::SCOUT_TOOLS
        .iter()
        .map(|id| id.as_str())
        .filter(|name| available(name))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Context tool {tool} is disabled in this runtime. {enable} Enabled context tools: {enabled}."
    )
}

/// Validation-stage availability check for a prepared clasify matrix: one
/// issue per resource whose context tool this runtime does not enable. MCP
/// rejects the same input through its availability-scoped schema.
pub(crate) fn unavailable_context_issues(
    query: &Value,
    prefix: &[String],
    available: impl Fn(&str) -> bool,
) -> Vec<contracts::ValidationIssue> {
    query["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, resource)| {
            let tool = resource.get("tool")?.as_str()?;
            (ToolId::from_name(tool).is_some_and(is_context_tool) && !available(tool)).then(|| {
                let mut path = prefix.to_vec();
                path.extend(["resources".to_owned(), index.to_string(), "tool".to_owned()]);
                contracts::ValidationIssue {
                    rule_id: "clasify.context-tool-available".into(),
                    path,
                    message: unavailable_context_message(tool, &available),
                    schema: None,
                    received: Some(Value::String(tool.to_owned())),
                }
            })
        })
        .collect()
}

// `ContextFailure` carries a sanitized failure envelope; boxing it would ripple
// through every caller for no runtime benefit on this cold error path.
#[allow(clippy::result_large_err)]
pub(super) fn resolve(
    source: &Value,
    dispatcher: &DomainDispatcher,
    context: &ExecutionContext,
) -> Result<(Value, Option<Value>), ContextFailure> {
    checked(context).map_err(ContextFailure::from)?;
    if let Some(value) = source.get("value") {
        return Ok((value.clone(), Some(value_receipt(value))));
    }
    let (tool, id) = read_tool(source, dispatcher)?;
    let prepared = prepare(tool, &source["query"]).map_err(ContextFailure::from)?;
    let checked_input = dispatcher.security.validate_input_parameters(&prepared);
    if !checked_input.is_valid {
        return Err(ContextFailure::from(error(
            "securityValidationFailed",
            "Context query is blocked by input security policy.",
        )));
    }
    let prepared = Value::Object(checked_input.sanitized_params);
    let read = run_read(id, &prepared, dispatcher, context)?;
    let operation = prepared.get("operation").and_then(Value::as_str);
    if read.empty {
        let mut empty = error(
            "classificationContextEmpty",
            format!("Context tool {tool} returned no evidence; classification was not called."),
        );
        let hints = tool_hints(&read.state, &prepared);
        if !hints.is_empty() {
            empty.hints = hints;
        }
        return Err(ContextFailure {
            error: empty,
            receipt: Some(receipt_with_evaluation(tool, &read.state, true, operation)),
        });
    }
    if read.failed {
        return Err(ContextFailure {
            error: read_error(tool, &read, &prepared),
            receipt: Some(failed_receipt(tool, &read.state, operation)),
        });
    }
    checked(context).map_err(ContextFailure::from)?;
    let mut receipt = receipt_with_evaluation(tool, &read.state, true, operation);
    attach_requested_reference(source, &mut receipt);
    if id == ToolId::GhGetHistoryItem {
        // Replays the selected page, including its independent content axes.
        // History can change between reads, so this is a verification lead.
        receipt["pageRead"] = json!({"tool":tool,"confidence":"high","query":prepared});
    }
    Ok((read.state, Some(receipt)))
}

/// The read tool a resource delegates to, when this runtime enables it.
#[allow(clippy::result_large_err)]
fn read_tool<'a>(
    source: &'a Value,
    dispatcher: &DomainDispatcher,
) -> Result<(&'a str, ToolId), ContextFailure> {
    let not_a_read = || {
        ContextFailure::from(error(
            "invalidClassificationContext",
            "Only read tools can provide classification context.",
        ))
    };
    let tool = source["tool"]
        .as_str()
        .filter(|tool| ToolId::from_name(tool).is_some_and(is_context_tool))
        .ok_or_else(not_a_read)?;
    // Normally rejected at validation (`unavailable_context_issues`); kept for
    // embedders that dispatch a prepared matrix directly.
    if !dispatcher.available_tools.contains(&tool) {
        let mut unavailable = error(
            "classificationContextUnavailable",
            unavailable_context_message(tool, |name| dispatcher.available_tools.contains(&name)),
        );
        unavailable.hints = Vec::new();
        return Err(ContextFailure::from(unavailable));
    }
    let id = ToolId::from_name(tool).ok_or_else(not_a_read)?;
    Ok((tool, id))
}

/// One delegated read, shaped and sanitized as a direct call returns it.
struct Read {
    state: Value,
    failed: bool,
    empty: bool,
    failure: Option<crate::tools::result::FailureKind>,
}

#[allow(clippy::result_large_err)]
fn run_read(
    id: ToolId,
    prepared: &Value,
    dispatcher: &DomainDispatcher,
    context: &ExecutionContext,
) -> Result<Read, ContextFailure> {
    let tool = id.as_str();
    let result = dispatcher
        .execute(id, prepared, context)
        .map_err(|failure| ContextFailure::from(dispatch_failure(tool, failure)))?;
    checked(context).map_err(ContextFailure::from)?;
    let failure = result.failure;
    let failed = failure.is_some() || result.status == Some("error");
    let empty = result.status == Some("empty");
    let mut row = rows::result_row(id, 0, prepared, result.data, result.status);
    rows::attach_diagnostics(&mut row, result.diagnostics);
    if result.cache {
        row["cache"] = json!(1);
    }
    rows::apply_hint_policy(&mut row, id, prepared);
    let mut state = rows::envelope_in(vec![row], id, &[Some(prepared)], &dispatcher.paths);
    rows::sanitize_output(
        &mut state,
        id,
        &dispatcher.security,
        context,
        dispatcher.config.resolved.output.redact_emails,
    )
    .map_err(|_| {
        ContextFailure::from(error(
            "classificationContextFailed",
            "Context output sanitization failed.",
        ))
    })?;
    // The public contract sees the read as a direct call would; the walk
    // keeps reading the tool's own `next`.
    let mut public = state.clone();
    crate::response::continuations::finalize(
        &mut public,
        id,
        &crate::response::continuations::Sources::Rows(&[Some(prepared)]),
        &crate::response::continuations::Scope::everything(),
    )
    .and_then(|()| contracts::validate_output(tool, &public))
    .map_err(|violation| {
        // Name the violated field (never the received value) so the defect is
        // reportable instead of an opaque failure.
        let detail = violation.issues.first().map_or_else(String::new, |issue| {
            format!(" at /{}: {}", issue.path.join("/"), issue.message)
        });
        ContextFailure::from(error(
            "classificationContextContractViolation",
            format!("Context tool {tool} returned invalid output{detail}."),
        ))
    })?;
    Ok(Read {
        state,
        failed,
        empty,
        failure,
    })
}

/// A failed read as clasify's error: the read tool's own (already
/// sanitized) code, reason, and repair hints; a bare "returned an error"
/// hides e.g. a sandbox refusal.
fn read_error(tool: &str, read: &Read, query: &Value) -> ClassificationError {
    let data = |field: &str| read.state.pointer(&format!("/results/0/data/{field}"));
    let code = data("errorCode")
        .and_then(Value::as_str)
        .unwrap_or("classificationContextFailed");
    let reason = data("error")
        .and_then(Value::as_str)
        .map_or_else(String::new, |reason| {
            format!(": {}", reason.trim_end_matches('.'))
        });
    let mut failure = error(
        code,
        format!("Context tool {tool} failed{reason}; classification was not called."),
    );
    failure.failure = read.failure;
    let hints = tool_hints(&read.state, query);
    if !hints.is_empty() {
        failure.hints = hints;
    }
    failure
}

/// The read tool's own recovery for a failed or empty read: its prose hints,
/// then each lead it offered (`next.<name>`) as the exact call to run. An
/// error's `hints` carry text only, so a lead is rendered as text, as the agent
/// would send it ([`agent_lead`]) against the read clasify ran (`query`).
fn tool_hints(state: &Value, query: &Value) -> Vec<String> {
    let Some(data) = state.pointer("/results/0/data") else {
        return Vec::new();
    };
    let prose = match data.get("hints") {
        Some(Value::Array(items)) => Some(items),
        Some(Value::Object(hints)) => hints.get("text").and_then(Value::as_array),
        _ => None,
    };
    let mut hints = prose
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let leads = [data.get("next"), state.pointer("/results/0/next")];
    for (name, lead) in leads
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .flatten()
    {
        if let (Some(tool), Some(lead)) =
            (lead.get("tool").and_then(Value::as_str), lead.get("query"))
        {
            let lead = agent_lead(lead, query);
            hints.push(format!("next.{name}: run {tool} {lead}"));
        }
    }
    hints
}

/// A lead's query without what the read inherited from clasify: the default
/// `debug:false`, the brief (`mainGoal`, `reasoning`), and the run's
/// `pageSize`, which clasify may have lowered to its candidate budget. A
/// paging lead (`page`) keeps its size: its offset depends on it.
fn agent_lead(lead: &Value, run: &Value) -> Value {
    let mut lead = lead.clone();
    let rows = match lead.get_mut("queries").and_then(Value::as_array_mut) {
        Some(rows) => rows.iter_mut().collect::<Vec<_>>(),
        None => vec![&mut lead],
    };
    for row in rows.into_iter().filter_map(Value::as_object_mut) {
        row.remove("mainGoal");
        row.remove("reasoning");
        if row.get("debug") == Some(&Value::Bool(false)) {
            row.remove("debug");
        }
        if !row.contains_key("page")
            && run.get("pageSize").is_some()
            && row.get("pageSize") == run.get("pageSize")
        {
            row.remove("pageSize");
        }
    }
    lead
}

fn attach_requested_reference(request: &Value, receipt: &mut Value) {
    if receipt.pointer("/source/ref").is_some() {
        return;
    }
    if let Some(reference) = request
        .get("query")
        .and_then(|query| query.get("ref"))
        .and_then(Value::as_str)
    {
        receipt["source"]["ref"] = json!(reference);
    }
}

/// Extracts the line or byte range of this page from a localFetch (or
/// compatible tool) result. Added to the receipt so callers always know
/// which chunk a classification page judgment covers without having to infer
/// it from the continuation query of the previous page.
fn page_scope(state: &Value) -> Option<Value> {
    let data = state
        .get("results")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("data"))
        .and_then(Value::as_object)?;
    let data = if data.contains_key("pagination") || data.contains_key("totalLines") {
        data
    } else {
        data.get("files")
            .and_then(Value::as_array)
            .and_then(|files| files.first())
            .and_then(Value::as_object)?
    };
    if data
        .get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
        && data
            .get("sourceLineRanges")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    {
        return None;
    }
    if let Some(ranges) = data.get("sourceLineRanges").and_then(Value::as_array)
        && ranges.len() > 1
    {
        let line_ranges = ranges
            .iter()
            .map(|range| {
                Some(json!({
                    "startLine":range.get("line")?.as_u64()?,
                    "endLine":range.get("endLine")?.as_u64()?
                }))
            })
            .collect::<Option<Vec<_>>>()?;
        return Some(json!({
            "lineRanges":line_ranges,
            "totalLines":data.get("totalLines")?.as_u64()?
        }));
    }
    let first_source_range = || {
        data.get("sourceLineRanges")
            .and_then(Value::as_array)
            .and_then(|r| r.first())
            .and_then(Value::as_object)
            .and_then(|r| Some((r.get("line")?.as_u64()?, r.get("endLine")?.as_u64()?)))
    };
    // Reads omit `pagination` when one page covers the whole view, and omit
    // view totals when they equal the source totals.
    let Some(pagination) = data.get("pagination").and_then(Value::as_object) else {
        let (start, end) = first_source_range()
            .or_else(|| {
                Some((
                    data.get("startLine")?.as_u64()?,
                    data.get("endLine")?.as_u64()?,
                ))
            })
            .or_else(|| Some((1, data.get("returnedLines")?.as_u64()?)))?;
        let total_lines = data.get("totalLines")?.as_u64()?;
        // Always scoped, even for a complete small file: agents read `scope`
        // to decide what to open, and a missing one read as an error in evals.
        return (start >= 1 && end >= start && total_lines >= 1)
            .then(|| json!({"startLine": start, "endLine": end, "totalLines": total_lines}));
    };
    let length = || pagination.get("length")?.as_u64();
    let unit = pagination.get("unit").and_then(Value::as_str)?;
    match unit {
        "lines" => {
            let (start, end) = first_source_range().or_else(|| {
                let offset = pagination.get("offset")?.as_u64()?;
                Some((offset + 1, offset + length()?))
            })?;
            let total_lines = data
                .get("totalLines")
                .or_else(|| pagination.get("totalLines"))?
                .as_u64()?;
            Some(json!({"startLine": start, "endLine": end, "totalLines": total_lines}))
        }
        "bytes" => {
            let source_lines = || {
                let (start, end) = first_source_range()?;
                let total_lines = data.get("totalLines")?.as_u64()?;
                Some(json!({"startLine": start, "endLine": end, "totalLines": total_lines}))
            };
            // Byte offsets of a line selection count from the selection start,
            // so only its source lines locate the page in the file.
            if data.contains_key("startLine") {
                return source_lines();
            }
            let bytes = || {
                let byte_offset = pagination.get("offset")?.as_u64()?;
                let byte_end = pagination
                    .get("nextOffset")
                    .and_then(Value::as_u64)
                    .or_else(|| Some(byte_offset + length()?))?;
                let total_bytes = pagination
                    .get("totalBytes")
                    .or_else(|| data.get("sourceBytes"))?
                    .as_u64()?;
                Some(
                    json!({"byteOffset": byte_offset, "byteEnd": byte_end, "totalBytes": total_bytes}),
                )
            };
            // Reads omit byte totals that equal the source; lines still scope it.
            bytes().or_else(source_lines)
        }
        _ => None,
    }
}

/// The returned content may be a transformed view. Its line offsets are
/// useful for continuing that view, but are not original source coordinates.
fn page_view(state: &Value) -> Option<Value> {
    let data = state.pointer("/results/0/data")?;
    let data = data
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .unwrap_or(data);
    let kind = data.get("contentView")?.as_str()?;
    if kind == "none" {
        return None;
    }
    let mut view = json!({"kind":kind});
    if let Some(pagination) = data.get("pagination") {
        if pagination["unit"] == "lines" {
            if let (Some(offset), Some(length)) =
                (pagination["offset"].as_u64(), pagination["length"].as_u64())
                && length > 0
            {
                view["startLine"] = json!(offset + 1);
                view["endLine"] = json!(offset + length);
            }
            if let Some(total) = pagination["totalLines"].as_u64().filter(|total| *total > 0) {
                view["totalLines"] = json!(total);
            }
        }
    } else if let Some(returned) = data["returnedLines"]
        .as_u64()
        .filter(|returned| *returned > 0)
    {
        view["startLine"] = json!(1);
        view["endLine"] = json!(returned);
        view["totalLines"] = json!(returned);
    }
    Some(view)
}

fn page_source(tool: &str, state: &Value, evidence_hash: &str) -> Value {
    let data = state.pointer("/results/0/data").unwrap_or(&Value::Null);
    // A page listing several files has no single source; name one only when
    // the page is that file.
    let file = match data.get("files").and_then(Value::as_array) {
        Some(files) if files.len() == 1 => &files[0],
        Some(_) => &Value::Null,
        None => data,
    };
    let mut source = json!({"evidenceHash":evidence_hash});
    if let Some(path) = file.get("path").and_then(Value::as_str) {
        let identity = match ToolId::from_name(tool) {
            Some(ToolId::LocalFetch | ToolId::LocalSearch) => state
                .get("root")
                .and_then(Value::as_str)
                .filter(|_| !Path::new(path).is_absolute())
                .map_or_else(
                    || path.to_owned(),
                    |base| Path::new(base).join(path).to_string_lossy().into_owned(),
                ),
            Some(ToolId::GhGetFileContent | ToolId::GhSearchCode) => {
                let owner = file
                    .get("owner")
                    .or_else(|| data.get("owner"))
                    .and_then(Value::as_str);
                let repo = file
                    .get("repo")
                    .or_else(|| data.get("repo"))
                    .and_then(Value::as_str);
                match (owner, repo) {
                    (Some(owner), Some(repo)) => format!("{owner}/{repo}/{path}"),
                    _ => path.to_owned(),
                }
            }
            _ => path.to_owned(),
        };
        source["path"] = json!(identity);
    }
    if let Some(modified) = file.get("modified").and_then(Value::as_str) {
        source["modified"] = json!(modified);
    }
    if let Some(reference) = file
        .get("commitSha")
        .or_else(|| file.get("ref"))
        .or_else(|| data.get("ref"))
        .and_then(Value::as_str)
    {
        source["ref"] = json!(reference);
    }
    source
}

pub(super) fn append_limitation(receipt: &mut Value, limitation: &str) {
    match receipt.get_mut("limitations").and_then(Value::as_array_mut) {
        Some(limitations) => limitations.push(json!(limitation)),
        None => receipt["limitations"] = json!([limitation]),
    }
}

/// Attach the exact source read that produced a hydrated candidate. This is
/// kept separate from capture pagination: callers use it to inspect the
/// judged evidence, while resource-level `next.clasify` continues discovery.
pub(super) fn attach_read(receipt: &mut Value, read: Value) {
    receipt["read"] = read;
}

#[cfg(test)]
fn receipt(tool: &str, state: &Value) -> Value {
    receipt_with_evaluation(tool, state, true, None)
}

/// Rebuild a receipt after one search response has been narrowed to a single
/// candidate. The candidate state keeps the original page continuation, while
/// its source identity and evidence hash describe only the file Jev judged.
pub(super) fn candidate_receipt(source: &Value, state: &Value) -> Value {
    let Some(tool) = source.get("tool").and_then(Value::as_str) else {
        return value_receipt(state);
    };
    let mut receipt = receipt_with_evaluation(
        tool,
        state,
        true,
        source.pointer("/query/operation").and_then(Value::as_str),
    );
    attach_requested_reference(source, &mut receipt);
    receipt
}

fn failed_receipt(tool: &str, state: &Value, operation: Option<&str>) -> Value {
    receipt_with_evaluation(tool, state, false, operation)
}

fn receipt_with_evaluation(
    tool: &str,
    state: &Value,
    evaluation_completed: bool,
    operation: Option<&str>,
) -> Value {
    let mut next = Map::new();
    let mut dropped = Vec::new();
    let mut reruns = Vec::new();
    let mut terminal = false;
    let mut partial = crate::response::pages::is_partial(state);
    let operation = operation.or_else(|| {
        state
            .pointer("/results/0/data/operation")
            .and_then(Value::as_str)
    });
    inspect(
        state,
        tool,
        operation,
        &mut Inspection {
            next: &mut next,
            dropped: &mut dropped,
            reruns: &mut reruns,
            partial: &mut partial,
            terminal: &mut terminal,
        },
    );
    partial |= !dropped.is_empty();
    let evidence_hash = crate::digest::json_sha256(&state);
    let mut receipt = json!({"source":page_source(tool, state, &evidence_hash),"tool":tool,"resultHash":evidence_hash,"coverage":if partial {"partial"}else{"bounded"}});
    if let Some(scope) = page_scope(state) {
        receipt["scope"] = scope;
    }
    if let Some(view) = page_view(state) {
        receipt["view"] = view;
    }
    if !next.is_empty() {
        receipt["next"] = Value::Object(next);
    }
    if partial {
        let limitation = if terminal {
            "The context tool reported a terminal limit; this result does not cover all matching evidence."
        } else if receipt.get("next").is_some() {
            if evaluation_completed {
                PAGE_ONLY_LIMITATION
            } else {
                "Context retrieval failed after returning a partial page; continue explicitly to recover additional evidence."
            }
        } else {
            "The context tool reported incomplete evidence without a safe continuation; inspect the ordinary tool result to change its bounds."
        };
        receipt["limitations"] = json!([limitation]);
    }
    if !dropped.is_empty() {
        append_limitation(
            &mut receipt,
            &format!(
                "The context tool offered continuations that fail input validation, so they were not kept: {}.",
                dropped.join(", ")
            ),
        );
    }
    if !reruns.is_empty() {
        append_limitation(
            &mut receipt,
            &format!(
                "Not followed: {} (a rerun of this read with wider analysis repeats the judged candidates); run the ordinary {tool} read and follow it, or send that rerun as its own resource.",
                reruns.join(", ")
            ),
        );
    }
    if !evaluation_completed {
        append_limitation(
            &mut receipt,
            "Context retrieval failed; classification was not run.",
        );
    }
    if receipt.to_string().len() > MAX_RECEIPT_BYTES {
        // Keep the walk's own continuation (the axis `continuation` follows);
        // only the other menu entries leave the receipt.
        let main = receipt
            .get("next")
            .and_then(Value::as_object)
            .and_then(main_axis)
            .map(|(key, value)| (key.clone(), value.clone()));
        if let Some((key, value)) = main {
            let mut kept = receipt.clone();
            kept["next"] = json!({ key.as_str(): value });
            append_limitation(
                &mut kept,
                &format!(
                    "Continuation metadata exceeded the receipt limit; only next.{key} was kept. Inspect the ordinary tool result for the other continuations."
                ),
            );
            if kept.to_string().len() <= MAX_RECEIPT_BYTES {
                return kept;
            }
        }
        if let Some(object) = receipt.as_object_mut() {
            object.remove("next");
        }
        receipt["limitations"] = if evaluation_completed {
            json!([
                "Continuation metadata exceeded the receipt limit; inspect the ordinary tool result to continue."
            ])
        } else {
            json!([
                "Continuation metadata exceeded the receipt limit; inspect the ordinary tool result to continue.",
                "Context retrieval failed; classification was not run."
            ])
        };
    }
    receipt
}

fn value_receipt(state: &Value) -> Value {
    let evidence_hash = crate::digest::json_sha256(&state);
    json!({
        "source":{"evidenceHash":evidence_hash},
        "resultHash":evidence_hash,
        "coverage":"bounded"
    })
}

/// Page axes nested inside an outer page. The last inner page re-emits the
/// outer continuation, so following the inner axis first visits every branch;
/// following the outer axis first drops the remaining inner pages for good.
const INNER_PAGE_AXES: &[&str] = &["nextMatchPage"];

/// Pages that re-run the shown page with wider analysis instead of reading
/// past it (lspSearch `expandFeatures`: every Cargo feature; astSearch
/// `expandCaptures`: whole captures). Their result repeats every candidate
/// already judged, so clasify discloses them and never follows one.
const RERUN_PAGE_AXES: &[&str] = &["expandFeatures", "expandCaptures"];

/// Select the canonical same-resource continuation from a body-free receipt:
/// an inner page axis first, else the first entry. Map iteration is stable,
/// so repeated runs choose the same axis.
pub(super) fn continuation(receipt: &Value) -> Option<Value> {
    let (_, continuation) = main_axis(receipt.get("next")?.as_object()?)?;
    let continuation = continuation.as_object()?;
    Some(json!({
        "tool": continuation.get("tool")?,
        "query": continuation.get("query")?
    }))
}

/// The canonical continuation entry of a `next` map: an inner page axis
/// first, else the first entry.
fn main_axis(next: &Map<String, Value>) -> Option<(&String, &Value)> {
    INNER_PAGE_AXES
        .iter()
        .find_map(|axis| next.get_key_value(*axis))
        .or_else(|| next.iter().next())
}

/// Whether the receipt still has an inner page (e.g. more matches of the
/// page's files) to visit before its outer page advances.
pub(super) fn has_inner_page(receipt: &Value) -> bool {
    receipt
        .get("next")
        .and_then(Value::as_object)
        .is_some_and(|next| INNER_PAGE_AXES.iter().any(|axis| next.contains_key(*axis)))
}

/// Select a named same-resource continuation. Hydrated search candidates use
/// only `nextPage`: `nextMatchPage` revisits files already hydrated and judged.
pub(super) fn continuation_named(receipt: &Value, name: &str) -> Option<Value> {
    let continuation = receipt
        .get("next")
        .and_then(Value::as_object)?
        .get(name)?
        .as_object()?;
    Some(json!({
        "tool": continuation.get("tool")?,
        "query": continuation.get("query")?
    }))
}

/// Recover from a context-tool error only when the tool supplied an exact,
/// validated continuation. Candidate continuations remain evidence hints and
/// must not silently replace a failed request.
/// A `restart` re-reads a changed source from its start; following it would
/// mix two versions of one resource, so the failure stands.
pub(super) fn exact_continuation(receipt: &Value) -> Option<Value> {
    let continuation = receipt
        .get("next")
        .and_then(Value::as_object)?
        .iter()
        .filter(|(name, _)| name.as_str() != "restart")
        .map(|(_, candidate)| candidate)
        .find(|candidate| candidate.get("confidence").and_then(Value::as_str) == Some("exact"))?
        .as_object()?;
    Some(json!({
        "tool": continuation.get("tool")?,
        "query": continuation.get("query")?
    }))
}

/// What [`inspect`] collects from one tool result.
struct Inspection<'a> {
    /// Same-resource continuations that pass input validation.
    next: &'a mut Map<String, Value>,
    /// `next.<name>` of same-resource continuations that fail it.
    dropped: &'a mut Vec<String>,
    /// `next.<name>` of overlapping reruns ([`RERUN_PAGE_AXES`]), not followed.
    reruns: &'a mut Vec<String>,
    partial: &'a mut bool,
    terminal: &'a mut bool,
}

/// Collect same-resource continuations. Only calls to the source tool can
/// continue this resource; cross-tool drill-downs (e.g. ghSearchHistory
/// `readPullRequest` → ghGetHistoryItem) are suggestions, not remaining coverage.
fn inspect(
    value: &Value,
    source_tool: &str,
    source_operation: Option<&str>,
    found: &mut Inspection<'_>,
) {
    match value {
        Value::Object(object) => {
            if object.get("terminalLimit") == Some(&Value::Bool(true)) {
                *found.terminal = true;
                *found.partial = true;
            }
            if object.get("partial") == Some(&Value::Bool(true))
                || object.get("status").and_then(Value::as_str) == Some("partial")
            {
                *found.partial = true;
            }
            if let Some(candidates) = object.get("next").and_then(Value::as_object) {
                for (name, candidate) in candidates {
                    let Some((tool, id)) = candidate
                        .get("tool")
                        .and_then(Value::as_str)
                        .filter(|tool| *tool == source_tool)
                        .and_then(|tool| Some((tool, ToolId::from_name(tool)?)))
                        .filter(|(_, id)| is_context_tool(*id))
                    else {
                        continue;
                    };
                    // A lead is an optional follow-up, not more of this
                    // result (contract `continuationChannels`); history menus
                    // keep their own rule (`is_history_expansion`).
                    if id != ToolId::GhGetHistoryItem
                        && crate::tools::id::channel(id, name) == crate::tools::id::Channel::Lead
                    {
                        continue;
                    }
                    if RERUN_PAGE_AXES.contains(&crate::tools::id::kind(name)) {
                        found.reruns.push(format!("next.{name}"));
                        *found.partial = true;
                        continue;
                    }
                    // A tool's continuation is a complete input; a clasify
                    // receipt walks its one row as a resource read.
                    let Some(query) = crate::tools::result::continuation_row(candidate) else {
                        continue;
                    };
                    // A discovery handoff (code search → repository tree) is not
                    // another page of the same evidence.
                    if source_operation.is_some_and(|operation| {
                        query
                            .get("operation")
                            .and_then(Value::as_str)
                            .is_some_and(|next| next != operation)
                    }) || is_history_expansion(name, tool, query)
                    {
                        continue;
                    }
                    if prepare(tool, query).is_err() {
                        found.dropped.push(format!("next.{name}"));
                        continue;
                    }
                    let mut continuation = json!({"tool":tool,"query":query});
                    if let Some(confidence) = candidate
                        .get("confidence")
                        .and_then(Value::as_str)
                        .filter(|s| matches!(*s, "exact" | "candidate"))
                    {
                        continuation["confidence"] = json!(confidence);
                    }
                    let mut key = name.clone();
                    let mut index = 2;
                    while found.next.contains_key(&key) {
                        key = format!("{name}{index}");
                        index += 1;
                    }
                    found.next.insert(key, continuation);
                }
            }
            for (key, value) in object {
                if key != "next" {
                    inspect(value, source_tool, source_operation, found);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                inspect(value, source_tool, source_operation, found);
            }
        }
        _ => {}
    }
}

fn is_history_expansion(name: &str, tool: &str, query: &Value) -> bool {
    tool == ToolId::GhGetHistoryItem.as_str()
        && query.get("operation").and_then(Value::as_str) == Some("pullRequest")
        && matches!(
            name,
            "readBody" | "readFiles" | "readSelectedPatches" | "readPatches" | "readDiscussion"
        )
        && query.as_object().is_some_and(|query| {
            // Menus are fresh first-page reads, even with formatting, sizing,
            // or file selectors. Explicit cursors can continue the same view.
            ![
                "offset",
                "filePage",
                "commentPage",
                "reviewPage",
                "commitPage",
                "page",
            ]
            .iter()
            .any(|key| query.contains_key(*key))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_match_windows_keep_each_source_range() {
        let state = json!({"results":[{"data":{
            "path":"/tmp/example.rs","content":"first\nsecond\n",
            "totalLines":80,
            "sourceLineRanges":[{"line":4,"endLine":4},{"line":63,"endLine":63}]
        }}]});
        assert_eq!(
            page_scope(&state),
            Some(json!({
                "lineRanges":[
                    {"startLine":4,"endLine":4},
                    {"startLine":63,"endLine":63}
                ],
                "totalLines":80
            }))
        );
    }

    #[test]
    fn transformed_view_offsets_are_not_source_scope() {
        let state = json!({"root":"/repo/docs","results":[{"data":{
            "path":"Hooks.md", "content":"3\t## Hooks\n467\t### onClose\n",
            "contentView":"symbols", "totalLines":954, "returnedLines":2,
            "modified":"2026-09-25T01:00:00Z"
        }}]});
        assert!(page_scope(&state).is_none());
        assert_eq!(
            page_view(&state),
            Some(json!({
                "kind":"symbols","startLine":1,"endLine":2,"totalLines":2
            }))
        );
        let receipt = receipt("localFetch", &state);
        assert!(receipt.get("scope").is_none());
        assert_eq!(receipt["source"]["path"], "/repo/docs/Hooks.md");
        assert_eq!(receipt["source"]["modified"], "2026-09-25T01:00:00Z");
        assert_eq!(receipt["source"]["evidenceHash"], receipt["resultHash"]);
    }

    #[test]
    fn github_source_identity_includes_repository_and_ref() {
        let state = json!({"results":[{"data":{
            "owner":"fastify", "repo":"fastify",
            "path":"docs/Reference/Hooks.md", "content":"# Hooks\n",
            "totalLines":1, "returnedLines":1,
            "commitSha":"resolved-commit"
        }}]});
        let mut receipt = receipt("ghGetFileContent", &state);
        assert_eq!(receipt["source"]["ref"], "resolved-commit");
        attach_requested_reference(
            &json!({"tool":"ghGetFileContent","query":{
                "ref":"6ed472b6fe023cd0e6283badda231b84e45582d0"
            }}),
            &mut receipt,
        );
        assert_eq!(
            receipt["source"]["path"],
            "fastify/fastify/docs/Reference/Hooks.md"
        );
        assert_eq!(receipt["source"]["ref"], "resolved-commit");
        receipt["source"]["ref"] = json!("returned-commit");
        attach_requested_reference(&json!({"query":{"ref":"main"}}), &mut receipt);
        assert_eq!(receipt["source"]["ref"], "returned-commit");
    }

    #[test]
    fn transformed_view_keeps_explicit_verified_source_ranges() {
        let state = json!({"results":[{"data":{
            "path":"Server.md", "content":"x\n", "contentView":"standard",
            "totalLines":2458, "returnedLines":1,
            "sourceLineRanges":[{"line":641,"endLine":680}]
        }}]});
        assert_eq!(
            page_scope(&state),
            Some(json!({
                "startLine":641,"endLine":680,"totalLines":2458
            }))
        );
        assert_eq!(page_view(&state).unwrap()["kind"], "standard");
    }

    #[test]
    fn first_github_file_page_uses_its_nested_file_scope() {
        let state = json!({"results":[{"data":{
            "owner":"expressjs","repo":"express","files":[{
                "path":"lib/application.js","totalLines":631,
                "sourceLineRanges":[{"line":1,"endLine":100}],
                "pagination":{"unit":"lines","offset":0,"length":100,"hasMore":true}
            }]
        }}]});
        assert_eq!(
            page_scope(&state),
            Some(json!({
                "startLine":1,"endLine":100,"totalLines":631
            }))
        );
    }

    #[test]
    fn nested_context_uses_the_canonical_query_contract() {
        for query in [
            json!({}),
            json!({"queries":[{"path":"/tmp/f","mainGoal": "test", "reasoning":"Read"}]}),
            json!({"path":"/tmp/f","mainGoal": "test", "reasoning":"Read","responseOffset":0}),
            json!({"cursor":"opaque"}),
        ] {
            assert!(prepare("localFetch", &query).is_err(), "{query}");
        }
        assert!(
            prepare(
                "localFetch",
                &json!({"path":"/tmp/f","mainGoal": "test", "reasoning":"Read"})
            )
            .is_ok()
        );
        assert!(
            prepare("localFetch", &json!({"path":"/tmp/f"})).is_ok(),
            "a nested read needs no brief"
        );
    }

    #[test]
    fn nested_context_cannot_materialize_tree_files() {
        let query = json!({
            "owner":"o", "repo":"r",
            "mainGoal": "test", "reasoning":"Inspect tree", "materialize":true
        });
        let error = prepare("ghStructure", &query).expect_err("materialization writes files");
        assert_eq!(error.code, "invalidClassificationContext");
        assert!(error.message.contains("materialize"));
        assert!(
            prepare(
                "ghStructure",
                &json!({
                    "owner":"o", "repo":"r",
                    "mainGoal": "test", "reasoning":"Inspect tree", "materialize":false
                })
            )
            .is_ok()
        );
    }

    #[test]
    fn contract_violation_in_nested_context_includes_field_details() {
        let err = prepare("localFetch", &json!({"path":42}))
            .expect_err("non-string path must be rejected");
        assert_eq!(err.code, "invalidClassificationContext");
        assert!(
            err.message.contains("path"),
            "error message must name the failing field; got: {}",
            err.message
        );
        let err_unknown = prepare(
            "localFetch",
            &json!({"path":"/tmp/f","mainGoal": "test", "reasoning":"r","typo":1}),
        )
        .expect_err("unknown field must be rejected");
        assert_eq!(err_unknown.code, "invalidClassificationContext");
        assert!(
            err_unknown.message.contains("typo") || err_unknown.message.contains("localFetch"),
            "error must surface field or tool context; got: {}",
            err_unknown.message
        );
    }

    #[test]
    fn artifact_pages_are_valid_context_and_receipt_continuations() {
        let artifact = json!({"ecosystem":"npm","keywords":["parser"],"mainGoal": "test", "reasoning":"Find packages","page":2,"pageSize":2});
        assert!(prepare("artifactSearch", &artifact).is_ok());
        let receipt = receipt(
            "artifactSearch",
            &json!({"pagination":{"hasMore":true},"next":{"nextPage":{"tool":"artifactSearch","query":{"queries":[artifact]}}}}),
        );
        assert_eq!(receipt["next"]["nextPage"]["query"], artifact);
    }

    #[test]
    fn failed_context_recovery_requires_an_exact_executable_continuation() {
        let exact = json!({"next":{"continue":{"tool":"localFetch","confidence":"exact","query":{
            "path":"/tmp/f","mainGoal": "test", "reasoning":"Recover","offset":0,"length":100
        }}}});
        assert_eq!(
            exact_continuation(&exact),
            Some(json!({"tool":"localFetch","query":{
                "path":"/tmp/f","mainGoal": "test", "reasoning":"Recover","offset":0,"length":100
            }}))
        );
        let candidate = json!({"next":{"continue":{"tool":"localFetch","confidence":"candidate","query":{
            "path":"/tmp/f","mainGoal": "test", "reasoning":"Guess","offset":0,"length":100
        }}}});
        assert!(exact_continuation(&candidate).is_none());
        assert!(exact_continuation(&json!({"next":{}})).is_none());
    }

    #[test]
    fn an_invalid_same_resource_continuation_is_named_not_silently_dropped() {
        let state = json!({"results":[{"index":0,"data":{"content":"x","next":{
            "continue":{"tool":"localFetch","query":{"queries":[{}]}}
        }}}]});
        let receipt = receipt("localFetch", &state);
        assert_eq!(receipt["coverage"], "partial", "{receipt}");
        assert!(receipt.get("next").is_none(), "{receipt}");
        assert!(
            receipt["limitations"]
                .as_array()
                .is_some_and(|limitations| limitations.iter().any(|limitation| limitation
                    .as_str()
                    .is_some_and(|text| text.contains("next.continue")))),
            "{receipt}"
        );
    }

    /// A lead (contract `continuationChannels`) is an optional follow-up the
    /// response stage moves to `hints`, not more of this result: following
    /// it re-judged the same candidates (astTopology `readDiagnostics`
    /// re-ran the dependents page with diagnostic rows).
    #[test]
    fn leads_are_not_same_resource_continuations() {
        let rerun = json!({"operation":"dependents","path":"/repo","source":"/repo/a.rs",
            "diagnosticPage":1,"diagnosticSnapshot":"a".repeat(64)});
        assert!(prepare("astTopology", &rerun).is_ok());
        let state = json!({"results":[{"index":0,"data":{
            "results":[{"file":"b.rs","importLine":3,"via":"a.rs"}],
            "next":{"readDiagnostics":{"tool":"astTopology","query":{"queries":[rerun]}}}
        }}]});
        let receipt = receipt("astTopology", &state);
        assert!(receipt.get("next").is_none(), "{receipt}");
        assert!(continuation(&receipt).is_none(), "{receipt}");
        assert_eq!(receipt["coverage"], "bounded", "{receipt}");
    }

    /// A page that re-runs the shown page with wider analysis (lspSearch
    /// `expandFeatures`) repeats every judged candidate: Scout would judge
    /// them twice. It is disclosed, never followed; a real page still is.
    #[test]
    fn overlapping_reruns_are_disclosed_not_followed() {
        let row =
            json!({"path":"/repo/a.rs","operation":"references","symbolName":"f","lineHint":3});
        let mut rerun = row.clone();
        rerun["rustContext"] = json!({"features":"all"});
        assert!(prepare("lspSearch", &rerun).is_ok());
        let state = json!({"results":[{"index":0,"data":{"isPartial":true,
            "payload":{"kind":"references","files":[{"path":"b.rs","matches":[{"line":4}]}]},
            "next":{"expandFeatures":{"tool":"lspSearch","query":{"queries":[rerun]}}}
        }}]});
        let first = receipt("lspSearch", &state);
        assert!(continuation(&first).is_none(), "{first}");
        assert_eq!(first["coverage"], "partial", "{first}");
        assert!(
            first["limitations"]
                .to_string()
                .contains("next.expandFeatures"),
            "{first}"
        );
        let mut paged = state.clone();
        let mut page = row;
        page["importerPage"] = json!(2);
        paged["results"][0]["data"]["next"] = json!({
            "expandFeatures": state["results"][0]["data"]["next"]["expandFeatures"].clone(),
            "nextImporterPage": {"tool":"lspSearch","query":{"queries":[page.clone()]}}
        });
        let receipt = receipt("lspSearch", &paged);
        assert_eq!(
            continuation(&receipt),
            Some(json!({"tool":"lspSearch","query":page})),
            "{receipt}"
        );
    }

    /// A failed or empty read keeps the read tool's own recovery: its prose
    /// hints and each lead it offered, as the exact call to run.
    #[test]
    fn failed_and_empty_reads_keep_the_tool_recovery_hints() {
        let tree = json!({"tool":"structureSearch","query":{"queries":[{"operation":"tree","path":"repo"}]}});
        let failed = Read {
            state: json!({"results":[{"index":0,"status":"error","data":{
                "error":"Path does not exist: repo/nope","errorCode":"pathNotFound",
                "next":{"viewTree":tree}
            }}]}),
            failed: true,
            empty: false,
            failure: None,
        };
        let error = read_error("localSearch", &failed, &json!({}));
        assert_eq!(error.code, "pathNotFound");
        assert!(
            error
                .hints
                .iter()
                .any(|hint| hint.contains("next.viewTree") && hint.contains("structureSearch")),
            "{:?}",
            error.hints
        );
        let empty = json!({"results":[{"index":0,"status":"empty","data":{
            "pullRequests":[],"hints":["Try fewer keywords."],
            "next":{"broadenSearch":{"tool":"ghSearchHistory","query":{"queries":[{"operation":"pullRequest","owner":"o","repo":"r"}]}}}
        }}]});
        let hints = tool_hints(&empty, &json!({}));
        assert_eq!(hints[0], "Try fewer keywords.", "{hints:?}");
        assert!(
            hints[1].starts_with("next.broadenSearch: run ghSearchHistory "),
            "{hints:?}"
        );
        assert!(tool_hints(&json!({"results":[{"data":{}}]}), &json!({})).is_empty());
    }

    /// A lead in an error hint reads as the agent would send it: no
    /// `debug:false`, no brief, and not the page size clasify ran the read
    /// with. A paging lead keeps its size (its page offset depends on it).
    /// Each rendered lead still passes its tool's input contract.
    #[test]
    fn error_leads_render_without_clasify_internal_fields() {
        let run = json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["zz"],
            "pageSize":12,"debug":false,"mainGoal":"g","reasoning":"why"});
        let broaden = json!({"operation":"pullRequest","debug":false,"pageSize":12,
            "mainGoal":"g","reasoning":"why","owner":"o","repo":"r"});
        let paged = json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["zz"],
            "page":2,"pageSize":12,"debug":false});
        let state = json!({"results":[{"index":0,"status":"empty","data":{"pullRequests":[],"next":{
            "broadenSearch":{"tool":"ghSearchHistory","query":{"queries":[broaden]}},
            "nextPage":{"tool":"ghSearchHistory","query":{"queries":[paged]}}
        }}}]});
        let hints = tool_hints(&state, &run);
        let lead = |name: &str| -> Value {
            let prefix = format!("next.{name}: run ghSearchHistory ");
            let hint = hints
                .iter()
                .find_map(|hint| hint.strip_prefix(&prefix))
                .unwrap_or_else(|| panic!("{name}: {hints:?}"));
            serde_json::from_str(hint).expect("lead query is JSON")
        };
        assert_eq!(
            lead("broadenSearch"),
            json!({"queries":[{"operation":"pullRequest","owner":"o","repo":"r"}]}),
            "{hints:?}"
        );
        assert_eq!(
            lead("nextPage"),
            json!({"queries":[{"operation":"pullRequest","owner":"o","repo":"r",
                "keywords":["zz"],"page":2,"pageSize":12}]}),
            "{hints:?}"
        );
        for name in ["broadenSearch", "nextPage"] {
            contracts::prepare_many_and_validate("ghSearchHistory", lead(name))
                .unwrap_or_else(|error| panic!("{name}: {:?}", error.issues));
        }
    }

    #[test]
    fn coverage_receipts_never_copy_bodies_and_preserve_only_valid_continuations() {
        let state = json!({"results":[{"index":0,"data":{"content":"SECRET_BODY","isPartial":true,"next":{
            "continue":{"tool":"localFetch","query":{"queries":[{"path":"/tmp/f","mainGoal": "test", "reasoning":"Read","offset":2}]},"confidence":"exact","content":"SECRET_BODY"},
            "invalid":{"tool":"localFetch","query":{"queries":[{}]}},"effect":{"tool":"astRewrite","query":{"queries":[{}]}}
        }}}]});
        let receipt = receipt("localFetch", &state);
        assert_eq!(receipt["coverage"], "partial");
        assert_eq!(receipt["next"].as_object().unwrap().len(), 1);
        assert!(!receipt.to_string().contains("SECRET_BODY"));
        assert!(prepare("localFetch", &receipt["next"]["continue"]["query"]).is_ok());
        let bounded = receipt_for_terminal();
        assert_eq!(bounded["coverage"], "partial");
        assert!(bounded.get("next").is_none());
        assert!(
            bounded["limitations"][0]
                .as_str()
                .unwrap()
                .contains("terminal")
        );
    }

    /// An oversized continuation menu keeps the walk's own continuation, so
    /// the receipt stays executable; only the extra menu entries go.
    #[test]
    fn an_oversized_receipt_keeps_its_main_continuation() {
        let state = json!({"results":[{"index":0,"data":{"isPartial":true,"next":{
            "continue":{"tool":"localFetch","query":{"queries":[{"path":"/tmp/f","mainGoal":"test","reasoning":"Read","offset":2}]},"confidence":"exact"},
            "readBoundedLines":{"tool":"localFetch","query":{"queries":[{"path":format!("/tmp/{}", "a".repeat(MAX_RECEIPT_BYTES)),"mainGoal":"test","reasoning":"Read"}]}}
        }}}]});
        let receipt = receipt("localFetch", &state);
        assert!(receipt.to_string().len() <= MAX_RECEIPT_BYTES);
        let next = receipt["next"].as_object().expect("main continuation kept");
        assert_eq!(next.len(), 1, "{receipt}");
        assert_eq!(next["continue"]["query"]["offset"], 2);
        assert!(
            receipt["limitations"].to_string().contains("next.continue"),
            "{receipt}"
        );
    }

    fn receipt_for_terminal() -> Value {
        receipt(
            "astSearch",
            &json!({"terminalLimit":true,"content":"SECRET_BODY"}),
        )
    }

    fn history_expansions() -> Value {
        let mut next = Map::new();
        for (name, selection) in [
            ("readBody", json!({"sections":["body"]})),
            ("readFiles", json!({"sections":["files"]})),
            (
                "readSelectedPatches",
                json!({"sections":["patches"],"include":["a.rs"]}),
            ),
            ("readPatches", json!({"sections":["patches"]})),
            (
                "readDiscussion",
                json!({"sections":["comments","reviewComments","reviews"]}),
            ),
        ] {
            let mut query = json!({
                "operation":"pullRequest","owner":"example","repo":"repo","number":1,
                "mainGoal": "test", "reasoning":"Inspect selected evidence","debug":false
            });
            query
                .as_object_mut()
                .expect("query")
                .extend(selection.as_object().expect("selection").clone());
            next.insert(
                name.into(),
                json!({"tool":"ghGetHistoryItem","confidence":"exact","query":{"queries":[query]}}),
            );
        }
        Value::Object(next)
    }

    #[test]
    fn empty_code_search_does_not_replay_tree_discovery_as_a_continuation() {
        let state = json!({"results":[{"status":"empty","data":{"next":{
            "viewTree":{"tool":"ghStructure","confidence":"exact","query":{"queries":[{
                "mainGoal": "test", "reasoning":"Verify the repository scope","owner":"fastify",
                "repo":"fastify","path":"","pageSize":100
            }]}}
        }}}]});
        let receipt = receipt("ghSearchCode", &state);
        assert!(continuation(&receipt).is_none(), "{receipt}");
    }

    #[test]
    fn nested_match_pages_are_exhausted_before_the_file_page_advances() {
        let search = |page: u32, match_page: u32| {
            json!({"mainGoal": "test", "reasoning":"r","path":"/w","matchString":"marker","pageSize":1,
                "matchPageSize":1,"page":page,"matchPage":match_page})
        };
        // localSearch emits the outer axis first; following it would skip the
        // current file's remaining match rows for good.
        let state = json!({"results":[{"index":0,"data":{"next":{
            "nextPage":{"tool":"localSearch","confidence":"exact","query":{"queries":[search(2, 1)]}},
            "nextMatchPage":{"tool":"localSearch","confidence":"exact","query":{"queries":[search(1, 2)]}}
        }}}]});
        let receipt = receipt("localSearch", &state);
        let next = continuation(&receipt).expect("continuation");
        assert_eq!(next["query"]["page"], 1, "{next}");
        assert_eq!(next["query"]["matchPage"], 2, "{next}");
    }

    #[test]
    fn cross_tool_drill_downs_are_not_same_resource_continuations() {
        let search = json!({"mainGoal": "test", "reasoning":"r","operation":"pullRequest","owner":"o","repo":"r",
            "keywords":["k"],"page":2});
        let state = json!({"results":[{"index":0,"data":{"next":{
            "readPullRequest":{"tool":"ghGetHistoryItem","confidence":"low","query":{"queries":[{
                "mainGoal": "test", "reasoning":"r","operation":"pullRequest","owner":"o","repo":"r","number":7}]}},
            "nextPage":{"tool":"ghSearchHistory","confidence":"exact","query":{"queries":[search]}}
        }}}]});
        let receipt = receipt("ghSearchHistory", &state);
        let next = receipt["next"]
            .as_object()
            .expect("same-tool continuation kept");
        assert_eq!(next.len(), 1, "{receipt}");
        assert_eq!(next["nextPage"]["tool"], "ghSearchHistory");
    }

    #[test]
    fn complete_history_receipt_omits_unrequested_content_menu() {
        let state = json!({"pullRequests":[{"changedFiles":[{"path":"a.rs","patch":"SOURCE_BODY"}],
            "next":history_expansions(),"contentPagination":{"patches":{"hasMore":false}}}]});
        let compact = receipt("ghGetHistoryItem", &state);
        assert_eq!(compact["coverage"], "bounded");
        assert_eq!(
            compact["resultHash"],
            hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
                state.to_string().as_bytes()
            ))
        );
        assert!(compact.get("next").is_none());
        assert!(compact.get("limitations").is_none());
        assert!(!compact.to_string().contains("SOURCE_BODY"));
    }

    #[test]
    fn partial_history_receipt_preserves_all_page_axes_without_expansion_menu() {
        let mut state = json!({"pullRequests":[{"next":history_expansions(),
            "contentPagination":{"patches":{"hasMore":true}}}],"next":{}});
        for (name, field) in [
            ("continueBody", "offset"),
            ("continuePatch", "offset"),
            ("continueCommentBody", "offset"),
            ("continueReviewBody", "offset"),
            ("nextFilePage", "filePage"),
            ("nextCommentPage", "commentPage"),
            ("nextReviewPage", "reviewPage"),
            ("nextCommitPage", "commitPage"),
        ] {
            let mut action = history_expansions()["readBody"].clone();
            action["query"]["queries"][0][field] = json!(2);
            assert!(prepare("ghGetHistoryItem", &action["query"]["queries"][0]).is_ok());
            state["next"][name] = action;
        }
        let compact = receipt("ghGetHistoryItem", &state);
        assert_eq!(compact["coverage"], "partial");
        let kept = compact["next"].as_object().expect("page axes");
        let source = state["next"].as_object().expect("source axes");
        assert_eq!(kept.len(), source.len());
        for (name, action) in source {
            let row = kept.get(name).unwrap_or_else(|| panic!("dropped {name}"));
            assert_eq!(row["tool"], action["tool"], "{name}");
            assert_eq!(row["query"], action["query"]["queries"][0], "{name}");
            assert_eq!(row["confidence"], action["confidence"], "{name}");
        }
        assert!(
            compact["limitations"][0]
                .as_str()
                .unwrap()
                .contains("Only the returned tool page")
        );
    }

    #[test]
    fn history_filter_keeps_unfamiliar_paging_shapes_and_terminal_limits() {
        let mut state = json!({"next":history_expansions(),"terminalLimit":true});
        state["next"]["readBody"]["query"]["queries"][0]["offset"] = json!(2);
        let compact = receipt("ghGetHistoryItem", &state);
        assert_eq!(compact["coverage"], "partial");
        assert_eq!(compact["next"].as_object().unwrap().len(), 1);
        assert_eq!(
            compact["next"]["readBody"]["query"],
            state["next"]["readBody"]["query"]["queries"][0]
        );
        assert_eq!(compact["next"]["readBody"]["tool"], "ghGetHistoryItem");
        assert!(
            compact["limitations"][0]
                .as_str()
                .unwrap()
                .contains("terminal limit")
        );
        state["next"] = history_expansions();
        let compact = receipt("ghGetHistoryItem", &state);
        assert!(compact.get("next").is_none());
        assert_eq!(compact["coverage"], "partial");
        assert!(
            compact["limitations"][0]
                .as_str()
                .unwrap()
                .contains("terminal limit")
        );
    }

    fn line_paginated_state(offset: u64, length: u64, total_lines: u64, next_offset: u64) -> Value {
        let start = offset + 1;
        let end = offset + length;
        json!({
            "results": [{"data": {
                "content": "...",
                "totalLines": total_lines,
                "returnedLines": length,
                "isPartial": true,
                "pagination": {
                    "unit": "lines",
                    "offset": offset,
                    "length": length,
                    "length": length,
                    "totalLines": total_lines,
                    "totalBytes": 17454,
                    "hasMore": true,
                    "nextOffset": next_offset
                },
                "sourceLineRanges": [{"line":start,"endLine":end}],
                "next": {"continue": {"tool": "localFetch",
                    "query": {"path": "/tmp/f", "mainGoal": "test", "reasoning": "R", "offset": next_offset, "length": length}
                }}
            }}]
        })
    }

    fn byte_paginated_state(offset: u64, length: u64, total_bytes: u64, has_more: bool) -> Value {
        let next_offset: Value = if has_more {
            json!(offset + length)
        } else {
            json!(null)
        };
        json!({
            "results": [{"data": {
                "content": "...",
                "totalLines": 1,
                "isPartial": has_more,
                "pagination": {
                    "unit": "bytes",
                    "offset": offset,
                    "length": length,
                    "length": length,
                    "totalLines": 1,
                    "totalBytes": total_bytes,
                    "hasMore": has_more,
                    "nextOffset": next_offset
                },
                "sourceLineRanges": [{"line":1,"endLine":1}],
                "next": if has_more { json!({"continue": {"tool": "localFetch",
                    "query": {"path": "/tmp/f", "mainGoal": "test", "reasoning": "R",
                        "offset": offset + length, "length": length}
                }}) } else { json!(null) }
            }}]
        })
    }

    #[test]
    fn gh_file_nested_pagination_receipt_includes_line_scope() {
        let state = json!({
            "results": [{"data": {
                "owner": "o", "repo": "r",
                "files": [{
                    "path": "lib/a.js",
                    "content": "...",
                    "totalLines": 306,
                    "isPartial": true,
                    "pagination": {
                        "unit": "lines",
                        "offset": 100,
                        "length": 50,
                        "length": 50,
                        "totalLines": 306,
                        "totalBytes": 9039,
                        "hasMore": true,
                        "nextOffset": 150
                    },
                    "sourceLineRanges": [{"line":101,"endLine":150}]
                }]
            }}]
        });
        let r = receipt("ghGetFileContent", &state);
        assert_eq!(r["scope"]["startLine"], 101);
        assert_eq!(r["scope"]["endLine"], 150);
        assert_eq!(r["scope"]["totalLines"], 306);
    }

    #[test]
    fn line_paginated_receipt_includes_scope_start_end_total() {
        let r = receipt("localFetch", &line_paginated_state(0, 30, 479, 30));
        assert_eq!(r["scope"]["startLine"], 1, "page 0 starts at line 1");
        assert_eq!(r["scope"]["endLine"], 30);
        assert_eq!(r["scope"]["totalLines"], 479);
        assert!(r["scope"].get("byteOffset").is_none());

        let r2 = receipt("localFetch", &line_paginated_state(30, 30, 479, 60));
        assert_eq!(r2["scope"]["startLine"], 31, "page 1 starts at line 31");
        assert_eq!(r2["scope"]["endLine"], 60);
        assert_eq!(r2["scope"]["totalLines"], 479);
    }

    #[test]
    fn selected_line_window_scope_uses_the_full_source_total() {
        let state = json!({"results":[{"data":{
            "totalLines":763,
            "sourceLineRanges":[{"line":111,"endLine":240}],
            "pagination":{
                "unit":"lines",
                "offset":0,
                "length":130,
                "totalLines":130
            }
        }}]});
        let r = receipt("localFetch", &state);
        assert_eq!(r["scope"]["startLine"], 111);
        assert_eq!(r["scope"]["endLine"], 240);
        assert_eq!(r["scope"]["totalLines"], 763);
    }

    #[test]
    fn byte_chunked_line_selection_is_scoped_by_original_source_lines() {
        // Byte offsets of a line selection count from the selection, not the
        // file; only the source line range locates the judged evidence.
        let state = json!({"results":[{"data":{
            "totalLines":481,
            "startLine":230,
            "endLine":350,
            "sourceLineRanges":[{"line":230,"endLine":294}],
            "pagination":{
                "unit":"bytes",
                "offset":0,
                "length":11428,
                "totalBytes":16389,
                "hasMore":true,
                "nextOffset":11428
            }
        }}]});
        assert_eq!(
            receipt("localFetch", &state)["scope"],
            json!({"startLine":230,"endLine":294,"totalLines":481})
        );
    }

    #[test]
    fn byte_page_without_byte_totals_is_scoped_by_source_lines() {
        let state = json!({"results":[{"data":{
            "totalLines":481,
            "sourceLineRanges":[{"line":1,"endLine":93}],
            "pagination":{
                "unit":"bytes",
                "offset":0,
                "length":11428,
                "hasMore":true,
                "nextOffset":11428
            }
        }}]});
        assert_eq!(
            receipt("localFetch", &state)["scope"],
            json!({"startLine":1,"endLine":93,"totalLines":481})
        );
    }

    #[test]
    fn a_failed_context_dispatch_names_its_execution_failure() {
        let timeout = dispatch_failure("ghSearchCode", crate::runtime::ExecutionError::Timeout);
        assert_eq!(timeout.code, "timeout");
        assert!(
            timeout.message.contains("ghSearchCode"),
            "{}",
            timeout.message
        );
        assert!(timeout.message.contains("timed out"), "{}", timeout.message);
        let cancelled = dispatch_failure("localFetch", crate::runtime::ExecutionError::Cancelled);
        assert_eq!(cancelled.code, "cancelled");
        let worker = dispatch_failure("localFetch", crate::runtime::ExecutionError::WorkerFailed);
        assert_eq!(worker.code, "classificationContextFailed");
        assert!(
            worker.message.contains("workerFailed"),
            "{}",
            worker.message
        );
    }

    #[test]
    fn byte_paginated_receipt_includes_scope_offset_end_total() {
        let r = receipt("localFetch", &byte_paginated_state(0, 16384, 16500, true));
        assert_eq!(r["scope"]["byteOffset"], 0);
        assert_eq!(r["scope"]["byteEnd"], 16384);
        assert_eq!(r["scope"]["totalBytes"], 16500);
        assert!(r["scope"].get("startLine").is_none());

        let r2 = receipt(
            "localFetch",
            &byte_paginated_state(16384, 116, 16500, false),
        );
        assert_eq!(r2["scope"]["byteOffset"], 16384);
        assert_eq!(r2["scope"]["byteEnd"], 16500);
        assert_eq!(r2["scope"]["totalBytes"], 16500);
    }

    #[test]
    fn file_receipts_are_always_scoped_and_value_receipts_never() {
        let complete = json!({"results": [{"data": {
            "content": "all here", "totalLines": 5, "returnedLines": 5
        }}]});
        assert_eq!(
            receipt("localFetch", &complete)["scope"],
            json!({"startLine": 1, "endLine": 5, "totalLines": 5})
        );
        // A complete bounded read (no pagination emitted) still reports its window.
        let window = json!({"results": [{"data": {
            "content": "x", "totalLines": 132, "sourceLineRanges": [{"line":25,"endLine":31}]
        }}]});
        assert_eq!(
            receipt("localFetch", &window)["scope"],
            json!({"startLine": 25, "endLine": 31, "totalLines": 132})
        );
        assert!(value_receipt(&json!({"key": "val"})).get("scope").is_none());
    }
    #[test]
    fn include_history_expansions_do_not_expand_the_requested_evidence_scope() {
        let state = json!({"results":[{"data":{"pullRequests":[{"number":1,"next":{
            "readFiles":{"tool":"ghGetHistoryItem","query":{
                "operation":"pullRequest","owner":"o","repo":"r","number":1,
                "sections":["files","body"],"minify":"standard","pageSize":20,"mainGoal":"g","reasoning":"r"}},
            "readDiscussion":{"tool":"ghGetHistoryItem","query":{
                "operation":"pullRequest","owner":"o","repo":"r","number":1,
                "sections":["comments","reviews"],"minify":"standard","mainGoal":"g","reasoning":"r"}},
            "readSelectedPatches":{"tool":"ghGetHistoryItem","query":{
                "operation":"pullRequest","owner":"o","repo":"r","number":1,
                "sections":["patches"],"include":["a.rs"],"minify":"standard","mainGoal":"g","reasoning":"r"}}
        }}]}}]});
        let compact = receipt("ghGetHistoryItem", &state);
        assert!(compact.get("next").is_none(), "{compact}");
        assert_eq!(compact["coverage"], "bounded");
    }
    #[test]
    fn history_receipts_keep_the_requested_operation_without_an_output_echo() {
        let state = json!({"results":[{"data":{"type":"issues","issues":[{"number":1,"next":{
            "readMergedFix":{"tool":"ghGetHistoryItem","query":{
                "operation":"pullRequest","owner":"o","repo":"r","number":2,
                "mainGoal":"g","reasoning":"r"}}
        }}]}}]});
        let compact = receipt_with_evaluation("ghGetHistoryItem", &state, true, Some("issue"));
        assert!(compact.get("next").is_none(), "{compact}");
        assert_eq!(compact["coverage"], "bounded");
    }
}
