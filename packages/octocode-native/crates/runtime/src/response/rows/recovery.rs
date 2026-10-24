use crate::id::ToolId;
use crate::response::output::SANDBOX_HINT;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

pub(crate) fn attach_diagnostics(
    row: &mut Value,
    diagnostics: crate::response::result::ToolDiagnostics,
) {
    let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
        return;
    };
    if diagnostics.partial {
        data.insert("isPartial".into(), json!(true));
        append(data, "partialReasons", diagnostics.codes);
    }
    append(data, "hints", diagnostics.hints);
}

fn append(data: &mut serde_json::Map<String, Value>, key: &str, values: Vec<String>) {
    if values.is_empty() {
        return;
    }
    let target = data.entry(key).or_insert_with(|| json!([]));
    if let Some(existing) = target.as_array_mut() {
        for value in values {
            let value = Value::String(value);
            if !existing.contains(&value) {
                existing.push(value);
            }
        }
    }
}

/// Hints are rendered whole only up to the response guidance limit (120
/// chars); keep every GitHub recovery hint within it.
pub(crate) const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// The one recovery for a cancelled call, on every tool and stage.
pub(crate) const CANCELLED_HINT: &str = "Retry only if the result is still needed and what cancelled it changed; otherwise report the cancellation.";

const TRANSIENT_HINT: &str =
    "Retry once; if it persists, narrow scope and verify provider availability.";

/// The one recovery table for the `errorCode` values every tool shares
/// (provider kinds, policy, locks, input). A tool answers only for the codes
/// it alone emits, or its own timeout (`ToolOutput::error_hint`); any other
/// code gets the tool's fallback hint.
pub(crate) fn error_code_hint(
    tool: ToolId,
    code: ErrorCode,
    query: &Value,
) -> Option<&'static str> {
    Some(match code {
        ErrorCode::OutsideAllowedRoots | ErrorCode::SymlinkEscape => SANDBOX_HINT,
        ErrorCode::Authentication => GITHUB_AUTH_RECOVERY_HINT,
        ErrorCode::Permission | ErrorCode::PermissionDenied => {
            "Verify access and token scopes; do not treat denial as absence."
        }
        ErrorCode::RateLimited | ErrorCode::ClassificationRateLimited => {
            "Wait for Retry-After or the rate-limit reset before retrying."
        }
        // A tool that knows why its own calls time out says so.
        ErrorCode::Timeout => tool
            .output()
            .error_hint(code, query)
            .unwrap_or(TRANSIENT_HINT),
        ErrorCode::Transport | ErrorCode::Server => TRANSIENT_HINT,
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
            if let Some(hint) = tool.output().error_hint(code, query) {
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
