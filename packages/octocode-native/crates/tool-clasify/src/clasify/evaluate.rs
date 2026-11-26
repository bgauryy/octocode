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

pub(super) fn rejected(error: ClassificationError, captured: Option<Box<Captured>>) -> RowOutcome {
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
            if row.per_hit && super::units::rows_tool(source).is_none() {
                return rejected(no_hits(source), None);
            }
            if row.collapse {
                // Each kind is keyed by its line in the file, which the
                // caller reads next: a row source has no lines, a URL's text
                // is a capture the units row does not lead to, and a ranges
                // read numbers its lines within the ranges.
                let whole_file = source.get("url").is_none()
                    && super::units::rows_tool(source).is_none()
                    && source["query"].get("ranges").is_none();
                if !whole_file {
                    return rejected(
                        ClassificationError::new(
                            ErrorCode::InvalidClassificationContext,
                            "collapse judges the kinds of line of one whole local or GitHub file, keyed by file line; this source is a URL, a search or listing, or a ranges read, so nothing was read or sent.",
                            "Name a whole file as the source, or drop collapse.",
                        ),
                        None,
                    );
                }
                return match judged_state(target, dispatcher, context, can_resume) {
                    Ok((state, _)) => super::units::judge_kinds(
                        evaluator,
                        evaluations,
                        row,
                        state["text"].as_str().unwrap_or_default(),
                        dispatcher,
                        context,
                        gate,
                    ),
                    Err(error) => rejected(error, None),
                };
            }
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
    // A source or content judged whole is what the row names, every
    // question included: its first round is the row's own.
    let plan = prepared.plan(evaluator, row);
    if let Some(over) = unfollowed_refusal(evaluator, row, plan) {
        return RowOutcome::OverBudget(over);
    }
    let admitted =
        match super::admission::admit(row.max_requests, plan, plan.first, evaluator.retry_room()) {
            Ok(admitted) => admitted,
            Err(over) => return RowOutcome::OverBudget(over),
        };
    judge_prepared(
        evaluator,
        evaluations,
        row,
        target,
        prepared,
        admitted.cap.as_ref(),
        admitted.price,
        dispatcher,
        context,
        gate,
        can_resume,
    )
}

