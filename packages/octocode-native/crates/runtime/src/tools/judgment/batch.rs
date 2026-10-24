//! One evaluation's provider requests. Every question over the state goes in
//! one request when each question alone and the whole group fit the
//! serialized headroom; otherwise each question is its own request. Every
//! answer is validated against its question before use, and an evaluation
//! succeeds only when every question has a valid answer.
use super::{EvaluationInput, Judged, QuestionOutcomes};
use crate::providers::classification::gate::GateLease;
use crate::providers::classification::transport::{
    ClassificationError, ReportedUsage, check_budget, check_key, endpoint, post,
};
use crate::providers::classification::{
    ClassificationProvider, ProviderContractError, ProviderResponse, ProviderUsage,
};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use serde_json::Value;

// Conservative serialized-byte headroom for shared-state requests. This is
// not a token estimate or a guarantee that a provider accepts the request.
const MAX_STATE_AND_QUESTION_BYTES: usize = 72 * 1024;
const MAX_GROUP_BYTES: usize = 120 * 1024;
const MAX_TEXT_REQUEST_BYTES: usize = 4 * 1024 * 1024;

fn max_request_bytes(input: EvaluationInput<'_>) -> usize {
    if input.assets.is_some() {
        // Base64 expansion, per-image padding and the existing text allowance.
        crate::id::clasify_policy::MAX_TOTAL_IMAGE_BYTES.div_ceil(3) * 4
            + crate::id::clasify_policy::MAX_IMAGE_ASSETS * 4
            + MAX_TEXT_REQUEST_BYTES
    } else {
        MAX_TEXT_REQUEST_BYTES
    }
}

/// Where and how one evaluation's requests are sent.
pub(crate) struct Connection<'a> {
    pub key: &'a SecretString,
    pub(crate) base_url: &'a str,
    pub provider: &'static dyn ClassificationProvider,
    pub retries: u32,
}

/// One provider response: its answers in question order, model and usage.
struct Response {
    answers: Vec<Value>,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
}

fn request_error(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::InvalidClassificationRequest,
        message,
        "Inspect the current clasify query schema.",
    )
}

/// The questions as one grouped request body, when it fits the headroom.
/// Sizes are measured over requests that hold `null` for the state, plus the
/// state's own size: a request embeds the state once as given, so only the
/// request actually sent copies it.
fn grouped(
    input: EvaluationInput<'_>,
    model: &str,
    provider: &dyn ClassificationProvider,
) -> Option<(Value, bytes::Bytes)> {
    const NULL_BYTES: usize = "null".len();
    let state_bytes = octocode_engine::json_size::bytes(input.state)?;
    let size = |questions: &[&Value]| {
        octocode_engine::json_size::bytes(&provider.build(&Value::Null, questions, model, None))
            .map_or(usize::MAX, |bytes| {
                (bytes - NULL_BYTES).saturating_add(state_bytes)
            })
    };
    let questions: Vec<&Value> = input.questions.iter().collect();
    let (each_limit, group_limit) = if input.long_context {
        (MAX_TEXT_REQUEST_BYTES, MAX_TEXT_REQUEST_BYTES)
    } else {
        (MAX_STATE_AND_QUESTION_BYTES, MAX_GROUP_BYTES)
    };
    if questions
        .iter()
        .any(|question| size(std::slice::from_ref(question)) > each_limit)
        || size(&questions) > group_limit
    {
        return None;
    }
    let request = provider.build(input.state, &questions, model, input.assets);
    let body = serde_json::to_vec(&request).ok()?;
    (body.len() <= max_request_bytes(input)).then(|| (request, body.into()))
}

/// Evaluate questions through the same grouping, transport and validation path.
pub(crate) async fn answer_questions(
    input: EvaluationInput<'_>,
    model: &str,
    connection: &Connection<'_>,
    budget: &RequestBudget,
    gate: &GateLease,
) -> QuestionOutcomes {
    match answer_inner(input, model, connection, budget, gate).await {
        Ok(outcomes) => outcomes,
        Err(error) => QuestionOutcomes::from_complete(Err(error), input.questions.len()),
    }
}

