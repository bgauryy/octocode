//! Tool errors of astSearch: path-policy refusals, I/O failures and engine
//! errors, each under its declared code.

use octocode_contract_types::ErrorCode;

use octocode_runtime_base::policy::PolicyError;
use octocode_runtime_base::response::result::ToolError;

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

/// `notFound` with the path policy's message.
fn path_not_found(path: &str) -> ToolError {
    ToolError::new(ErrorCode::NotFound, format!("Path does not exist: {path}"))
}

/// An engine error as a tool error: the engine's typed kind names its code
/// (never the message text).
pub(super) fn native_error(error: octocode_engine::error::Error) -> ToolError {
    use octocode_engine::error::ErrorKind;
    let message = error.to_string();
    let code = match error.kind() {
        ErrorKind::NotFound => {
            return ToolError::new(
                ErrorCode::NotFound,
                format!("Path does not exist ({message})"),
            );
        }
        ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        ErrorKind::InvalidPattern => ErrorCode::InvalidPattern,
        ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
        ErrorKind::InvalidInput => ErrorCode::InvalidInput,
        ErrorKind::Timeout => ErrorCode::Timeout,
        ErrorKind::Cancelled => ErrorCode::Cancelled,
        ErrorKind::SourceCorrespondence(error) => return ToolError::source(*error.clone()),
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
        declared => octocode_runtime_base::id::error_codes::parse(declared)
            .unwrap_or(ErrorCode::ExecutionFailed),
    }
}

#[cfg(test)]
mod tests;
