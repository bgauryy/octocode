//! Capture every context independently, then group only identical captured states.
use super::{
    ExecutionContext, ExecutionError,
    dispatch::{self, DomainResult},
    domain_dispatch::DomainDispatcher,
};
use crate::tools::jev::{self, transport::JevProviderError};
use futures_util::{StreamExt, stream};
use secrecy::SecretString;
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) struct ProviderConfig<'a> {
    pub key: &'a SecretString,
    pub base_url: &'a str,
    pub model: &'a str,
    pub retries: u32,
}

struct Captured<'a> {
    index: usize,
    state: Value,
    identity: String,
    receipt: Option<Value>,
    question: &'a Value,
}

#[derive(Clone)]
enum CaptureOutcome {
    Ready {
        state: Value,
        receipt: Option<Value>,
    },
    Failed {
        error: JevProviderError,
        receipt: Option<Value>,
    },
}

fn capture(
    query: &Value,
    dispatcher: &DomainDispatcher,
    context: &ExecutionContext,
) -> CaptureOutcome {
    match super::jev_context::resolve(query, dispatcher, context) {
        Ok((state, receipt)) => CaptureOutcome::Ready { state, receipt },
        Err(failure) => CaptureOutcome::Failed {
            error: failure.error,
            receipt: failure.receipt,
        },
    }
}

fn matrix_capture_key(query: &Value) -> Option<(String, String)> {
    let resource_id = query.get("resourceId")?.as_str()?.to_owned();
    let mut context = query.get("context")?.clone();
    context.sort_all_objects();
    Some((resource_id, context.to_string()))
}

fn failed(error: JevProviderError, receipt: Option<Value>) -> DomainResult {
    let mut result = dispatch::provider_failure(error.message, error.code, error.hints, None);
    if let Some(receipt) = receipt {
        result.data["context"] = receipt;
    }
    result
}

pub(super) fn execute(
    queries: &[Value],
    dispatcher: &DomainDispatcher,
    context: &ExecutionContext,
    config: ProviderConfig<'_>,
    mut record_usage: impl FnMut(&Value),
) -> Result<Vec<DomainResult>, ExecutionError> {
    let mut rows: Vec<Option<DomainResult>> = (0..queries.len()).map(|_| None).collect();
    let mut captured = Vec::new();
    let mut matrix_captures: HashMap<(String, String), CaptureOutcome> = HashMap::new();
    for (index, query) in queries.iter().enumerate() {
        context.check()?;
        let outcome = match jev::preflight(query) {
            Err(error) => CaptureOutcome::Failed {
                error,
                receipt: None,
            },
            Ok(()) => match matrix_capture_key(query) {
                Some(key) => matrix_captures
                    .entry(key)
                    .or_insert_with(|| capture(query, dispatcher, context))
                    .clone(),
                None => capture(query, dispatcher, context),
            },
        };
        match outcome {
            CaptureOutcome::Ready { state, receipt } => {
                let mut canonical = state.clone();
                canonical.sort_all_objects();
                captured.push(Captured {
                    index,
                    identity: canonical.to_string(),
                    state,
                    receipt,
                    question: &query["question"],
                });
            }
            CaptureOutcome::Failed { error, receipt } => {
                rows[index] = Some(failed(error, receipt));
            }
        }
    }
    let mut groups: Vec<Vec<Captured<'_>>> = Vec::new();
    for row in captured {
        if let Some(group) = groups.iter_mut().find(|group| {
            group[0].identity == row.identity && {
                let mut questions: Vec<_> =
                    group.iter().map(|row| (row.index, row.question)).collect();
                questions.push((row.index, row.question));
                jev::batch::fits(&row.state, &questions, config.model)
            }
        }) {
            group.push(row);
        } else {
            groups.push(vec![row]);
        }
    }
    let budget = jev::transport::budget(context.deadline, context.cancellation.clone());
    // Capture remains serial; only independent provider groups overlap. All
    // futures share the admitted deadline/cancellation and are drained before
    // returning, so each completed response records its usage immediately.
    let pending = groups.into_iter().map(|group| {
        let budget = &budget;
        let config = &config;
        async move {
            let first = &group[0];
            let result = if group.len() == 1 {
                jev::execute(
                    &first.state,
                    first.question,
                    config.key.clone(),
                    config.base_url,
                    config.model,
                    budget.clone(),
                    config.retries,
                )
                .await
                .map(|data| jev::batch::GroupResponse {
                    usage: data["usage"].clone(),
                    answers: vec![Ok(data)],
                })
            } else {
                let questions: Vec<_> = group.iter().map(|row| (row.index, row.question)).collect();
                jev::batch::execute(
                    &first.state,
                    &questions,
                    config.key,
                    config.base_url,
                    config.model,
                    budget,
                    config.retries,
                )
                .await
            };
            (group, result)
        }
    });
    dispatcher.handle.block_on(async {
        let mut completions = stream::iter(pending).buffer_unordered(5);
        while let Some((group, result)) = completions.next().await {
            match result {
                Err(error) => {
                    for row in group {
                        rows[row.index] = Some(failed(error.clone(), None));
                    }
                }
                Ok(result) => {
                    record_usage(&result.usage);
                    let owner = group
                        .iter()
                        .zip(&result.answers)
                        .find_map(|(row, answer)| answer.is_ok().then_some(row.index));
                    let shared: Vec<_> = group.iter().map(|row| row.index).collect();
                    for (row, answer) in group.into_iter().zip(result.answers) {
                        rows[row.index] = Some(match answer {
                            Err(error) => failed(error, None),
                            Ok(mut data) => {
                                if shared.len() > 1 {
                                    if owner != Some(row.index) {
                                        data["usage"] = json!({"input_tokens":0,"output_tokens":0});
                                    }
                                    data["usageAttribution"] =
                                        json!({"ownerIndex":owner,"sharedWith":shared});
                                }
                                if let Some(receipt) = row.receipt {
                                    data["context"] = receipt;
                                }
                                dispatch::value_result(data)
                            }
                        });
                    }
                }
            }
        }
    });
    rows.into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ExecutionError::WorkerFailed)
}