/// Answer every question over `state`.
async fn answer_inner(
    input: EvaluationInput<'_>,
    model: &str,
    connection: &Connection<'_>,
    budget: &RequestBudget,
    gate: &GateLease,
) -> Result<QuestionOutcomes, ClassificationError> {
    let questions = input.questions;
    if questions.is_empty() {
        return Err(request_error("An evaluation needs at least one question."));
    }
    check_budget(budget)?;
    check_key(connection.key)?;
    let provider = connection.provider;
    // One question is one plain request; several share one when they fit.
    if questions.len() > 1
        && let Some((request, body)) = grouped(input, model, provider)
    {
        let (response, calls) = send(body, input, connection, budget, gate).await?;
        return grouped_outcomes(&request, &response, calls, provider);
    }
    // One request per question, concurrently; the gate bounds them.
    let singles = stream::iter(questions.iter())
        .map(|question| async move {
            let assets = input.assets.and_then(Value::as_object).and_then(|assets| {
                let selected = assets
                    .iter()
                    .filter(|(key, _)| {
                        super::referenced(input.state, key) || super::referenced(question, key)
                    })
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<serde_json::Map<String, Value>>();
                (!selected.is_empty()).then_some(Value::Object(selected))
            });
            let single = EvaluationInput {
                questions: std::slice::from_ref(question),
                assets: assets.as_ref(),
                ..input
            };
            let request = provider.build(single.state, &[question], model, single.assets);
            let body = serde_json::to_vec(&request)
                .map_err(|_| request_error("Classification request could not be serialized."))?;
            if body.len() > max_request_bytes(single) {
                return Err(ClassificationError::new(
                    ErrorCode::ClassificationStateTooLarge,
                    "One classification request exceeded the configured serialized body limit.",
                    "Narrow the state, reduce image bytes, or shorten the questions.",
                ));
            }
            let (response, calls) = send(body.into(), single, connection, budget, gate).await?;
            let read = provider.read(&request, &response);
            let answer = read
                .answers
                .and_then(|answers| answers.into_iter().next().ok_or_else(missing_answer))
                .and_then(|answer| answer)
                .map_err(|error| response_error(error, read.usage, calls))?;
            Ok::<_, ClassificationError>((parse(read.model, read.usage, vec![answer]), calls))
        })
        .buffered(questions.len())
        .collect::<Vec<_>>()
        .await;
    Ok(fold_singles(singles, questions.len()))
}

fn fold_singles(
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

fn grouped_outcomes(
    request: &Value,
    response: &Value,
    calls: u64,
    provider: &dyn ClassificationProvider,
) -> Result<QuestionOutcomes, ClassificationError> {
    let ProviderResponse {
        answers,
        model,
        usage,
    } = provider.read(request, response);
    let parts = answers.map_err(|error| response_error(error, usage, calls))?;
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

fn missing_answer() -> ProviderContractError {
    ProviderContractError {
        request: false,
        message: "Classification provider response is missing the expected answer.".into(),
    }
}

fn response_error(
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

/// POST once through the shared retry, transport and accounting boundary.
async fn send(
    body: bytes::Bytes,
    input: EvaluationInput<'_>,
    connection: &Connection<'_>,
    budget: &RequestBudget,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    post(
        body,
        connection
            .provider
            .headers(input.images(), input.long_context),
        connection.key,
        endpoint(
            connection.base_url,
            connection.provider.defaults().endpoint_path,
        )?,
        budget,
        connection.retries,
        gate,
    )
    .await
}

fn parse(model: Option<String>, usage: Option<ProviderUsage>, answers: Vec<Value>) -> Response {
    let usage = usage.unwrap_or_default();
    Response {
        answers,
        model: model.unwrap_or_default(),
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
    }
}

fn judged(parts: Vec<(Response, u64)>) -> Result<Judged, ClassificationError> {
    let mut judged = Judged {
        answers: Vec::new(),
        model: String::new(),
        input_tokens: 0,
        output_tokens: 0,
        provider_calls: 0,
        known_usage_calls: 0,
        replayed: false,
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

#[cfg(test)]
mod tests;
