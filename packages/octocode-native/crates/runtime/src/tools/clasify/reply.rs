//! A row's answer or failure as the row reports it.
use super::{Row, recovery};
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

pub(super) fn failed(row: &Row, error: &ClassificationError, captured: Option<&Captured>) -> Value {
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
