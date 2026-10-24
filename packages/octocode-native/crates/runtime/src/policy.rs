pub mod discovery;
pub mod include;
pub mod network;
pub mod path;
pub mod prune;

use octocode_contract_types::ErrorCode;
use std::fmt::{self, Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyErrorCode {
    EmptyPath,
    OutsideAllowedRoots,
    SymlinkEscape,
    IgnoredPath,
    PermissionDenied,
    SymlinkLoop,
    NameTooLong,
    NotRegular,
    NotFound,
    InvalidInput,
    InputTooLarge,
    BinaryContent,
    Io,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PolicyError {
    pub code: PolicyErrorCode,
    pub message: String,
    pub(crate) safe_path: Option<String>,
}

impl PolicyError {
    pub fn new(code: PolicyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            safe_path: None,
        }
    }

    pub(crate) fn with_path(mut self, safe_path: impl Into<String>) -> Self {
        self.safe_path = Some(safe_path.into());
        self
    }

    /// The one flat `errorCode` of this failure, the same in every local
    /// tool: the only mapping from a path-policy failure to a public code.
    pub(crate) fn shared_code(&self) -> ErrorCode {
        match self.code {
            PolicyErrorCode::OutsideAllowedRoots => ErrorCode::OutsideAllowedRoots,
            PolicyErrorCode::SymlinkEscape => ErrorCode::SymlinkEscape,
            PolicyErrorCode::PermissionDenied => ErrorCode::PermissionDenied,
            // Every local tool's code for a path that does not exist.
            PolicyErrorCode::NotFound => ErrorCode::PathNotFound,
            PolicyErrorCode::InvalidInput => ErrorCode::InvalidInput,
            PolicyErrorCode::InputTooLarge => ErrorCode::FileTooLarge,
            // Every local tool's code for a path the security path policy
            // withholds (credential stores, credential-named files such as
            // `.env` or `*.pem`): a denial no flag or config setting lifts,
            // never a missing or misspelled path.
            PolicyErrorCode::IgnoredPath => ErrorCode::PathPolicyDenied,
            // A path that names no regular file (a directory, a FIFO).
            PolicyErrorCode::NotRegular => ErrorCode::NotAFile,
            PolicyErrorCode::BinaryContent => ErrorCode::BinaryFileUnsupported,
            // A path that cannot be resolved as written: empty, looping,
            // over-long, or an unexpected filesystem error while resolving.
            PolicyErrorCode::EmptyPath
            | PolicyErrorCode::SymlinkLoop
            | PolicyErrorCode::NameTooLong
            | PolicyErrorCode::Io => ErrorCode::PathValidationFailed,
        }
    }
}

impl Display for PolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PolicyError {}

#[cfg(test)]
mod tests {
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
            (PolicyErrorCode::NotFound, ErrorCode::PathNotFound),
        ] {
            let error = PolicyError::new(code, "x");
            assert_eq!(error.shared_code(), expected, "{code:?}");
        }
    }
}