/// The price of a row whose first round its window may refuse while its
/// `maxRequests` holds none of the requests a refusal adds (the long-window
/// retry, the per-question re-asks): a refusal it cannot follow answers
/// nothing, and no window bound survives the process that learned it
/// ([`octocode_tools_shared::judgment::windows`]), so nothing is sent and the
/// row states the whole price first. Each added request is taken at most as
/// large as the long window.
fn unfollowed_refusal(
    evaluator: &Evaluator,
    row: &Row,
    plan: super::admission::Plan,
) -> Option<Box<super::reply::OverBudget>> {
    let max = row.max_requests?;
    if !plan.bounded || plan.first > max || plan.at_most <= max {
        return None;
    }
    let retries = evaluator.retry_room();
    let added = plan.at_most - plan.first;
    Some(Box::new(super::reply::OverBudget {
        units: plan.units,
        price: super::reply::Price {
            planned: plan.first,
            ceiling: Some(RequestCap::ceiling(plan.at_most, retries)),
            room: retries,
            tokens: Some(
                plan.tokens
                    .saturating_add(added.saturating_mul(evaluator.request_window(true))),
            ),
        },
        limit: super::reply::Limit::Max(max),
        room: retries,
        spent: 0,
        whole_windows: None,
        learned: 0,
    }))
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
    pub(super) fn sent<'a>(&'a self, row: &Row) -> std::borrow::Cow<'a, serde_json::Value> {
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

    /// What its evaluation sends before any is sent
    /// ([`super::admission::Plan`]): a source the last window may refuse is
    /// unbounded under `maxRequests`, whose cap its sections may then use;
    /// without it no section follows unconfirmed, so its evaluation bounds
    /// it.
    pub(super) fn plan(&self, evaluator: &Evaluator, row: &Row) -> super::admission::Plan {
        let sent = self.sent(row);
        let sends = evaluator.sends(self.input(row, &sent, None), row.model.as_deref());
        super::admission::Plan {
            first: sends.first,
            at_most: sends.at_most,
            bounded: !(sends.refusable && row.max_requests.is_some() && self.captured.is_some()),
            units: 1,
            tokens: evaluator.first_round_tokens_at_most(
                row.model.as_deref(),
                row.long_context == Some(true),
                &sends,
                self.questions.len(),
            ),
            window: Some(evaluator.request_window(row.long_context == Some(true))),
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
    let state = judged_state(target, dispatcher, context, can_resume);
    prepare_with_state(row, target, dispatcher, context, state)
}

/// Local preview uses the text capture, and leaves an image as its selector.
/// Preparing a real image would persist its pixels and construct the provider payload.
pub(super) fn preview_text(
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<Option<Prepared>, Box<RowOutcome>> {
    let state = judged_state(target, dispatcher, context, false);
    if matches!(&state, Err(error) if error.code == ErrorCode::BinaryFileUnsupported) {
        return Ok(None);
    }
    prepare_with_state(row, target, dispatcher, context, state).map(Some)
}

fn prepare_with_state(
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    state: Result<(serde_json::Value, Option<Box<Captured>>), ClassificationError>,
) -> Result<Prepared, Box<RowOutcome>> {
    let (state, captured, image) = match state {
        Ok((state, captured)) => (state, captured, None),
        // A source that is no text may be an image: it is judged as one.
        Err(refusal) => match image_source(row, target, &refusal) {
            Some((key, assets)) => (serde_json::json!(key), None, Some((assets, refusal))),
            None => return Err(Box::new(rejected(refusal, None))),
        },
    };
    let field = matches!(target, Target::Content(_)).then_some("content");
    let named = image
        .as_ref()
        .map(|(assets, _)| assets)
        .or(row.assets.as_ref());
    let assets = match named
        .map(|assets| {
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
        // Bytes that are no PNG or JPEG keep the text reader's refusal: the
        // source is binary, and no image failed.
        Err(error) => {
            let error = match image {
                Some((_, refusal)) if error.code == ErrorCode::InvalidClassificationContext => {
                    refusal
                }
                _ => error,
            };
            return Err(Box::new(rejected(error, captured)));
        }
    };
    if assets.is_none()
        && row
            .questions
            .iter()
            .any(|question| question["type"] == "box")
    {
        return Err(Box::new(rejected(
            ClassificationError::new(
                ErrorCode::InvalidClassificationContext,
                "A box question locates a region of an image, and this row judges text; nothing was sent.",
                "Name a PNG or JPEG as the source or resource, or drop the box question.",
            ),
            captured,
        )));
    }
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

/// The key a source judged as an image is named by in its state. The provider
/// puts an image wherever a request string holds its key, so the key is no
/// word a question would use: one that said "the image" under the key `image`
/// was billed the image a second time (measured 2026-11-21: 4,444 input
/// tokens against 2,304).
const IMAGE_KEY: &str = "sourceImage";

/// `target` as an image beside the row's own assets, when its text reader
/// refused it as binary: the state's key and the assets that hold it. The
/// asset reads the source's identity; line ranges select text and are left.
fn image_source(
    row: &Row,
    target: &Target,
    refusal: &ClassificationError,
) -> Option<(String, serde_json::Value)> {
    let Target::Source(source) = target else {
        return None;
    };
    if refusal.code != ErrorCode::BinaryFileUnsupported {
        return None;
    }
    let mut asset = source.clone();
    if let Some(query) = asset
        .get_mut("query")
        .and_then(serde_json::Value::as_object_mut)
    {
        query.shift_remove("ranges");
    }
    let mut assets = row
        .assets
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    let key = source_capture::glob::free_key(IMAGE_KEY, |key| assets.contains_key(key));
    assets.insert(key.clone(), serde_json::json!({ "source": asset }));
    Some((key, serde_json::Value::Object(assets)))
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
    // Literal content has no sections: it is not sent to be refused again.
    let refused = evaluator.known_refused(input, row.model.as_deref());
    if refused && !matches!(target, Target::Source(_)) {
        drop(sent);
        return rejected(known_too_large(), prepared.captured);
    }
    // A source's sections are the source the row named, part by part: a row
    // without `maxRequests` judges them all, as it would have billed the
    // whole source in one window. Under `maxRequests` they are judged only
    // while its cap still holds their first round; otherwise priced, and
    // none is sent ([`sections_over`]).
    let admits =
        |sections: u64| row.max_requests.is_none() || cap.is_some_and(|cap| sections <= cap.left());
    // An uncapped row's own ceiling prices one request, not its parts.
    let section_cap = cap.filter(|_| row.max_requests.is_some());
    let retries = evaluator.retry_room();
    // What asking the source whole sends at most, its long-window retry
    // included: a fresh run asks it again before its sections.
    let whole = evaluator.sends(input, row.model.as_deref()).at_most;
    let mut sectioned = None;
    if let Target::Source(source) = target
        && refused
    {
        match super::combined::judge(
            row,
            source,
            0,
            section_cap,
            &admits,
            gate,
            dispatcher,
            context,
            can_resume,
        ) {
            Ok(combined) => return RowOutcome::Combined(Box::new(combined)),
            Err(failed) => match failed.unadmitted {
                Some(sections) => {
                    return RowOutcome::OverBudget(sections_over(
                        row,
                        sections,
                        whole,
                        0,
                        retries,
                        evaluator.request_window(true),
                    ));
                }
                None => sectioned = Some(failed),
            },
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
    } else if let (Err(refusal), Target::Source(source), None) = (&mut judged, target, &assets)
        && refusal.code == ErrorCode::ClassificationStateTooLarge
    {
        match super::combined::judge(
            row,
            source,
            refusal.provider_calls,
            section_cap,
            &admits,
            gate,
            dispatcher,
            context,
            can_resume,
        ) {
            Ok(combined) => return RowOutcome::Combined(Box::new(combined)),
            Err(failed) => match failed.unadmitted {
                // The whole request was refused; its sections wait for a
                // bound that holds them.
                Some(sections) => {
                    return RowOutcome::OverBudget(sections_over(
                        row,
                        sections,
                        whole,
                        refusal.provider_calls,
                        retries,
                        evaluator.request_window(true),
                    ));
                }
                // The sections' requests were sent too: the refusal counts them.
                None => settle_sections(&mut judged, *failed),
            },
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

/// The refusal of `perHit` on a source judged whole: a file or page read has
/// no hits to send alone, so nothing was read or sent.
fn no_hits(source: &serde_json::Value) -> ClassificationError {
    let what = source["tool"].as_str().map_or_else(
        || "a URL source".to_owned(),
        |tool| format!("a {tool} source"),
    );
    ClassificationError::new(
        ErrorCode::InvalidClassificationContext,
        format!(
            "perHit sends each hit of a search or listing source alone; {what} is one text with no hits, so nothing was read or sent."
        ),
        "Drop perHit to judge the text whole, or name a search tool as source.tool.",
    )
}

/// The refusal of literal content the provider's window refused at this
/// size earlier in this process ([`Evaluator::known_refused`]): it has no
/// sections, so nothing was sent.
fn known_too_large() -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationStateTooLarge,
        "The provider refused a request this large for this model and window earlier in this process, so this one was not sent.",
        "Send less content, or save it under the workspace and judge that path as a source.",
    )
}

/// A source over the window whose `sections` were not admitted: none was
/// sent, after `spent` requests of the whole source. Its price is a fresh
/// run's: the source asked whole once more (`whole` requests at most, its
/// long-window retry included), then its sections, with the `retries` they
/// share; `next.confirm` holds it.
fn sections_over(
    row: &Row,
    sections: u64,
    whole: u64,
    spent: u64,
    retries: u64,
    long_window: u64,
) -> Box<super::reply::OverBudget> {
    let at_most = sections.saturating_add(whole);
    Box::new(super::reply::OverBudget {
        units: usize::try_from(sections).unwrap_or(usize::MAX),
        price: super::reply::Price {
            planned: sections,
            ceiling: Some(RequestCap::ceiling(at_most, retries)),
            room: retries,
            // Each request is taken at most as large as the long window.
            tokens: Some(at_most.saturating_mul(long_window)),
        },
        limit: row
            .max_requests
            .map_or(super::reply::Limit::Named(1), super::reply::Limit::Max),
        room: retries,
        spent,
        whole_windows: None,
        learned: 0,
    })
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
/// in parallel under the gate. Every resource is captured and priced before
/// any is sent, and the row is admitted as one plan
/// ([`super::admission::admit`]): with `maxRequests` the resources share the
/// one cap; without it the row sends one request per resource key it names,
/// so a glob that matched more sends nothing until it is bounded.
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
    let named = resources.as_object().unwrap_or(&none);
    let expanded = source_capture::glob::expand(named, dispatcher, context);
    // The width the provider gate grants one call: more workers would only
    // wait for it.
    let width =
        octocode_runtime_base::providers::classification::gate::call_cap(evaluator.max_concurrency);
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
    // The resources' plans as one: the row's transient retries are shared
    // once.
    let retries = evaluator.retry_room();
    // Each resource's tokens are priced in the same window.
    let mut plan = super::admission::Plan {
        first: 0,
        at_most: 0,
        bounded: true,
        units: 0,
        tokens: 0,
        window: Some(evaluator.request_window(row.long_context == Some(true))),
    };
    for (_, entry) in &prepared {
        if let Ok((_, Ok(prepared))) = entry {
            let one = prepared.plan(evaluator, row);
            plan.first = plan.first.saturating_add(one.first);
            plan.at_most = plan.at_most.saturating_add(one.at_most);
            plan.bounded &= one.bounded;
            plan.tokens = plan.tokens.saturating_add(one.tokens);
            plan.units += 1;
        }
    }
    let named = u64::try_from(named.len()).unwrap_or(u64::MAX);
    let admitted = match super::admission::admit(row.max_requests, plan, named, retries) {
        Ok(admitted) => admitted,
        Err(over) => return RowOutcome::OverBudget(over),
    };
    let resources = each(prepared, width, |(key, entry)| {
        let _enter = dispatcher.handle().enter();
        match entry {
            Ok((source, Ok(prepared))) => {
                let target = Target::Source(source.clone());
                // A capped row's resources share its one cap; an uncapped
                // row's are each held to their own ceiling, as a source is.
                let one = prepared.plan(evaluator, row);
                let own = super::admission::own_ceiling(one, retries);
                let (cap, price) = if row.max_requests.is_some() {
                    let room = admitted.price.room;
                    (
                        admitted.cap.as_ref(),
                        super::reply::Price {
                            planned: one.first,
                            ceiling: Some(RequestCap::ceiling(one.at_most, room)),
                            room,
                            tokens: Some(one.tokens),
                        },
                    )
                } else {
                    (own.cap.as_ref(), own.price)
                };
                ResourceOutcome {
                    key,
                    outcome: judge_prepared(
                        evaluator,
                        evaluations,
                        row,
                        &target,
                        prepared,
                        cap,
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

/// `work` over every item, `width` at a time, in input order.
pub(super) fn each<T: Send, U: Send>(
    items: Vec<T>,
    width: usize,
    work: impl Fn(T) -> U + Sync,
) -> Vec<U> {
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
