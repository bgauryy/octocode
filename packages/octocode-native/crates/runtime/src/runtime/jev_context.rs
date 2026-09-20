//! Validated, bounded tool context; never re-enters public request admission.
use super::{ExecutionContext, domain_dispatch::DomainDispatcher, response};
use crate::{
    contracts::{self, PrepareOptions},
    tools::jev::{is_context_tool, transport::JevProviderError},
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const MAX_RECEIPT_BYTES: usize = 16 * 1024;

fn error(code: &str, message: impl Into<String>) -> JevProviderError {
    JevProviderError {
        code: code.into(),
        message: message.into(),
        hints: vec!["Run the ordinary context tool to inspect or correct its request.".into()],
    }
}

fn checked(context: &ExecutionContext) -> Result<(), JevProviderError> {
    context.check().map_err(|failure| {
        error(
            match failure {
                super::ExecutionError::Timeout => "timeout",
                _ => "cancelled",
            },
            "Context execution stopped before Jev evaluation.",
        )
    })
}

pub(super) struct ContextFailure {
    pub error: JevProviderError,
    pub receipt: Option<Value>,
}

impl From<JevProviderError> for ContextFailure {
    fn from(error: JevProviderError) -> Self {
        Self {
            error,
            receipt: None,
        }
    }
}

fn prepare(tool: &str, query: &Value) -> Result<Value, JevProviderError> {
    let object = query.as_object().ok_or_else(|| {
        error(
            "invalidJevContext",
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
            "invalidJevContext",
            "Context query cannot contain a bulk envelope, cursor, or response paging options.",
        ));
    }
    let mut queries =
        contracts::prepare_many_and_validate(tool, query.clone(), PrepareOptions::default())
            .map_err(|_| {
                error(
                    "invalidJevContext",
                    format!("Context query does not satisfy the {tool} input contract."),
                )
            })?;
    if queries.len() != 1 {
        return Err(error(
            "invalidJevContext",
            "Context must contain exactly one ordinary query.",
        ));
    }
    queries
        .pop()
        .ok_or_else(|| error("invalidJevContext", "Context query is missing."))
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
                "invalidJevContext",
                "Only read tools can provide Jev context.",
            )
        })
        .map_err(ContextFailure::from)?;
    if !dispatcher.available_tools.contains(&tool) {
        return Err(ContextFailure::from(error(
            "jevContextUnavailable",
            format!("Context tool {tool} is disabled by runtime policy."),
        )));
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
            "jevContextFailed",
            format!("Context tool {tool} could not complete."),
        ))
    })?;
    checked(context).map_err(ContextFailure::from)?;
    let failed = result.failure.is_some() || result.status == Some("error");
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
            "jevContextFailed",
            "Context output sanitization failed.",
        ))
    })?;
    contracts::validate_output(tool, &state).map_err(|_| {
        ContextFailure::from(error(
            "jevContextContractViolation",
            format!("Context tool {tool} returned invalid output."),
        ))
    })?;
    if failed {
        let receipt = failed_receipt(tool, &state);
        let code = state
            .pointer("/results/0/data/errorCode")
            .and_then(Value::as_str)
            .unwrap_or("jevContextFailed");
        return Err(ContextFailure {
            error: error(
                code,
                format!("Context tool {tool} returned an error; Jev was not called."),
            ),
            receipt: Some(receipt),
        });
    }
    checked(context).map_err(ContextFailure::from)?;
    let receipt = receipt(tool, &state);
    Ok((state, Some(receipt)))
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
    inspect(state, &mut next, &mut partial, &mut terminal);
    let mut receipt = json!({"source":"tool","tool":tool,"resultHash":hex::encode(Sha256::digest(state.to_string().as_bytes())),"coverage":if partial {"partial"}else{"bounded"}});
    if !next.is_empty() {
        receipt["next"] = Value::Object(next);
    }
    if partial {
        let limitation = if terminal {
            "The context tool reported a terminal limit; this result does not cover all matching evidence."
        } else if receipt.get("next").is_some() {
            if evaluation_completed {
                "Only the returned tool page was evaluated; continue explicitly for additional evidence."
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
            "Context retrieval failed; Jev evaluation was not run.",
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
                "Context retrieval failed; Jev evaluation was not run."
            ])
        };
    }
    receipt
}

fn value_receipt(state: &Value) -> Value {
    json!({
        "source":"value",
        "resultHash":hex::encode(Sha256::digest(state.to_string().as_bytes())),
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

fn inspect(value: &Value, next: &mut Map<String, Value>, partial: &mut bool, terminal: &mut bool) {
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
                        .filter(|tool| is_context_tool(tool))
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
                    inspect(value, next, partial, terminal);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                inspect(value, next, partial, terminal);
            }
        }
        _ => {}
    }
}

fn is_history_expansion(name: &str, tool: &str, query: &Value) -> bool {
    // pr_next_menu offers unrequested content, not another page of captured evidence.
    // Keep unfamiliar shapes (including any paging fields) so this filter cannot
    // silently discard a continuation if the history contract evolves.
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
    fn nested_context_uses_the_canonical_query_contract() {
        for query in [
            json!({}),
            json!({"path":"/tmp/f"}),
            json!({"queries":[{"path":"/tmp/f","reasoning":"Read"}]}),
            json!({"path":"/tmp/f","reasoning":"Read","responseCharOffset":0}),
            json!({"cursor":"opaque"}),
        ] {
            assert!(prepare("localFetch", &query).is_err());
        }
        assert!(prepare("localFetch", &json!({"path":"/tmp/f","reasoning":"Read"})).is_ok());
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
            "path":"/tmp/f","reasoning":"Recover","offset":0,"limit":100
        }}}});
        assert_eq!(
            exact_continuation(&exact),
            Some(json!({"tool":"localFetch","query":{
                "path":"/tmp/f","reasoning":"Recover","offset":0,"limit":100
            }}))
        );

        let candidate = json!({"next":{"continue":{"tool":"localFetch","confidence":"candidate","query":{
            "path":"/tmp/f","reasoning":"Guess","offset":0,"limit":100
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
}
