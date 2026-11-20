//! Public `clasify`: typed judgments and model discovery. Evaluation rows
//! hold keyed `noul`/`choice`/`score`/`box` questions over
//! exactly one input: the caller's literal `content`, a `source` captured
//! completely through the authorized readers ([`source_capture`](octocode_tools_shared::source_capture)), or
//! `resources`, each judged whole as a `source` is. The row
//! returns the provider's validated answers, the answering model and usage,
//! and source provenance; never a body. A source the provider window refuses
//! is judged in sections and answered once per question ([`combined`]).
//! Explicit image assets are captured
//! internally as original bytes, with immutable local inspection copies.
//!
//! Every row evaluates through the shared [`Evaluator`]: one provider client,
//! retry policy and accounting for this tool and retrieval `classifiers`
//! alike. Every call's evaluations are fresh provider requests; nothing is
//! saved. Identical evaluations inside one call (same provider request
//! payload) share one request and are counted once.
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::response::result::ErrorClass;
use octocode_runtime_base::security::CheckedQuery;
use octocode_tools_shared::judgment::{
    ClassificationError, Evaluator, Judged, shared::Evaluations,
};
use octocode_tools_shared::source_capture::{Captured, SourceReader};
use serde_json::{Value, json};

mod across;
pub mod admission;
pub mod combined;
pub mod evaluate;
mod locate;
mod recovery;
mod reply;
mod rows;
pub mod sections;
pub mod units;
mod view;

pub use admission::unavailable_source_issues;
use evaluate::evaluate_row;
use reply::{answered, error_kind, failed, resources_answered, unread_resources_code};
use rows::parse_rows;

/// The finished result rows and their call-level facts.
pub struct Receipts {
    pub rows: Vec<Value>,
    pub failure: Option<ErrorClass>,
}

/// One parsed row: its id, the provider model it asks for, its question keys
/// and provider questions (in key order), and its state or source.
/// `long_context` is the row's explicit choice; omitted, evaluation asks for the
/// long-context window only after the provider refuses the default one.
struct Row {
    context: Option<String>,
    model: Option<String>,
    keys: Vec<String>,
    questions: Vec<Value>,
    input: Input,
    assets: Option<Value>,
    long_context: Option<bool>,
    /// `view: "details"`: the whole row; otherwise the summary ([`view`]).
    details: bool,
    /// `perItem`: each item of a `content` array is one unit ([`units`]).
    per_item: bool,
    /// `sourceProgress`: a tool source's private progress on a page after its
    /// first, which only an issued cursor carries ([`units`]).
    source_progress: Option<serde_json::Map<String, Value>>,
    /// `maxRequests`: the provider requests the row may send, its ceiling
    /// included ([`reply::OverBudget`]).
    max_requests: Option<u64>,
    /// Each search hit in its own request instead of its file's shared one
    /// ([`rows::PER_HIT`]).
    per_hit: bool,
    /// `retryUnits`: the units of the source's page (or the content items)
    /// a `next.retry` asks again; the others are neither judged nor shown.
    /// Only an issued retry carries it ([`units`]).
    retry_units: Option<Vec<String>>,
    /// The row as the caller sent it: an over-budget row's confirm call is
    /// it with `maxRequests` raised.
    query: Value,
}

enum Input {
    Judge(Target),
    /// `evaluate` with `resources`: each judged whole, with every question.
    Resources(Value),
    Models,
    Sections(Value),
}

/// What an evaluation row judges: the caller's literal state or a source.
enum Target {
    Content(Value),
    Source(Value),
}

