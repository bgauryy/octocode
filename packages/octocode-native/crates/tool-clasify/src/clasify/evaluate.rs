//! One evaluation row: capture its source when it names one, capture its
//! assets, and judge the state.
use super::{Row, RowOutcome, Target};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_tools_shared::judgment::{
    ClassificationError, EvaluationInput, Evaluator, Judged, LongContext, RequestCap,
    shared::Evaluations,
};
use octocode_tools_shared::source_capture::{self, Captured, SourceReader};

fn rejected(error: ClassificationError, captured: Option<Box<Captured>>) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured,
        assets: None,
        identity: None,
        price: None,
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
    gate: &octocode_runtime_base::providers::classification::gate::GateLease,
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
                can_resume,
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
    let prepared = match prepare(row, target, dispatcher, context, can_resume) {
        Ok(prepared) => prepared,
        Err(outcome) => return *outcome,
    };
    let planned = prepared.planned(evaluator, row);
    let retries = evaluator.retry_room();
    if let Some(max) = row.max_requests
        && planned.at_most > max
    {
        return RowOutcome::OverBudget(Box::new(super::reply::OverBudget {
            units: 1,
            price: planned.price(retries),
            input_tokens_at_most: prepared.tokens_at_most(evaluator, row),
            max,
            room: retries,
        }));
    }
    let room = room(row.max_requests, planned.at_most, retries);
    let price = planned.price(room);
    // Every finite ceiling is held, capped or not; an uncapped source that
    // may be judged in sections states none and has none.
    let cap = price.ceiling.map(|_| held(row, planned.at_most, room));
    judge_prepared(
        evaluator,
        evaluations,
        row,
        target,
        prepared,
        cap.as_ref(),
        price,
        dispatcher,
        context,
        gate,
        can_resume,
    )
}

/// The transient retries a row of `at_most` requests shares, of the
/// `retries` the runtime allows it: all of them without `maxRequests`, else
/// only the room its cap leaves past `at_most`. A retry is never needed to
/// answer, so a cap that holds every planned request, re-ask and split
/// admits the row with fewer retries, never refuses it for them.
pub(super) fn room(max: Option<u64>, at_most: u64, retries: u64) -> u64 {
    max.map_or(retries, |max| retries.min(max.saturating_sub(at_most)))
}

/// The cap a row of `at_most` requests sharing `room` transient retries is
/// held to: its `maxRequests` admitted it, else its stated ceiling rests on
/// the provider's windows ([`RequestCap::implicit`]).
pub(super) fn held(row: &Row, at_most: u64, room: u64) -> RequestCap {
    if row.max_requests.is_some() {
        RequestCap::held(at_most, room)
    } else {
        RequestCap::implicit(at_most, room)
    }
}

/// A row's state, captured and ready to judge: its capture, its assets,
/// where its answers sit, and the questions as they are sent.
pub(super) struct Prepared {
    state: serde_json::Value,
    captured: Option<Box<Captured>>,
    assets: Option<source_capture::assets::CapturedAssets>,
    located: Option<super::locate::Located>,
    questions: Vec<serde_json::Value>,
}

impl Prepared {
    /// The judged state with the row's task context.
    fn sent<'a>(&'a self, row: &Row) -> std::borrow::Cow<'a, serde_json::Value> {
        super::rows::contextual_state(
            self.located
                .as_ref()
                .map_or(&self.state, |located| &located.state),
            row.context.as_deref(),
        )
    }

    /// The evaluation sent, over `sent`.
    fn input<'a>(
        &'a self,
        row: &Row,
        sent: &'a serde_json::Value,
        cap: Option<&'a octocode_tools_shared::judgment::RequestCap>,
    ) -> EvaluationInput<'a> {
        EvaluationInput {
            state: sent,
            questions: &self.questions,
            assets: self.assets.as_ref().map(|assets| &assets.provider),
            long_context: LongContext::from_field(row.long_context),
            companions: self
                .located
                .as_ref()
                .map_or(0, |located| located.companions.len()),
            cap,
        }
    }

    /// What its evaluation sends before any is sent ([`Planned`]).
    pub(super) fn planned(&self, evaluator: &Evaluator, row: &Row) -> Planned {
        let sent = self.sent(row);
        let sends = evaluator.sends(self.input(row, &sent, None), row.model.as_deref());
        Planned {
            first: sends.first,
            at_most: sends.at_most,
            bounded: !(sends.refusable && row.max_requests.is_none()),
        }
    }

    /// The input tokens its first round costs at most; `None` while nothing
    /// bounds the provider's own prompt ([`Evaluator::tokens_at_most`]).
    pub(super) fn tokens_at_most(&self, evaluator: &Evaluator, row: &Row) -> Option<u64> {
        let sent = self.sent(row);
        let sends = evaluator.sends(self.input(row, &sent, None), row.model.as_deref());
        evaluator.first_round_tokens_at_most(
            row.model.as_deref(),
            row.long_context == Some(true),
            &sends,
            self.questions.len(),
        )
    }
}

