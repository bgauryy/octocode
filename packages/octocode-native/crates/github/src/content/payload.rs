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
    if payload.is_submodule() {
        return Err(not_a_file(match payload.submodule_git_url.as_deref() {
            Some(url) => format!(
                "Path \"{path}\" is a git submodule ({url}); read files from the submodule repository instead."
            ),
            None => format!(
                "Path \"{path}\" is a git submodule; read files from the submodule repository instead."
            ),
        })
        .with_reason(crate::ProviderErrorReason::Submodule));
    }
    match payload.kind.as_deref() {
        Some("file") => Ok(payload),
        Some("dir") => Err(is_a_directory(path)),
        Some("symlink") => Err(symlink_refused(path, payload.target.as_deref())),
        _ => Err(not_a_file(format!("GitHub path \"{path}\" is not a file"))),
    }
}
/// The refusal of a symlink read: the link names its target, and the file it
/// reaches is read by that path.
pub(super) fn symlink_refused(path: &str, target: Option<&str>) -> ProviderError {
    not_a_file(match target {
        Some(target) => {
            format!("Path \"{path}\" is a symlink to \"{target}\"; read the target path instead.")
        }
        None => format!("Path \"{path}\" is a symlink; read its target path instead."),
    })
}

/// The repository path a symlink at `link` points to: its `target` read
/// relative to the link's directory, with `.` and `..` folded. `None` for an
/// absolute target, one that climbs above the repository root, or one that
/// names the root.
pub(super) fn resolve_link_target(link: &str, target: &str) -> Option<String> {
    if target.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = link.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A directory read, typed so ghGetFileContent's failure row keys its
/// tree-listing recovery on the reason ([`crate::ProviderErrorReason::PathIsDirectory`])
/// rather than the message.
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

impl ContentPayload {
    pub(super) fn is_submodule(&self) -> bool {
        self.kind.as_deref() == Some("submodule") || self.submodule_git_url.is_some()
    }
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
    let content = content.ok_or_else(|| {
        ProviderError::new(ProviderErrorKind::Decode, "GitHub file content is missing")
    })?;
    let bytes = match encoding {
        Some("base64") => STANDARD
            .decode(content.replace(['\r', '\n'], ""))
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid base64 file content")
            })?,
        Some("utf-8") => content.into_bytes(),
        _ => {
            return Err(ProviderError::new(
                ProviderErrorKind::Decode,
                "unsupported GitHub content encoding",
            ));
        }
    };
    Ok(bytes)
}

#[cfg(test)]
mod tests;
