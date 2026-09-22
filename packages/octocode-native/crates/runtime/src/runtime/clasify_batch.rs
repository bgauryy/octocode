//! Resource-major semantic assessment with capture-once paging.
use super::{
    ExecutionContext, ExecutionError,
    dispatch::{self, DomainResult},
    domain_dispatch::DomainDispatcher,
};
use crate::tools::clasify::{self, transport::ClassificationError};
use futures_util::{StreamExt, stream};
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const MAX_CONCURRENT_PROVIDER_CALLS: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct ProviderConfig<'a> {
    pub key: &'a SecretString,
    pub base_url: &'a str,
    pub endpoint_path: &'a str,
    pub model: &'a str,
    pub provider: &'a dyn crate::providers::classification::ClassificationProvider,
    pub retries: u32,
}

enum CapturedPage {
    Ready {
        state: Value,
        context: Value,
    },
    Failed {
        error: ClassificationError,
        context: Value,
    },
}

fn fallback_context(source: &Value) -> Value {
    let digest = hex::encode(Sha256::digest(source.to_string().as_bytes()));
    match source.get("tool").and_then(Value::as_str) {
        Some(tool) => json!({"source":"tool","tool":tool,"resultHash":digest,"coverage":"partial",
            "limitations":["Context retrieval failed before a complete page was captured."]}),
        None => json!({"source":"value","resultHash":digest,"coverage":"partial",
            "limitations":["Context retrieval failed before a complete page was captured."]}),
    }
}

fn bounded_prefix(state: &Value, context: &mut Value, max_chars: usize) -> Value {
    let prefix = state
        .to_string()
        .chars()
        .take(max_chars)
        .collect::<String>();
    context["resultHash"] = json!(hex::encode(Sha256::digest(prefix.as_bytes())));
    context["coverage"] = json!("partial");
    if let Some(object) = context.as_object_mut() {
        object.remove("next");
    }
    context["limitations"] = json!([
        "The first sanitized page exceeded maxChars and was assessed only as a bounded prefix; no safe within-page continuation is available."
    ]);
    Value::String(prefix)
}

fn logical_chars(value: &Value) -> usize {
    match value {
        Value::Null => 0,
        Value::Bool(value) => value.to_string().chars().count(),
        Value::Number(value) => value.to_string().chars().count(),
        Value::String(value) => value.chars().count(),
        Value::Array(values) => values.iter().map(logical_chars).sum(),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| key.chars().count().saturating_add(logical_chars(value)))
            .sum(),
    }
}

fn tool_data_chars(data: &Value) -> usize {
    let Some(fields) = data.as_object() else {
        return logical_chars(data);
    };
    fields
        .iter()
        .filter(|(key, _)| key.as_str() != "next")
        .map(|(key, value)| key.chars().count().saturating_add(logical_chars(value)))
        .sum()
}

fn tool_payload(state: &Value) -> Value {
    let Some(rows) = state.get("results").and_then(Value::as_array) else {
        return state.clone();
    };
    let mut payload = rows
        .iter()
        .filter_map(|row| row.get("data"))
        .cloned()
        .collect::<Vec<_>>();
    for data in &mut payload {
        if let Some(fields) = data.as_object_mut() {
            fields.remove("next");
        }
    }
    if payload.len() == 1 {
        payload.pop().unwrap_or(Value::Null)
    } else {
        Value::Array(payload)
    }
}

/// Count the sanitized resource payload rather than its transport envelope.
/// JSON punctuation, escaping, row wrappers, and executable continuations are
/// control-plane overhead and must not reduce the caller's `maxChars` budget.
fn assessed_payload_chars(source: &Value, state: &Value) -> usize {
    if source.get("value").is_some() {
        return state.to_string().chars().count();
    }
    state
        .get("results")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("data"))
                .map(tool_data_chars)
                .sum()
        })
        .unwrap_or_else(|| logical_chars(state))
}

fn bounded_payload_prefix(
    source: &Value,
    state: &Value,
    context: &mut Value,
    max_chars: usize,
) -> Value {
    if source.get("value").is_some() {
        bounded_prefix(state, context, max_chars)
    } else {
        bounded_prefix(&tool_payload(state), context, max_chars)
    }
}

