use octocode_engine::text::lexical;
use std::path::{Path, PathBuf};

use super::discovery::{is_sensitive_path, withheld_message};
use super::{PolicyError, PolicyErrorCode};

mod messages;
mod output;
#[cfg(test)]
mod tests;
mod walk;

pub(crate) use walk::DiscoveryWalk;

#[derive(Clone, Debug, Default)]
pub struct PathPolicyConfig {
    pub(crate) workspace_root: Option<PathBuf>,
    pub(crate) additional_roots: Vec<PathBuf>,
    pub(crate) include_home: bool,
    /// The user's home, used for `~` expansion, `~/` redaction and
    /// `include_home`; `None` turns all three off (the runtime always sets it).
    pub home_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPath {
    pub canonical: PathBuf,
    pub display: String,
}

#[derive(Clone, Debug)]
pub struct PathPolicy {
    roots: Vec<PathBuf>,
    workspace_root: Option<PathBuf>,
    /// The workspace root with symlinks resolved (e.g. macOS `/var` →
    /// `/private/var`), so canonical paths still display workspace-relative.
    workspace_real: Option<PathBuf>,
    home_dir: Option<PathBuf>,
}

impl PathPolicy {
    pub fn new(config: PathPolicyConfig) -> Result<Self, PolicyError> {
        let home = config.home_dir;
        let mut roots = Vec::new();
        for root in config
            .workspace_root
            .iter()
            .chain(config.additional_roots.iter())
            .chain(config.include_home.then_some(home.as_ref()).flatten())
        {
            add_root(&mut roots, root);
        }
        let workspace_root = config.workspace_root.map(absolutize);
        let workspace_real = workspace_root
            .as_deref()
            .and_then(projected_canonical_root)
            .filter(|real| Some(real) != workspace_root.as_ref());
        Ok(Self {
            roots,
            workspace_root,
            workspace_real,
            home_dir: home,
        })
    }

    pub(crate) fn validate_read(
        &self,
        input: impl AsRef<Path>,
    ) -> Result<ValidatedPath, PolicyError> {
        self.validate_impl(input.as_ref(), true)
    }

    /// Validate an existing path while preserving the reference validator's
    /// file-or-directory behavior. Reads use `validate_read` to require a file.
    pub fn validate(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_impl(input.as_ref(), false)
    }

    fn validate_impl(
        &self,
        input: &Path,
        require_regular: bool,
    ) -> Result<ValidatedPath, PolicyError> {
        if input.as_os_str().is_empty() || input.to_string_lossy().trim().is_empty() {
            return Err(PolicyError::new(
                PolicyErrorCode::EmptyPath,
                "Path cannot be empty",
            ));
        }
        reject_nul(input)?;
        let absolute = self.expand_and_resolve(input);
        match std::fs::canonicalize(&absolute) {
            Ok(real) => self.validate_resolved(input, &absolute, real, require_regular),
            Err(error) => {
                // Canonicalization can reveal whether an outside path exists.
                // Resolve only its nearest existing ancestor to decide the
                // policy boundary, then give the same denial for missing and
                // existing targets outside the allowed roots.
                let projected = projected_canonical_root(&absolute);
                if projected.as_ref().is_none_or(|path| !self.allowed(path)) {
                    return Err(PolicyError::new(
                        PolicyErrorCode::OutsideAllowedRoots,
                        format!(
                            "Path '{}' is outside allowed directories{}",
                            self.display_requested(input, &absolute),
                            self.describe_roots()
                        ),
                    )
                    .with_path(self.display_requested(input, &absolute)));
                }
                Err(self.io_error(error, input))
            }
        }
    }

    pub(crate) fn allowed_roots(&self) -> Vec<PathBuf> {
        self.roots.clone()
    }

