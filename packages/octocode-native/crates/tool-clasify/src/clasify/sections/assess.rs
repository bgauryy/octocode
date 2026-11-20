//! One bounded assessment batch over the frozen plan, saved before and
//! after its provider requests.
use super::emit::{Held, answered_cell, attach_continuations, emit, terminal_saved};
use super::{Budget, Outcome, Runtime, Session, error_value, token};
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::{
    ClasifyChannel, ClasifyQuery, ClasifyQuerySectionsAssessment,
    ClasifyQuerySectionsAssessmentOperation, ClasifyView, ErrorCode,
};
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::providers::classification::gate::{GateLease, call_cap};
use octocode_runtime_base::response::result::{Continuation, insert_next};
use octocode_tools_shared::judgment::stats::ClassificationUsage;
use octocode_tools_shared::judgment::{
    ClassificationError, EvaluationInput, QuestionOutcomes, RequestCap,
};
use octocode_tools_shared::source_capture::SourceReader;
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
    gate: &GateLease,
) -> Result<Outcome, ClassificationError> {
    let mut usage = ClassificationUsage::default();
    let (admitted, deferred_by_sibling) =
        admit_units(session, shared, !runtime.can_resume, dispatcher);
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
        .map(|&[resource, unit]| pending_of(&session.resources[resource].units[unit]))
        .collect();
    // The page's price, and the ceiling it is held to: the row's
    // `maxRequests`, which its admission fits, else the provider's windows
    // ([`RequestCap::implicit`]).
    let plan = plan_of(session, &admitted, dispatcher);
    let room = dispatcher.evaluator().retry_room();
    let held = match session.max_requests {
        Some(max) => super::super::admission::within(max, plan, room),
        None => super::super::admission::own_ceiling(plan, room),
    };
    let cap = held
        .cap
        .unwrap_or_else(|| RequestCap::implicit(plan.at_most, room));
    usage.requests_planned = Some(held.price.planned);
    usage.requests_ceiling = held.price.ceiling;
    let inputs = inputs(session, &admitted, &pending);
    let results = judge_units(session, &inputs, dispatcher, context, gate, Some(&cap));
    let (settled, deferred) = apply_results(
        session,
        &admitted,
        pending,
        results.into_iter(),
        &mut usage,
        runtime.can_resume,
    );
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
    if let Some(notice) =
        super::super::reply::past_ceiling(usage.calls, usage.requests_ceiling.unwrap_or(u64::MAX))
    {
        octocode_runtime_base::response::paging::push_warnings(&mut output, [notice]);
    }
    if deferred > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            &mut output,
            [format!(
                "{} left unanswered by a transient failure: next.nextPage asks {} again, once.",
                octocode_engine::text::counted(
                    u64::try_from(deferred).unwrap_or(u64::MAX),
                    "section answer",
                    "section answers"
                ),
                if deferred == 1 { "it" } else { "them" },
            )],
        );
    }
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
        octocode_runtime_base::response::collection::insert_walk(
            &mut output["next"],
            octocode_runtime_base::id::ToolId::Clasify,
            Some("sections"),
            Continuation::continued_query(next),
        );
    }
    let source_disclosures = session.push_batch(&output);
    if runtime.can_resume
        && !runtime.sessions.save(
            runtime.sessions.key(&runtime.access, &session.id),
            session.clone(),
            context,
        )
    {
        session.batches.pop();
        session.source_disclosures = source_disclosures;
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

/// One admitted unit's evaluation: its scoped state and its pending
/// questions.
pub(super) struct Input {
    state: serde_json::Value,
    questions: Vec<serde_json::Value>,
}

impl Input {
    /// The evaluation it is sent as, counted against `cap`.
    pub(super) fn evaluation<'a>(
        &'a self,
        session: &Session,
        cap: Option<&'a RequestCap>,
    ) -> EvaluationInput<'a> {
        EvaluationInput {
            state: &self.state,
            questions: &self.questions,
            assets: None,
            long_context: session.long_context,
            companions: 0,
            cap,
        }
    }
}

/// Every admitted unit's evaluation of its `pending` questions.
pub(super) fn inputs(
    session: &Session,
    admitted: &[[usize; 2]],
    pending: &[Vec<usize>],
) -> Vec<Input> {
    admitted
        .iter()
        .zip(pending)
        .map(|(&[resource, unit], pending)| Input {
            state: super::super::rows::scoped(super::super::rows::contextual_state(
                &session.resources[resource].units[unit].state,
                session.context.as_deref(),
            )),
            questions: pending
                .iter()
                .map(|&index| session.questions[&session.keys[index]].clone())
                .collect(),
        })
        .collect()
}

