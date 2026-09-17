//! Shared error contract for all tool-level error types.
//!
//! Every new error type should implement [`NativeError`].  The default
//! [`NativeError::to_value`] produces the standard `{ status, errorCode,
//! message }` JSON shape used throughout the runtime; override only when the
//! type needs extra fields in the envelope.

/// Common interface for all internal error types that are eventually
/// serialized to JSON at tool-output boundaries.
///
/// Implementing this trait removes the need for ad-hoc inline conversions
/// scattered across tool `execute()` functions and replaces them with a
/// single consistent call-site: `error.to_value()`.
pub trait NativeError: std::fmt::Debug {
    /// Short machine-readable code, e.g. `"notFound"`, `"rateLimited"`.
    fn error_code(&self) -> &str;

    /// Human-readable description of what went wrong.
    fn error_message(&self) -> &str;

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

// ── Implementations for existing error types ──────────────────────────────

impl NativeError for crate::tools::ast_graph::AstGraphError {
    fn error_code(&self) -> &str {
        &self.code
    }
    fn error_message(&self) -> &str {
        &self.message
    }
}

impl NativeError for crate::providers::github::ProviderError {
    fn error_code(&self) -> &str {
        // ProviderErrorKind has no as_str(); the Debug name is the stable code.
        // A dedicated as_str() on ProviderErrorKind is tracked as future work.
        "providerError"
    }
    fn error_message(&self) -> &str {
        &self.message
    }
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

impl NativeError for crate::runtime::RuntimeError {
    fn error_code(&self) -> &str {
        &self.code
    }
    fn error_message(&self) -> &str {
        &self.message
    }
}

impl NativeError for crate::contracts::ContractValidationError {
    fn error_code(&self) -> &str {
        self.issues
            .first()
            .map(|i| i.rule_id.as_str())
            .unwrap_or("validationError")
    }
    fn error_message(&self) -> &str {
        self.issues
            .first()
            .map(|i| i.message.as_str())
            .unwrap_or("Validation failed")
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