/// What a prepared row's evaluation sends: its first round, and the most
/// requests it sends without transient retries. `bounded`: a capped row is
/// never judged in sections, so its evaluation bounds it; an uncapped one
/// whose last window may refuse it may be, and is unbounded.
#[derive(Clone, Copy)]
pub(super) struct Planned {
    pub first: u64,
    pub at_most: u64,
    pub bounded: bool,
}

impl Planned {
    /// Its price when it shares `room` transient retries: the stated
    /// ceiling adds them ([`RequestCap::held`]).
    pub(super) fn price(self, room: u64) -> super::reply::Price {
        super::reply::Price {
            planned: self.first,
            ceiling: self
                .bounded
                .then(|| RequestCap::ceiling(self.at_most, room)),
            room,
        }
    }
}

/// Capture `target`'s state and assets and locate its answers; a capture
/// that fails is the row's outcome.
pub(super) fn prepare(
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    can_resume: bool,
) -> Result<Prepared, Box<RowOutcome>> {
    let (state, captured) = judged_state(target, dispatcher, context, can_resume)
        .map_err(|error| Box::new(rejected(error, None)))?;
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
        Err(error) => return Err(Box::new(rejected(error, captured))),
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
                octocode_runtime_base::response::page_windows::page_budget(Some(
                    dispatcher.limits().auto_page_chars,
                )),
            )
        })
        .flatten();
    let questions = super::locate::questions(located.as_ref(), &row.questions).into_owned();
    Ok(Prepared {
        state,
        captured,
        assets,
        located,
        questions,
    })
}

/// Judge a prepared row: one evaluation, and for an uncapped text source
/// the window refuses, its sections.
#[expect(
    clippy::too_many_arguments,
    reason = "one row's evaluation: its runtime, its capture and its cap"
)]
pub(super) fn judge_prepared(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    target: &Target,
    prepared: Prepared,
    cap: Option<&octocode_tools_shared::judgment::RequestCap>,
    price: super::reply::Price,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &octocode_runtime_base::providers::classification::gate::GateLease,
    can_resume: bool,
) -> RowOutcome {
    let sent = prepared.sent(row);
    let input = prepared.input(row, &sent, cap);
    // One request's time starts when it runs, after the capture.
    let budget = evaluator.budget(
        octocode_runtime_base::execution::cancel::deadline_of(context),
        context.cancellation.clone(),
    );
    // A source the provider is known to refuse at this size is not uploaded
    // whole to be refused again: it goes straight to its sections. When they
    // cannot answer, the provider is asked after all, and its answer stands.
    // A capped row is never judged in sections: their requests are not
    // priced before they are sent.
    let mut sectioned = None;
    if let Target::Source(source) = target
        && cap.is_none()
        && evaluator.known_refused(input, row.model.as_deref())
    {
        match super::combined::judge(row, source, 0, gate, dispatcher, context, can_resume) {
            Ok(combined) => return RowOutcome::Combined(Box::new(combined)),
            Err(failed) => sectioned = Some(failed),
        }
    }
    let (mut judged, identity) = dispatcher.handle().block_on(evaluator.evaluate_once(
        evaluations,
        input,
        row.model.as_deref(),
        &budget,
        gate,
    ));
    drop(sent);
    let Prepared {
        state,
        mut captured,
        assets,
        located,
        ..
    } = prepared;
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
    if let Some(failed) = sectioned {
        // Its sections were tried before the provider was asked: the row
        // counts their requests, and they are not sent twice.
        settle_sections(&mut judged, *failed);
    } else if let (Err(refusal), Target::Source(source), None, None) =
        (&mut judged, target, &assets, cap)
        && refusal.code == ErrorCode::ClassificationStateTooLarge
    {
        match super::combined::judge(
            row,
            source,
            refusal.provider_calls,
            gate,
            dispatcher,
            context,
            can_resume,
        ) {
            Ok(combined) => return RowOutcome::Combined(Box::new(combined)),
            // The sections' requests were sent too: the refusal counts them.
            Err(failed) => settle_sections(&mut judged, *failed),
        }
    }
    RowOutcome::Judgment {
        judged,
        captured,
        assets: assets.map(|assets| assets.provenance),
        identity,
        price: Some(price),
    }
}

