//! Public `clasify`: typed judgments and model discovery. Evaluation rows
//! hold keyed `noul`/`choice`/`score`/`box` questions over
//! exactly one state: the caller's literal `state`, or a `source` captured
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
use crate::tools::judgment::{
    ClassificationError, EvaluationInput, Evaluator, Judged, stats::ClassificationUsage,
};
use crate::tools::source_capture::SourceReader;
use crate::tools::source_capture::{self, Captured};
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value, json};

mod recovery;
pub(crate) mod sections;

/// clasify's output facts: rows are keyed query results with their own shape.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Inspect the questions, the state or source, and OCTOCODE_CLASSIFICATION_API."
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn collection_paths(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "/resources",
            "/resources/*/sections",
            "/resources/*/sections/*/answers",
            "/answers",
        ])
    }
    fn first_fragment_fields(&self) -> &'static [&'static str] {
        &["questions"]
    }
    fn resource_major(&self) -> bool {
        true
    }
}

/// The finished `{"queries": rows}` envelope and its call-level facts.
pub(crate) struct Receipts {
    pub structured: Value,
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
    id: String,
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
    State(Value),
    Source(Value),
}

/// A row contract validation admitted. The provider receives the validated
/// row's own values: a typed form would reorder choice labels and map keys,
/// which are semantic input (the contract tests keep the shapes aligned).
fn parse(position: usize, row: &Value) -> Row {
    let questions = row["questions"].as_object();
    let input = if row["operation"] == "sections" {
        Input::Sections(row.clone())
    } else if row["operation"] == "models" {
        Input::Models
    } else if row.get("state").is_some() {
        Input::Judge(Target::State(row["state"].clone()))
    } else {
        Input::Judge(Target::Source(row["source"].clone()))
    };
    Row {
        id: row["id"]
            .as_str()
            .map_or_else(|| format!("q{position}"), str::to_owned),
        model: row["model"].as_str().map(str::to_owned),
        keys: questions
            .into_iter()
            .flat_map(|questions| questions.keys().cloned())
            .collect(),
        questions: questions
            .into_iter()
            .flat_map(|questions| questions.values().cloned())
            .collect(),
        input,
        assets: row.get("assets").cloned(),
        long_context: row["longContext"].as_bool() == Some(true),
    }
}

fn parse_rows(rows: &[Value]) -> Vec<Row> {
    let mut reserved = rows
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect::<std::collections::HashSet<_>>();
    rows.iter()
        .enumerate()
        .map(|(position, row)| {
            let mut parsed = parse(position, row);
            if row.get("id").is_none() {
                let mut next = position;
                while !reserved.insert(parsed.id.clone()) {
                    next += 1;
                    parsed.id = format!("q{next}");
                }
            }
            parsed
        })
        .collect()
}

