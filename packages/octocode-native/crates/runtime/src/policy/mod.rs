pub mod discovery;
pub mod gitignore;
pub mod include;
pub mod path;
pub mod prune;

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
    pub safe_path: Option<String>,
}

impl PolicyError {
    pub fn new(code: PolicyErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            safe_path: None,
        }
    }

    pub fn with_path(mut self, safe_path: impl Into<String>) -> Self {
        self.safe_path = Some(safe_path.into());
        self
    }

    /// Public `errorCode` for a local tool's path-policy failure: the shared
    /// flat code ([`Self::shared_code`]), otherwise the tool's own access code.
    pub fn local_error_code(&self, access_code: &'static str) -> &'static str {
        self.shared_code().unwrap_or(access_code)
    }

    /// The flat `errorCode` every local tool shares for this failure.
    /// `None` for a tool-specific failure (the tool's own access code).
    pub fn shared_code(&self) -> Option<&'static str> {
        Some(match self.code {
            PolicyErrorCode::OutsideAllowedRoots => "outsideAllowedRoots",
            PolicyErrorCode::SymlinkEscape => "symlinkEscape",
            PolicyErrorCode::PermissionDenied => "permissionDenied",
            PolicyErrorCode::NotFound => PATH_NOT_FOUND,
            PolicyErrorCode::InvalidInput => "invalidInput",
            PolicyErrorCode::InputTooLarge => "fileTooLarge",
            PolicyErrorCode::IgnoredPath => PATH_POLICY_DENIED,
            _ => return None,
        })
    }
}

/// `errorCode` every local tool emits for a path the security path policy
/// withholds (credential stores, `secrets/`, `.env` files): a denial no
/// flag or config setting lifts, never a missing or misspelled path.
pub const PATH_POLICY_DENIED: &str = "pathPolicyDenied";

/// `errorCode` every local tool emits for a path that does not exist.
pub const PATH_NOT_FOUND: &str = "pathNotFound";

impl Display for PolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PolicyError {}
