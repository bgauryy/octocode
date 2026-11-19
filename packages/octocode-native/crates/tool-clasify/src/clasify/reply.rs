//! A row's answer or failure as the row reports it.
use super::{Input, Row, Target, recovery};
use octocode_contract_types::ErrorCode;
use octocode_engine::num::saturating_i64;
use octocode_runtime_base::id::ToolId;
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
        requests_planned: None,
        requests_ceiling: None,
        input_tokens_at_most: None,
    })
    .unwrap_or(Value::Null)
}

/// What a row's requests were planned at before any was sent: the first
/// round, and the most its refusals, re-asks and the transient retries it
/// shares can send (`None`: unbounded, a source over one window may be
/// judged in sections).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Price {
    pub planned: u64,
    pub ceiling: Option<u64>,
    /// Of `ceiling`, the transient retries the row shares.
    pub room: u64,
}

/// A row whose ceiling is over its `maxRequests`, priced before anything
/// was sent: how many units it holds, its price, the input tokens its first
/// round costs at most, and the cap.
#[derive(Clone, Copy, Debug)]
pub struct OverBudget {
    pub units: usize,
    pub price: Price,
    pub input_tokens_at_most: u64,
    pub max: u64,
    /// The transient retries the row's ceiling counts.
    pub room: u64,
}

/// The usage fields of `price` as the contract types them; nothing for
/// `None`.
fn price_fields(price: Price, input_tokens_at_most: Option<u64>) -> Map<String, Value> {
    let count = |value| saturating_i64(value);
    let typed = octocode_contract_types::ClasifyEvaluationUsage {
        input_tokens: None,
        output_tokens: None,
        calls: None,
        refused: None,
        requests_planned: Some(count(price.planned)),
        requests_ceiling: price.ceiling.map(count),
        input_tokens_at_most: input_tokens_at_most.map(count),
    };
    match serde_json::to_value(typed) {
        Ok(Value::Object(fields)) => fields
            .into_iter()
            .filter(|(_, value)| !value.is_null())
            .collect(),
        _ => Map::new(),
    }
}

/// States `price` in the row's `usage`, when the row sent a request, and
/// says so when it sent more than its ceiling ([`past_ceiling`]).
pub(super) fn priced(row: &mut Value, price: Option<Price>) {
    let (Some(price), Some(usage)) = (price, row.get_mut("usage").and_then(Value::as_object_mut))
    else {
        return;
    };
    usage.extend(price_fields(price, None));
    let calls = usage.get("calls").and_then(Value::as_u64).unwrap_or(0);
    if let Some(notice) = price
        .ceiling
        .and_then(|ceiling| past_ceiling(calls, ceiling))
    {
        octocode_runtime_base::response::paging::push_warnings(row, [notice]);
    }
}

/// What a row without `maxRequests` that sent `calls` requests past its
/// `ceiling` says: the provider refused a request its stated window holds,
/// and the refusal's retry was sent anyway
/// ([`octocode_tools_shared::judgment::RequestCap::implicit`]).
pub(super) fn past_ceiling(calls: u64, ceiling: u64) -> Option<String> {
    let past = calls.checked_sub(ceiling).filter(|past| *past > 0)?;
    Some(format!(
        "{} past requestsCeiling: the provider refused a request its stated window holds, and the refusal's retry was sent.",
        octocode_engine::text::counted(past, "request", "requests")
    ))
}

