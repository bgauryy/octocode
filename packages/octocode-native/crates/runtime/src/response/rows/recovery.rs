use crate::id::ToolId;
use crate::response::output::SANDBOX_HINT;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

pub(crate) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::response::result::ToolDiagnostics,
) {
    if row.get("meta").is_none()
        || (diagnostics.codes.is_empty() && diagnostics.hints.is_empty() && !diagnostics.partial)
    {
        return;
    }
    for (field, values) in [("codes", diagnostics.codes), ("hints", diagnostics.hints)] {
        if values.is_empty() {
            continue;
        }
        let target = &mut row["meta"]["diagnostics"][field];
        if !target.is_array() {
            *target = json!([]);
        }
        if let Some(existing) = target.as_array_mut() {
            for value in values {
                let value = Value::String(value);
                if !existing.contains(&value) {
                    existing.push(value);
                }
            }
        }
    }
    if diagnostics.partial {
        row["meta"]["diagnostics"]["partial"] = json!(true);
    }
}

/// Budget every hint and lead `why` source fits: the runtime never
/// clips guidance, so each source is written within it
/// (`every_hint_source_fits_the_guidance_budget`).
pub(crate) const MAX_GUIDANCE_CHARS: usize = 120;

/// Row containers whose prose `hints` are agent guidance. `meta` (the
/// `diagnostics` extra) keeps its own hints and never takes the row's one tip.
const METADATA_CONTAINERS: &[&str] = &[
    "data",
    "diagnostics",
    "error",
    "files",
    "directories",
    "nodes",
    "packages",
    "items",
];

/// One guidance line: whitespace collapsed and a closing period. Never
/// clipped: a cut hint loses the recovery it names.
pub(in super::super) fn concise(value: &str) -> String {
    let mut text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() && !matches!(text.chars().last(), Some('.' | '!' | '?' | '…')) {
        text.push('.');
    }
    text
}

pub(crate) fn has_recovery(value: &Value) -> bool {
    if let Some(values) = value.as_array() {
        return values.iter().any(has_recovery);
    }
    let Some(node) = value.as_object() else {
        return false;
    };
    if node
        .get("hints")
        .and_then(Value::as_array)
        .is_some_and(|hints| {
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|text| !text.trim().is_empty()))
        })
    {
        return true;
    }
    let has_text = |hints: &Value| {
        hints.as_array().is_some_and(|hints| {
            hints
                .iter()
                .any(|hint| hint.as_str().is_some_and(|text| !text.trim().is_empty()))
        })
    };
    if node
        .get("hints")
        .and_then(|hints| hints.get(crate::id::continuation_channels::HINT_TEXT_KEY))
        .is_some_and(has_text)
    {
        return true;
    }
    if ["next", "hints"].iter().any(|key| {
        node.get(*key)
            .and_then(Value::as_object)
            .is_some_and(|calls| calls.values().any(crate::response::result::is_call))
    }) {
        return true;
    }
    for (key, child) in node {
        if METADATA_CONTAINERS.contains(&key.as_str()) && has_recovery(child) {
            return true;
        }
        if key == "repositories"
            && child
                .as_object()
                .is_some_and(|repos| repos.values().any(has_recovery))
        {
            return true;
        }
    }
    false
}

/// Hints are rendered whole only up to the response guidance limit (120
/// chars); keep every GitHub recovery hint within it.
pub(crate) const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// The one recovery for a cancelled call, on every tool and stage.
pub(crate) const CANCELLED_HINT: &str = "Retry only if the result is still needed and what cancelled it changed; otherwise report the cancellation.";

/// The one recovery table for the `errorCode` values every tool shares
/// (provider kinds, policy, locks, input). A tool answers only for the codes
/// it alone emits (`ToolOutput::error_hint`); any other code gets the tool's
/// fallback hint.
pub(crate) fn error_code_hint(tool: ToolId, code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::OutsideAllowedRoots | ErrorCode::SymlinkEscape => SANDBOX_HINT,
        ErrorCode::Authentication => GITHUB_AUTH_RECOVERY_HINT,
        ErrorCode::Permission | ErrorCode::PermissionDenied => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        ErrorCode::RateLimited | ErrorCode::ClassificationRateLimited => {
            "Wait for Retry-After or the rate-limit reset before retrying."
        }
        ErrorCode::Timeout | ErrorCode::Transport | ErrorCode::Server => {
            "Retry once; if it persists, narrow scope and verify provider availability."
        }
        ErrorCode::Cancelled => CANCELLED_HINT,
        ErrorCode::ResponseTooLarge => "Narrow the requested scope.",
        ErrorCode::RedirectDenied => "Use the canonical allowed host and repository.",
        ErrorCode::Decode => {
            "Retry once; report a provider response incompatibility if it persists."
        }
        ErrorCode::Configuration | ErrorCode::CredentialStoreUnavailable => {
            "Correct the provider authentication and configuration."
        }
        ErrorCode::Unavailable => {
            "The provider blocks this resource for legal reasons; retrying will not help."
        }
        ErrorCode::HttpStatus => {
            "Check httpStatus and the message; this status is not a network failure."
        }
        // One code, one recovery on every tool.
        ErrorCode::PathNotFound => crate::response::output::VERIFY_PATH_HINT,
        ErrorCode::PathPolicyDenied => crate::policy::discovery::WITHHELD_HINT,
        code => {
            if let Some(hint) = tool.output().error_hint(code) {
                hint
            } else if crate::id::error_codes::class(code)
                == crate::id::error_codes::ErrorClass::InvalidInput
            {
                "Correct the rejected field named in the error; broadening will not help."
            } else {
                return None;
            }
        }
    })
}

