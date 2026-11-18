//! A row's answer or failure as the row reports it.
use super::{Input, Row, Target, recovery};
use octocode_contract_types::ErrorCode;
use octocode_engine::num::saturating_i64;
use octocode_runtime_base::response::result::ErrorClass;
use octocode_tools_shared::judgment::{ClassificationError, Judged};
use octocode_tools_shared::source_capture::Captured;
use serde_json::{Map, Value, json};

pub(super) fn answered(
    row: &Row,
    judged: Judged,
    captured: Option<Box<Captured>>,
    assets: Option<Value>,
) -> Value {
    let mut usage = octocode_tools_shared::judgment::stats::ClassificationUsage::default();
    usage.add_retry(&judged);
    // An answer names its type by its value key (noul, choice, score, box), as
    // a sections answer does; the request's question states it too.
    let answers: Map<String, Value> = row
        .keys
        .iter()
        .cloned()
        .zip(judged.answers.into_iter().map(|mut answer| {
            if let Some(fields) = answer.as_object_mut() {
                fields.remove("type");
            }
            answer
        }))
        .collect();
    // A twin row that carries no requests carries none of their refusals.
    let refused = judged
        .long_context_retry
        .map_or(0, |retry| retry.refused_calls.min(judged.provider_calls));
    let mut out = json!({
        "model": judged.model,
        "answers": answers,
        "usage": evaluation_usage(
            judged.input_tokens,
            judged.output_tokens,
            judged.provider_calls,
            refused,
        ),
    });
    if let Some(notice) = usage.long_context_notice() {
        octocode_runtime_base::response::paging::push_warnings(&mut out, [notice]);
    }
    if let Some(assets) = assets {
        out["assets"] = assets;
    }
    if let Some(captured) = captured {
        recovery::disclose_paged_read(&mut out, &captured);
        if let Some(source) = captured.source {
            out["source"] = source;
        }
        out["readChars"] = json!(captured.read_chars);
        octocode_runtime_base::response::result::attach_calls(
            &mut out,
            json!({"read":captured.read}),
        );
    }
    // Where a yes sits is offered across rows ([`super::across`]).
    out
}

pub(super) fn failed(
    row: &Row,
    error: &ClassificationError,
    captured: Option<&Captured>,
    can_resume: bool,
) -> Value {
    let mut failure = json!({
        "errorCode": error.code,
        "error": error.message,
    });
    octocode_runtime_base::response::result::add_hints(&mut failure, error.hints.clone());
    // Every request sent counts, a refused one included: whether the provider
    // bills it is not documented, and its source left this machine.
    if error.provider_calls > 0 {
        let usage = error.usage();
        failure["usage"] = evaluation_usage(
            usage.input_tokens,
            usage.output_tokens,
            error.provider_calls,
            0,
        );
    }
    octocode_runtime_base::response::result::attach_calls(
        &mut failure,
        Value::Object(error.leads.iter().cloned().collect()),
    );
    recovery::add(row, captured, &mut failure, can_resume);
    failure
}

/// One evaluation's provider spend as the contract types it
/// (`ClasifyEvaluationUsage`): the field names are the generated type's, never
/// spelled here, so a contract rename reaches every evaluate row.
/// `refused`: of `calls`, the requests the window refused as too large
/// before the long window or the sections answered.
pub(super) fn evaluation_usage(
    input_tokens: u64,
    output_tokens: u64,
    calls: u64,
    refused: u64,
) -> Value {
    let count = |value| saturating_i64(value);
    serde_json::to_value(octocode_contract_types::ClasifyEvaluationUsage {
        input_tokens: Some(count(input_tokens)),
        output_tokens: Some(count(output_tokens)),
        calls: Some(count(calls)),
        refused: std::num::NonZeroU64::new(refused),
    })
    .unwrap_or(Value::Null)
}

pub(super) fn error_kind(error: &ClassificationError) -> ErrorClass {
    error
        .failure
        .unwrap_or_else(|| octocode_runtime_base::id::error_codes::class(error.code))
}

/// A sections row whose every resource failed to be read judged nothing,
/// so it fails as its reads did: their one code, else a failed context. A
/// row with any ready or unsupported resource is no failure.
pub(super) fn unread_resources_code(output: &Value) -> Option<ErrorCode> {
    one_code(output["resources"].as_object()?.values().map(|resource| {
        (resource["status"] == "error").then(|| {
            octocode_runtime_base::id::error_code_of(&resource["error"])
                .unwrap_or(ErrorCode::ClassificationContextFailed)
        })
    }))
}