/// A row refused before anything was sent: its ceiling is over
/// `maxRequests`. It states the price and offers the same row with
/// `maxRequests` raised to the ceiling (`next.confirm`, billed).
pub(super) fn over_budget(row: &Row, over: &OverBudget, can_resume: bool) -> Value {
    let ceiling = over.price.ceiling.unwrap_or(over.price.planned);
    let what = if over.units == 1 {
        "This source".to_owned()
    } else {
        format!("These {} units", over.units)
    };
    let retries = match over.room {
        0 => String::new(),
        1 => " and the 1 transient retry it shares".to_owned(),
        room => format!(" and the {room} transient retries it shares"),
    };
    let mut failure = json!({
        "errorCode": ErrorCode::ClassificationOverBudget,
        "error": format!(
            "{what} would send {} provider {}, at most {ceiling} with every re-ask and split a refusal can add{retries}; maxRequests is {}, so nothing was sent.",
            over.price.planned,
            if over.price.planned == 1 { "request" } else { "requests" },
            over.max
        ),
    });
    let mut usage = Map::new();
    usage.insert("calls".into(), json!(0));
    usage.extend(price_fields(over.price, Some(over.input_tokens_at_most)));
    failure["usage"] = Value::Object(usage);
    octocode_runtime_base::response::result::add_hints(
        &mut failure,
        vec!["next.confirm runs the same row with maxRequests at the ceiling (billed); or narrow the source: fewer files, hits or resources.".to_owned()],
    );
    if let Some(mut query) = row.query.as_object().cloned() {
        query.insert("maxRequests".into(), json!(ceiling));
        let query = Value::Object(query);
        let call = if can_resume {
            octocode_runtime_base::response::result::Continuation::continued(ToolId::Clasify, query)
        } else {
            octocode_runtime_base::response::result::Continuation::related(ToolId::Clasify, query)
        };
        octocode_runtime_base::response::result::attach_calls(
            &mut failure,
            json!({"confirm": call.build()}),
        );
    }
    failure
}

/// Whether a retry of the same request can get past `error`: a transient
/// failure, never a cancelled call.
pub(super) fn transient(error: &ClassificationError) -> bool {
    error.code != ErrorCode::Cancelled
        && octocode_runtime_base::id::error_codes::class(error.code) == ErrorClass::Transient
}

/// The row as the caller sent it, its `maxRequests` at `ceiling` when one is
/// stated: the call that asks it again at its price.
pub(super) fn priced_query(row: &Row, ceiling: Option<u64>) -> Option<Value> {
    let mut query = row.query.as_object()?.clone();
    if let Some(ceiling) = ceiling {
        query.insert("maxRequests".into(), json!(ceiling));
    }
    Some(Value::Object(query))
}

/// Offers `query` after a transient failure: `next.retry` in a runtime that
/// keeps saved work, else a related call. Nothing when the row already
/// offers a retry (a source read's own) or the call would not pass the
/// contract.
pub(super) fn offer_retry(data: &mut Value, query: Value, can_resume: bool) {
    if data["next"].get("retry").is_some()
        || octocode_runtime_base::contracts::validate_built_query(
            ToolId::Clasify.as_str(),
            query.clone(),
        )
        .is_err()
    {
        return;
    }
    if can_resume {
        let call = octocode_runtime_base::response::result::Continuation::continued(
            ToolId::Clasify,
            query,
        );
        octocode_runtime_base::response::result::attach_calls(data, json!({"retry": call.build()}));
    } else {
        let call =
            octocode_runtime_base::response::result::Continuation::related(ToolId::Clasify, query);
        octocode_runtime_base::response::result::attach_related(data, call.build());
    }
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
    // The resources a transient failure left unjudged, and their price.
    let mut again = Map::new();
    let mut again_ceiling = Some(0_u64);
    let mut room = 0;
    for resource in resources {
        let (judged, captured, assets, price) = match resource.outcome {
            super::RowOutcome::Judgment {
                judged,
                captured,
                assets,
                price,
                ..
            } => (judged, captured, assets, price),
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
        if let (Err(error), Some(source)) = (&judged, &resource.source)
            && transient(error)
        {
            again.insert(resource.key.clone(), source.clone());
            // The resources share the row's retries once.
            again_ceiling = again_ceiling
                .zip(price.and_then(|price| price.ceiling))
                .map(|(sum, ceiling)| {
                    let room_of = price.map_or(0, |price| price.room);
                    room = room.max(room_of);
                    sum.saturating_add(ceiling.saturating_sub(room_of))
                });
        }
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
        priced(&mut value, price);
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
    // The same row over only the resources left unjudged, at their price.
    if !again.is_empty()
        && let Some(mut query) =
            priced_query(row, again_ceiling.map(|sum| sum.saturating_add(room)))
    {
        query["resources"] = Value::Object(again);
        offer_retry(&mut out, query, can_resume);
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
        max_requests: row.max_requests,
        per_hit: row.per_hit,
        retry_units: None,
        query: row.query.clone(),
    }
}
