pub mod discovery;
pub mod gitignore;
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

    /// The path resolves outside the allowed roots (directly or through a
    /// symlink): a sandbox refusal about where the process runs, not the query.
    pub fn is_sandbox_refusal(&self) -> bool {
        matches!(
            self.code,
            PolicyErrorCode::OutsideAllowedRoots | PolicyErrorCode::SymlinkEscape
        )
    }

    /// Public `errorCode` for a local tool's path-policy failure: the dedicated
    /// sandbox code for a refusal, otherwise the tool's own access code.
    pub fn local_error_code(&self, access_code: &'static str) -> &'static str {
        if self.is_sandbox_refusal() {
            PATH_OUTSIDE_ALLOWED_ROOTS
        } else {
            access_code
        }
    }
}

/// `errorCode` every local tool (localSearch, localFetch, lspSearch,
/// astRewrite) emits when the path policy refuses a path outside the allowed
/// roots. Recovery hints key on this code, never on message text.
pub const PATH_OUTSIDE_ALLOWED_ROOTS: &str = "pathOutsideAllowedRoots";

impl Display for PolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PolicyError {}
