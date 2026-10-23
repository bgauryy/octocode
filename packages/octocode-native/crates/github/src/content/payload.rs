//! The Contents API payload of a file read: its typed non-file failures
//! (directory, symlink, submodule) and the decoded bytes.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;

use crate::{ProviderError, ProviderErrorKind};

/// Parse a Contents API response for a single file. Directories (arrays),
/// symlinks and submodules get their own actionable validation errors instead
/// of an opaque decode failure.
pub(super) fn parse_content_payload(
    body: &[u8],
    path: &str,
) -> Result<ContentPayload, ProviderError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
    })?;
    if value.is_array() {
        return Err(is_a_directory(path));
    }
    let payload: ContentPayload = serde_json::from_value(value).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
    })?;
    match payload.kind.as_deref() {
        Some("file") => Ok(payload),
        Some("dir") => Err(is_a_directory(path)),
        Some("symlink") => Err(not_a_file(match payload.target.as_deref() {
            Some(target) => format!(
                "Path \"{path}\" is a symlink to \"{target}\"; read the target path instead."
            ),
            None => format!("Path \"{path}\" is a symlink; read its target path instead."),
        })),
        Some("submodule") => Err(not_a_file(match payload.submodule_git_url.as_deref() {
            Some(url) => format!(
                "Path \"{path}\" is a git submodule ({url}); read files from the submodule repository instead."
            ),
            None => format!(
                "Path \"{path}\" is a git submodule; read files from the submodule repository instead."
            ),
        })
        .with_reason(crate::ProviderErrorReason::Submodule)),
        _ => Err(not_a_file(format!("GitHub path \"{path}\" is not a file"))),
    }
}
/// A directory read, typed so `runtime::github::file_error` keys its
/// tree-listing recovery on the reason rather than the message.
fn is_a_directory(path: &str) -> ProviderError {
    not_a_file(format!("Path \"{path}\" is a directory, not a file"))
        .with_reason(crate::ProviderErrorReason::PathIsDirectory)
}
fn not_a_file(message: String) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}
#[derive(Deserialize)]
pub(super) struct ContentPayload {
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) encoding: Option<String>,
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) size: Option<u64>,
    #[serde(default)]
    pub(super) sha: Option<String>,
    #[serde(default)]
    pub(super) target: Option<String>,
    #[serde(default)]
    pub(super) submodule_git_url: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct DirectoryEntry {
    pub(super) name: String,
    pub(super) sha: String,
    #[serde(rename = "type")]
    pub(super) kind: String,
}
/// The bytes of a Contents API body by its `encoding` (`base64`, wrapped
/// at line breaks, or `utf-8`); any other encoding (`none` for a body over
/// 1 MB) is a decode error.
pub fn decode_content(
    encoding: Option<&str>,
    content: Option<String>,
) -> Result<Vec<u8>, ProviderError> {
    let bytes = match encoding {
        Some("base64") => STANDARD
            .decode(content.unwrap_or_default().replace(['\r', '\n'], ""))
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid base64 file content")
            })?,
        Some("utf-8") => content.unwrap_or_default().into_bytes(),
        _ => {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "unsupported GitHub content encoding",
            ));
        }
    };
    Ok(bytes)
}
