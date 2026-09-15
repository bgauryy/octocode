mod files;
mod matches;
#[cfg(test)]
mod policy_tests;
mod symbols;
mod syntax;
mod tree;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::policy::{PolicyError, PolicyErrorCode};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AstError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Value>>,
}

impl AstError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: Vec::new(),
            next: None,
        }
    }
}
impl std::fmt::Display for AstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AstError {}
impl From<String> for AstError {
    fn from(message: String) -> Self {
        Self::new("ast.execution.failed", message)
    }
}
impl From<&str> for AstError {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}

impl From<PolicyError> for AstError {
    fn from(error: PolicyError) -> Self {
        let suffix = match error.code {
            PolicyErrorCode::EmptyPath => "emptyPath",
            PolicyErrorCode::OutsideAllowedRoots => "outsideAllowedRoots",
            PolicyErrorCode::SymlinkEscape => "symlinkEscape",
            PolicyErrorCode::IgnoredPath => "ignoredPath",
            PolicyErrorCode::PermissionDenied => "permissionDenied",
            PolicyErrorCode::SymlinkLoop => "symlinkLoop",
            PolicyErrorCode::NameTooLong => "nameTooLong",
            PolicyErrorCode::NotRegular => "notRegular",
            PolicyErrorCode::NotFound => "notFound",
            PolicyErrorCode::InvalidInput => "invalidInput",
            PolicyErrorCode::InputTooLarge => "inputTooLarge",
            PolicyErrorCode::BinaryContent => "binaryContent",
            PolicyErrorCode::CommandDenied => "commandDenied",
            PolicyErrorCode::RegistryFrozen => "registryFrozen",
            PolicyErrorCode::UnsupportedRegex => "unsupportedRegex",
            PolicyErrorCode::Io => "io",
        };
        Self::new(format!("ast.policy.{suffix}"), error.message)
    }
}

pub(super) fn cancelled(error: String) -> AstError {
    AstError::new("ast.execution.cancelled", error)
}

pub(super) fn io_error(error: std::io::Error) -> AstError {
    let code = match error.kind() {
        std::io::ErrorKind::NotFound => "ast.policy.notFound",
        std::io::ErrorKind::PermissionDenied => "ast.policy.permissionDenied",
        std::io::ErrorKind::InvalidInput => "ast.policy.invalidInput",
        _ => "ast.execution.io",
    };
    AstError::new(code, error.to_string())
}

pub(super) fn native_error(error: impl ToString) -> AstError {
    let message = error.to_string();
    if let Some(rest) = message.strip_prefix('[')
        && let Some((code, _)) = rest.split_once(']')
        && (code.starts_with("structural.") || code.starts_with("ast."))
    {
        return AstError::new(code.to_owned(), message);
    }
    let lower = message.to_ascii_lowercase();
    let code = if lower.contains("no such file") || lower.contains("not found") {
        "ast.policy.notFound"
    } else if lower.contains("permission denied") {
        "ast.policy.permissionDenied"
    } else if lower.contains("invalid") && lower.contains("regex") {
        "ast.query.invalidPattern"
    } else {
        "ast.execution.failed"
    };
    AstError::new(code, message)
}

pub(super) fn allow_discovery(
    path: &std::path::Path,
    paths: &crate::policy::path::PathPolicy,
    cancel: &dyn crate::tools::local_fetch::CancellationCheck,
) -> Result<bool, String> {
    cancel
        .check()
        .map_err(|message| format!("[ast.execution.cancelled] {message}"))?;
    Ok(paths.permits_discovery(path))
}

pub(super) fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
pub type AstResult = Result<Value, AstError>;

pub fn execute_ast(
    query: Value,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::tools::local_fetch::CancellationCheck,
) -> AstResult {
    let operation = query
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| AstError::new("ast.input.invalid", "operation is required"))?;
    let decode = |error: serde_json::Error| AstError::new("ast.input.invalid", error.to_string());
    match operation {
        "files" => execute_files(
            &serde_json::from_value(query).map_err(decode)?,
            paths,
            security,
            cancellation,
        ),
        "symbols" => execute_symbols(
            &serde_json::from_value(query).map_err(decode)?,
            paths,
            security,
            cancellation,
        ),
        "match" => execute_match(
            &serde_json::from_value(query).map_err(decode)?,
            paths,
            security,
            cancellation,
        ),
        "tree" if query.get("treeKind").and_then(Value::as_str) == Some("syntax") => {
            execute_syntax(
                &serde_json::from_value(query).map_err(decode)?,
                paths,
                security,
                cancellation,
            )
        }
        "tree" => execute_tree(
            &serde_json::from_value(query).map_err(decode)?,
            paths,
            security,
            cancellation,
        ),
        "topology" => crate::tools::ast_graph::execute_topology(
            &serde_json::from_value(query).map_err(decode)?,
            paths,
            security,
            cancellation,
        )
        .map_err(|error| AstError::new(error.code, error.message)),
        _ => Err(AstError::new(
            "ast.input.invalid",
            format!("unsupported operation: {operation}"),
        )),
    }
}

pub use files::{AstFilesQuery, execute_files};
pub use matches::{AstMatchQuery, execute_match};
pub use symbols::{AstSymbolsQuery, execute_symbols};
pub use syntax::{AstSyntaxQuery, execute_syntax};
pub use tree::{AstTreeQuery, execute_tree};

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::{
        policy::path::{PathPolicy, PathPolicyConfig},
        security::{ContentSecurity, SecurityRegistry},
        tools::local_fetch::CancellationCheck,
    };

    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled by test".to_owned())
        }
    }

    struct Active;
    impl CancellationCheck for Active {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn context() -> (PathPolicy, ContentSecurity) {
        (
            PathPolicy::new(PathPolicyConfig::default()).expect("default path policy"),
            ContentSecurity::new(Arc::new(SecurityRegistry::default())),
        )
    }

    #[test]
    fn input_and_cancellation_fail_with_owned_codes() {
        let (paths, security) = context();
        let missing = execute_ast(json!({"path":"."}), &paths, &security, &Active)
            .expect_err("missing operation");
        assert_eq!(missing.code, "ast.input.invalid");

        let unknown = execute_ast(
            json!({"operation":"files","path":".","unknown":true}),
            &paths,
            &security,
            &Active,
        )
        .expect_err("unknown field");
        assert_eq!(unknown.code, "ast.input.invalid");

        let cancelled = execute_ast(
            json!({"operation":"files","path":"."}),
            &paths,
            &security,
            &Cancelled,
        )
        .expect_err("cancelled before filesystem access");
        assert_eq!(cancelled.code, "ast.execution.cancelled");
    }

    #[test]
    fn policy_and_native_failures_keep_specific_codes() {
        let policy = AstError::from(PolicyError::new(PolicyErrorCode::SymlinkEscape, "escape"));
        assert_eq!(policy.code, "ast.policy.symlinkEscape");
        assert_eq!(
            native_error("[structural.query.compileFailed] bad pattern").code,
            "structural.query.compileFailed"
        );
        assert_eq!(
            native_error("invalid regex: unclosed group").code,
            "ast.query.invalidPattern"
        );
    }
}
