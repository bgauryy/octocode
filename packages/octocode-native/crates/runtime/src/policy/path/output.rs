use std::path::Path;

use super::{PathPolicy, ValidatedPath, reject_nul};
use crate::policy::discovery::{is_sensitive_path, withheld_message};
use crate::policy::{PolicyError, PolicyErrorCode};

impl PathPolicy {
    pub fn validate_output(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        let input = input.as_ref();
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
                false,
            );
        }
        // Target does not exist yet. Walk up to the nearest existing ancestor,
        // recording the not-yet-created lexical tail. Canonicalize that ancestor
        // so any symlink in the parent chain is resolved *now* (TOCTOU), verify
        // the resolved parent still lives inside the allowed roots, then re-join
        // the tail so the returned path carries no unresolved symlink component.
        let mut ancestor = absolute.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while !ancestor.exists() {
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
        if !self.allowed(&real_ancestor) {
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
        let mut resolved = real_ancestor.clone();
        for name in tail.into_iter().rev() {
            resolved.push(name);
        }
        // Guard against the resolved target re-entering an allowed root only via
        // the symlinked parent: the canonicalized destination must itself remain
        // inside the roots.
        if !self.allowed(&resolved) {
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
        if let Some(sensitive) = [&absolute, &real_ancestor, &resolved]
            .into_iter()
            .find(|path| is_sensitive_path(path))
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
