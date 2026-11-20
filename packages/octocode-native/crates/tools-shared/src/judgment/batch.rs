//! One evaluation's provider requests. Every question over the state goes in
//! one request unless the provider refuses it as too large; then each
//! question is its own request. Every answer is validated against its
//! question before use, and an evaluation succeeds only when every question
//! has a valid answer.
use super::{EvaluationInput, LongContext, QuestionOutcomes};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use octocode_io::budget::RequestBudget;
use octocode_runtime_base::providers::classification::ClassificationProvider;
use octocode_runtime_base::providers::classification::gate::GateLease;
use octocode_runtime_base::providers::classification::transport::{
    ClassificationError, check_budget, check_key, endpoint, post,
};
use secrecy::SecretString;
use serde_json::Value;

mod outcomes;
#[cfg(test)]
use outcomes::{Response, judged};
use outcomes::{fold_singles, grouped_outcomes, missing_answer, parse, response_error};

/// The provider's request body limit (`clasify.semanticPolicy.requestBytes`);
/// its context window, not a byte count, decides whether tokens fit.
const MAX_REQUEST_BYTES: usize = octocode_runtime_base::id::clasify_policy::REQUEST_BYTES;

/// Where and how one evaluation's requests are sent.
pub(crate) struct Connection<'a> {
    pub key: &'a SecretString,
    pub(crate) base_url: &'a str,
    pub provider: &'static dyn ClassificationProvider,
    pub retries: u32,
}

fn request_error(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::InvalidClassificationRequest,
        message,
        "Inspect the current clasify query schema.",
    )
}

/// The questions as one grouped request body, when it fits the body ceiling.
/// Token fit is the provider's call: its refusal splits the group.
fn grouped(
    input: EvaluationInput<'_>,
    model: &str,
    provider: &dyn ClassificationProvider,
) -> Option<(Value, bytes::Bytes)> {
    let questions: Vec<&Value> = input.questions.iter().collect();
    let request = provider.build(input.state, &questions, model, input.assets);
    let body = serde_json::to_vec(&request).ok()?;
    (body.len() <= MAX_REQUEST_BYTES).then(|| (request, body.into()))
}

/// Evaluate questions through the same grouping, transport and validation path.
pub(crate) async fn answer_questions(
    input: EvaluationInput<'_>,
    model: &str,
    connection: &Connection<'_>,
    budget: &RequestBudget,
    gate: &GateLease,
) -> QuestionOutcomes {
    let outcomes = match answer_inner(input, model, connection, budget, gate).await {
        Ok(outcomes) => outcomes,
        Err(error) => QuestionOutcomes::from_complete(Err(error), input.asked().len()),
    };
    let refused = match &outcomes.complete {
        Err(error)
            if input.long_context == LongContext::OnRefusal
                && error.code == ErrorCode::ClassificationStateTooLarge =>
        {
            error.provider_calls
        }
        _ => return outcomes,
    };
    if !fits_long_window(input, model, connection.provider) {
        return not_retried(outcomes, connection.provider);
    }
    let long = EvaluationInput {
        long_context: LongContext::Always,
        ..input
    };
    let mut outcomes = match answer_inner(long, model, connection, budget, gate).await {
        Ok(outcomes) => outcomes,
        Err(error) => QuestionOutcomes::from_complete(Err(error), input.asked().len()),
    };
    if let Ok(judged) = &mut outcomes.complete {
        judged.long_context_retry = Some(super::LongContextRetry {
            unreported_calls: judged
                .provider_calls
                .saturating_sub(judged.known_usage_calls),
            refused_calls: refused,
        });
    }
    add_refused(&mut outcomes, refused);
    outcomes
}

