//! One evaluation row: capture its source when it names one, capture its
//! assets, and judge the state.
use super::{Row, RowOutcome, Target};
use crate::execution::ExecutionContext;
use crate::tools::judgment::{
    ClassificationError, EvaluationInput, Evaluator, LongContext, shared::Evaluations,
};
use crate::tools::source_capture::{self, Captured, SourceReader};
use octocode_contract_types::ErrorCode;

fn rejected(error: ClassificationError, captured: Option<Box<Captured>>) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured,
        assets: None,
        identity: None,
    }
}

/// Whether literal `content` holds nothing to judge.
fn blank(state: &serde_json::Value) -> bool {
    match state {
        serde_json::Value::Null => true,
        serde_json::Value::String(text) => text.trim().is_empty(),
        serde_json::Value::Array(items) => items.is_empty(),
        serde_json::Value::Object(fields) => fields.is_empty(),
        _ => false,
    }
}

pub(super) fn evaluate_row(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &crate::providers::classification::gate::GateLease,
    can_resume: bool,
) -> RowOutcome {
    match target {
        Target::Content(content) if row.per_item => {
            return super::units::judge_items(
                evaluator,
                evaluations,
                row,
                content,
                dispatcher,
                context,
                gate,
            );
        }
        Target::Source(source) => {
            if let Some(tool) = super::units::rows_tool(source) {
                return super::units::judge_source(
                    evaluator,
                    evaluations,
                    row,
                    tool,
                    source,
                    dispatcher,
                    context,
                    gate,
                    can_resume,
                );
            }
        }
        Target::Content(_) => {}
    }
    let (state, mut captured) = match judged_state(target, dispatcher, context, can_resume) {
        Ok(judged) => judged,
        Err(error) => return rejected(error, None),
    };
    let assets = match row
        .assets
        .as_ref()
        .map(|assets| {
            let field = matches!(target, Target::Content(_)).then_some("content");
            source_capture::assets::capture(
                assets,
                &state,
                field,
                &row.questions,
                dispatcher,
                context,
            )
        })
        .transpose()
    {
        Ok(assets) => assets,
        Err(error) => return rejected(error, captured),
    };
    // A text judged whole says where its answers sit
    // ([`super::locate`]); images point with their own boxes.
    let located = assets
        .is_none()
        .then(|| {
            super::locate::locate(
                target,
                &state,
                &row.questions,
                dispatcher.limits().auto_page_chars,
            )
        })
        .flatten();
    let questions = super::locate::questions(located.as_ref(), &row.questions);
    let sent = super::rows::contextual_state(
        located.as_ref().map_or(&state, |located| &located.state),
        row.context.as_deref(),
    );
    // One request's time starts when it runs, after the capture.
    let budget = evaluator.budget(
        crate::execution::cancel::deadline_of(context),
        context.cancellation.clone(),
    );
    let input = EvaluationInput {
        state: &sent,
        questions: &questions,
        assets: assets.as_ref().map(|assets| &assets.provider),
        long_context: LongContext::from_field(row.long_context),
        companions: located
            .as_ref()
            .map_or(0, |located| located.companions.len()),
    };
    // A source the provider is known to refuse at this size is not uploaded
    // whole to be refused again: it goes straight to its sections. When they
    // cannot answer, the provider is asked after all, and its answer stands.
    let mut sectioned = None;
    if let Target::Source(source) = target
        && evaluator.known_refused(input, row.model.as_deref())
    {
        match super::combined::judge(row, source, 0, dispatcher, context, can_resume) {
            Ok(combined) => return RowOutcome::Combined(Box::new(combined)),
            Err(calls) => sectioned = Some(calls),
        }
    }
    let (mut judged, identity) = dispatcher.handle().block_on(evaluator.evaluate_once(
        evaluations,
        input,
        row.model.as_deref(),
        &budget,
        gate,
    ));
    // Jev's boxes are fractions of the fitted canvas; callers also get them
    // on the original image.
    if let (Ok(judged), Some(assets)) = (&mut judged, &assets) {
        assets.attach_source_boxes(&row.questions, &mut judged.answers);
    }
    if let (Ok(judged), Some(located)) = (&mut judged, &located) {
        located.settle(&mut judged.answers, row.questions.len());
        if let (Target::Source(source), Some(captured)) = (target, captured.as_deref_mut()) {
            super::locate::narrow_read(
                captured,
                source,
                &state,
                &judged.answers,
                can_resume,
                dispatcher.limits().auto_page_chars,
            );
        }
    }
    // A text source over the provider's window is judged in sections and
    // answered once per question; when no section can answer, the refusal
    // stays the row's error.
    if let (Err(refusal), Some(calls)) = (&mut judged, sectioned) {
        // Its sections were tried before the provider was asked: the
        // refusal counts their requests, and they are not sent twice.
        refusal.provider_calls = refusal.provider_calls.saturating_add(calls);
    } else if let (Err(refusal), Target::Source(source), None) = (&mut judged, target, &assets)
        && refusal.code == ErrorCode::ClassificationStateTooLarge
    {
        match super::combined::judge(
            row,
            source,
            refusal.provider_calls,
            dispatcher,
            context,
            can_resume,
        ) {
            Ok(combined) => return RowOutcome::Combined(Box::new(combined)),
            // The sections' requests were sent too: the refusal counts them.
            Err(calls) => refusal.provider_calls = refusal.provider_calls.saturating_add(calls),
        }
    }
    RowOutcome::Judgment {
        judged,
        captured,
        assets: assets.map(|assets| assets.provenance),
        identity,
    }
}

