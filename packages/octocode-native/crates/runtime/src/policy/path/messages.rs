use std::path::Path;

use super::{PathPolicy, normalize};
use crate::policy::{PolicyError, PolicyErrorCode};

impl PathPolicy {
    /// A denied path for public messages: workspace-relative when the
    /// expanded path lies in the workspace, otherwise the caller's literal
    /// input. Expansion joins relative input to the workspace root, so
    /// echoing the expanded form could add a prefix the caller never sent;
    /// canonical targets such as symlink destinations stay on `redact`.
    pub(super) fn display_requested(&self, input: &Path, absolute: &Path) -> String {
        let normalized = normalize(absolute);
        if self
            .workspace_root
            .as_ref()
            .is_some_and(|root| normalized.starts_with(root))
        {
            self.redact(&normalized)
        } else {
            input.to_string_lossy().into_owned()
        }
    }

    pub(super) fn describe_roots(&self) -> String {
        if self.roots.is_empty() {
            return String::new();
        }
        // Debug builds may surface the full filesystem layout to aid diagnosis.
        if cfg!(debug_assertions) {
            return format!(
                " (allowed: {})",
                self.roots
                    .iter()
                    .map(|root| root.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        // Release builds must not leak absolute filesystem paths in user-facing
        // errors; show only redacted/abbreviated roots.
        format!(
            " (allowed: {})",
            self.roots
                .iter()
                .map(|root| self.redact(root))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    pub(super) fn io_error(&self, error: std::io::Error, input: &Path) -> PolicyError {
        let (code, prefix) = match error.kind() {
            std::io::ErrorKind::NotFound => (PolicyErrorCode::NotFound, "Path does not exist"),
            std::io::ErrorKind::PermissionDenied => (
                PolicyErrorCode::PermissionDenied,
                "Permission denied accessing path",
            ),
            _ if error.raw_os_error() == Some(40) || error.raw_os_error() == Some(62) => (
                PolicyErrorCode::SymlinkLoop,
                "Symlink loop detected at path",
            ),
            _ if error.raw_os_error() == Some(36) => {
                (PolicyErrorCode::NameTooLong, "Path name too long")
            }
            _ => (PolicyErrorCode::Io, "Unexpected error validating path"),
        };
        let shown = self.display_requested(input, &self.expand_and_resolve(input));
        PolicyError::new(code, format!("{prefix}: {shown}")).with_path(shown)
    }
}
