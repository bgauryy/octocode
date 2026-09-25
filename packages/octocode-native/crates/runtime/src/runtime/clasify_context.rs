//! Validated, bounded tool context; never re-enters public request admission.
use super::{ExecutionContext, domain_dispatch::DomainDispatcher, response};
use crate::{
    contracts::{self, PrepareOptions},
    tools::clasify::{is_context_tool, transport::ClassificationError},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
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
                super::ExecutionError::Timeout => "timeout",
                _ => "cancelled",
            },
            "Context execution stopped before classification.",
        )
    })
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

fn prepare(tool: &str, query: &Value) -> Result<Value, ClassificationError> {
    let object = query.as_object().ok_or_else(|| {
        error(
            "invalidClassificationContext",
            "Context query must be one ordinary query object.",
        )
    })?;
    if [
        "queries",
        "renderText",
        "responseCharOffset",
        "responseCharLength",
        "responseSnapshot",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
        || (object.len() == 1 && object.contains_key("cursor"))
    {
        return Err(error(
            "invalidClassificationContext",
            "Context query cannot contain a bulk envelope, cursor, or response paging options.",
        ));
    }
    if tool == "ghSearch" && query.get("materialize") == Some(&Value::Bool(true)) {
        return Err(error(
            "invalidClassificationContext",
            "Clasify context cannot materialize a repository tree; use ghSearch directly when files are needed locally.",
        ));
    }
    let mut queries =
        contracts::prepare_many_and_validate(tool, query.clone(), PrepareOptions::default())
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
                            "Context query does not satisfy the {tool} input contract: {detail}."
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
    let tool = source["tool"]
        .as_str()
        .filter(|tool| is_context_tool(tool))
        .ok_or_else(|| {
            error(
                "invalidClassificationContext",
                "Only read tools can provide classification context.",
            )
        })
        .map_err(ContextFailure::from)?;
    if !dispatcher.available_tools.contains(&tool) {
        let mut unavailable = error(
            "classificationContextUnavailable",
            format!("Context tool {tool} is disabled by runtime policy."),
        );
        // Retrying the disabled tool cannot help; name the enabled readers.
        unavailable.hints = vec![
            "Use an enabled read tool (localFetch, localSearch, ghGetFileContent) as this resource's context.".into(),
        ];
        return Err(ContextFailure::from(unavailable));
    }
    let prepared = prepare(tool, &source["query"]).map_err(ContextFailure::from)?;
    let checked_input = dispatcher.security.validate_input_parameters(&prepared);
    if !checked_input.is_valid {
        return Err(ContextFailure::from(error(
            "securityValidationFailed",
            "Context query is blocked by input security policy.",
        )));
    }
    let prepared = Value::Object(checked_input.sanitized_params);
    let result = dispatcher.execute(tool, &prepared, context).map_err(|_| {
        ContextFailure::from(error(
            "classificationContextFailed",
            format!("Context tool {tool} could not complete."),
        ))
    })?;
    checked(context).map_err(ContextFailure::from)?;
    let failed = result.failure.is_some() || result.status == Some("error");
    let empty = result.status == Some("empty");
    let mut row = response::result_row(tool, 0, &prepared, result.data, result.status);
    response::attach_diagnostics(&mut row, result.diagnostics);
    if result.cache {
        row["cache"] = json!(1);
    }
    response::apply_hint_policy(&mut row, tool, &prepared);
    let mut state = response::envelope(vec![row]);
    response::attach_query_base(&mut state, tool, &prepared);
    response::finalize_output_fields(
        &mut state,
        tool,
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
    contracts::validate_output(tool, &state).map_err(|_| {
        ContextFailure::from(error(
            "classificationContextContractViolation",
            format!("Context tool {tool} returned invalid output."),
        ))
    })?;
    if empty {
        return Err(ContextFailure {
            error: error(
                "classificationContextEmpty",
                format!("Context tool {tool} returned no evidence; classification was not called."),
            ),
            receipt: Some(receipt(tool, &state)),
        });
    }
    if failed {
        let receipt = failed_receipt(tool, &state);
        let code = state
            .pointer("/results/0/data/errorCode")
            .and_then(Value::as_str)
            .unwrap_or("classificationContextFailed");
        // Surface the read tool's own (already sanitized) reason and repair
        // hints; a bare "returned an error" hides e.g. a sandbox refusal.
        let reason = state
            .pointer("/results/0/data/error")
            .and_then(Value::as_str)
            .map_or_else(String::new, |reason| format!(": {reason}"));
        let mut failure = error(
            code,
            format!("Context tool {tool} failed{reason}; classification was not called."),
        );
        if let Some(hints) = state
            .pointer("/results/0/data/hints")
            .and_then(Value::as_array)
        {
            let hints = hints
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if !hints.is_empty() {
                failure.hints = hints;
            }
        }
        return Err(ContextFailure {
            error: failure,
            receipt: Some(receipt),
        });
    }
    checked(context).map_err(ContextFailure::from)?;
    let mut receipt = receipt(tool, &state);
    attach_requested_reference(tool, source, &mut receipt);
    Ok((state, Some(receipt)))
}

fn attach_requested_reference(tool: &str, request: &Value, receipt: &mut Value) {
    if receipt.pointer("/source/ref").is_some() {
        return;
    }
    let field = if tool == "ghGetFileContent" {
        "branch"
    } else {
        "ref"
    };
    if let Some(reference) = request
        .get("query")
        .and_then(|query| query.get(field))
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
                    "startLine":range.get("start")?.as_u64()?,
                    "endLine":range.get("end")?.as_u64()?
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
            .and_then(|r| Some((r.get("start")?.as_u64()?, r.get("end")?.as_u64()?)))
    };
    // Reads omit `pagination` when one page covers the whole view, and omit
    // `length`/view totals when they equal `chunkSize`/the source totals.
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
    let length = || {
        pagination
            .get("length")
            .or_else(|| pagination.get("chunkSize"))?
            .as_u64()
    };
    let chunk_type = pagination.get("chunkType").and_then(Value::as_str)?;
    match chunk_type {
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
            let byte_offset = pagination.get("offset")?.as_u64()?;
            let byte_end = pagination
                .get("nextOffset")
                .and_then(Value::as_u64)
                .or_else(|| Some(byte_offset + length()?))?;
            let total_bytes = pagination
                .get("totalBytes")
                .or_else(|| data.get("sourceBytes"))?
                .as_u64()?;
            Some(json!({"byteOffset": byte_offset, "byteEnd": byte_end, "totalBytes": total_bytes}))
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
        if pagination["chunkType"] == "lines" {
            if let (Some(offset), Some(length)) = (
                pagination["offset"].as_u64(),
                pagination["length"]
                    .as_u64()
                    .or_else(|| pagination["chunkSize"].as_u64()),
            ) && length > 0
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
    let file = data
        .get("files")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .unwrap_or(data);
    let mut source = json!({"evidenceHash":evidence_hash});
    if let Some(path) = file.get("path").and_then(Value::as_str) {
        let identity = match tool {
            "localFetch" => state
                .get("base")
                .and_then(Value::as_str)
                .filter(|_| !Path::new(path).is_absolute())
                .map_or_else(
                    || path.to_owned(),
                    |base| Path::new(base).join(path).to_string_lossy().into_owned(),
                ),
            "ghGetFileContent" => match (data["owner"].as_str(), data["repo"].as_str()) {
                (Some(owner), Some(repo)) => format!("{owner}/{repo}/{path}"),
                _ => path.to_owned(),
            },
            _ => path.to_owned(),
        };
        source["path"] = json!(identity);
    }
    if let Some(modified) = file.get("modified").and_then(Value::as_str) {
        source["modified"] = json!(modified);
    }
    if let Some(reference) = file
        .get("ref")
        .or_else(|| data.get("ref"))
        .and_then(Value::as_str)
    {
        source["ref"] = json!(reference);
    }
    source
}

fn append_limitation(receipt: &mut Value, limitation: &str) {
    match receipt.get_mut("limitations").and_then(Value::as_array_mut) {
        Some(limitations) => limitations.push(json!(limitation)),
        None => receipt["limitations"] = json!([limitation]),
    }
}

fn receipt(tool: &str, state: &Value) -> Value {
    receipt_with_evaluation(tool, state, true)
}

fn failed_receipt(tool: &str, state: &Value) -> Value {
    receipt_with_evaluation(tool, state, false)
}

fn receipt_with_evaluation(tool: &str, state: &Value, evaluation_completed: bool) -> Value {
    let mut next = Map::new();
    let mut terminal = false;
    let mut partial = response::is_partial(state);
    inspect(state, tool, &mut next, &mut partial, &mut terminal);
    let evidence_hash = hex::encode(Sha256::digest(state.to_string().as_bytes()));
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
    if !evaluation_completed {
        append_limitation(
            &mut receipt,
            "Context retrieval failed; classification was not run.",
        );
    }
    if receipt.to_string().len() > MAX_RECEIPT_BYTES {
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
    let evidence_hash = hex::encode(Sha256::digest(state.to_string().as_bytes()));
    json!({
        "source":{"evidenceHash":evidence_hash},
        "resultHash":evidence_hash,
        "coverage":"bounded"
    })
}

/// Select the first canonical same-resource continuation from a body-free
/// receipt. Map iteration is stable, so repeated runs choose the same axis.
pub(super) fn continuation(receipt: &Value) -> Option<Value> {
    let continuation = receipt
        .get("next")
        .and_then(Value::as_object)
        .and_then(|next| next.values().next())
        .and_then(Value::as_object)?;
    Some(json!({
        "tool": continuation.get("tool")?,
        "query": continuation.get("query")?
    }))
}

/// Recover from a context-tool error only when the tool supplied an exact,
/// validated continuation. Candidate continuations remain evidence hints and
/// must not silently replace a failed request.
pub(super) fn exact_continuation(receipt: &Value) -> Option<Value> {
    let continuation = receipt
        .get("next")
        .and_then(Value::as_object)?
        .values()
        .find(|candidate| candidate.get("confidence").and_then(Value::as_str) == Some("exact"))?
        .as_object()?;
    Some(json!({
        "tool": continuation.get("tool")?,
        "query": continuation.get("query")?
    }))
}

/// Collect same-resource continuations. Only calls to the source tool can
/// continue this resource; cross-tool drill-downs (e.g. ghSearchHistory
/// `readPr` → ghGetHistoryItem) are suggestions, not remaining coverage.
fn inspect(
    value: &Value,
    source_tool: &str,
    next: &mut Map<String, Value>,
    partial: &mut bool,
    terminal: &mut bool,
) {
    match value {
        Value::Object(object) => {
            if object.get("terminalLimit") == Some(&Value::Bool(true)) {
                *terminal = true;
                *partial = true;
            }
            if object.get("partial") == Some(&Value::Bool(true))
                || object.get("limitReached") == Some(&Value::Bool(true))
                || object.get("status").and_then(Value::as_str) == Some("partial")
            {
                *partial = true;
            }
            if let Some(candidates) = object.get("next").and_then(Value::as_object) {
                for (name, candidate) in candidates {
                    let Some(tool) = candidate
                        .get("tool")
                        .and_then(Value::as_str)
                        .filter(|tool| *tool == source_tool && is_context_tool(tool))
                    else {
                        continue;
                    };
                    let Some(query) = candidate.get("query") else {
                        continue;
                    };
                    if is_history_expansion(name, tool, query) {
                        continue;
                    }
                    if prepare(tool, query).is_err() {
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
                    while next.contains_key(&key) {
                        key = format!("{name}{index}");
                        index += 1;
                    }
                    next.insert(key, continuation);
                }
            }
            for (key, value) in object {
                if key != "next" {
                    inspect(value, source_tool, next, partial, terminal);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                inspect(value, source_tool, next, partial, terminal);
            }
        }
        _ => {}
    }
}

fn is_history_expansion(name: &str, tool: &str, query: &Value) -> bool {
    tool == "ghGetHistoryItem"
        && query.get("operation").and_then(Value::as_str) == Some("pullRequest")
        && matches!(
            name,
            "getBody"
                | "getChangedFiles"
                | "getSelectedPatches"
                | "getAllPatches"
                | "getComments"
                | "getReviews"
                | "getCommits"
        )
        && query.as_object().is_some_and(|query| {
            query.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "operation" | "owner" | "repo" | "number" | "content" | "reasoning" | "debug"
                )
            })
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
            "sourceLineRanges":[{"start":4,"end":4},{"start":63,"end":63}]
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
        let state = json!({"base":"/repo/docs","results":[{"data":{
            "path":"Hooks.md", "content":"3| ## Hooks\n467| ### onClose\n",
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
            "owner":"fastify", "repo":"fastify", "files":[{
                "path":"docs/Reference/Hooks.md", "content":"# Hooks\n",
                "totalLines":1, "returnedLines":1
            }]
        }}]});
        let mut receipt = receipt("ghGetFileContent", &state);
        attach_requested_reference(
            "ghGetFileContent",
            &json!({"tool":"ghGetFileContent","query":{
                "branch":"6ed472b6fe023cd0e6283badda231b84e45582d0"
            }}),
            &mut receipt,
        );
        assert_eq!(
            receipt["source"]["path"],
            "fastify/fastify/docs/Reference/Hooks.md"
        );
        assert_eq!(
            receipt["source"]["ref"],
            "6ed472b6fe023cd0e6283badda231b84e45582d0"
        );
        receipt["source"]["ref"] = json!("returned-commit");
        attach_requested_reference(
            "ghGetFileContent",
            &json!({"query":{"branch":"main"}}),
            &mut receipt,
        );
        assert_eq!(receipt["source"]["ref"], "returned-commit");
    }

    #[test]
    fn transformed_view_keeps_explicit_verified_source_ranges() {
        let state = json!({"results":[{"data":{
            "path":"Server.md", "content":"x\n", "contentView":"standard",
            "totalLines":2458, "returnedLines":1,
            "sourceLineRanges":[{"start":641,"end":680}]
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
                "sourceLineRanges":[{"start":1,"end":100}],
                "pagination":{"chunkType":"lines","offset":0,"chunkSize":100,"hasMore":true}
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
            json!({"queries":[{"path":"/tmp/f","reasoning":"Read"}]}),
            json!({"path":"/tmp/f","reasoning":"Read","responseCharOffset":0}),
            json!({"cursor":"opaque"}),
        ] {
            assert!(prepare("localFetch", &query).is_err(), "{query}");
        }
        assert!(prepare("localFetch", &json!({"path":"/tmp/f","reasoning":"Read"})).is_ok());
        assert!(prepare("localFetch", &json!({"path":"/tmp/f"})).is_err());
    }

    #[test]
    fn nested_context_cannot_materialize_tree_files() {
        let query = json!({
            "operation":"tree", "owner":"o", "repo":"r",
            "reasoning":"Inspect tree", "materialize":true
        });
        let error = prepare("ghSearch", &query).expect_err("materialization writes files");
        assert_eq!(error.code, "invalidClassificationContext");
        assert!(error.message.contains("materialize"));
        assert!(
            prepare(
                "ghSearch",
                &json!({
                    "operation":"tree", "owner":"o", "repo":"r",
                    "reasoning":"Inspect tree", "materialize":false
                })
            )
            .is_ok()
        );
    }

    #[test]
    fn contract_violation_in_nested_context_includes_field_details() {
        let err = prepare("localFetch", &json!({"path":"/tmp/f"}))
            .expect_err("path-only localFetch must be rejected");
        assert_eq!(err.code, "invalidClassificationContext");
        assert!(
            err.message.contains("reasoning"),
            "error message must name the failing field; got: {}",
            err.message
        );
        let err_unknown = prepare(
            "localFetch",
            &json!({"path":"/tmp/f","reasoning":"r","typo":1}),
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
    fn artifact_domain_cursors_are_valid_context_and_receipt_continuations() {
        let artifact = json!({"type":"npm","keywords":["parser"],"reasoning":"Find packages","cursor":"provider-cursor","pageSize":2});
        assert!(prepare("artifactSearch", &artifact).is_ok());
        let receipt = receipt(
            "artifactSearch",
            &json!({"pagination":{"hasMore":true},"next":{"nextPage":{"tool":"artifactSearch","query":artifact}}}),
        );
        assert_eq!(receipt["next"]["nextPage"]["query"], artifact);
    }

    #[test]
    fn failed_context_recovery_requires_an_exact_executable_continuation() {
        let exact = json!({"next":{"continue":{"tool":"localFetch","confidence":"exact","query":{
            "path":"/tmp/f","reasoning":"Recover","offset":0,"chunkSize":100
        }}}});
        assert_eq!(
            exact_continuation(&exact),
            Some(json!({"tool":"localFetch","query":{
                "path":"/tmp/f","reasoning":"Recover","offset":0,"chunkSize":100
            }}))
        );
        let candidate = json!({"next":{"continue":{"tool":"localFetch","confidence":"candidate","query":{
            "path":"/tmp/f","reasoning":"Guess","offset":0,"chunkSize":100
        }}}});
        assert!(exact_continuation(&candidate).is_none());
        assert!(exact_continuation(&json!({"next":{}})).is_none());
    }

    #[test]
    fn coverage_receipts_never_copy_bodies_and_preserve_only_valid_continuations() {
        let state = json!({"results":[{"index":0,"data":{"content":"SECRET_BODY","isPartial":true,"next":{
            "continue":{"tool":"localFetch","query":{"path":"/tmp/f","reasoning":"Read","offset":2},"confidence":"exact","content":"SECRET_BODY"},
            "invalid":{"tool":"localFetch","query":{}},"effect":{"tool":"astRewrite","query":{}}
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

    fn receipt_for_terminal() -> Value {
        receipt(
            "astSearch",
            &json!({"terminalLimit":true,"content":"SECRET_BODY"}),
        )
    }

    fn history_expansions() -> Value {
        let mut next = Map::new();
        for (name, content) in [
            ("getBody", json!({"body":true})),
            ("getChangedFiles", json!({"changedFiles":true})),
            (
                "getSelectedPatches",
                json!({"patches":{"mode":"selected","files":["a.rs"]}}),
            ),
            ("getAllPatches", json!({"patches":{"mode":"all"}})),
            (
                "getComments",
                json!({"comments":{"discussion":true,"reviewInline":true}}),
            ),
            ("getReviews", json!({"reviews":true})),
            ("getCommits", json!({"commits":{}})),
        ] {
            next.insert(
                name.into(),
                json!({"tool":"ghGetHistoryItem","confidence":"exact","query":{
                    "operation":"pullRequest","owner":"example","repo":"repo","number":1,
                    "content":content,"reasoning":"Inspect selected evidence","debug":false
                }}),
            );
        }
        Value::Object(next)
    }

    #[test]
    fn cross_tool_drill_downs_are_not_same_resource_continuations() {
        let search = json!({"reasoning":"r","operation":"pullRequest","owner":"o","repo":"r",
            "keywords":["k"],"page":2});
        let state = json!({"results":[{"index":0,"data":{"next":{
            "readPr":{"tool":"ghGetHistoryItem","confidence":"low","query":{
                "reasoning":"r","operation":"pullRequest","owner":"o","repo":"r","number":7}},
            "nextPage":{"tool":"ghSearchHistory","confidence":"exact","query":search}
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
            hex::encode(Sha256::digest(state.to_string().as_bytes()))
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
            ("continueBody", "charOffset"),
            ("continuePatch", "charOffset"),
            ("continueCommentBody", "commentBodyOffset"),
            ("continueReviewBody", "charOffset"),
            ("nextChangedFilesPage", "filePage"),
            ("nextFilePathsPage", "filePage"),
            ("nextCommentsPage", "commentPage"),
            ("nextReviewsPage", "reviewPage"),
            ("nextCommitsPage", "commitPage"),
        ] {
            let mut action = history_expansions()["getBody"].clone();
            action["query"][field] = json!(2);
            assert!(prepare("ghGetHistoryItem", &action["query"]).is_ok());
            state["next"][name] = action;
        }
        let compact = receipt("ghGetHistoryItem", &state);
        assert_eq!(compact["coverage"], "partial");
        assert_eq!(compact["next"], state["next"]);
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
        state["next"]["getBody"]["query"]["charOffset"] = json!(2);
        let compact = receipt("ghGetHistoryItem", &state);
        assert_eq!(compact["coverage"], "partial");
        assert_eq!(compact["next"].as_object().unwrap().len(), 1);
        assert_eq!(compact["next"]["getBody"], state["next"]["getBody"]);
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
                    "chunkType": "lines",
                    "offset": offset,
                    "length": length,
                    "chunkSize": length,
                    "totalLines": total_lines,
                    "totalBytes": 17454,
                    "hasMore": true,
                    "nextOffset": next_offset
                },
                "sourceLineRanges": [{"start": start, "end": end}],
                "next": {"continue": {"tool": "localFetch",
                    "query": {"path": "/tmp/f", "reasoning": "R", "offset": next_offset, "chunkSize": length}
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
                    "chunkType": "bytes",
                    "offset": offset,
                    "length": length,
                    "chunkSize": length,
                    "totalLines": 1,
                    "totalBytes": total_bytes,
                    "hasMore": has_more,
                    "nextOffset": next_offset
                },
                "sourceLineRanges": [{"start": 1, "end": 1}],
                "next": if has_more { json!({"continue": {"tool": "localFetch",
                    "query": {"path": "/tmp/f", "reasoning": "R",
                        "offset": offset + length, "chunkSize": length}
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
                        "chunkType": "lines",
                        "offset": 100,
                        "length": 50,
                        "chunkSize": 50,
                        "totalLines": 306,
                        "totalBytes": 9039,
                        "hasMore": true,
                        "nextOffset": 150
                    },
                    "sourceLineRanges": [{"start": 101, "end": 150}]
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
            "sourceLineRanges":[{"start":111,"end":240}],
            "pagination":{
                "chunkType":"lines",
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
            "content": "x", "totalLines": 132, "sourceLineRanges": [{"start": 25, "end": 31}]
        }}]});
        assert_eq!(
            receipt("localFetch", &window)["scope"],
            json!({"startLine": 25, "endLine": 31, "totalLines": 132})
        );
        assert!(value_receipt(&json!({"key": "val"})).get("scope").is_none());
    }
}