/// The state `target` is judged by, and its capture when it names a source.
fn judged_state(
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    can_resume: bool,
) -> Result<(serde_json::Value, Option<Box<Captured>>), ClassificationError> {
    match target {
        // An empty literal is nothing to judge, as an empty source is.
        Target::Content(state) if blank(state) => Err(ClassificationError::new(
            ErrorCode::ClassificationContextEmpty,
            "content is empty; nothing was judged.",
            "Supply the text to judge.",
        )),
        Target::Content(state) => Ok((state.clone(), None)),
        Target::Source(source) => {
            // Only local memory bounds a capture: the provider's own window
            // decides what it judges, and its refusal says so.
            let limit = dispatcher.text_capture_limit();
            let mut captured =
                source_capture::capture(source, dispatcher, context, limit, can_resume)?;
            // The answer row reports the capture's read and provenance, never
            // its state.
            Ok((
                std::mem::take(&mut captured.state),
                Some(Box::new(captured)),
            ))
        }
    }
}

/// One resource of an `evaluate` row with `resources`: its key, the source
/// judged (none when it could not be expanded), and its outcome.
pub(super) struct ResourceOutcome {
    pub key: String,
    pub source: Option<serde_json::Value>,
    pub outcome: RowOutcome,
}

/// An `evaluate` row with `resources`: every glob expanded
/// ([`source_capture::glob`]), then each resource judged whole with all the
/// row's questions, as a `source` is: one grouped request per resource, run
/// in parallel under the gate.
pub(super) fn resources_row(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    resources: &serde_json::Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &crate::providers::classification::gate::GateLease,
    can_resume: bool,
) -> RowOutcome {
    let none = serde_json::Map::new();
    let expanded =
        source_capture::glob::expand(resources.as_object().unwrap_or(&none), dispatcher, context);
    // The width the provider gate grants one call: more workers would only
    // wait for it.
    let width = crate::providers::classification::gate::call_cap(evaluator.max_concurrency);
    let resources = each_resource(expanded.resources, width, |source| {
        let _enter = dispatcher.handle().enter();
        evaluate_row(
            evaluator,
            evaluations,
            row,
            &Target::Source(source.clone()),
            dispatcher,
            context,
            gate,
            can_resume,
        )
    });
    RowOutcome::Resources {
        resources,
        notices: expanded.notices,
        related: expanded.related,
    }
}

/// Judge every resource whole with `judge`, `width` at a time, in input
/// order; a resource that could not be expanded keeps its error.
pub(super) fn each_resource(
    resources: Vec<(String, Result<serde_json::Value, ClassificationError>)>,
    width: usize,
    judge: impl Fn(&serde_json::Value) -> RowOutcome + Sync,
) -> Vec<ResourceOutcome> {
    let width = width.clamp(1, resources.len().max(1));
    // Each resource is taken once, by whichever worker reaches it first, and
    // leaves exactly one outcome with its input position.
    let queue = resources
        .into_iter()
        .map(|resource| std::sync::Mutex::new(Some(resource)))
        .collect::<Vec<_>>();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (queue, next, judge) = (&queue, &next, &judge);
    let mut judged = std::thread::scope(|scope| {
        let workers = (0..width)
            .map(|_| {
                scope.spawn(octocode_io::notice::carried(move || {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(slot) = queue.get(index) else {
                            break done;
                        };
                        let taken = slot
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .take();
                        let Some((key, entry)) = taken else {
                            continue;
                        };
                        let outcome = match entry {
                            Ok(source) => ResourceOutcome {
                                key,
                                outcome: judge(&source),
                                source: Some(source),
                            },
                            Err(error) => ResourceOutcome {
                                key,
                                source: None,
                                outcome: rejected(error, None),
                            },
                        };
                        done.push((index, outcome));
                    }
                }))
            })
            .collect::<Vec<_>>();
        // A worker's panic goes on as the panic it is, as a row's does.
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    judged.sort_unstable_by_key(|(index, _)| *index);
    judged.into_iter().map(|(_, outcome)| outcome).collect()
}
