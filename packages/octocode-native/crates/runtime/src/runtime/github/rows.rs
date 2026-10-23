//! GitHub failure rows: the shared provider row, its rate-limit block and the wording each arm adds.
use crate::response::outcome::{DomainResult, Upstream};
use crate::response::result::ToolData;
use crate::tools::gh_shared::{GhFailure, provider_hint};
use octocode_contract_types::ErrorCode;
use octocode_github::{ProviderError, RateLimit};
use serde_json::{Value, json};

const ANONYMOUS_RATE_LIMIT_HINT: &str =
    "Wait for the rate-limit reset, or run octocode auth login for a higher quota.";

/// Every GitHub provider failure row: the shared provider row
/// ([`DomainResult::provider`]) with the arm's own hint (a per-call fact
/// the `errorCode` does not state), plus the debug-only request id and
/// documentation link. A row with no hint and no lead gets the code's one
/// shared recovery from the response stage
/// ([`crate::response::rows::apply_hint_policy`]).
pub(super) fn provider_row(
    error: &ProviderError,
    message: impl Into<String>,
    hints: Vec<String>,
    next: Option<Value>,
) -> DomainResult {
    let code = error.kind.code();
    let upstream = Upstream {
        retryable: error.retryable,
        http_status: error.status,
        rate_limit: error.rate_limit.as_deref().map(rate_limit_block),
    };
    let mut row = DomainResult::provider(code, message, hints, next, upstream);
    if let Some(request_id) = &error.request_id {
        row.data["requestId"] = json!(request_id);
    }
    if let Some(documentation_url) = &error.documentation_url {
        row.data["documentationUrl"] = json!(documentation_url);
    }
    if let Some(sso_url) = &error.sso_url {
        row.data["ssoUrl"] = json!(sso_url);
    }
    row
}

/// `{resource?, remaining?, resetEpochSeconds?, retryAfterSeconds?}`: the
/// members GitHub reported, in seconds; an unknown member is absent (a
/// secondary limit has no remaining count to state).
fn rate_limit_block(rate: &RateLimit) -> Value {
    let mut block = serde_json::Map::new();
    if let Some(resource) = &rate.resource {
        block.insert("resource".into(), json!(resource));
    }
    for (key, value) in [
        ("remaining", rate.remaining),
        ("resetEpochSeconds", rate.reset_epoch_seconds),
        ("retryAfterSeconds", rate.retry_after_seconds),
    ] {
        if let Some(value) = value {
            block.insert(key.into(), json!(value));
        }
    }
    Value::Object(block)
}

/// Authenticating raises only the primary quota, so only an anonymous
/// caller whose primary bucket is spent (`rateLimit.remaining == 0`) is told
/// to log in; a secondary limit or circuit cooldown keeps its wait hint.
pub(super) fn advise_login_for_anonymous_quota(data: &mut Value, anonymous: bool) {
    if anonymous
        && crate::id::error_code_of(data) == Some(ErrorCode::RateLimited)
        && data.pointer("/rateLimit/remaining") == Some(&json!(0))
    {
        data["hints"] = json!([ANONYMOUS_RATE_LIMIT_HINT]);
    }
}

/// A GitHub tool's row: its payload, or the failure it stated plus the
/// provider's status and rate-limit metadata.
pub(super) fn tool_row(result: Result<ToolData, GhFailure>) -> DomainResult {
    match result {
        Ok(output) => DomainResult {
            diagnostics: output.diagnostics,
            remaining: output.remaining,
            ..DomainResult::payload(output.data, output.status)
        },
        Err(failure) => {
            let mut row = provider_row(
                &failure.error,
                failure.message,
                failure.hints,
                failure.next.map(|next| *next),
            );
            for (field, value) in failure.fields {
                row.data[field] = value;
            }
            row
        }
    }
}

/// A provider failure no tool worded (services setup, clone admission):
/// the provider's own message on the shared row, nothing else.
pub(in crate::runtime) fn provider_error(error: ProviderError) -> DomainResult {
    provider_row(
        &error,
        error.message.to_string(),
        provider_hint(&error)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        None,
    )
}

/// A history failure row; the wording comes from the history tools
/// (`search` marks ghSearchHistory), `next` is ghGetHistoryItem's recovery
/// lead.
pub(super) fn history_error(
    error: ProviderError,
    search: bool,
    next: Option<Value>,
) -> DomainResult {
    let (message, hint) = crate::tools::gh_shared::history::history_failure(&error, search);
    let hints = hint.map(str::to_owned).into_iter().collect();
    provider_row(&error, message, hints, next)
}
