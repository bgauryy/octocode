use super::*;
use octocode_runtime_base::policy::PolicyErrorCode;

#[test]
fn policy_and_native_failures_keep_specific_codes() {
    // Path failures share the local tools' codes.
    let policy = policy_error(PolicyError::new(PolicyErrorCode::SymlinkEscape, "escape"));
    assert_eq!(policy.code, ErrorCode::SymlinkEscape);
    let missing = policy_error(PolicyError::new(PolicyErrorCode::NotFound, "missing"));
    assert_eq!(missing.code, ErrorCode::NotFound);
    let gone = io_error(
        "src/gone.rs",
        std::io::Error::from(std::io::ErrorKind::NotFound),
    );
    assert_eq!(gone.code, ErrorCode::NotFound);
    assert_eq!(gone.message, "Path does not exist: src/gone.rs");
    use octocode_engine::error::{Error, ErrorKind};
    let engine = native_error(Error::with_kind(
        ErrorKind::NotFound,
        "Cannot access structural search path '/w/x'",
    ));
    assert_eq!(engine.code, ErrorCode::NotFound);
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
