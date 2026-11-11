//! One bounded assessment batch over the frozen plan, saved before and
//! after its provider requests.
use super::emit::{Held, answered_cell, attach_continuations, emit, terminal_saved};
use super::{Budget, Outcome, Runtime, Session, error_value, token};
use crate::execution::ExecutionContext;
use crate::providers::classification::gate::call_cap;
use crate::response::result::{Continuation, insert_next};
use crate::tools::judgment::stats::ClassificationUsage;
use crate::tools::judgment::{ClassificationError, EvaluationInput, QuestionOutcomes};
use crate::tools::source_capture::SourceReader;
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::{
    ClasifyChannel, ClasifyQuery, ClasifyQuerySectionsAssessment,
    ClasifyQuerySectionsAssessmentOperation, ClasifyView, ErrorCode,
};
use serde_json::json;

/// Characters of the sections still to judge.
fn pending_chars(session: &Session) -> (usize, usize) {
    session
        .resources
        .iter()
        .flat_map(|resource| &resource.units)
        .filter(|unit| unit.cells.iter().any(Option::is_none))
        .fold((0usize, 0usize), |(units, chars), unit| {
            (units + 1, chars.saturating_add(unit.chars))
        })
}

pub(super) fn assess_and_save(
    session: &mut Session,
    ordinal: usize,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    runtime: Runtime<'_>,
    shared: &Budget,
) -> Result<Outcome, ClassificationError> {
    let mut usage = ClassificationUsage::default();
    let (admitted, deferred_by_sibling) = admit_units(session, shared, !runtime.can_resume);
    let judged_chars = admitted
        .iter()
        .map(|&[resource, unit]| session.resources[resource].units[unit].chars)
        .sum::<usize>();
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
    let results = judge_units(session, &admitted, &pending, dispatcher, context);
    let settled = apply_results(session, &admitted, pending, results.into_iter(), &mut usage);
    session.active.clear();
    let (remaining, remaining_chars) = pending_chars(session);
    usage.estimate_rest(judged_chars, remaining_chars);
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
    let held = if runtime.can_resume {
        let held = attach_continuations(&mut output, session, remaining == 0);
        // The kept page in view details, replayed without inference.
        let Ok(assessment) = token(&session.id, ordinal).try_into() else {
            unreachable!("a batch token is a valid assessment token");
        };
        let details = ClasifyQuery {
            subtype_0: None,
            subtype_1: None,
            subtype_2: None,
            subtype_3: Some(ClasifyQuerySectionsAssessment {
                assessment,
                main_goal: None,
                operation: ClasifyQuerySectionsAssessmentOperation::Sections,
                reasoning: None,
                view: Some(ClasifyView::Details),
            }),
        };
        insert_next(
            &mut output["next"],
            ClasifyChannel::Details,
            Continuation::continued_query(details),
        );
        held
    } else {
        Held::default()
    };
    if remaining > 0 {
        let Ok(assessment) = token(&session.id, ordinal + 1).try_into() else {
            unreachable!("a batch token is a valid assessment token");
        };
        let next = ClasifyQuery {
            subtype_0: None,
            subtype_1: None,
            subtype_2: None,
            subtype_3: Some(ClasifyQuerySectionsAssessment {
                assessment,
                main_goal: None,
                operation: ClasifyQuerySectionsAssessmentOperation::Sections,
                reasoning: None,
                view: None,
            }),
        };
        insert_next(
            &mut output["next"],
            ClasifyChannel::NextPage,
            Continuation::continued_query(next),
        );
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
        // provisional output has not yet reached the caller: its quiet
        // sections too, which no kept plan can list.
        held.restore(&mut output);
        for resource in &mut session.resources {
            if output["resources"].get(&resource.key).is_some() {
                resource.shown = None;
            }
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
    Ok(Outcome { output })
}

/// A section's provider state with the template's section scope as its
/// `question`: the judge answers about the target, never from its context
/// entries. Added when the batch runs, so a saved plan gets it too.
pub(super) fn scoped(state: std::borrow::Cow<'_, serde_json::Value>) -> serde_json::Value {
    let mut state = state.into_owned();
    if let (Some(question), Some(fields)) = (
        crate::tools::selection::section_question(),
        state.as_object_mut(),
    ) {
        fields.insert("question".into(), question.clone());
    }
    state
}

/// Every admitted unit's `pending` questions, judged concurrently under the
/// call's budget and gate; the answers come in admitted order.
pub(super) fn judge_units(
    session: &Session,
    admitted: &[[usize; 2]],
    pending: &[Vec<usize>],
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Vec<QuestionOutcomes> {
    let evaluator = dispatcher.evaluator();
    let gate = evaluator.gate();
    let questions: Vec<_> = session
        .keys
        .iter()
        .map(|key| session.questions[key].clone())
        .collect();
    let (gate, questions) = (&gate, &questions);
    let mut results = dispatcher.handle().block_on(
        stream::iter(admitted.iter().zip(pending).enumerate())
            .map(|(position, (&[resource, unit], pending))| async move {
                let remaining: Vec<_> = pending
                    .iter()
                    .map(|&index| questions[index].clone())
                    .collect();
                let state = scoped(super::super::rows::contextual_state(
                    &session.resources[resource].units[unit].state,
                    session.context.as_deref(),
                ));
                // One request's time starts when it runs; the call's
                // deadline bounds them all.
                let budget = evaluator.budget(
                    crate::execution::cancel::deadline_of(context),
                    context.cancellation.clone(),
                );
                let result = evaluator
                    .evaluate_questions(
                        EvaluationInput {
                            state: &state,
                            questions: &remaining,
                            assets: None,
                            long_context: session.long_context,
                        },
                        session.model.as_deref(),
                        &budget,
                        gate,
                    )
                    .await;
                (position, result)
            })
            .buffer_unordered(call_cap(evaluator.max_concurrency))
            .collect::<Vec<_>>(),
    );
    results.sort_unstable_by_key(|(position, _)| *position);
    results.into_iter().map(|(_, result)| result).collect()
}

/// The units this batch assesses, as `[resource, unit]`, in plan order
/// while the shared budget admits them; and whether a sibling's admissions
/// deferred the whole batch.
fn admit_units(session: &Session, shared: &Budget, whole: bool) -> (Vec<[usize; 2]>, bool) {
    let mut admitted = Vec::new();
    for (resource_index, resource) in session.resources.iter().enumerate() {
        for (unit_index, unit) in resource.units.iter().enumerate() {
            if unit.cells.iter().all(Option::is_some) {
                continue;
            }
            if !shared.admit(unit.chars, whole) {
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
pub(super) fn apply_results(
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

#[cfg(test)]
#[path = "assess/retry_tests.rs"]
mod retry_tests;
