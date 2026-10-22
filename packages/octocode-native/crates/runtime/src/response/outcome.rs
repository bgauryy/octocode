//! What execution hands to response shaping: one domain row's result
//! ([`DomainResult`]) and one finished call ([`ToolOutcome`]).
use crate::response::pager::TextContent;
use crate::response::result::{FailureKind, failure_kind};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

#[derive(Debug)]
pub struct ToolOutcome {
    pub structured_content: Value,
    pub content: Vec<TextContent>,
    pub failure: Option<FailureKind>,
    pub all_failed: bool,
}

pub(crate) struct DomainResult {
    pub diagnostics: crate::response::result::ToolDiagnostics,
    pub data: Value,
    pub cache: bool,
    pub status: Option<&'static str>,
    pub(crate) source_digest: Option<String>,
    pub failure: Option<FailureKind>,
    /// The tool's typed notes of what its pages leave to read
    /// ([`crate::response::result::ToolData::remaining`]).
    pub remaining: Vec<crate::response::pages::Remaining>,
}

impl DomainResult {
    /// A tool's own payload. Typed error codes use the shared failure class,
    /// whether a tool returns its error as a payload or as a ToolError.
    pub(crate) fn payload(data: Value, status: Option<&'static str>) -> Self {
        let failure = (status == Some("error")).then(|| {
            data.get("errorCode")
                .and_then(Value::as_str)
                .and_then(crate::id::error_codes::parse)
                .map_or(FailureKind::Execution, failure_kind)
        });
        Self {
            diagnostics: Default::default(),
            cache: false,
            data,
            status,
            source_digest: None,
            failure,
            remaining: Vec::new(),
        }
    }

    /// The one error row: `error`, `errorCode`, then `hints` (when any) and
    /// `next` (when any). Arms add context fields to `data` afterwards.
    pub(crate) fn failure(
        code: ErrorCode,
        message: impl Into<String>,
        hints: Vec<String>,
        next: Option<Value>,
    ) -> Self {
        // One exit class per code: the contract's table decides it.
        let kind = failure_kind(code);
        let mut data = json!({"error": message.into(), "errorCode": code});
        if !hints.is_empty() {
            data["hints"] = json!(hints);
        }
        if let Some(next) = next {
            data["next"] = next;
        }
        Self {
            diagnostics: Default::default(),
            cache: false,
            data,
            status: Some("error"),
            source_digest: None,
            failure: Some(kind),
            remaining: Vec::new(),
        }
    }

    /// The one provider/transport error row (GitHub, package registries, git,
    /// language servers). Every fact is stated once, in this order:
    ///
    /// ```text
    /// {error, errorCode, retryable?, httpStatus?,
    ///  rateLimit?: {resource?, remaining?, resetEpochSeconds?, retryAfterSeconds?},
    ///  hints?, next?}
    /// ```
    ///
    /// `retryable:true` appears only when a retry can help (absence = do not
    /// retry unchanged). `httpStatus` appears only
    /// when the upstream answered. Rate-limit members are seconds and absent
    /// when unknown, never null or a made-up 0. There is no `status`, `type`,
    /// `rateLimitRemaining`, `rateLimitReset` sibling and no raw provider
    /// dump. Arms add identity fields (owner/repo/path) or debug diagnostics
    /// (`requestId`, `documentationUrl`) afterwards. Other error emitters
    /// (clasify's resource errors) mirror these field names.
    pub(crate) fn provider(
        code: ErrorCode,
        message: impl Into<String>,
        hints: Vec<String>,
        next: Option<Value>,
        upstream: Upstream,
    ) -> Self {
        let mut row = Self::failure(code, message, Vec::new(), None);
        let data = &mut row.data;
        if upstream.retryable {
            data["retryable"] = json!(true);
        }
        if let Some(status) = upstream.http_status {
            data["httpStatus"] = json!(status);
        }
        if let Some(rate_limit) = upstream.rate_limit {
            data["rateLimit"] = rate_limit;
        }
        if !hints.is_empty() {
            data["hints"] = json!(hints);
        }
        if let Some(next) = next {
            data["next"] = next;
        }
        row
    }
}

impl From<crate::response::result::ToolError> for DomainResult {
    fn from(error: crate::response::result::ToolError) -> Self {
        let mut result = Self::failure(
            error.code,
            error.message,
            error.hints,
            error.next.map(|next| *next),
        );
        if let Some(selection) = error.selection {
            result.data["selection"] = serde_json::to_value(*selection).unwrap_or(Value::Null);
        }
        result
    }
}

/// What the upstream said about a failed call (see [`DomainResult::provider`]).
#[derive(Default)]
pub(crate) struct Upstream {
    pub retryable: bool,
    pub(crate) http_status: Option<u16>,
    /// `{resource?, remaining?, resetEpochSeconds?, retryAfterSeconds?}`.
    pub(crate) rate_limit: Option<Value>,
}