/// The one code of `codes` when every one is a failure (`Some`): their code
/// when they agree, else a failed context; `None` when any one is no failure
/// or there are none.
fn one_code(mut codes: impl Iterator<Item = Option<ErrorCode>>) -> Option<ErrorCode> {
    let first = codes.next()??;
    codes.try_fold(first, |code, other| {
        other.map(|other| {
            if other == code {
                code
            } else {
                ErrorCode::ClassificationContextFailed
            }
        })
    })
}

/// An `evaluate` row with `resources`: each resource's answers, usage, read
/// and its read lead, or its own error. The row states the answering model,
/// the related calls once; `notices` are the expansion's. Resource warnings
/// and failed-resource recovery stay with the resource they describe.
pub(super) fn resources_answered(
    row: &Row,
    resources: Vec<super::evaluate::ResourceOutcome>,
    notices: Vec<String>,
    can_resume: bool,
) -> Value {
    let mut out = json!({"resources":{}});
    let mut model = None;
    let mut related = Vec::new();
    for resource in resources {
        let (judged, captured, assets) = match resource.outcome {
            super::RowOutcome::Judgment {
                judged,
                captured,
                assets,
                ..
            } => (judged, captured, assets),
            // A resource over the window, answered from its sections: its
            // row is whole already.
            super::RowOutcome::Combined(combined) => {
                let mut value = combined.output;
                if let Some(fields) = value.as_object_mut() {
                    model.get_or_insert_with(|| {
                        fields
                            .get("model")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    });
                    if fields.get("model").and_then(Value::as_str) == model.as_deref() {
                        fields.remove("model");
                    }
                    if let Some(Value::Array(calls)) = fields.remove("related") {
                        related.extend(calls);
                    }
                }
                out["resources"][resource.key] = value;
                continue;
            }
            _ => continue,
        };
        let mut value = match judged {
            Ok(judged) => {
                model.get_or_insert_with(|| judged.model.clone());
                answered(row, judged, captured, assets)
            }
            // The failure a `source` row over this resource reports: its
            // spend, its leads, its read and its recovery calls.
            Err(error) => match &resource.source {
                Some(source) => failed(
                    &view(row, Target::Source(source.clone())),
                    &error,
                    captured.as_deref(),
                    can_resume,
                ),
                // A glob that did not expand has no file to read: its way
                // back, when one can succeed, is the same row over it.
                None => {
                    let mut failure = failed(
                        &view(row, Target::Content(Value::Null)),
                        &error,
                        None,
                        can_resume,
                    );
                    if recovery::rereadable(&failure)
                        && let Input::Resources(asked) = &row.input
                        && let Some(glob) = asked.get(&resource.key)
                        && let Some(call) = recovery::rerun_resource(row, &resource.key, glob)
                    {
                        octocode_runtime_base::response::result::attach_related(&mut failure, call);
                    }
                    failure
                }
            },
        };
        if let Some(fields) = value.as_object_mut() {
            // The row states the model once; a resource another model
            // answered names its own.
            if fields.get("model").and_then(Value::as_str) == model.as_deref() {
                fields.remove("model");
            }
            if let Some(Value::Array(calls)) = fields.remove("related") {
                related.extend(calls);
            }
        }
        out["resources"][resource.key] = value;
    }
    if let Some(model) = model {
        out["model"] = json!(model);
    }
    // A row none of whose resources was judged fails as they did.
    let unjudged = out["resources"].as_object().and_then(|resources| {
        one_code(resources.values().map(|resource| {
            resource.get("answers").is_none().then(|| {
                octocode_runtime_base::id::error_code_of(resource)
                    .unwrap_or(ErrorCode::ClassificationContextFailed)
            })
        }))
    });
    if let Some(code) = unjudged {
        out["errorCode"] = json!(code);
        out["error"] = json!("No resource was judged; each resource's error says why.");
    }
    octocode_runtime_base::response::paging::push_warnings(&mut out, notices);
    for call in related {
        octocode_runtime_base::response::result::attach_related(&mut out, call);
    }
    out
}

/// `row` as a row over `target`: a resource as its own `source` row.
fn view(row: &Row, target: Target) -> Row {
    Row {
        context: row.context.clone(),
        model: row.model.clone(),
        keys: row.keys.clone(),
        questions: row.questions.clone(),
        input: Input::Judge(target),
        assets: row.assets.clone(),
        long_context: row.long_context,
        details: row.details,
        per_item: row.per_item,
        // A resource is a file read: no tool walk has progress here.
        source_progress: None,
    }
}