/// Evaluate every row; a failed row keeps its own error and never hides a
/// sibling's answers. The shared runtime merges admission failures back
/// into their original positions and sanitizes the complete envelope.
pub fn call(
    evaluator: &Evaluator,
    rows: &[CheckedQuery],
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    section_runtime: sections::Runtime<'_>,
) -> Result<Receipts, ExecutionError> {
    if !evaluator.is_configured() {
        return Err(ExecutionError::WorkerFailed);
    }
    let parsed = parse_rows(rows);
    let gate = evaluator.gate();
    let evaluations = Evaluations::default();
    let section_budget = sections::Budget::new(dispatcher.limits().selection);
    let outcomes = std::thread::scope(|scope| {
        let tasks = parsed
            .iter()
            .map(|row| {
                let (gate, evaluations, section_budget) = (&gate, &evaluations, &section_budget);
                scope.spawn(octocode_io::notice::carried(move || {
                    let _enter = dispatcher.handle().enter();
                    if let Some(refused) =
                        admission::unretainable_source(row, section_runtime.can_resume)
                    {
                        return refused;
                    }
                    match &row.input {
                        Input::Sections(input) => RowOutcome::Sections(sections::run(
                            row,
                            input,
                            dispatcher,
                            context,
                            section_runtime,
                            section_budget,
                            gate,
                        )),
                        Input::Models => {
                            RowOutcome::Models(dispatcher.handle().block_on(evaluator.models(
                                &evaluator.budget(
                                    octocode_runtime_base::execution::cancel::deadline_of(context),
                                    context.cancellation.clone(),
                                ),
                                gate,
                            )))
                        }
                        Input::Resources(resources) => evaluate::resources_row(
                            evaluator,
                            evaluations,
                            row,
                            resources,
                            dispatcher,
                            context,
                            gate,
                            section_runtime.can_resume,
                        ),
                        Input::Judge(target) => evaluate_row(
                            evaluator,
                            evaluations,
                            row,
                            target,
                            dispatcher,
                            context,
                            gate,
                            section_runtime.can_resume,
                        ),
                    }
                }))
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().map_err(|_| ExecutionError::WorkerFailed))
            .collect::<Vec<_>>()
    });
    context.check()?;
    let mut outcomes = outcomes.into_iter().collect::<Result<Vec<_>, _>>()?;
    settle_shared_spend(&mut outcomes);
    let can_resume = section_runtime.can_resume;
    let mut output = Vec::with_capacity(parsed.len());
    let mut kinds = Vec::with_capacity(parsed.len());
    let mut across = across::Across::default();
    for (index, (row, outcome)) in parsed.iter().zip(outcomes).enumerate() {
        let (value, kind) = shaped(
            index,
            row,
            outcome,
            &evaluator.model,
            can_resume,
            &mut across,
        );
        output.push(value);
        kinds.push(kind);
    }
    across.attach(&mut output, &parsed, can_resume);
    for (row, data) in parsed.iter().zip(&mut output) {
        if row.details {
            view::carry_details(data);
        } else {
            view::summarize(data);
        }
    }
    Ok(receipts(output, kinds))
}

/// Row `index`'s result data and its failure kind, from its `outcome`; its
/// judgments are recorded in `across`. `model` is the one a row without
/// `model` is judged by.
fn shaped(
    index: usize,
    row: &Row,
    outcome: RowOutcome,
    model: &str,
    can_resume: bool,
    across: &mut across::Across,
) -> (Value, Option<ErrorClass>) {
    match outcome {
        RowOutcome::Judgment {
            judged,
            captured,
            assets,
            price,
            ..
        } => {
            let kind = judged.as_ref().err().map(error_kind);
            let mut value = match judged {
                Ok(judged) => {
                    let source = match &row.input {
                        Input::Judge(Target::Source(source)) => Some(source),
                        _ => None,
                    };
                    across.judged(index, row, source, None, &judged.answers);
                    answered(row, judged, captured, assets)
                }
                Err(error) => {
                    let mut value = failed(row, &error, captured.as_deref(), can_resume);
                    // A transient failure is the same row again, at its price.
                    if reply::transient(&error)
                        && let Some(query) =
                            reply::priced_query(row, price.and_then(|price| price.ceiling))
                    {
                        reply::offer_retry(&mut value, query, can_resume);
                    }
                    value
                }
            };
            reply::priced(&mut value, price);
            (value, kind)
        }
        RowOutcome::OverBudget(over) => (
            reply::over_budget(row, &over, can_resume),
            Some(octocode_runtime_base::id::error_codes::class(
                octocode_contract_types::ErrorCode::ClassificationOverBudget,
            )),
        ),
        RowOutcome::Combined(combined) => (combined.output, None),
        RowOutcome::Units(Ok(units)) => (units.finish(), None),
        RowOutcome::Units(Err(error)) => (
            failed(row, &error, None, can_resume),
            Some(error_kind(&error)),
        ),
        RowOutcome::Resources {
            resources,
            notices,
            related,
        } => {
            for resource in &resources {
                if let RowOutcome::Judgment {
                    judged: Ok(judged), ..
                } = &resource.outcome
                {
                    across.judged(
                        index,
                        row,
                        resource.source.as_ref(),
                        Some(&resource.key),
                        &judged.answers,
                    );
                }
            }
            // Each resource keeps its own error; a row none of whose
            // resources was judged fails as they did.
            let mut value = resources_answered(row, resources, notices, can_resume);
            for call in related {
                octocode_runtime_base::response::result::attach_related(&mut value, call);
            }
            let kind = octocode_runtime_base::id::error_code_of(&value)
                .map(octocode_runtime_base::id::error_codes::class);
            (value, kind)
        }
        RowOutcome::Sections(result) => match result {
            Ok(mut outcome) => {
                let unread = unread_resources_code(&outcome.output)
                    .filter(|_| outcome.output.get("error").is_none());
                if let Some(code) = unread {
                    outcome.output["errorCode"] = json!(code);
                    outcome.output["error"] = json!("No resource was read; nothing was judged.");
                }
                let failure = octocode_runtime_base::id::error_code_of(&outcome.output)
                    .map(octocode_runtime_base::id::error_codes::class);
                (outcome.output, failure)
            }
            Err(error) => (
                failed(row, &error, None, can_resume),
                Some(error_kind(&error)),
            ),
        },
        RowOutcome::Models(result) => {
            let kind = result.as_ref().err().map(error_kind);
            let value = match result {
                Ok(mut models) => {
                    mark_default(&mut models, model);
                    models
                }
                Err(error) => failed(row, &error, None, can_resume),
            };
            (value, kind)
        }
    }
}

/// The result rows of every row's data and failure kind, and the call-level
/// kind, which holds only when every row failed the same way.
fn receipts(output: Vec<Value>, kinds: Vec<Option<ErrorClass>>) -> Receipts {
    let first = kinds.first().copied().flatten();
    let failure = first.filter(|kind| kinds.iter().all(|other| *other == Some(*kind)));
    let rows = output
        .into_iter()
        .zip(kinds)
        .enumerate()
        .map(|(index, (data, failure))| {
            let mut row = json!({"index":index,"data":data});
            if failure.is_some() {
                row["status"] = json!("error");
            }
            row
        })
        .collect();
    Receipts { rows, failure }
}

/// Marks the listed model a row without `model` is judged by (the
/// configured model, else the provider's default) `default: true`; marks
/// nothing when the listing does not name it.
fn mark_default(models: &mut Value, model: &str) {
    for entry in models
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if entry["name"] == model {
            entry["default"] = json!(true);
        }
    }
}

