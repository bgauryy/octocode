//! One bounded assessment batch over the frozen plan, saved before and
//! after its provider requests.
use super::emit::{answered_cell, emit, terminal_saved};
use super::{Budget, Outcome, Runtime, Session, error_value, token};
use crate::execution::{ExecutionContext, SelectionLimits};
use crate::id::ToolId;
use crate::providers::classification::gate::call_cap;
use crate::response::result::Continuation;
use crate::tools::judgment::stats::ClassificationUsage;
use crate::tools::judgment::{ClassificationError, EvaluationInput, QuestionOutcomes};
use crate::tools::source_capture::SourceReader;
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use serde_json::json;

pub(super) fn pending_cost(session: &Session) -> (usize, usize) {
    session
        .resources
        .iter()
        .flat_map(|resource| &resource.units)
        .filter(|unit| unit.cells.iter().any(Option::is_none))
        .fold((0usize, 0usize), |(units, chars), unit| {
            (units + 1, chars.saturating_add(unit.chars))
        })
}

pub(super) fn settle_unsupported(session: &mut Session, limits: SelectionLimits) -> usize {
    let mut settled = 0;
    for resource in &mut session.resources {
        for unit in &mut resource.units {
            if unit.chars > super::super::max_state_chars(session.long_context)
                || unit.chars > limits.max_chars
            {
                let error = error_value(&ClassificationError::new(
                    ErrorCode::ClassificationContextTooLarge,
                    "This exact target and its supporting context exceed the serialized state limit.",
                    "Use the pinned raw reader to inspect the complete target.",
                ));
                for cell in &mut unit.cells {
                    if cell.is_none() {
                        *cell = Some(json!({"status":"unsupported", "error":error}));
                        settled += 1;
                    }
                }
            }
        }
    }
    settled
}

pub(super) fn assess_and_save(
    session: &mut Session,
    ordinal: usize,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    runtime: Runtime<'_>,
    shared: &Budget,
) -> Result<Outcome, ClassificationError> {
    let evaluator = dispatcher.evaluator();
    let budget = evaluator.budget(
        shared.deadline(context.deadline),
        context.cancellation.clone(),
    );
    let gate = evaluator.gate();
    let questions: Vec<_> = session
        .keys
        .iter()
        .map(|key| session.questions[key].clone())
        .collect();
    let questions =
        super::super::rows::contextual_questions(&questions, session.context.as_deref());
    let mut usage = ClassificationUsage::default();
    let mut settled = settle_unsupported(session, shared.limits);
    let (admitted, deferred_by_sibling) = admit_units(session, shared);
    session.active = admitted.clone();
    if runtime.can_resume
        && !admitted.is_empty()
        && !runtime.sessions.save(
            runtime.sessions.key(&runtime.access, &session.id),
            session.clone(),
            context,
        )
    {
        session.active.clear();
        return Ok(terminal_saved(
            session,
            "The assessment transaction could not be retained before inference; no requests were submitted for this batch.",
            usage,
            runtime,
            context,
        ));
    }
    // Every admitted unit is judged concurrently under the call's budget and
    // gate; results apply in admitted order, so the pinned model and every
    // mismatch decision match a serial run whatever order calls finish in.
    let pending: Vec<Vec<usize>> = admitted
        .iter()
        .map(|&[resource, unit]| {
            session.resources[resource].units[unit]
                .cells
                .iter()
                .enumerate()
                .filter_map(|(index, cell)| cell.is_none().then_some(index))
                .collect()
        })
        .collect();
    let mut results = {
        let (session, budget, gate, questions) = (&*session, &budget, &gate, &questions);
        dispatcher.handle().block_on(
            stream::iter(admitted.iter().zip(&pending).enumerate())
                .map(|(position, (&[resource, unit], pending))| async move {
                    let remaining: Vec<_> = pending
                        .iter()
                        .map(|&index| questions[index].clone())
                        .collect();
                    let result = evaluator
                        .evaluate_questions(
                            EvaluationInput {
                                state: &session.resources[resource].units[unit].state,
                                questions: &remaining,
                                assets: None,
                                long_context: session.long_context,
                            },
                            session.model.as_deref(),
                            budget,
                            gate,
                        )
                        .await;
                    (position, result)
                })
                .buffer_unordered(call_cap(evaluator.max_concurrency))
                .collect::<Vec<_>>(),
        )
    };
    results.sort_unstable_by_key(|(position, _)| *position);
    settled += apply_results(
        session,
        &admitted,
        pending,
        results.into_iter().map(|(_, result)| result),
        &mut usage,
    );
    session.active.clear();
    let remaining = pending_cost(session).0;
    if remaining > 0 && (!runtime.can_resume || (settled == 0 && !deferred_by_sibling)) {
        return Ok(terminal_saved(
            session,
            if runtime.can_resume {
                "This assessment batch made no progress before its execution budget ended; remaining cells were not assessed."
            } else {
                "The execution budget ended and this runtime cannot retain a later assessment batch; remaining cells were not assessed."
            },
            usage,
            runtime,
            context,
        ));
    }
    let mut output = emit(session, &usage, None);
    if remaining > 0 {
        output["next"] = json!({"nextSectionPage":Continuation::continued(ToolId::Clasify, json!({"operation":"sections","assessment":token(&session.id, ordinal + 1)})).build()});
    }
    session.batches.push(output.clone());
    if runtime.can_resume
        && !runtime.sessions.save(
            runtime.sessions.key(&runtime.access, &session.id),
            session.clone(),
            context,
        )
    {
        session.batches.pop();
        // Include this batch again in the full terminal result because its
        // provisional output has not yet reached the caller.
        for resource in &mut session.resources {
            if let Some(shown) = output["resources"][&resource.key]["sections"].as_array() {
                for unit in &mut resource.units {
                    if shown.iter().any(|item| item["id"] == unit.metadata["id"]) {
                        unit.emitted = false;
                    }
                }
            }
        }
        return Ok(terminal_saved(
            session,
            "The next assessment batch could not be retained; remaining cells were not assessed.",
            usage,
            runtime,
            context,
        ));
    }
    Ok(Outcome { output, usage })
}

