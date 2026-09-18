pub mod command;
pub mod discovery;
pub mod path;

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
    CommandDenied,
    RegistryFrozen,
    UnsupportedRegex,
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
}

impl Display for PolicyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PolicyError {}
