use std::path::Path;

use super::{Access, PathPolicy, ValidatedPath, reject_nul};
use crate::policy::discovery::{EntryKind, is_sensitive_path, withheld_message};
use crate::policy::{PolicyError, PolicyErrorCode};

impl PathPolicy {
    /// A path Octocode writes, or reads back where it wrote it, which may
    /// not exist yet: under the allowed roots or an output root
    /// ([`super::PathPolicyConfig::output_roots`]).
    pub fn validate_output(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_target(input.as_ref(), Access::Output)
    }

    /// A path a tool reads under, which may no longer exist (a snapshot's
    /// scanned root): under the allowed roots only.
    pub fn validate_location(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_target(input.as_ref(), Access::Read)
    }

    fn validate_target(&self, input: &Path, access: Access) -> Result<ValidatedPath, PolicyError> {
        if input.as_os_str().is_empty() {
            return Err(PolicyError::new(
                PolicyErrorCode::EmptyPath,
                "Path cannot be empty",
            ));
        }
        reject_nul(input)?;
        let absolute = self.expand_and_resolve(input);
        if absolute.exists() {
            return self.validate_resolved(
                input,
                &absolute,
                std::fs::canonicalize(&absolute).map_err(|error| self.io_error(error, input))?,
                access,
            );
        }
        // Target does not exist yet. Walk up to the nearest existing entry,
        // recording the not-yet-created lexical tail. An entry is what the
        // directory lists: a symlink is one even when its target is missing,
        // so a dangling link ends the walk and fails to canonicalize instead
        // of joining the tail. Canonicalize that ancestor so any symlink in
        // the parent chain is resolved *now* (TOCTOU), then re-join the tail,
        // which holds no symlink. That resolved target, not its ancestor,
        // must lie inside the roots: a configured root that does not exist
        // yet (its parent outside every root) admits paths at and under it.
        let mut ancestor = absolute.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while std::fs::symlink_metadata(ancestor).is_err() {
            let name = ancestor.file_name().ok_or_else(|| {
                PolicyError::new(
                    PolicyErrorCode::NotFound,
                    format!(
                        "Path does not exist: {}",
                        self.display_requested(input, &absolute)
                    ),
                )
            })?;
            tail.push(name.to_os_string());
            ancestor = ancestor.parent().ok_or_else(|| {
                PolicyError::new(
                    PolicyErrorCode::NotFound,
                    format!(
                        "Path does not exist: {}",
                        self.display_requested(input, &absolute)
                    ),
                )
            })?;
        }
        let real_ancestor =
            std::fs::canonicalize(ancestor).map_err(|error| self.io_error(error, input))?;
        let mut resolved = real_ancestor.clone();
        for name in tail.into_iter().rev() {
            resolved.push(name);
        }
        // Guard against the resolved target re-entering a root only via the
        // symlinked parent: the canonicalized destination must itself remain
        // inside the roots.
        if !self.admits(access, &resolved) {
            return Err(PolicyError::new(
                PolicyErrorCode::OutsideAllowedRoots,
                self.outside_roots(input, &absolute),
            )
            .with_path(self.display_requested(input, &absolute)));
        }
        if let Some(sensitive) = [&absolute, &real_ancestor, &resolved]
            .into_iter()
            .find(|path| is_sensitive_path(path, EntryKind::stat(path)))
        {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                withheld_message(&self.redact(input), sensitive),
            )
            .with_path(self.redact(input)));
        }
        Ok(ValidatedPath {
            canonical: resolved.clone(),
            display: self.redact(&resolved),
        })
    }
}