pub enum RowOutcome {
    Judgment {
        judged: Result<Judged, ClassificationError>,
        captured: Option<Box<Captured>>,
        assets: Option<Value>,
        /// The evaluation's identity: rows with one share one request.
        identity: Option<octocode_tools_shared::judgment::shared::Identity>,
        /// What its requests were planned at before any was sent.
        price: Option<reply::Price>,
    },
    /// A row whose ceiling is over its `maxRequests`: nothing was sent.
    OverBudget(Box<reply::OverBudget>),
    /// A source judged in sections and answered once per question.
    Combined(Box<combined::Combined>),
    /// A tool's rows or a content array's items, each judged as one unit.
    Units(Result<units::UnitsOutcome, ClassificationError>),
    Models(Result<Value, ClassificationError>),
    Sections(Result<sections::Outcome, ClassificationError>),
    /// An `evaluate` row with `resources`: one judgment per resource, and
    /// what the expansion's walk withheld.
    Resources {
        resources: Vec<evaluate::ResourceOutcome>,
        notices: Vec<String>,
        /// The listings of what each glob's walk left out.
        related: Vec<Value>,
    },
}

/// Identical evaluations share one provider request, and whichever asked
/// first sent it. The first asker in input order carries the request's
/// spend (tokens and calls); every later twin, in this row or another,
/// carries none, so the rows' usage sums to what was billed and reads the
/// same on every run.
fn settle_shared_spend(outcomes: &mut [RowOutcome]) {
    // Every asker, a resource's and a unit's included: twins may sit in any row.
    let mut askers = outcomes
        .iter_mut()
        .flat_map(|outcome| match outcome {
            RowOutcome::Resources { resources, .. } => resources
                .iter_mut()
                .map(|resource| Asker::Judgment(&mut resource.outcome))
                .collect::<Vec<_>>(),
            RowOutcome::Units(Ok(units)) => {
                units.spends.iter_mut().map(Asker::Unit).collect::<Vec<_>>()
            }
            other => vec![Asker::Judgment(other)],
        })
        .collect::<Vec<_>>();
    let mut lead = std::collections::HashMap::new();
    for index in 0..askers.len() {
        let Some(identity) = askers[index].identity() else {
            continue;
        };
        let first = *lead.entry(identity).or_insert(index);
        if first == index {
            continue;
        }
        let spend = askers[index].take_spend();
        askers[first].add_spend(spend);
    }
}