/// Count the requests of sections that could not answer into the row's own
/// outcome, tokens included; a refusal whose every question failed for one
/// other reason takes that reason.
fn settle_sections(
    judged: &mut Result<Judged, ClassificationError>,
    failed: super::combined::Failed,
) {
    let usage = failed.usage;
    match judged {
        Ok(judged) => {
            judged.provider_calls = judged.provider_calls.saturating_add(usage.calls);
            judged.known_usage_calls = judged
                .known_usage_calls
                .saturating_add(usage.known_usage_calls);
            judged.input_tokens = judged.input_tokens.saturating_add(usage.input_tokens);
            judged.output_tokens = judged.output_tokens.saturating_add(usage.output_tokens);
        }
        Err(refusal) => {
            refusal.provider_calls = refusal.provider_calls.saturating_add(usage.calls);
            let mut reported = refusal
                .reported_usage
                .as_deref()
                .copied()
                .unwrap_or_default();
            reported.input_tokens = reported.input_tokens.saturating_add(usage.input_tokens);
            reported.output_tokens = reported.output_tokens.saturating_add(usage.output_tokens);
            reported.known_usage_calls = reported
                .known_usage_calls
                .saturating_add(usage.known_usage_calls);
            refusal.reported_usage = Some(Box::new(reported));
            if let Some((code, message)) = failed.cause {
                refusal.code = code;
                refusal.message = message;
                refusal.hints.clear();
            }
        }
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
pub struct ResourceOutcome {
    pub key: String,
    pub source: Option<serde_json::Value>,
    pub outcome: RowOutcome,
}

/// An `evaluate` row with `resources`: every glob expanded
/// ([`source_capture::glob`]), then each resource judged whole with all the
/// row's questions, as a `source` is: one grouped request per resource, run
/// in parallel under the gate. A row with `maxRequests` captures and prices
/// every resource first: a total ceiling over the cap sends nothing, and the
/// resources' sends share the one cap.
pub(super) fn resources_row(
    evaluator: &Evaluator,
    evaluations: &Evaluations,
    row: &Row,
    resources: &serde_json::Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    gate: &octocode_runtime_base::providers::classification::gate::GateLease,
    can_resume: bool,
) -> RowOutcome {
    let none = serde_json::Map::new();
    let expanded =
        source_capture::glob::expand(resources.as_object().unwrap_or(&none), dispatcher, context);
    // The width the provider gate grants one call: more workers would only
    // wait for it.
    let width =
        octocode_runtime_base::providers::classification::gate::call_cap(evaluator.max_concurrency);
    let Some(max) = row.max_requests else {
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
        return RowOutcome::Resources {
            resources,
            notices: expanded.notices,
            related: expanded.related,
        };
    };
    // Capture and price every resource before any is sent.
    let prepared = each(expanded.resources, width, |(key, entry)| {
        let _enter = dispatcher.handle().enter();
        let prepared = entry.map(|source| {
            let prepared = prepare(
                row,
                &Target::Source(source.clone()),
                dispatcher,
                context,
                can_resume,
            );
            (source, prepared)
        });
        (key, prepared)
    });
    // The resources share one cap, and the row's transient retries once.
    let retries = evaluator.retry_room();
    let (mut planned, mut at_most, mut tokens, mut units) = (0_u64, 0_u64, Some(0_u64), 0_usize);
    for (_, entry) in &prepared {
        if let Ok((_, Ok(prepared))) = entry {
            let sends = prepared.planned(evaluator, row);
            planned = planned.saturating_add(sends.first);
            at_most = at_most.saturating_add(sends.at_most);
            tokens = tokens
                .zip(prepared.tokens_at_most(evaluator, row))
                .map(|(sum, more)| sum.saturating_add(more));
            units += 1;
        }
    }
    if at_most > max {
        return RowOutcome::OverBudget(Box::new(super::reply::OverBudget {
            units,
            price: super::reply::Price {
                planned,
                ceiling: Some(RequestCap::ceiling(at_most, retries)),
                room: retries,
            },
            input_tokens_at_most: tokens,
            max,
            room: retries,
        }));
    }
    let room = room(Some(max), at_most, retries);
    let cap = RequestCap::held(at_most, room);
    let resources = each(prepared, width, |(key, entry)| {
        let _enter = dispatcher.handle().enter();
        match entry {
            Ok((source, Ok(prepared))) => {
                let price = prepared.planned(evaluator, row).price(room);
                let target = Target::Source(source.clone());
                ResourceOutcome {
                    key,
                    outcome: judge_prepared(
                        evaluator,
                        evaluations,
                        row,
                        &target,
                        prepared,
                        Some(&cap),
                        price,
                        dispatcher,
                        context,
                        gate,
                        can_resume,
                    ),
                    source: Some(source),
                }
            }
            Ok((source, Err(outcome))) => ResourceOutcome {
                key,
                outcome: *outcome,
                source: Some(source),
            },
            Err(error) => ResourceOutcome {
                key,
                source: None,
                outcome: rejected(error, None),
            },
        }
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
    each(resources, width, |(key, entry)| match entry {
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
    })
}

/// `work` over every item, `width` at a time, in input order.
fn each<T: Send, U: Send>(items: Vec<T>, width: usize, work: impl Fn(T) -> U + Sync) -> Vec<U> {
    let width = width.clamp(1, items.len().max(1));
    // Each item is taken once, by whichever worker reaches it first, and
    // leaves exactly one outcome with its input position.
    let queue = items
        .into_iter()
        .map(|item| std::sync::Mutex::new(Some(item)))
        .collect::<Vec<_>>();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let (queue, next, work) = (&queue, &next, &work);
    let mut done = std::thread::scope(|scope| {
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
                        let Some(item) = taken else {
                            continue;
                        };
                        done.push((index, work(item)));
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
    done.sort_unstable_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, outcome)| outcome).collect()
}