/// Evaluate every row; a failed row keeps its own error and never hides a
/// sibling's answers. Admission refuses a clasify call with any invalid
/// row as a whole (its rows share batch rules), so every row runs here.
pub(crate) fn call(
    evaluator: &Evaluator,
    rows: &[Value],
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
                kinds.push(result.as_ref().err().map(error_kind));
                output.push(match result {
                    Ok(outcome) => outcome.output,
                    Err(error) => failed(row, &error, None),
                });
            }
            RowOutcome::Models(result) => {
                kinds.push(result.as_ref().err().map(error_kind));
                output.push(match result {
                    Ok(mut models) => {
                        models["id"] = json!(row.id);
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
    let mut structured = json!({ "queries": output });
    crate::response::rows::sanitize_fields(&mut structured, dispatcher.security(), context)?;
    Ok(Receipts {
        structured,
        failure,
    })
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

fn rejected(error: ClassificationError, captured: Option<Box<Captured>>) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured,
        assets: None,
    }
}

fn evaluate_row(
    evaluator: &Evaluator,
    row: &Row,
    target: &Target,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
    budget: &octocode_io::budget::RequestBudget,
    gate: &crate::providers::classification::gate::GateLease,
) -> RowOutcome {
    let max_chars = max_state_chars(row.long_context);
    let (state, captured) = match target {
        // Admission measured a literal state against the same budget.
        Target::State(state) => (state.clone(), None),
        Target::Source(source) => {
            let mut captured = match source_capture::capture(source, dispatcher, context, max_chars)
            {
                Ok(captured) => captured,
                Err(error) => return rejected(error, None),
            };
            // The admission budget in serialized Unicode scalars: captured
            // identities, line labels and JSON escapes count too; no source
            // is judged from a silently shortened state.
            if crate::tools::selection::serialized_chars(&captured.state) > max_chars {
                return rejected(
                    ClassificationError::new(
                        ErrorCode::ClassificationContextTooLarge,
                        format!(
                            "The serialized state exceeds the {max_chars}-character limit; nothing was judged."
                        ),
                        "Select narrower source ranges or split the state across query rows.",
                    ),
                    Some(Box::new(captured)),
                );
            }
            // The answer row reports the capture's read and provenance, never
            // its state.
            (
                std::mem::take(&mut captured.state),
                Some(Box::new(captured)),
            )
        }
    };
    let assets = match row
        .assets
        .as_ref()
        .map(|assets| {
            source_capture::assets::capture(assets, &state, &row.questions, dispatcher, context)
        })
        .transpose()
    {
        Ok(assets) => assets,
        Err(error) => return rejected(error, captured),
    };
    let judged = dispatcher.handle().block_on(evaluator.evaluate_prepared(
        EvaluationInput {
            state: &state,
            questions: &row.questions,
            assets: assets.as_ref().map(|assets| &assets.provider),
            long_context: row.long_context,
        },
        row.model.as_deref(),
        budget,
        gate,
    ));
    RowOutcome::Judgment {
        judged,
        captured,
        assets: assets.map(|assets| assets.provenance),
    }
}

fn answered(
    row: &Row,
    judged: Judged,
    captured: Option<Box<Captured>>,
    assets: Option<Value>,
) -> Value {
    let answers: Map<String, Value> = row.keys.iter().cloned().zip(judged.answers).collect();
    let mut out = json!({
        "id": row.id,
        "model": judged.model,
        "answers": answers,
        "usage": {
            "input_tokens": judged.input_tokens,
            "output_tokens": judged.output_tokens,
        },
    });
    if judged.replayed {
        out["replayed"] = json!(true);
    }
    if let Some(assets) = assets {
        out["assets"] = assets;
    }
    if let Some(captured) = captured {
        if let Some(source) = captured.source {
            out["source"] = source;
        }
        out["hints"] = json!({ "read": captured.read });
    }
    out
}

fn failed(row: &Row, error: &ClassificationError, captured: Option<&Captured>) -> Value {
    let mut failure = json!({
        "errorCode": error.code,
        "error": error.message,
    });
    if !error.hints.is_empty() {
        failure["hints"] = json!({ "text": error.hints });
    }
    for (name, lead) in &error.leads {
        failure["hints"][name] = lead.clone();
    }
    recovery::add(row, captured, &mut failure);
    json!({ "id": row.id, "error": failure })
}

fn error_kind(error: &ClassificationError) -> FailureKind {
    error
        .failure
        .unwrap_or_else(|| crate::response::result::failure_kind(error.code))
}

/// The batch rule the row schema cannot express: query ids stay unique so
/// every answer row is correlatable. Core authors the wording (parity
/// fixture `clasify.unique-query-ids`).
pub(crate) fn admit<'a>(
    queries: impl IntoIterator<Item = &'a Value>,
) -> Result<(), crate::contracts::ContractValidationError> {
    let mut seen = std::collections::HashSet::new();
    for (index, query) in queries.into_iter().enumerate() {
        if let Some(id) = query.get("id").and_then(Value::as_str)
            && !seen.insert(id)
        {
            return Err(crate::contracts::ContractValidationError {
                issues: vec![crate::contracts::ValidationIssue {
                    rule_id: "clasify.unique-query-ids".into(),
                    path: vec!["queries".into(), index.to_string(), "id".into()],
                    message: format!("Duplicate query id: {id}"),
                    schema: None,
                    received: None,
                }],
            });
        }
    }
    Ok(())
}

/// The reader a row's source needs: its tool, or localFetch for a URL (its
/// capture is read back through localFetch).
fn source_reader(source: &Value) -> &str {
    source
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or(crate::id::ToolId::LocalFetch.as_str())
}

/// One issue for a row whose source reader this runtime does not enable.
/// MCP rejects the same input through its availability-scoped schema.
pub(crate) fn unavailable_source_issues(
    query: &Value,
    prefix: &[String],
    availability: &crate::execution::availability::ToolAvailability,
) -> Vec<crate::contracts::ValidationIssue> {
    let mut sources = Vec::new();
    if let Some(source) = query.get("source") {
        sources.push((source, vec!["source".to_owned()]));
    }
    if let Some(resources) = query["resources"].as_object() {
        for (key, source) in resources {
            sources.push((source, vec!["resources".into(), key.clone()]));
        }
    }
    if let Some(assets) = query["assets"].as_object() {
        for (key, asset) in assets {
            sources.push((
                &asset["source"],
                vec!["assets".into(), key.clone(), "source".into()],
            ));
        }
    }
    sources.into_iter().filter_map(|(source, suffix)| {
    let reader = source_reader(source);
    if availability.is_available(reader) { return None; }
    let reason = availability
        .get(reader)
        .map_or_else(|| "no such tool.".to_owned(), |entry| entry.reason_text());
    let mut path = prefix.to_vec();
    path.extend(suffix);
    Some(crate::contracts::ValidationIssue {
        rule_id: "clasify.source-reader-available".into(),
        path,
        message: format!(
            "This source reads through {reader}, which is disabled in this runtime: {reason} Supply the data as state instead."
        ),
        schema: None,
        received: None,
    })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_ids(rows: &[Value]) -> Vec<String> {
        parse_rows(rows).into_iter().map(|row| row.id).collect()
    }

    #[test]
    fn auto_ids_do_not_collide_with_earlier_explicit_ids() {
        assert_eq!(
            parsed_ids(&[
                json!({"operation":"models", "id":"q1"}),
                json!({"operation":"models"}),
            ]),
            ["q1", "q2"]
        );
    }

    #[test]
    fn auto_ids_reserve_later_explicit_ids() {
        assert_eq!(
            parsed_ids(&[
                json!({"operation":"models"}),
                json!({"operation":"models", "id":"q0"}),
            ]),
            ["q1", "q0"]
        );
    }

    #[test]
    fn auto_ids_skip_already_generated_ids() {
        assert_eq!(
            parsed_ids(&[
                json!({"operation":"models", "id":"q1"}),
                json!({"operation":"models"}),
                json!({"operation":"models"}),
            ]),
            ["q1", "q2", "q3"]
        );
    }

    #[test]
    fn auto_ids_keep_free_positional_ids_and_explicit_names() {
        assert_eq!(
            parsed_ids(&[
                json!({"operation":"models"}),
                json!({"operation":"models", "id":"chosen"}),
                json!({"operation":"models"}),
            ]),
            ["q0", "chosen", "q2"]
        );
    }

    #[test]
    fn duplicate_query_ids_reject_with_the_core_wording() {
        let rows = [json!({"id":"a"}), json!({"id":"b"}), json!({"id":"a"})];
        let error = admit(rows.iter()).expect_err("duplicate");
        assert_eq!(error.issues[0].message, "Duplicate query id: a");
        assert_eq!(error.issues[0].path, ["queries", "2", "id"]);
        assert!(admit([json!({"id":"a"}), json!({})].iter()).is_ok());
    }

    #[test]
    fn admission_matches_the_core_clasify_fixtures() {
        let fixtures: Value =
            serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
                .expect("generated fixture JSON");
        let mut checked = 0;
        for fixture in fixtures.as_array().expect("fixture array") {
            if fixture["tool"] != "clasify" {
                continue;
            }
            let validated =
                crate::contracts::prepare_many_and_validate("clasify", fixture["input"].clone())
                    .and_then(|rows| admit(rows.iter()));
            assert_eq!(
                validated.is_ok(),
                fixture["accepted"].as_bool().expect("accepted"),
                "{}: {validated:?}",
                fixture["id"],
            );
            if let Some(expected) = fixture.get("messages") {
                let messages = validated
                    .expect_err("a worded fixture is rejected")
                    .issues
                    .into_iter()
                    .map(|issue| Value::String(issue.message))
                    .collect::<Vec<_>>();
                assert_eq!(&Value::Array(messages), expected, "{}", fixture["id"]);
            }
            checked += 1;
        }
        assert!(checked >= 10, "clasify fixtures: {checked}");
    }
}
