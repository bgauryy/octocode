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

pub(super) fn resolve(
    query: &Value,
    dispatcher: &DomainDispatcher,
    context: &ExecutionContext,
) -> Result<(Value, Option<Value>), JevProviderError> {
    checked(context)?;
    let source = &query["context"];
    if let Some(value) = source.get("value") {
        return Ok((value.clone(), None));
    }
    let tool = source["tool"]
        .as_str()
        .filter(|tool| is_context_tool(tool))
        .ok_or_else(|| {
            error(
                "invalidJevContext",
                "Only read tools can provide Jev context.",
            )
        })?;
    if !dispatcher.available_tools.contains(&tool) {
        return Err(error(
            "jevContextUnavailable",
            format!("Context tool {tool} is disabled by runtime policy."),
        ));
    }
    let prepared = prepare(tool, &source["query"])?;
    let checked_input = dispatcher.security.validate_input_parameters(&prepared);
    if !checked_input.is_valid {
        return Err(error(
            "securityValidationFailed",
            "Context query is blocked by input security policy.",
        ));
    }
    let prepared = Value::Object(checked_input.sanitized_params);
    let result = dispatcher.execute(tool, &prepared, context).map_err(|_| {
        error(
            "jevContextFailed",
            format!("Context tool {tool} could not complete."),
        )
    })?;
    checked(context)?;
    let failed = result.failure.is_some() || result.status == Some("error");
    let mut row = response::result_row(tool, 0, &prepared, result.data, result.status);
    response::attach_diagnostics(&mut row, result.diagnostics);
    if result.cache {
        row["cache"] = json!(1);
    }
    response::apply_hint_policy(&mut row, tool, &prepared);
    let mut state = response::envelope(vec![row]);
    response::attach_query_base(&mut state, tool, &prepared);
    response::sanitize_fields(&mut state, &dispatcher.security, context)
        .map_err(|_| error("jevContextFailed", "Context output sanitization failed."))?;
    contracts::validate_output(tool, &state).map_err(|_| {
        error(
            "jevContextContractViolation",
            format!("Context tool {tool} returned invalid output."),
        )
    })?;
    if failed {
        return Err(error(
            "jevContextFailed",
            format!("Context tool {tool} returned an error; Jev was not called."),
        ));
    }
    checked(context)?;
    let receipt = receipt(tool, &state);
    Ok((state, Some(receipt)))
}

fn receipt(tool: &str, state: &Value) -> Value {
    let mut next = Map::new();
    let mut terminal = false;
    let mut partial = response::is_partial(state);
    inspect(state, &mut next, &mut partial, &mut terminal);
    let mut receipt = json!({"tool":tool,"resultHash":hex::encode(Sha256::digest(state.to_string().as_bytes())),"coverage":if partial {"partial"}else{"bounded"}});
    if !next.is_empty() {
        receipt["next"] = Value::Object(next);
    }
    if partial {
        let limitation = if terminal {
            "The context tool reported a terminal limit; this result does not cover all matching evidence."
        } else if receipt.get("next").is_some() {
            "Only the returned tool page was evaluated; continue explicitly for additional evidence."
        } else {
            "The context tool reported incomplete evidence without a safe continuation; inspect the ordinary tool result to change its bounds."
        };
        receipt["limitations"] = json!([limitation]);
    }
    if receipt.to_string().len() > MAX_RECEIPT_BYTES {
        if let Some(object) = receipt.as_object_mut() {
            object.remove("next");
        }
        receipt["limitations"] = json!([
            "Continuation metadata exceeded the receipt limit; inspect the ordinary tool result to continue."
        ]);
    }
    receipt
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
}
