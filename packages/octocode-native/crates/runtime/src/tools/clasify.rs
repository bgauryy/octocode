//! Public `clasify`: typed judgments and model discovery. Evaluation rows
//! hold keyed `noul`/`choice`/`score`/`box` questions over
//! exactly one input: the caller's literal `content`, or a `source` captured
//! completely through the authorized readers ([`source_capture`]). The row
//! returns the provider's validated answers, the answering model and usage,
//! and source provenance; never a body. Explicit image assets are captured
//! internally as original bytes, with immutable local inspection copies.
//!
//! Every row evaluates through the shared [`Evaluator`]: one provider client,
//! judgment cache, single flight, retry policy and accounting for this tool
//! and retrieval `classifiers` alike.
use crate::execution::{ExecutionContext, ExecutionError};
use crate::response::result::FailureKind;
use crate::security::CheckedQuery;
use crate::tools::judgment::{ClassificationError, Evaluator, Judged, stats::ClassificationUsage};
use crate::tools::source_capture::{Captured, SourceReader};
use serde_json::{Value, json};

mod admission;
mod evaluate;
mod recovery;
mod reply;
mod rows;
pub(crate) mod sections;

pub(crate) use admission::unavailable_source_issues;
use evaluate::evaluate_row;
use reply::{answered, error_kind, failed, unread_resources_code};
use rows::parse_rows;

/// clasify's output facts: rows are keyed query results with their own shape.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn collection_paths(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "/data/resources",
            "/data/resources/*/sections",
            "/data/resources/*/sections/*/answers",
            "/data/answers",
        ])
    }
    fn first_fragment_fields(&self) -> &'static [&'static str] {
        &["/data/questions"]
    }
}

/// The finished result rows and their call-level facts.
pub(crate) struct Receipts {
    pub rows: Vec<Value>,
    pub failure: Option<FailureKind>,
}

/// The largest state one row judges (`clasify.semanticPolicy.maxStateChars`).
pub(crate) fn max_state_chars(long_context: bool) -> usize {
    if long_context {
        crate::id::clasify_policy::MAX_LONG_CONTEXT_STATE_CHARS
    } else {
        crate::id::clasify_policy::MAX_STATE_CHARS
    }
}

/// One parsed row: its id, the provider model it asks for, its question keys
/// and provider questions (in key order), and its state or source.
struct Row {
    context: Option<String>,
    model: Option<String>,
    keys: Vec<String>,
    questions: Vec<Value>,
    input: Input,
    assets: Option<Value>,
    long_context: bool,
}

enum Input {
    Judge(Target),
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
pub(crate) fn call(
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
    let budget = evaluator.budget(context.deadline, context.cancellation.clone());
    let gate = evaluator.gate();
    let section_budget = sections::Budget::new(dispatcher.limits().selection);
    let outcomes = std::thread::scope(|scope| {
        let tasks = parsed
            .iter()
            .map(|row| {
                let (budget, gate, section_budget) = (&budget, &gate, &section_budget);
                scope.spawn(move || {
                    let _enter = dispatcher.handle().enter();
                    match &row.input {
                        Input::Sections(input) => RowOutcome::Sections(sections::run(
                            row,
                            input,
                            dispatcher,
                            context,
                            section_runtime,
                            section_budget,
                        )),
                        Input::Models => RowOutcome::Models(
                            dispatcher.handle().block_on(evaluator.models(budget, gate)),
                        ),
                        Input::Judge(target) => {
                            evaluate_row(evaluator, row, target, dispatcher, context, budget, gate)
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().map_err(|_| ExecutionError::WorkerFailed))
            .collect::<Vec<_>>()
    });
    let mut usage = ClassificationUsage::default();
    for outcome in outcomes.iter().filter_map(|outcome| outcome.as_ref().ok()) {
        if let RowOutcome::Judgment { judged, .. } = outcome {
            usage.add(judged);
        } else if let RowOutcome::Sections(Ok(result)) = outcome {
            usage.merge(&result.usage);
        }
    }
    evaluator.record(usage);
    context.check()?;
    let outcomes = outcomes.into_iter().collect::<Result<Vec<_>, _>>()?;
    let mut output = Vec::with_capacity(parsed.len());
    let mut kinds = Vec::with_capacity(parsed.len());
    for (row, outcome) in parsed.iter().zip(outcomes) {
        match outcome {
            RowOutcome::Judgment {
                judged,
                captured,
                assets,
            } => {
                kinds.push(judged.as_ref().err().map(error_kind));
                output.push(match judged {
                    Ok(judged) => answered(row, judged, captured, assets),
                    Err(error) => failed(row, &error, captured.as_deref()),
                });
            }
            RowOutcome::Sections(result) => {
                let (value, kind) = match result {
                    Ok(mut outcome) => {
                        let unread = unread_resources_code(&outcome.output)
                            .filter(|_| outcome.output.get("error").is_none());
                        if let Some(code) = unread {
                            outcome.output["errorCode"] = json!(code);
                            outcome.output["error"] =
                                json!("No resource was read; nothing was judged.");
                        }
                        let failure = crate::id::error_code_of(&outcome.output)
                            .map(crate::response::result::failure_kind);
                        (outcome.output, failure)
                    }
                    Err(error) => (failed(row, &error, None), Some(error_kind(&error))),
                };
                kinds.push(kind);
                output.push(value);
            }
            RowOutcome::Models(result) => {
                kinds.push(result.as_ref().err().map(error_kind));
                output.push(match result {
                    Ok(mut models) => {
                        mark_default(&mut models, &evaluator.model);
                        models
                    }
                    Err(error) => failed(row, &error, None),
                });
            }
        }
    }
    // A call-level kind holds only when every row failed the same way.
    let first = kinds.first().copied().flatten();
    let failure = first.filter(|kind| kinds.iter().all(|other| *other == Some(*kind)));
    let results: Vec<_> = output
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
    Ok(Receipts {
        rows: results,
        failure,
    })
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

enum RowOutcome {
    Judgment {
        judged: Result<Judged, ClassificationError>,
        captured: Option<Box<Captured>>,
        assets: Option<Value>,
    },
    Models(Result<Value, ClassificationError>),
    Sections(Result<sections::Outcome, ClassificationError>),
}

#[cfg(test)]
mod tests;