fn capture_resource(
    resource: &Value,
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
) -> Result<(Vec<CapturedPage>, Option<Value>), ExecutionError> {
    let max_chars = resource
        .get("maxChars")
        .and_then(Value::as_u64)
        .unwrap_or(80_000) as usize;
    let mut source = resource["context"].clone();
    let mut pages = Vec::new();
    let mut captured_chars = 0usize;
    let mut seen = HashSet::new();
    let mut remaining = None;

    loop {
        execution.check()?;
        let identity = source.to_string();
        if !seen.insert(identity) {
            pages.push(CapturedPage::Failed {
                error: ClassificationError {
                    code: "classificationContextContinuationLoop".into(),
                    message: "Context continuation repeated without advancing.".into(),
                    hints: vec![
                        "Run the ordinary context tool and inspect its executable continuation."
                            .into(),
                    ],
                },
                context: fallback_context(&source),
            });
            break;
        }
        match super::clasify_context::resolve(&source, dispatcher, execution) {
            Ok((state, receipt)) => {
                let state_chars = assessed_payload_chars(&source, &state);
                let remaining_chars = max_chars.saturating_sub(captured_chars);
                if state_chars > remaining_chars && !pages.is_empty() {
                    remaining = Some(source);
                    break;
                }
                let mut context = receipt.unwrap_or_else(|| fallback_context(&source));
                if state_chars > remaining_chars {
                    let state = bounded_payload_prefix(
                        &source,
                        &state,
                        &mut context,
                        remaining_chars.max(1),
                    );
                    pages.push(CapturedPage::Ready { state, context });
                    break;
                }
                captured_chars = captured_chars.saturating_add(state_chars);
                let continuation = super::clasify_context::continuation(&context);
                pages.push(CapturedPage::Ready { state, context });
                let Some(next) = continuation else {
                    break;
                };
                if captured_chars >= max_chars || pages.len() >= 100 {
                    remaining = Some(next);
                    break;
                }
                source = next;
            }
            Err(failure) => {
                let context = failure.receipt.unwrap_or_else(|| fallback_context(&source));
                if let Some(next) = super::clasify_context::exact_continuation(&context) {
                    source = next;
                    continue;
                }
                pages.push(CapturedPage::Failed {
                    error: failure.error,
                    context,
                });
                break;
            }
        }
    }
    Ok((pages, remaining))
}

fn error_page(index: usize, context: Value, error: ClassificationError) -> Value {
    json!({
        "pageIndex": index,
        "context": context,
        "status":"error",
        "error":{"code":error.code,"message":error.message,"hints":error.hints}
    })
}

fn cell_coverage(pages: &[Value], has_continuation: bool) -> &'static str {
    let successes = pages
        .iter()
        .filter(|page| page["status"] == "success")
        .count();
    let terminal_page_is_partial = pages
        .last()
        .is_some_and(|page| page["context"]["coverage"] == "partial");
    if successes == 0 {
        "error"
    } else if successes != pages.len() || has_continuation || terminal_page_is_partial {
        "partial"
    } else {
        "complete"
    }
}

fn concurrency_error() -> ClassificationError {
    ClassificationError {
        code: "classificationProviderError".into(),
        message: "Classification provider concurrency gate closed unexpectedly.".into(),
        hints: vec!["Retry the semantic assessment.".into()],
    }
}

async fn provider_permit(
    concurrency: Arc<Semaphore>,
) -> Result<OwnedSemaphorePermit, ClassificationError> {
    concurrency
        .acquire_owned()
        .await
        .map_err(|_| concurrency_error())
}

async fn assess_page(
    state: &Value,
    questions: &[Value],
    config: &ProviderConfig<'_>,
    budget: &crate::providers::RequestBudget,
    concurrency: &Arc<Semaphore>,
) -> (Vec<Result<Value, ClassificationError>>, Option<Value>) {
    let indexed = questions
        .iter()
        .enumerate()
        .map(|(index, question)| (index, &question["question"]))
        .collect::<Vec<_>>();
    if indexed.len() > 1 && clasify::batch::fits(state, &indexed, config.model, config.provider) {
        let permit = match provider_permit(concurrency.clone()).await {
            Ok(permit) => permit,
            Err(error) => return (vec![Err(error); questions.len()], None),
        };
        let result = clasify::batch::execute(
            state,
            &indexed,
            config.key,
            config.base_url,
            config.endpoint_path,
            config.model,
            config.provider,
            budget,
            config.retries,
        )
        .await;
        drop(permit);
        return match result {
            Ok(mut response) => {
                for answer in response.answers.iter_mut().skip(1).flatten() {
                    if let Some(object) = answer.as_object_mut() {
                        object.remove("usage");
                    }
                }
                (response.answers, Some(response.usage))
            }
            Err(error) => (vec![Err(error); questions.len()], None),
        };
    }

    let assessed = stream::iter(questions.iter())
        .map(|question| {
            let concurrency = concurrency.clone();
            async move {
                let permit = provider_permit(concurrency).await?;
                let result = clasify::execute(
                    state,
                    &question["question"],
                    config.key.clone(),
                    config.base_url,
                    config.endpoint_path,
                    config.model,
                    config.provider,
                    budget.clone(),
                    config.retries,
                )
                .await;
                drop(permit);
                result
            }
        })
        .buffered(questions.len().max(1))
        .collect::<Vec<_>>()
        .await;
    let usages = assessed
        .iter()
        .filter_map(|result| result.as_ref().ok().map(|data| data["usage"].clone()))
        .collect::<Vec<_>>();
    (assessed, Some(json!({"calls":usages})))
}

