//! Provider responses folded into one evaluation's outcomes: per-question
//! answers, the models they name and every attempt's usage.
use crate::judgment::{Judged, QuestionOutcomes};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::providers::classification::transport::{
    ClassificationError, ReportedUsage,
};
use octocode_runtime_base::providers::classification::{
    ClassificationProvider, ProviderContractError, ProviderResponse, ProviderUsage,
};
use serde_json::Value;

/// One provider response: its answers in question order, model and usage.
pub(super) struct Response {
    pub(super) answers: Vec<Value>,
    pub(super) model: String,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
}

pub(super) fn fold_singles(
    singles: Vec<Result<(Response, u64), ClassificationError>>,
    count: usize,
) -> QuestionOutcomes {
    let mut answers = Vec::with_capacity(singles.len());
    let mut parts = Vec::with_capacity(singles.len());
    let mut models = std::collections::BTreeSet::new();
    let mut calls = 0_u64;
    let mut input_tokens = 0_u64;
    let mut output_tokens = 0_u64;
    let mut known_usage_calls = 0_u64;
    let mut failure = None;
    for single in singles {
        match single {
            Ok((part, part_calls)) => {
                models.insert(part.model.clone());
                answers.extend(part.answers.iter().cloned().map(Ok));
                calls = calls.saturating_add(part_calls);
                input_tokens = input_tokens.saturating_add(part.input_tokens);
                output_tokens = output_tokens.saturating_add(part.output_tokens);
                known_usage_calls = known_usage_calls.saturating_add(1);
                parts.push((part, part_calls));
            }
            Err(error) => {
                answers.push(Err(error.clone()));
                let usage = error.usage();
                calls = calls.saturating_add(error.provider_calls);
                input_tokens = input_tokens.saturating_add(usage.input_tokens);
                output_tokens = output_tokens.saturating_add(usage.output_tokens);
                known_usage_calls = known_usage_calls.saturating_add(usage.known_usage_calls);
                failure.get_or_insert(error);
            }
        }
    }
    let complete = match failure {
        Some(error) => Err(ClassificationError {
            provider_calls: calls,
            reported_usage: Some(Box::new(ReportedUsage {
                input_tokens,
                output_tokens,
                known_usage_calls,
            })),
            ..error
        }),
        None => judged(parts),
    };
    if complete
        .as_ref()
        .err()
        .is_some_and(|error| error.code == ErrorCode::InvalidClassificationResponse)
        && answers.iter().all(Result::is_ok)
    {
        return QuestionOutcomes::from_complete(complete, count);
    }
    QuestionOutcomes {
        answers,
        complete,
        models,
    }
}

/// A grouped response's outcomes. Its last `companions` answers ride along
/// ([`crate::judgment::EvaluationInput::companions`]): one that failed
/// validation is `null` in the complete answers and fails nothing.
pub(super) fn grouped_outcomes(
    request: &Value,
    response: &Value,
    calls: u64,
    provider: &dyn ClassificationProvider,
    companions: usize,
) -> Result<QuestionOutcomes, ClassificationError> {
    let ProviderResponse {
        answers,
        model,
        usage,
    } = provider.read(request, response);
    let mut parts = answers.map_err(|error| response_error(error, usage, calls))?;
    let riders: Vec<Value> = parts
        .split_off(parts.len().saturating_sub(companions))
        .into_iter()
        .map(|part| part.unwrap_or(Value::Null))
        .collect();
    let failure = parts.iter().find_map(|part| part.as_ref().err()).cloned();
    let models = parts
        .iter()
        .any(Result::is_ok)
        .then(|| model.clone().unwrap_or_default())
        .into_iter()
        .collect();
    let complete = match failure {
        Some(error) => Err(response_error(error, usage, calls)),
        None => judged(vec![(
            parse(
                model,
                usage,
                parts
                    .iter()
                    .filter_map(|part| part.as_ref().ok().cloned())
                    .chain(riders)
                    .collect(),
            ),
            calls,
        )]),
    };
    Ok(QuestionOutcomes {
        answers: parts
            .into_iter()
            .map(|part| part.map_err(|error| response_error(error, usage, calls)))
            .collect(),
        complete,
        models,
    })
}

pub(super) fn missing_answer() -> ProviderContractError {
    ProviderContractError {
        request: false,
        message: "Classification provider response is missing the expected answer.".into(),
    }
}

pub(super) fn response_error(
    error: ProviderContractError,
    usage: Option<ProviderUsage>,
    calls: u64,
) -> ClassificationError {
    let reported = usage.unwrap_or_default();
    ClassificationError {
        code: error.code(),
        provider_calls: calls,
        reported_usage: Some(Box::new(ReportedUsage {
            input_tokens: reported.input_tokens,
            output_tokens: reported.output_tokens,
            known_usage_calls: u64::from(usage.is_some()),
        })),
        ..ClassificationError::invalid_response(error.message)
    }
}

pub(super) fn parse(
    model: Option<String>,
    usage: Option<ProviderUsage>,
    answers: Vec<Value>,
) -> Response {
    let usage = usage.unwrap_or_default();
    Response {
        answers,
        model: model.unwrap_or_default(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
    }
}

pub(super) fn judged(parts: Vec<(Response, u64)>) -> Result<Judged, ClassificationError> {
    let mut judged = Judged {
        answers: Vec::new(),
        model: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        provider_calls: 0,
        known_usage_calls: 0,
        long_context_retry: None,
    };
    let mut model_mismatch = false;
    for (part, calls) in parts {
        if judged.model.is_empty() {
            judged.model = part.model;
        } else if judged.model != part.model {
            model_mismatch = true;
        }
        judged.answers.extend(part.answers);
        judged.input_tokens = judged.input_tokens.saturating_add(part.input_tokens);
        judged.output_tokens = judged.output_tokens.saturating_add(part.output_tokens);
        judged.provider_calls = judged.provider_calls.saturating_add(calls);
        judged.known_usage_calls = judged.known_usage_calls.saturating_add(1);
    }
    if model_mismatch {
        Err(ClassificationError {
            provider_calls: judged.provider_calls,
            reported_usage: Some(Box::new(ReportedUsage {
                input_tokens: judged.input_tokens,
                output_tokens: judged.output_tokens,
                known_usage_calls: judged.known_usage_calls,
            })),
            ..ClassificationError::invalid_response(
                "Classification provider resolved one evaluation to different models across requests.",
            )
        })
    } else {
        Ok(judged)
    }
}