    /// Digest of the allowed roots: a result stored under one policy never
    /// serves a caller with other roots.
    pub fn identity(&self) -> String {
        crate::digest::json_sha256(
            &self
                .roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
    }

    fn validate_resolved(
        &self,
        input: &Path,
        lexical: &Path,
        real: PathBuf,
        require_regular: bool,
    ) -> Result<ValidatedPath, PolicyError> {
        if !self.allowed(&real) {
            let lexical_allowed = self.allowed(lexical);
            let (code, message) = if lexical_allowed {
                (
                    PolicyErrorCode::SymlinkEscape,
                    format!(
                        "Symlink '{}' resolves to {}, outside allowed directories{}",
                        self.display_requested(input, lexical),
                        self.describe_target(&real),
                        self.describe_roots()
                    ),
                )
            } else {
                (
                    PolicyErrorCode::OutsideAllowedRoots,
                    format!(
                        "Path '{}' is outside allowed directories{}",
                        self.display_requested(input, lexical),
                        self.describe_roots()
                    ),
                )
            };
            return Err(
                PolicyError::new(code, message).with_path(self.display_requested(input, lexical))
            );
        }
        if is_sensitive_path(lexical) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                withheld_message(&self.redact(lexical), lexical),
            )
            .with_path(self.redact(lexical)));
        }
        if is_sensitive_path(&real) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                format!(
                    "Symlink target: {}",
                    withheld_message(&self.redact(&real), &real)
                ),
            )
            .with_path(self.redact(&real)));
        }
        if require_regular
            && !std::fs::metadata(&real)
                .map_err(|error| self.io_error(error, &real))?
                .is_file()
        {
            return Err(PolicyError::new(
                PolicyErrorCode::NotRegular,
                format!("Path is not a regular file: {}", self.redact(&real)),
            )
            .with_path(self.redact(&real)));
        }
        Ok(ValidatedPath {
            canonical: real.clone(),
            display: self.redact(&real),
        })
    }

    fn allowed(&self, path: &Path) -> bool {
        self.roots
            .iter()
            .any(|root| path == root || path.starts_with(root))
    }

    pub(crate) fn expand_and_resolve(&self, input: &Path) -> PathBuf {
        let expanded = input.to_string_lossy();
        let path = if expanded == "~" {
            self.home_dir.clone().unwrap_or_else(|| PathBuf::from("~"))
        } else if let Some(tail) = expanded.strip_prefix("~/") {
            self.home_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(tail)
        } else {
            input.to_path_buf()
        };
        // Relative tool paths belong to the workspace, not to whatever
        // directory the host process happened to start in (WORKSPACE_ROOT
        // exists so a CLI run from /tmp still reads the configured project).
        match &self.workspace_root {
            Some(root) if path.is_relative() => lexical(&root.join(path)),
            _ => absolutize(path),
        }
    }

    /// `path` relative to the workspace root (`.` for the root itself), or
    /// `None` when it lies outside the workspace or none is configured.
    pub(crate) fn workspace_relative(&self, path: impl AsRef<Path>) -> Option<String> {
        let normalized = lexical(path.as_ref());
        let relative = [&self.workspace_root, &self.workspace_real]
            .into_iter()
            .flatten()
            .find_map(|root| normalized.strip_prefix(root).ok())?;
        Some(if relative.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            relative.to_string_lossy().into_owned()
        })
    }

    /// The closest ancestor of `requested` the policy admits as a
    /// directory, named relative to the workspace when it lies inside it;
    /// `None` when no admitted ancestor exists (the climb never leaves the
    /// allowed roots).
    pub(crate) fn nearest_existing_dir(&self, requested: &str) -> Option<String> {
        let mut path = self.expand_and_resolve(Path::new(requested));
        while let Some(parent) = path.parent().map(Path::to_path_buf) {
            if let Ok(valid) = self.validate(&parent)
                && valid.canonical.is_dir()
            {
                return Some(
                    self.workspace_relative(&valid.canonical)
                        .unwrap_or_else(|| valid.canonical.to_string_lossy().into_owned()),
                );
            }
            path = parent;
        }
        None
    }

    pub fn redact(&self, path: impl AsRef<Path>) -> String {
        let normalized = lexical(path.as_ref());
        if let Some(relative) = self.workspace_relative(&normalized) {
            return relative;
        }
        if let Some(home) = &self.home_dir
            && let Ok(relative) = normalized.strip_prefix(home)
        {
            return if relative.as_os_str().is_empty() {
                "~".to_owned()
            } else {
                format!("~/{}", relative.to_string_lossy())
            };
        }
        normalized
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }
}

/// A NUL byte ends a path at the operating system: no file has that name,
/// so the request is malformed, not a filesystem failure.
fn reject_nul(input: &Path) -> Result<(), PolicyError> {
    if input.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(PolicyError::new(
            PolicyErrorCode::InvalidInput,
            "Path contains a NUL byte; send the path without it",
        ));
    }
    Ok(())
}

fn add_root(roots: &mut Vec<PathBuf>, root: &Path) {
    let absolute = absolutize(root);
    if !roots.contains(&absolute) {
        roots.push(absolute.clone());
    }
    if let Some(real) = projected_canonical_root(&absolute)
        && !roots.contains(&real)
    {
        roots.push(real);
    }
}

/// Resolve an allowed root even when its final components do not exist yet.
/// The missing tail is lexical; any existing symlinked ancestor is resolved.
/// Once the root is created, validation still canonicalizes the actual target
/// and rejects a symlink that redirects it outside this projected root.
fn projected_canonical_root(root: &Path) -> Option<PathBuf> {
    let mut ancestor = root;
    let mut missing = Vec::new();
    while !ancestor.exists() {
        missing.push(ancestor.file_name()?.to_os_string());
        ancestor = ancestor.parent()?;
    }
    let mut projected = std::fs::canonicalize(ancestor).ok()?;
    for name in missing.into_iter().rev() {
        projected.push(name);
    }
    Some(projected)
}

fn absolutize(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    lexical(&joined)
}