/// Every input judged concurrently under the call's budget and gate, its
/// sends counted against `cap`; the answers come in input order.
pub(super) fn judge_units(
    session: &Session,
    inputs: &[Input],
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &GateLease,
    cap: Option<&RequestCap>,
) -> Vec<QuestionOutcomes> {
    let evaluator = dispatcher.evaluator();
    let mut results = dispatcher.handle().block_on(
        stream::iter(inputs.iter().enumerate())
            .map(|(position, input)| async move {
                // One request's time starts when it runs; the call's
                // deadline bounds them all.
                let budget = evaluator.budget(
                    octocode_runtime_base::execution::cancel::deadline_of(context),
                    context.cancellation.clone(),
                );
                let result = evaluator
                    .evaluate_questions(
                        input.evaluation(session, cap),
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

/// Every unit with a question still pending, as `[resource, unit]`, in plan
/// order.
pub(super) fn pending(session: &Session) -> Vec<[usize; 2]> {
    session
        .resources
        .iter()
        .enumerate()
        .flat_map(|(resource, planned)| {
            planned
                .units
                .iter()
                .enumerate()
                .filter(|(_, unit)| unit.cells.iter().any(Option::is_none))
                .map(move |(unit, _)| [resource, unit])
        })
        .collect()
}

/// The requests judging `units` (their pending questions) sends: each
/// one's first round, and every request its refusals can add.
pub(super) fn plan_of(
    session: &Session,
    units: &[[usize; 2]],
    dispatcher: &dyn SourceReader,
) -> super::super::admission::Plan {
    let evaluator = dispatcher.evaluator();
    let pending = units
        .iter()
        .map(|&[resource, unit]| pending_of(&session.resources[resource].units[unit]))
        .collect::<Vec<_>>();
    let (first, at_most) =
        inputs(session, units, &pending)
            .iter()
            .fold((0_u64, 0_u64), |(first, at_most), input| {
                let sends =
                    evaluator.sends(input.evaluation(session, None), session.model.as_deref());
                (
                    first.saturating_add(sends.first),
                    at_most.saturating_add(sends.at_most),
                )
            });
    super::super::admission::Plan {
        first,
        at_most,
        bounded: true,
        units: units.len(),
        tokens: None,
    }
}

/// The questions of `unit` still pending, in question order.
fn pending_of(unit: &super::Unit) -> Vec<usize> {
    unit.cells
        .iter()
        .enumerate()
        .filter_map(|(index, cell)| cell.is_none().then_some(index))
        .collect()
}

/// The units this batch assesses, as `[resource, unit]`, in plan order
/// while the shared budget admits them and, with `maxRequests`, while their
/// first round fits it (the first always does: a section is one request);
/// and whether a sibling's admissions deferred the whole batch. What a
/// batch does not take, the next takes.
fn admit_units(
    session: &Session,
    shared: &Budget,
    whole: bool,
    dispatcher: &dyn SourceReader,
) -> (Vec<[usize; 2]>, bool) {
    let mut admitted = Vec::new();
    let mut first = 0_u64;
    for at in pending(session) {
        if let Some(max) = session.max_requests {
            let more = plan_of(session, &[at], dispatcher).first;
            if !admitted.is_empty() && first.saturating_add(more) > max {
                return (admitted, false);
            }
            first = first.saturating_add(more);
        }
        let unit = &session.resources[at[0]].units[at[1]];
        if !shared.admit(unit.chars, whole) {
            let deferred_by_sibling = admitted.is_empty() && shared.has_admissions();
            return (admitted, deferred_by_sibling);
        }
        admitted.push(at);
    }
    (admitted, false)
}

/// Apply each admitted unit's answers, in admitted order, to its pending
/// cells; an answer from a model other than the session's pinned one is
/// refused. With `defer` (a plan a later batch continues), a cell a
/// transient failure left unanswered stays pending once, for the next batch
/// to ask again. Returns how many cells settled, the deferred ones
/// included, and how many were deferred.
pub(super) fn apply_results(
    session: &mut Session,
    admitted: &[[usize; 2]],
    pending: Vec<Vec<usize>>,
    results: impl Iterator<Item = QuestionOutcomes>,
    usage: &mut ClassificationUsage,
    defer: bool,
) -> (usize, usize) {
    let (mut settled, mut deferred) = (0, 0);
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
                Err(error)
                    if defer
                        && super::super::reply::transient(&error)
                        && unit.deferred.insert(index) =>
                {
                    deferred += 1;
                    settled += 1;
                }
                Err(error) => {
                    unit.cells[index] =
                        Some(json!({"status":"error", "error":error_value(&error)}));
                    settled += 1;
                }
            }
        }
    }
    (settled, deferred)
}

#[cfg(test)]
#[path = "assess/retry_tests.rs"]
mod retry_tests;