/// The units this batch assesses, as `[resource, unit]`, in plan order
/// while the shared budget admits them; and whether a sibling's admissions
/// deferred the whole batch.
fn admit_units(session: &Session, shared: &Budget) -> (Vec<[usize; 2]>, bool) {
    let mut admitted = Vec::new();
    for (resource_index, resource) in session.resources.iter().enumerate() {
        for (unit_index, unit) in resource.units.iter().enumerate() {
            if unit.cells.iter().all(Option::is_some) {
                continue;
            }
            if !shared.admit(unit.chars) {
                let deferred_by_sibling = admitted.is_empty() && shared.has_admissions();
                return (admitted, deferred_by_sibling);
            }
            admitted.push([resource_index, unit_index]);
        }
    }
    (admitted, false)
}

/// Apply each admitted unit's answers, in admitted order, to its pending
/// cells; an answer from a model other than the session's pinned one is
/// refused. Returns how many cells settled.
fn apply_results(
    session: &mut Session,
    admitted: &[[usize; 2]],
    pending: Vec<Vec<usize>>,
    results: impl Iterator<Item = QuestionOutcomes>,
    usage: &mut ClassificationUsage,
) -> usize {
    let mut settled = 0;
    for ((&[resource, unit], pending), result) in admitted.iter().zip(pending).zip(results) {
        let unit = &mut session.resources[resource].units[unit];
        usage.add(&result.complete);
        usage.models.extend(result.models.iter().cloned());
        let model_mismatch = result.models.len() > 1
            || session
                .resolved_model
                .as_ref()
                .is_some_and(|model| result.models.iter().any(|actual| actual != model));
        if !model_mismatch && session.resolved_model.is_none() {
            session.resolved_model = result.models.iter().next().cloned();
        }
        for (index, answer) in pending.into_iter().zip(result.answers) {
            let answer = if model_mismatch && answer.is_ok() {
                Err(ClassificationError::invalid_response(
                    "The provider resolved this frozen assessment plan to different model identities; this answer was not accepted.",
                ))
            } else {
                answer
            };
            match answer {
                Ok(answer) => {
                    unit.cells[index] = Some(answered_cell(answer));
                    settled += 1;
                }
                Err(error)
                    if matches!(error.code, ErrorCode::Timeout | ErrorCode::Cancelled)
                        && error.provider_calls == 0 => {}
                Err(error) => {
                    unit.cells[index] =
                        Some(json!({"status":"error", "error":error_value(&error)}));
                    settled += 1;
                }
            }
        }
    }
    settled
}