/// One asker of an evaluation: a row's judgment or one unit of a units row.
enum Asker<'a> {
    Judgment(&'a mut RowOutcome),
    Unit(&'a mut units::UnitSpend),
}

impl Asker<'_> {
    fn identity(&self) -> Option<octocode_tools_shared::judgment::shared::Identity> {
        match self {
            Asker::Judgment(RowOutcome::Judgment { identity, .. }) => *identity,
            Asker::Unit(spend) => spend.identity,
            Asker::Judgment(_) => None,
        }
    }

    /// The calls this asker counts, cleared with its tokens.
    fn take_spend(&mut self) -> (u64, u64) {
        match self {
            Asker::Judgment(outcome) => take_spend(outcome),
            Asker::Unit(spend) => {
                let calls = std::mem::take(&mut spend.calls);
                (spend.input_tokens, spend.output_tokens) = (0, 0);
                (calls, 0)
            }
        }
    }

    fn add_spend(&mut self, spend: (u64, u64)) {
        match self {
            Asker::Judgment(outcome) => add_spend(outcome, spend),
            Asker::Unit(unit) => unit.calls = unit.calls.saturating_add(spend.0),
        }
    }
}

/// The calls a row's outcome counts, cleared with its tokens.
fn take_spend(outcome: &mut RowOutcome) -> (u64, u64) {
    let RowOutcome::Judgment { judged, .. } = outcome else {
        return (0, 0);
    };
    match judged {
        Ok(judged) => {
            let spend = (judged.provider_calls, judged.known_usage_calls);
            (judged.input_tokens, judged.output_tokens) = (0, 0);
            (judged.provider_calls, judged.known_usage_calls) = (0, 0);
            spend
        }
        Err(error) => {
            let spend = (error.provider_calls, 0);
            error.provider_calls = 0;
            error.reported_usage = None;
            spend
        }
    }
}

fn add_spend(outcome: &mut RowOutcome, (calls, known): (u64, u64)) {
    let RowOutcome::Judgment { judged, .. } = outcome else {
        return;
    };
    match judged {
        Ok(judged) => {
            judged.provider_calls = judged.provider_calls.saturating_add(calls);
            judged.known_usage_calls = judged.known_usage_calls.saturating_add(known);
        }
        Err(error) => error.provider_calls = error.provider_calls.saturating_add(calls),
    }
}

#[cfg(test)]
pub mod tests;