/// Every question over `state` as one request, sent once: never split per
/// question and never retried in the long window, because its questions are
/// about several units and its caller splits the units instead. A body over
/// the ceiling is refused as too large and not sent.
pub(crate) async fn answer_group(
    input: EvaluationInput<'_>,
    model: &str,
    connection: &Connection<'_>,
    budget: &RequestBudget,
    gate: &GateLease,
) -> QuestionOutcomes {
    let count = input.asked().len();
    let sent = async {
        if count == 0 {
            return Err(request_error("An evaluation needs at least one question."));
        }
        check_budget(budget)?;
        check_key(connection.key)?;
        let Some((request, body)) = grouped(input, model, connection.provider) else {
            return Err(ClassificationError::new(
                ErrorCode::ClassificationStateTooLarge,
                format!(
                    "One classification request is over the provider's {MAX_REQUEST_BYTES}-byte body limit; nothing was sent."
                ),
                "Split the units into smaller requests.",
            ));
        };
        let (response, calls) =
            send(body, &request, input, model, connection, budget, gate).await?;
        grouped_outcomes(
            &request,
            &response,
            calls,
            connection.provider,
            input.companions,
        )
    };
    sent.await
        .unwrap_or_else(|error| QuestionOutcomes::from_complete(Err(error), count))
}

/// Whether the long-context retry of a refused evaluation provably fits the
/// provider's long window. A token covers at least one byte of the text sent,
/// so a text request of at most that many bytes holds at most that many
/// tokens; over it, nothing local tells. An image request has no such bound
/// ([`EvaluationInput::long_window_takes`]).
fn fits_long_window(
    input: EvaluationInput<'_>,
    model: &str,
    provider: &dyn ClassificationProvider,
) -> bool {
    if !input.long_window_takes() {
        return false;
    }
    let questions: Vec<&Value> = input.questions.iter().collect();
    serde_json::to_vec(&provider.build(input.state, &questions, model, None)).is_ok_and(|body| {
        u64::try_from(body.len())
            .is_ok_and(|bytes| bytes <= provider.defaults().long_context_tokens)
    })
}

/// A default-window refusal the runtime does not carry into the long window:
/// the agent splits the source, losslessly, instead of one oversized request.
fn not_retried(
    mut outcomes: QuestionOutcomes,
    provider: &dyn ClassificationProvider,
) -> QuestionOutcomes {
    let hint = format!(
        "Not retried in the provider's {}-token long-context window: this request cannot be shown to fit it.",
        provider.defaults().long_context_tokens
    );
    let errors = outcomes
        .answers
        .iter_mut()
        .filter_map(|answer| answer.as_mut().err())
        .chain(outcomes.complete.as_mut().err());
    for error in errors {
        if error.code == ErrorCode::ClassificationStateTooLarge {
            error.hints.push(hint.clone());
        }
    }
    outcomes
}

/// Requests the provider refused before these outcomes: unbilled, yet sent.
fn add_refused(outcomes: &mut QuestionOutcomes, calls: u64) {
    match &mut outcomes.complete {
        Ok(judged) => judged.provider_calls = judged.provider_calls.saturating_add(calls),
        Err(error) => error.provider_calls = error.provider_calls.saturating_add(calls),
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
    if input.asked().is_empty() {
        return Err(request_error("An evaluation needs at least one question."));
    }
    check_budget(budget)?;
    check_key(connection.key)?;
    let provider = connection.provider;
    // One question is one plain request; several share one when they fit.
    let mut refused_calls = 0_u64;
    if questions.len() > 1
        && let Some((request, body)) = grouped(input, model, provider)
    {
        match send(body, &request, input, model, connection, budget, gate).await {
            Ok((response, calls)) => {
                return grouped_outcomes(&request, &response, calls, provider, input.companions);
            }
            // The provider's window, not a byte count, decides whether a
            // group fits: its refusal tries the long window, then splits the
            // group into the per-question requests below. An image group
            // splits at once: the long window never takes it
            // ([`EvaluationInput::long_window_takes`]), and each image
            // question is billed for its images whether grouped or alone.
            Err(error)
                if error.code == ErrorCode::ClassificationStateTooLarge
                    && (input.long_context != LongContext::OnRefusal
                        || !input.long_window_takes()) =>
            {
                refused_calls = error.provider_calls;
            }
            Err(error) => return Err(error),
        }
    }
    // One request per asked question, concurrently; the gate bounds them.
    // Companions are not sent alone.
    let asked = input.asked();
    let singles = stream::iter(asked.iter())
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
                companions: 0,
                ..input
            };
            let request = provider.build(single.state, &[question], model, single.assets);
            let body = serde_json::to_vec(&request)
                .map_err(|_| request_error("Classification request could not be serialized."))?;
            if body.len() > MAX_REQUEST_BYTES {
                return Err(ClassificationError::new(
                    ErrorCode::ClassificationStateTooLarge,
                    format!("One classification request is over the provider's {MAX_REQUEST_BYTES}-byte body limit; nothing was sent."),
                    "Split the source with ranges or resources, send fewer images, or shorten the questions.",
                ));
            }
            let (response, calls) =
                send(body.into(), &request, single, model, connection, budget, gate).await?;
            let read = provider.read(&request, &response);
            let answer = read
                .answers
                .and_then(|answers| answers.into_iter().next().ok_or_else(missing_answer))
                .and_then(|answer| answer)
                .map_err(|error| response_error(error, read.usage, calls))?;
            Ok::<_, ClassificationError>((parse(read.model, read.usage, vec![answer]), calls))
        })
        .buffered(asked.len())
        .collect::<Vec<_>>()
        .await;
    let mut outcomes = fold_singles(singles, asked.len());
    add_refused(&mut outcomes, refused_calls);
    Ok(outcomes)
}