struct CapturedResource<'a> {
    resource: &'a Value,
    pages: Vec<CapturedPage>,
    continuation: Option<Value>,
}

type PageAssessment = (
    usize,
    usize,
    Vec<Result<Value, ClassificationError>>,
    Option<Value>,
);

pub(super) fn execute(
    queries: &[Value],
    dispatcher: &DomainDispatcher,
    execution: &ExecutionContext,
    config: ProviderConfig<'_>,
    mut record_usage: impl FnMut(&Value),
) -> Result<Vec<DomainResult>, ExecutionError> {
    let budget = clasify::transport::budget(execution.deadline, execution.cancellation.clone());
    let concurrency = Arc::new(Semaphore::new(MAX_CONCURRENT_PROVIDER_CALLS));
    let mut outputs = Vec::with_capacity(queries.len());

    for query in queries {
        execution.check()?;
        clasify::preflight(query).map_err(|_| ExecutionError::WorkerFailed)?;
        let questions = query["questions"]
            .as_array()
            .ok_or(ExecutionError::WorkerFailed)?;
        let resources = query["resources"]
            .as_array()
            .ok_or(ExecutionError::WorkerFailed)?;
        let mut captured = Vec::with_capacity(resources.len());
        for resource in resources {
            let (pages, continuation) = capture_resource(resource, dispatcher, execution)?;
            captured.push(CapturedResource {
                resource,
                pages,
                continuation,
            });
        }

        let jobs = captured
            .iter()
            .enumerate()
            .flat_map(|(resource_index, resource)| {
                resource
                    .pages
                    .iter()
                    .enumerate()
                    .filter_map(move |(page_index, page)| match page {
                        CapturedPage::Ready { state, .. } => {
                            Some((resource_index, page_index, state))
                        }
                        CapturedPage::Failed { .. } => None,
                    })
            })
            .collect::<Vec<_>>();
        let mut assessments = dispatcher.handle.block_on(async {
            stream::iter(jobs)
                .map(|(resource_index, page_index, state)| {
                    let budget = budget.clone();
                    let concurrency = concurrency.clone();
                    async move {
                        let (answers, usage) =
                            assess_page(state, questions, &config, &budget, &concurrency).await;
                        (resource_index, page_index, answers, usage)
                    }
                })
                .buffer_unordered(MAX_CONCURRENT_PROVIDER_CALLS)
                .collect::<Vec<PageAssessment>>()
                .await
        });
        assessments
            .sort_by_key(|(resource_index, page_index, _, _)| (*resource_index, *page_index));
        let mut assessments = assessments.into_iter();
        let mut results = Vec::with_capacity(resources.len().saturating_mul(questions.len()));
        let mut continuation_resources = Vec::new();

        for (resource_index, captured_resource) in captured.into_iter().enumerate() {
            let CapturedResource {
                resource,
                pages,
                continuation,
            } = captured_resource;
            let mut cell_pages = vec![Vec::new(); questions.len()];
            for (page_index, page) in pages.into_iter().enumerate() {
                match page {
                    CapturedPage::Failed { error, context } => {
                        for output in &mut cell_pages {
                            output.push(error_page(page_index, context.clone(), error.clone()));
                        }
                    }
                    CapturedPage::Ready { context, .. } => {
                        let Some((assessed_resource, assessed_page, answers, usage)) =
                            assessments.next()
                        else {
                            return Err(ExecutionError::WorkerFailed);
                        };
                        if assessed_resource != resource_index || assessed_page != page_index {
                            return Err(ExecutionError::WorkerFailed);
                        }
                        if let Some(usage) = usage {
                            if let Some(calls) = usage.get("calls").and_then(Value::as_array) {
                                for call in calls {
                                    record_usage(call);
                                }
                            } else {
                                record_usage(&usage);
                            }
                        }
                        for (output, answer) in cell_pages.iter_mut().zip(answers) {
                            output.push(match answer {
                                Ok(mut data) => {
                                    data["pageIndex"] = json!(page_index);
                                    data["context"] = context.clone();
                                    data["status"] = json!("success");
                                    data
                                }
                                Err(error) => error_page(page_index, context.clone(), error),
                            });
                        }
                    }
                }
            }

            let has_continuation = continuation.is_some();
            if let Some(context) = continuation {
                let mut pending = resource.clone();
                pending["context"] = context;
                continuation_resources.push(pending);
            }
            for (question, pages) in questions.iter().zip(cell_pages) {
                let coverage = cell_coverage(&pages, has_continuation);
                results.push(json!({
                    "resourceId":resource["id"],
                    "questionId":question["id"],
                    "coverage":coverage,
                    "pages":pages
                }));
            }
        }
        if assessments.next().is_some() {
            return Err(ExecutionError::WorkerFailed);
        }

        let mut output = json!({"queryId":query["id"],"results":results});
        if !continuation_resources.is_empty() {
            output["next"] = json!({"clasify":{
                "id":query["id"],
                "reasoning":query["reasoning"],
                "resources":continuation_resources,
                "questions":query["questions"]
            }});
        }
        outputs.push(dispatch::value_result(output));
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_value_budget_counts_object_keys_and_fields_named_next() {
        let state = json!({"next":{"this-key-is-evidence":"retained"}});
        let source = json!({"value":state});
        assert_eq!(
            assessed_payload_chars(&source, &state),
            state.to_string().chars().count()
        );
    }

    #[test]
    fn tool_budget_excludes_only_the_canonical_data_continuation() {
        let source = json!({"tool":"localFetch","query":{"path":"/tmp/source"}});
        let first = json!({"results":[{"data":{
            "content":{"next":"evidence"},
            "next":{"continue":{"tool":"localFetch","query":{"path":"short"}}}
        }}]});
        let second = json!({"results":[{"data":{
            "content":{"next":"evidence"},
            "next":{"continue":{"tool":"localFetch","query":{"path":"a".repeat(10_000)}}}
        }}]});
        assert_eq!(
            assessed_payload_chars(&source, &first),
            assessed_payload_chars(&source, &second),
            "continuation metadata must not consume the evidence budget"
        );
        assert!(
            assessed_payload_chars(&source, &first) >= "contentnextevidence".chars().count(),
            "nested evidence named next must still consume the budget"
        );
    }

    #[test]
    fn oversized_tool_page_prefix_uses_sanitized_payload_not_envelope() {
        let source = json!({"tool":"localFetch","query":{"path":"/tmp/source"}});
        let state = json!({
            "results":[{"index":0,"data":{
                "content":"deciding-marker",
                "next":{"continue":{"tool":"localFetch","query":{"path":"a".repeat(10_000)}}}
            }}],
            "base":"/tmp"
        });
        let mut context = json!({"next":{"continue":true}});
        let prefix = bounded_payload_prefix(&source, &state, &mut context, 100);
        let prefix = prefix.as_str().expect("bounded prefix string");
        assert!(prefix.contains("deciding-marker"));
        assert!(!prefix.contains("results"));
        assert!(!prefix.contains("continue"));
        assert!(context.get("next").is_none());
    }

    #[test]
    fn completed_page_chain_is_complete_even_when_intermediate_receipts_are_partial() {
        let pages = vec![
            json!({"status":"success","context":{"coverage":"partial"}}),
            json!({"status":"success","context":{"coverage":"bounded"}}),
        ];
        assert_eq!(cell_coverage(&pages, false), "complete");
    }

    #[test]
    fn uncovered_terminal_scope_and_pending_continuations_remain_partial() {
        let terminal_partial = vec![json!({"status":"success","context":{"coverage":"partial"}})];
        assert_eq!(cell_coverage(&terminal_partial, false), "partial");

        let bounded = vec![json!({"status":"success","context":{"coverage":"bounded"}})];
        assert_eq!(cell_coverage(&bounded, true), "partial");
    }

    #[test]
    fn page_errors_are_not_promoted_to_complete_coverage() {
        let mixed = vec![
            json!({"status":"success","context":{"coverage":"bounded"}}),
            json!({"status":"error","context":{"coverage":"partial"}}),
        ];
        assert_eq!(cell_coverage(&mixed, false), "partial");

        let failed = vec![json!({"status":"error","context":{"coverage":"partial"}})];
        assert_eq!(cell_coverage(&failed, false), "error");
    }
}
