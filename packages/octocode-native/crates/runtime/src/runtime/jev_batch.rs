//! Capture every context independently, then group only identical captured states.
use super::{
    ExecutionContext, ExecutionError,
    dispatch::{self, DomainResult},
    domain_dispatch::DomainDispatcher,
};
use crate::tools::jev::{self, transport::JevProviderError};
use secrecy::SecretString;
use serde_json::{Value, json};

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

fn failed(error: JevProviderError) -> DomainResult {
    dispatch::provider_failure(error.message, error.code, error.hints, None)
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
    for (index, query) in queries.iter().enumerate() {
        context.check()?;
        match jev::preflight(query)
            .and_then(|()| super::jev_context::resolve(query, dispatcher, context))
        {
            Ok((state, receipt)) => {
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
            Err(error) => rows[index] = Some(failed(error)),
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
    for group in groups {
        context.check()?;
        let first = &group[0];
        if group.len() == 1 {
            let result = dispatcher.handle.block_on(jev::execute(
                &first.state,
                first.question,
                config.key.clone(),
                config.base_url,
                config.model,
                budget.clone(),
                config.retries,
            ));
            rows[first.index] = Some(match result {
                Ok(mut data) => {
                    record_usage(&data["usage"]);
                    if let Some(receipt) = &first.receipt {
                        data["context"] = receipt.clone();
                    }
                    dispatch::value_result(data)
                }
                Err(error) => failed(error),
            });
            continue;
        }
        let questions: Vec<_> = group.iter().map(|row| (row.index, row.question)).collect();
        let result = dispatcher.handle.block_on(jev::batch::execute(
            &first.state,
            &questions,
            config.key,
            config.base_url,
            config.model,
            &budget,
            config.retries,
        ));
        match result {
            Err(error) => {
                for row in group {
                    rows[row.index] = Some(failed(error.clone()));
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
                        Err(error) => failed(error),
                        Ok(mut data) => {
                            if owner != Some(row.index) {
                                data["usage"] = json!({"input_tokens":0,"output_tokens":0});
                            }
                            data["usageAttribution"] =
                                json!({"ownerIndex":owner,"sharedWith":shared});
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
    rows.into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(ExecutionError::WorkerFailed)
}