fn error_fallback_hint(tool: ToolId, query: &Value, row: &Value) -> &'static str {
    // Local tools report a sandbox refusal under its own code
    // (`outsideAllowedRoots`), so the hint is keyed by code alone.
    row.pointer("/data/errorCode")
        .and_then(Value::as_str)
        .and_then(crate::id::error_codes::parse)
        .and_then(|code| error_code_hint(tool, code))
        .unwrap_or_else(|| tool.output().fallback_hint(query))
}

/// The tool's fallback hint on an empty or failed row of `query` that has no
/// recovery of its own: the code's recovery for a failure, the tool's
/// fallback for both (an empty answer never ends bare). The continuation
/// stage calls it again after dropping a lead this surface cannot run or the
/// contract rejects.
pub(crate) fn ensure_recovery(row: &mut Value, tool: ToolId, query: &Value) {
    let status = row.get("status").and_then(Value::as_str);
    if !matches!(status, Some("error" | "empty" | "outOfRange")) || has_recovery(row) {
        return;
    }
    let hint = if status == Some("error") {
        error_fallback_hint(tool, query, row)
    } else {
        // An empty row that names its own category answers for itself: the
        // tool's hint for that category (or none), not the generic fallback.
        row.pointer("/data/category")
            .and_then(Value::as_str)
            .and_then(|category| tool.output().category_hint(category))
            .unwrap_or_else(|| tool.output().fallback_hint(query))
    };
    if hint == crate::response::output::NO_HINT {
        return;
    }
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    data.insert("hints".into(), json!([hint]));
}

fn visit(value: &mut Value, recovery: bool, seen: &mut std::collections::BTreeSet<String>) {
    if let Some(values) = value.as_array_mut() {
        for child in values {
            visit(child, recovery, seen);
        }
        return;
    }
    let Some(node) = value.as_object_mut() else {
        return;
    };
    let needs_help = matches!(
        node.get("status").and_then(Value::as_str),
        Some("error" | "empty" | "outOfRange")
    ) || recovery;
    let keys: Vec<String> = node.keys().cloned().collect();
    for key in keys {
        if METADATA_CONTAINERS.contains(&key.as_str())
            && let Some(child) = node.get_mut(&key)
        {
            visit(child, needs_help, seen);
        }
        if key == "repositories"
            && let Some(repos) = node.get_mut("repositories").and_then(Value::as_object_mut)
        {
            let names: Vec<String> = repos.keys().cloned().collect();
            for name in names {
                if let Some(repo) = repos.get_mut(&name) {
                    visit(repo, needs_help, seen);
                }
            }
        }
    }
    if node.contains_key("hints") {
        let mut hints = Vec::new();
        if needs_help && let Some(candidates) = node.get("hints").and_then(Value::as_array) {
            // An ordered list: the first hint is the recovery, later ones are
            // further findings; each keeps the one-hint budget, and a repeat
            // anywhere in the row is stated once.
            let candidates = candidates
                .iter()
                .filter_map(Value::as_str)
                .map(concise)
                .filter(|hint| !hint.is_empty());
            for short in candidates {
                // `hint_budget` sweeps authored literals; this checks each hint
                // as composed (counts, names, joined sentences) in every row a
                // test renders. Production never clips.
                #[cfg(test)]
                assert!(
                    short.chars().count() <= MAX_GUIDANCE_CHARS,
                    "hint over the {MAX_GUIDANCE_CHARS}-char guidance budget as composed: {short}"
                );
                if seen.insert(short.clone()) {
                    hints.push(short);
                }
            }
        }
        if hints.is_empty() {
            node.remove("hints");
        } else {
            node.insert("hints".into(), json!(hints));
        }
    }
}

/// One concise recovery hint on an empty or failed row (continuation calls
/// are shaped by the continuation stage).
pub(crate) fn apply_hint_policy(row: &mut Value, tool: ToolId, query: &Value) {
    ensure_recovery(row, tool, query);
    visit(row, false, &mut std::collections::BTreeSet::new());
}
