//! Diagnostic items: anchor diagnostics, disabled-macro notes, publish
//! ranges and de-duplication.
use super::*;

/// The published diagnostics the server holds for `path`, waiting at most
/// [`ANCHOR_DIAGNOSTICS_WAIT_MS`]. `None` when none arrive in time or the
/// request fails; only cancellation is an error.
pub(super) async fn anchor_diagnostics(
    client: &NativeLspClient,
    path: &str,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Value>, LspFailure> {
    let report = if client.has_capability("diagnosticProvider".to_owned()) {
        let wait = std::time::Duration::from_millis(ANCHOR_DIAGNOSTICS_WAIT_MS.into());
        cancellable(
            cancel,
            tokio::time::timeout(wait, client.get_diagnostics(path.to_owned())),
        )
        .await?
        .ok()
        .and_then(Result::ok)
    } else {
        cancellable(
            cancel,
            client.get_push_diagnostics(path.to_owned(), Some(ANCHOR_DIAGNOSTICS_WAIT_MS)),
        )
        .await?
        .ok()
        .flatten()
    };
    Ok(report.map(|report| diagnostic_items(Some(report)).0))
}

/// rust-analyzer codes for macros it did not expand. The engine starts it
/// headless (no build scripts or proc-macros), so these are expected noise.
pub(super) const DISABLED_MACRO_CODES: &[&str] = &[
    "macro-error",
    "unresolved-proc-macro",
    "proc-macro-disabled",
];

pub(super) const RUST_MACRO_NOTE: &str = "Proc-macros and build scripts are off by default (they run repository code), so rust-analyzer reports unexpanded macros; pass rustContext {buildScripts:true, procMacros:true} to expand them.";

pub(in super::super) fn disabled_macro_diagnostics(items: &Value) -> bool {
    super::super::render::items(items).iter().any(|item| {
        item.get("code")
            .and_then(Value::as_str)
            .is_some_and(|code| DISABLED_MACRO_CODES.contains(&code))
    })
}

/// Replace the zero-based LSP `range` on a diagnostic (and on each related
/// location) with the one-based public `displayRange` every other result uses.
pub(in super::super) fn publish_diagnostic_ranges(mut item: Value) -> Value {
    fn swap(object: &mut Value) {
        if let Some(display) = object.get("range").and_then(public_range)
            && let Some(map) = object.as_object_mut()
        {
            map.remove("range");
            map.insert("displayRange".into(), display);
        }
    }
    swap(&mut item);
    // LSP `DiagnosticSeverity` 1–4, by name like every other Octocode
    // diagnostic; an unknown number passes through unchanged.
    let label = match item.get("severity").and_then(Value::as_u64) {
        Some(1) => Some("error"),
        Some(2) => Some("warning"),
        Some(3) => Some("information"),
        Some(4) => Some("hint"),
        _ => None,
    };
    if let Some(label) = label {
        item["severity"] = json!(label);
    }
    if let Some(related) = item
        .get_mut("relatedInformation")
        .and_then(Value::as_array_mut)
    {
        for info in related {
            if let Some(location) = info.get_mut("location") {
                swap(location);
            }
        }
    }
    item
}

/// The diagnostics with each identical one (every field but its range
/// equal) listed once, where it first appears: one seen at several ranges
/// carries `count` and every range, in report order, as `displayRanges`
/// instead of its `displayRange`.
pub(in super::super) fn dedupe_diagnostics(items: Value) -> Value {
    let mut listed: Vec<(Value, Vec<Value>)> = Vec::new();
    for mut item in into_items(items) {
        let range = item
            .as_object_mut()
            .and_then(|object| object.remove("displayRange"))
            .unwrap_or(Value::Null);
        match listed.iter_mut().find(|(known, _)| *known == item) {
            Some((_, ranges)) => ranges.push(range),
            None => listed.push((item, vec![range])),
        }
    }
    Value::Array(
        listed
            .into_iter()
            .map(|(mut item, mut ranges)| {
                if let Some(object) = item.as_object_mut() {
                    match ranges.len() {
                        1 => {
                            let range = ranges.remove(0);
                            if !range.is_null() {
                                object.insert("displayRange".into(), range);
                            }
                        }
                        count => {
                            object.insert("count".into(), json!(count));
                            object.insert("displayRanges".into(), Value::Array(ranges));
                        }
                    }
                }
                item
            })
            .collect(),
    )
}

/// Flatten a pull (`DocumentDiagnosticReport`) or cached push report into the
/// list of diagnostics it carries, plus whether the push cache truncated it.
/// `unchanged` pull reports and absent reports carry no items.
pub(in super::super) fn diagnostic_items(report: Option<Value>) -> (Value, bool) {
    let Some(report) = report else {
        return (json!([]), false);
    };
    let truncated = report.get("truncated").and_then(Value::as_bool) == Some(true);
    let items = match report {
        Value::Array(items) => items,
        Value::Object(mut object) => match object.remove("items") {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    (
        Value::Array(items.into_iter().map(publish_diagnostic_ranges).collect()),
        truncated,
    )
}