/// The requests one evaluation ([`answer_questions`]) sends first, and the
/// most it can send, transient retries aside (they are the transport's).
/// A request whose body holds no more bytes than its windows hold tokens is
/// never refused, since a token covers at least one byte of the text sent
/// (the rule [`fits_long_window`] relies on); any other request may be, and
/// then the long window and the per-question requests it leads to count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sends {
    /// The first round: one grouped request, else one per question.
    pub first: u64,
    /// Every request the window's refusals can add, the first round's
    /// included.
    pub at_most: u64,
    /// Whether the last window asked may still refuse it (a source judged
    /// whole then falls back to its sections).
    pub refusable: bool,
    /// The first round's request bytes.
    pub bytes: usize,
}

/// Whether a request of `all` bytes, `one` of them with only its largest
/// question, is never refused by the `long` (or default) window.
pub(crate) fn never_refused(
    provider: &dyn ClassificationProvider,
    long: bool,
    all: usize,
    one: usize,
) -> bool {
    let defaults = provider.defaults();
    u64::try_from(all).is_ok_and(|all| all <= defaults.request_window(long))
        && u64::try_from(one).is_ok_and(|one| one <= defaults.question_window(long))
}

/// The bytes of `state` asked `questions`, and of `state` asked only the
/// largest of them.
pub(crate) fn request_sizes(
    state: &Value,
    questions: &[Value],
    model: &str,
    provider: &dyn ClassificationProvider,
) -> (usize, usize) {
    let size = |questions: &[&Value]| {
        serde_json::to_vec(&provider.build(state, questions, model, None))
            .map_or(usize::MAX, |b| b.len())
    };
    let all = size(&questions.iter().collect::<Vec<_>>());
    let largest = questions
        .iter()
        .max_by_key(|question| serde_json::to_vec(question).map_or(0, |b| b.len()));
    let one = largest.map_or(all, |largest| size(&[largest]));
    (all, one)
}

/// What `input` sends ([`Sends`]).
pub(crate) fn sends(
    input: EvaluationInput<'_>,
    model: &str,
    provider: &dyn ClassificationProvider,
) -> Sends {
    let asked = u64::try_from(input.asked().len()).unwrap_or(u64::MAX);
    let (all, one) = request_sizes(input.state, input.questions, model, provider);
    let grouped = input.questions.len() > 1 && all <= MAX_REQUEST_BYTES;
    // An image's tokens follow its pixels: no byte count bounds them.
    let images = input.images();
    let never = |long| !images && never_refused(provider, long, all, one);
    let singles_never = |long| !images && never_refused(provider, long, one, one);
    // One window's requests ([`answer_inner`]): the group, then, after a
    // refusal outside the long-window retry, one request per question.
    let window = |mode: LongContext| -> (u64, bool) {
        let long = mode == LongContext::Always;
        if !grouped {
            return (asked, !singles_never(long));
        }
        if never(long) {
            (1, false)
        } else if mode != LongContext::OnRefusal || !input.long_window_takes() {
            (1u64.saturating_add(asked), !singles_never(long))
        } else {
            (1, true)
        }
    };
    let (mut at_most, mut refusable) = window(input.long_context);
    if input.long_context == LongContext::OnRefusal
        && refusable
        && fits_long_window(input, model, provider)
    {
        let (long, still) = window(LongContext::Always);
        at_most = at_most.saturating_add(long);
        refusable = still;
    }
    Sends {
        first: if grouped { 1 } else { asked },
        at_most,
        refusable,
        bytes: if grouped {
            all
        } else {
            one.saturating_mul(input.asked().len())
        },
    }
}

