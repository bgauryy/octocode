use std::path::Path;

use super::{PathPolicy, projected_canonical_root};
use crate::policy::{PolicyError, PolicyErrorCode};
use octocode_engine::text::lexical;

impl PathPolicy {
    /// A denied path for public messages: workspace-relative when the
    /// expanded path lies in the workspace, otherwise the caller's literal
    /// input. Expansion joins relative input to the workspace root, so
    /// echoing the expanded form could add a prefix the caller never sent;
    /// canonical targets such as symlink destinations stay on `redact`.
    pub(super) fn display_requested(&self, input: &Path, absolute: &Path) -> String {
        let normalized = lexical(absolute);
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

    /// The allowed roots for a denial, as `redact` shows paths (never an
    /// absolute home or workspace layout, in any build). The symlink-resolved
    /// copy `add_root` keeps of a root is the same root, and a root inside
    /// another allowed root grants nothing more: both are left out, and each
    /// shown name appears once. Empty when there are no roots.
    pub(super) fn describe_roots(&self) -> String {
        let configured = self
            .roots
            .iter()
            .enumerate()
            .filter(|(index, root)| {
                !self.roots[..*index]
                    .iter()
                    .any(|earlier| projected_canonical_root(earlier).as_ref() == Some(*root))
            })
            .map(|(_, root)| root)
            .collect::<Vec<_>>();
        let mut shown: Vec<String> = Vec::new();
        for root in &configured {
            let nested = configured
                .iter()
                .any(|other| other != root && root.starts_with(other));
            let name = self.redact(root);
            if !nested && !shown.contains(&name) {
                shown.push(name);
            }
        }
        if shown.is_empty() {
            return String::new();
        }
        format!(" (allowed: {})", shown.join(", "))
    }

    /// A symlink's resolved target for a denial: as `redact` shows it, and
    /// marked when its directories are withheld (a target outside the
    /// workspace and home keeps only its name), so the name never reads as
    /// a path beside the link.
    pub(super) fn describe_target(&self, real: &Path) -> String {
        let shown = self.redact(real);
        let withheld = self.workspace_relative(real).is_none()
            && self
                .home_dir
                .as_ref()
                .is_none_or(|home| !lexical(real).starts_with(home));
        if withheld {
            format!("'…/{shown}' (its outside directories withheld)")
        } else {
            format!("'{shown}'")
        }
    }

    pub(super) fn io_error(&self, error: std::io::Error, input: &Path) -> PolicyError {
        let mut cause = "";
        let (code, prefix) = match error.kind() {
            std::io::ErrorKind::NotFound => (PolicyErrorCode::NotFound, "Path does not exist"),
            // A file used as a directory (`package.json/child`): the path
            // cannot exist, so it is missing, and the message says why.
            std::io::ErrorKind::NotADirectory => {
                cause = " (a parent of it is a file, not a directory)";
                (PolicyErrorCode::NotFound, "Path does not exist")
            }
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
        PolicyError::new(code, format!("{prefix}: {shown}{cause}")).with_path(shown)
    }
}
