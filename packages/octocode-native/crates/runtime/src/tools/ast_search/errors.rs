//! Tool errors of astSearch: path-policy refusals, I/O failures and engine
//! errors, each under its declared code.

use octocode_contract_types::ErrorCode;

use crate::policy::PolicyError;
use crate::response::result::ToolError;

/// A path-policy refusal under the code every local tool gives it
/// ([`PolicyError::shared_code`]).
pub(super) fn policy_error(error: PolicyError) -> ToolError {
    ToolError::new(error.shared_code(), error.message)
}
/// An I/O failure on `path` (as the caller wrote it); a missing path says
/// so in the path policy's words.
pub(super) fn io_error(path: &str, error: std::io::Error) -> ToolError {
    match error.kind() {
        std::io::ErrorKind::NotFound => path_not_found(path),
        std::io::ErrorKind::PermissionDenied => {
            ToolError::new(ErrorCode::PermissionDenied, error.to_string())
        }
        std::io::ErrorKind::InvalidInput => {
            ToolError::new(ErrorCode::InvalidInput, error.to_string())
        }
        _ => ToolError::new(ErrorCode::FileAccessFailed, error.to_string()),
    }
}

/// `pathNotFound` with the path policy's message.
fn path_not_found(path: &str) -> ToolError {
    ToolError::new(
        ErrorCode::PathNotFound,
        format!("Path does not exist: {path}"),
    )
}

/// An engine error as a tool error: the engine's typed kind names its code
/// (never the message text).
pub(super) fn native_error(error: octocode_engine::error::Error) -> ToolError {
    use octocode_engine::error::ErrorKind;
    let message = error.to_string();
    let code = match error.kind() {
        ErrorKind::NotFound => {
            return ToolError::new(
                ErrorCode::PathNotFound,
                format!("Path does not exist ({message})"),
            );
        }
        ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        ErrorKind::InvalidPattern => ErrorCode::InvalidPattern,
        ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
        ErrorKind::InvalidInput => ErrorCode::InvalidInput,
        ErrorKind::Timeout => ErrorCode::Timeout,
        ErrorKind::Cancelled => ErrorCode::Cancelled,
        _ => ErrorCode::ExecutionFailed,
    };
    ToolError::new(code, message)
}

/// The declared `errorCode` of an engine diagnostic code (`structural.…` or
/// an already-declared code): the engine's diagnostic names are its own, the
/// public set is the contract's.
pub(super) fn engine_error_code(code: &str) -> ErrorCode {
    match code {
        octocode_engine::structural::COMPILE_FAILED
        | octocode_engine::structural::QUERY_INVALID
        | octocode_engine::structural::RULE_INVALID => ErrorCode::InvalidPattern,
        "structural.language.unsupported" | "syntaxTree.language.unsupported" => {
            ErrorCode::LanguageUnsupported
        }
        "structural.content.tooLarge" | "structural.file.tooLarge" => ErrorCode::FileTooLarge,
        "syntaxTree.options.lineOutOfRange" => ErrorCode::InvalidInput,
        "structural.file.unreadable" => ErrorCode::FileReadFailed,
        octocode_engine::structural::PARSE_INTERRUPTED
        | octocode_engine::structural::MATCH_DEADLINE => ErrorCode::Timeout,
        declared => crate::id::error_codes::parse(declared).unwrap_or(ErrorCode::ExecutionFailed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyErrorCode;

    #[test]
    fn policy_and_native_failures_keep_specific_codes() {
        // Path failures share the local tools' codes.
        let policy = policy_error(PolicyError::new(PolicyErrorCode::SymlinkEscape, "escape"));
        assert_eq!(policy.code, ErrorCode::SymlinkEscape);
        let missing = policy_error(PolicyError::new(PolicyErrorCode::NotFound, "missing"));
        assert_eq!(missing.code, ErrorCode::PathNotFound);
        let gone = io_error(
            "src/gone.rs",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );
        assert_eq!(gone.code, ErrorCode::PathNotFound);
        assert_eq!(gone.message, "Path does not exist: src/gone.rs");
        use octocode_engine::error::{Error, ErrorKind};
        let engine = native_error(Error::with_kind(
            ErrorKind::NotFound,
            "Cannot access structural search path '/w/x'",
        ));
        assert_eq!(engine.code, ErrorCode::PathNotFound);
        assert_eq!(
            engine.message,
            "Path does not exist (Cannot access structural search path '/w/x')"
        );
        let denied = policy_error(PolicyError::new(PolicyErrorCode::PermissionDenied, "no"));
        assert_eq!(denied.code, ErrorCode::PermissionDenied);
        // The kind decides, never the words of the message (a bracketed
        // code in the text included).
        assert_eq!(
            native_error(Error::new("[structural.query.compileFailed] bad pattern")).code,
            ErrorCode::ExecutionFailed
        );
        assert_eq!(
            native_error(Error::with_kind(ErrorKind::InvalidPattern, "bad")).code,
            ErrorCode::InvalidPattern
        );
        assert_eq!(
            native_error(Error::with_kind(ErrorKind::PermissionDenied, "no")).code,
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            native_error(Error::with_kind(ErrorKind::InvalidGlob, "bad")).code,
            ErrorCode::InvalidGlob
        );
        assert_eq!(native_error(Error::cancelled()).code, ErrorCode::Cancelled);
        assert_eq!(
            native_error(Error::new("invalid regex: unclosed group")).code,
            ErrorCode::ExecutionFailed
        );
    }
}
