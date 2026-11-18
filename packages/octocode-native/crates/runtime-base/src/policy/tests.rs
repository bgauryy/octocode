use super::*;

/// One failure, one code: no tool supplies its own fallback.
#[test]
fn every_policy_failure_has_one_shared_code_whatever_the_tool() {
    for (code, expected) in [
        (
            PolicyErrorCode::SymlinkLoop,
            ErrorCode::PathValidationFailed,
        ),
        (
            PolicyErrorCode::NameTooLong,
            ErrorCode::PathValidationFailed,
        ),
        (PolicyErrorCode::EmptyPath, ErrorCode::PathValidationFailed),
        (PolicyErrorCode::Io, ErrorCode::PathValidationFailed),
        (PolicyErrorCode::NotRegular, ErrorCode::NotAFile),
        (
            PolicyErrorCode::BinaryContent,
            ErrorCode::BinaryFileUnsupported,
        ),
        (PolicyErrorCode::NotFound, ErrorCode::NotFound),
    ] {
        let error = PolicyError::new(code, "x");
        assert_eq!(error.shared_code(), expected, "{code:?}");
    }
}
