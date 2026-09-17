//! Shared error contract for all tool-level error types.
//!
//! Every new error type should implement [`NativeError`]. The default
//! [`NativeError::to_value`] produces the standard `{ status, errorCode,
//! message }` JSON shape used throughout the runtime; override only when the
//! type needs extra fields in the envelope.

/// Common interface for all internal error types that are eventually
/// serialized to JSON at tool-output boundaries.
///
/// Implementing this trait unifies the ad-hoc inline `json!({ "status":
/// "error", ... })` conversions scattered across tool `execute()` functions
/// into a single consistent call-site: `error.to_value()`.
pub trait NativeError: std::fmt::Debug {
    /// Short machine-readable code, e.g. `"notFound"`, `"rateLimited"`.
    fn error_code(&self) -> &str;

    /// Human-readable description of what went wrong.
    fn error_message(&self) -> String;

    /// Serialize to the standard `{ status, errorCode, message }` envelope.
    /// Override when the error carries additional structured fields.
    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "error",
            "errorCode": self.error_code(),
            "message": self.error_message(),
        })
    }
}

// ── Struct-based errors (code + message fields) ───────────────────────────

impl NativeError for crate::tools::ast_graph::AstGraphError {
    fn error_code(&self) -> &str { &self.code }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::tools::ast_search::AstError {
    fn error_code(&self) -> &str { &self.code }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::tools::gh_clone_repo::CloneError {
    fn error_code(&self) -> &str { &self.code }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::tools::local_search::LocalSearchError {
    fn error_code(&self) -> &str { self.code }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::providers::artifact::ArtifactError {
    fn error_code(&self) -> &str { &self.code }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::providers::github::ProviderError {
    fn error_code(&self) -> &str { "providerError" }
    fn error_message(&self) -> String { self.message.to_string() }
    fn to_value(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "status": "error",
            "errorCode": format!("{:?}", self.kind),
            "message": self.message.as_ref(),
        });
        if let Some(status) = self.status {
            v["httpStatus"] = serde_json::json!(status);
        }
        if self.retryable {
            v["retryable"] = serde_json::json!(true);
        }
        v
    }
}

impl NativeError for crate::providers::github::GraphQlError {
    fn error_code(&self) -> &str { "graphql" }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::runtime::RuntimeError {
    fn error_code(&self) -> &str { &self.code }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::policy::PolicyError {
    fn error_code(&self) -> &str {
        match self.code {
            crate::policy::PolicyErrorCode::EmptyPath => "emptyPath",
            crate::policy::PolicyErrorCode::OutsideAllowedRoots => "outsideAllowedRoots",
            crate::policy::PolicyErrorCode::SymlinkEscape => "symlinkEscape",
            crate::policy::PolicyErrorCode::IgnoredPath => "ignoredPath",
            crate::policy::PolicyErrorCode::PermissionDenied => "permissionDenied",
            crate::policy::PolicyErrorCode::SymlinkLoop => "symlinkLoop",
            crate::policy::PolicyErrorCode::NameTooLong => "nameTooLong",
            crate::policy::PolicyErrorCode::NotRegular => "notRegular",
            crate::policy::PolicyErrorCode::NotFound => "notFound",
            crate::policy::PolicyErrorCode::InvalidInput => "invalidInput",
            crate::policy::PolicyErrorCode::InputTooLarge => "inputTooLarge",
            crate::policy::PolicyErrorCode::BinaryContent => "binaryContent",
            crate::policy::PolicyErrorCode::CommandDenied => "commandDenied",
            crate::policy::PolicyErrorCode::RegistryFrozen => "registryFrozen",
            crate::policy::PolicyErrorCode::UnsupportedRegex => "unsupportedRegex",
            crate::policy::PolicyErrorCode::Io => "io",
        }
    }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::regex::RegexError {
    fn error_code(&self) -> &str {
        match self.code {
            crate::regex::RegexErrorCode::InvalidFlags => "invalidFlags",
            crate::regex::RegexErrorCode::InvalidPattern => "invalidPattern",
            crate::regex::RegexErrorCode::RequiresIsolatedEngine => "requiresIsolatedEngine",
            crate::regex::RegexErrorCode::InputTooLarge => "inputTooLarge",
        }
    }
    fn error_message(&self) -> String { self.message.clone() }
}

impl NativeError for crate::contracts::ContractInputError {
    fn error_code(&self) -> &str { "contractInput" }
    fn error_message(&self) -> String { format!("{self}") }
}

impl NativeError for crate::contracts::ContractValidationError {
    fn error_code(&self) -> &str {
        self.issues
            .first()
            .map(|i| i.rule_id.as_str())
            .unwrap_or("validationError")
    }
    fn error_message(&self) -> String {
        self.issues
            .first()
            .map(|i| i.message.clone())
            .unwrap_or_else(|| "Validation failed".into())
    }
    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "error",
            "errorCode": "validationError",
            "issues": self.issues.iter().map(|i| serde_json::json!({
                "ruleId": i.rule_id,
                "path": i.path.join("."),
                "message": i.message,
            })).collect::<Vec<_>>(),
        })
    }
}

// ── Enum-based errors ─────────────────────────────────────────────────────

impl NativeError for crate::runtime::CursorError {
    fn error_code(&self) -> &str {
        match self {
            Self::Invalid => "invalidCursor",
            Self::Expired => "expiredCursor",
            Self::StaleContract => "staleContract",
            Self::ChangedScope => "changedScope",
            Self::ChangedSource => "changedSource",
            Self::SourceUnavailable => "sourceUnavailable",
            Self::Timeout => "timeout",
        }
    }
    fn error_message(&self) -> String {
        match self {
            Self::Invalid => "Cursor token is malformed or has been tampered with".into(),
            Self::Expired => "Cursor token has expired (24 h TTL)".into(),
            Self::StaleContract => "Tool contract changed since this cursor was issued".into(),
            Self::ChangedScope => "Configuration changed since this cursor was issued".into(),
            Self::ChangedSource => "Source file changed since this cursor was issued".into(),
            Self::SourceUnavailable => "Source file is no longer accessible".into(),
            Self::Timeout => "Timed out reading source file for cursor verification".into(),
        }
    }
}

impl NativeError for crate::runtime::ExecutionError {
    fn error_code(&self) -> &str {
        match self {
            Self::Closed => "closed",
            Self::Busy => "busy",
            Self::DuplicateRequest => "duplicateRequest",
            Self::InvalidLimits => "invalidLimits",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::WorkerFailed => "workerFailed",
        }
    }
    fn error_message(&self) -> String {
        match self {
            Self::Closed => "Runtime has been shut down".into(),
            Self::Busy => "Too many concurrent requests".into(),
            Self::DuplicateRequest => "Duplicate request ID".into(),
            Self::InvalidLimits => "Invalid request limits".into(),
            Self::Cancelled => "Request was cancelled".into(),
            Self::Timeout => "Request timed out".into(),
            Self::WorkerFailed => "Worker thread panicked".into(),
        }
    }
}

impl NativeError for crate::response::ResponseError {
    fn error_code(&self) -> &str {
        match self {
            Self::Cancelled => "cancelled",
            Self::RenderedTextTooLarge => "renderedTextTooLarge",
            Self::StructuredContentMustBeObject => "structuredContentMustBeObject",
        }
    }
    fn error_message(&self) -> String {
        match self {
            Self::Cancelled => "Response paging was cancelled".into(),
            Self::RenderedTextTooLarge => "Rendered text exceeds the maximum allowed size".into(),
            Self::StructuredContentMustBeObject => {
                "Structured content envelope must be a JSON object".into()
            }
        }
    }
}
