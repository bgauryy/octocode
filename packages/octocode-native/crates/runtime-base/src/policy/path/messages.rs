use std::path::Path;

use super::{EntryKind, PathPolicy, projected_canonical_root};
use crate::policy::{PolicyError, PolicyErrorCode};
use octocode_engine::text::lexical;

/// Most allowed folders of one parent a denial names one by one; more are named
/// as their parent with a count.
const SIBLINGS_LISTED: usize = 3;

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
            // The caller's own spelling, with any credential-shaped name
            // replaced as a listing shows it.
            let typed = input.to_string_lossy().into_owned();
            octocode_engine::security::sanitizer::sanitize_leaf(&typed)
                .ok()
                .flatten()
                .unwrap_or(typed)
        }
    }

    /// The denial of a path outside every root: what works instead, with the
    /// path built from the rejected one when a tail of it exists in the
    /// workspace.
    pub(super) fn outside_roots(&self, input: &Path, absolute: &Path) -> String {
        let correction = if self.workspace_root.is_none() {
            String::new()
        } else {
            let example = self.workspace_tail(input).map_or_else(
                || "structureSearch path \".\" lists the root".to_owned(),
                |tail| format!("\"{tail}\" exists there"),
            );
            format!(" Use a path relative to the workspace root, or absolute inside it: {example}.")
        };
        format!(
            "Path '{}' is outside allowed directories.{correction}{}",
            self.display_requested(input, absolute),
            self.describe_roots()
        )
    }

    /// The longest tail of `input` that names something readable in the
    /// workspace, as a relative path: a checkout at another location.
    fn workspace_tail(&self, input: &Path) -> Option<String> {
        let root = self.workspace_root.as_ref()?;
        let names = input
            .components()
            .filter_map(|part| match part {
                std::path::Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect::<Vec<_>>();
        (0..names.len()).find_map(|skip| {
            let tail = names[skip..].iter().collect::<std::path::PathBuf>();
            let real = std::fs::canonicalize(root.join(&tail)).ok()?;
            (self.allowed(&real) && !self.withholds(&real, EntryKind::stat(&real)))
                .then(|| tail.to_string_lossy().into_owned())
        })
    }

    /// What else is readable, for a denial: configured roots as `redact` shows
    /// paths (never an absolute home or workspace layout, in any build), and
    /// what Octocode fetched, which lies under its home. The workspace is
    /// named by the correction; the symlink-resolved copy `add_root` keeps of
    /// a root is the same root, and a root inside another allowed root grants
    /// nothing more: both are left out, and each shown name appears once.
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
        let mut listed: Vec<&Path> = Vec::new();
        let mut fetched = false;
        for root in &configured {
            if self.workspace_root.as_ref() == Some(*root) {
                continue;
            }
            if self.octocode_home.iter().any(|home| root.starts_with(home)) {
                fetched = true;
                continue;
            }
            let nested = configured
                .iter()
                .any(|other| other != root && root.starts_with(other));
            if !nested && !listed.contains(&root.as_path()) {
                listed.push(root);
            }
        }
        let mut shown: Vec<String> = Vec::new();
        for root in &listed {
            let siblings = root.parent().map_or(0, |parent| {
                listed
                    .iter()
                    .filter(|other| other.parent() == Some(parent))
                    .count()
            });
            let name = match root.parent() {
                // Many folders of one parent (skill folders, say) read as one entry, not one each.
                Some(parent) if siblings > SIBLINGS_LISTED => {
                    format!("{}/* ({siblings} folders)", self.redact(parent))
                }
                _ => self.redact(root),
            };
            if !shown.contains(&name) {
                shown.push(name);
            }
        }
        if fetched {
            shown.push(
                "Octocode's clones, materialized trees and URL captures, by the location.path they return"
                    .to_owned(),
            );
        }
        if shown.is_empty() {
            return String::new();
        }
        format!(" Also readable: {}.", shown.join("; "))
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
            _ if is_symlink_loop(&error) => (
                PolicyErrorCode::SymlinkLoop,
                "Symlink loop detected at path",
            ),
            _ if is_name_too_long(&error) => (PolicyErrorCode::NameTooLong, "Path name too long"),
            _ => (PolicyErrorCode::Io, "Unexpected error validating path"),
        };
        let shown = self.display_requested(input, &self.expand_and_resolve(input));
        PolicyError::new(code, format!("{prefix}: {shown}{cause}")).with_path(shown)
    }
}

/// Whether the OS refused the path for too many link hops. Errno numbers
/// differ by OS (`ELOOP` is 40 on Linux and 62 on macOS), so each OS names
/// its own.
fn is_symlink_loop(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ELOOP)
    }
    #[cfg(windows)]
    {
        // ERROR_CANT_RESOLVE_FILENAME
        error.raw_os_error() == Some(1921)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = error;
        false
    }
}

/// Whether the OS refused a path component for its length (`ENAMETOOLONG`
/// is 36 on Linux and 63 on macOS, where 36 is `EINPROGRESS`).
fn is_name_too_long(error: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        error.raw_os_error() == Some(libc::ENAMETOOLONG)
    }
    #[cfg(windows)]
    {
        // ERROR_FILENAME_EXCED_RANGE
        error.raw_os_error() == Some(206)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = error;
        false
    }
}
