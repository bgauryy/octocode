//! A row's answer or failure as the row reports it.
use super::{Input, Row, Target, recovery};
use crate::response::result::FailureKind;
use crate::tools::judgment::{ClassificationError, Judged};
use crate::tools::source_capture::Captured;
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value, json};

pub(super) fn answered(
    row: &Row,
    judged: Judged,
    captured: Option<Box<Captured>>,
    assets: Option<Value>,
    can_resume: bool,
) -> Value {
    // A yes is worth checking, and the read is the whole source: offer where
    // the answer sits, ready to run, only then.
    let locate = judged.answers.iter().any(|answer| {
        answer["type"] != "box" && answer["noul"].as_f64().is_none_or(|noul| noul >= 0.5)
    });
    let mut usage = crate::tools::judgment::stats::ClassificationUsage::default();
    usage.add_retry(&judged);
    let answers: Map<String, Value> = row.keys.iter().cloned().zip(judged.answers).collect();
    let mut out = json!({
        "model": judged.model,
        "answers": answers,
        "usage": {
            "input_tokens": judged.input_tokens,
            "output_tokens": judged.output_tokens,
            "calls": judged.provider_calls,
        },
    });
    if let Some(notice) = usage.long_context_notice() {
        crate::response::result::add_hints(&mut out, [notice]);
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
        crate::response::result::attach_calls(&mut out, json!({"read":captured.read}));
    }
    if locate
        && let Input::Judge(Target::Source(source)) = &row.input
        && let Some(call) = recovery::sections_call(row, source, can_resume)
    {
        crate::response::result::attach_related(&mut out, call);
    }
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
    crate::response::result::add_hints(&mut failure, error.hints.clone());
    // Every request sent counts, a refused one included: whether the provider
    // bills it is not documented, and its source left this machine.
    if error.provider_calls > 0 {
        let usage = error.usage();
        failure["usage"] = json!({
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "calls": error.provider_calls,
        });
    }
    crate::response::result::attach_calls(
        &mut failure,
        Value::Object(error.leads.iter().cloned().collect()),
    );
    recovery::add(row, captured, &mut failure, can_resume);
    failure
}

pub(super) fn error_kind(error: &ClassificationError) -> FailureKind {
    error
        .failure
        .unwrap_or_else(|| crate::response::result::failure_kind(error.code))
}

/// A sections row whose every resource failed to be read judged nothing,
/// so it fails as its reads did: their one code, else a failed context. A
/// row with any ready or unsupported resource is no failure.
pub(super) fn unread_resources_code(output: &Value) -> Option<ErrorCode> {
    let mut codes = output["resources"].as_object()?.values().map(|resource| {
        (resource["status"] == "error").then(|| {
            crate::id::error_code_of(&resource["error"])
                .unwrap_or(ErrorCode::ClassificationContextFailed)
        })
    });
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