/// Whether every window this evaluation would be sent to is known to refuse
/// it ([`super::windows`]): the default one, and the long one when a refusal
/// would be retried there. An image request is never known: its size says
/// nothing of its tokens.
pub(crate) fn known_refused(
    input: EvaluationInput<'_>,
    model: &str,
    base_url: &str,
    provider: &dyn ClassificationProvider,
) -> bool {
    if input.assets.is_some() {
        return false;
    }
    let questions: Vec<&Value> = input.questions.iter().collect();
    let Ok(body) = serde_json::to_vec(&provider.build(input.state, &questions, model, None)) else {
        return false;
    };
    let refuses = |long| super::windows::refuses(base_url, model, long, body.len());
    match input.long_context {
        LongContext::Always => refuses(true),
        LongContext::Never => refuses(false),
        LongContext::OnRefusal => {
            refuses(false) && (!fits_long_window(input, model, provider) || refuses(true))
        }
    }
}

/// POST once through the shared retry, transport and accounting boundary.
/// What the window says of a text request's size, and the input tokens it
/// reports for one it took, are remembered ([`super::windows`]).
async fn send(
    body: bytes::Bytes,
    request: &Value,
    input: EvaluationInput<'_>,
    model: &str,
    connection: &Connection<'_>,
    budget: &RequestBudget,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    let (bytes, long) = (body.len(), input.long_context == LongContext::Always);
    // A capped row's send takes its room first, and its retries only the
    // room past the row's ceiling ([`super::cap`]).
    let grant = match input.cap {
        Some(cap) => Some(cap.take(connection.retries).ok_or_else(super::cap::spent)?),
        None => None,
    };
    let retries = grant.map_or(connection.retries, |grant| grant.retries);
    let mut sent = post_once(body, input, model, connection, retries, budget, gate).await;
    if let (Some(cap), Some(grant)) = (input.cap, grant) {
        let attempts = match &sent {
            Ok((_, calls)) => *calls,
            Err(error) => error.provider_calls,
        };
        cap.settle(grant, attempts);
        let granted = grant.retries;
        // A transient failure the runtime would have retried further, had
        // the row's shared retries not been spent, says so.
        if let Err(error) = &mut sent
            && granted < connection.retries
            && attempts > u64::from(granted)
            && octocode_runtime_base::id::error_codes::class(error.code)
                == octocode_runtime_base::id::error_codes::ErrorClass::Transient
            && error.code != ErrorCode::Cancelled
        {
            error.hints.push(super::cap::RETRIES_SPENT.to_owned());
        }
    }
    if input.assets.is_none() {
        match &sent {
            Ok((response, _)) => {
                let tokens = connection
                    .provider
                    .read(request, response)
                    .usage
                    .map(|usage| usage.input_tokens);
                super::windows::took(
                    connection.base_url,
                    model,
                    long,
                    bytes,
                    input.questions.len(),
                    tokens,
                );
            }
            Err(error) if error.code == ErrorCode::ClassificationStateTooLarge => {
                super::windows::refused(connection.base_url, model, long, bytes);
            }
            Err(_) => {}
        }
    }
    sent
}

async fn post_once(
    body: bytes::Bytes,
    input: EvaluationInput<'_>,
    model: &str,
    connection: &Connection<'_>,
    retries: u32,
    budget: &RequestBudget,
    gate: &GateLease,
) -> Result<(Value, u64), ClassificationError> {
    post(
        body,
        connection
            .provider
            .headers(input.images(), input.long_context == LongContext::Always),
        connection.key,
        endpoint(
            connection.base_url,
            connection.provider.defaults().endpoint_path,
        )?,
        budget,
        retries,
        gate,
    )
    .await
    .map_err(|error| error.naming_model(model))
}

#[cfg(test)]
mod tests;
