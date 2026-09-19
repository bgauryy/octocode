//! Path sandbox for jevScout (mirrors resolve-content-ref.mjs `boundedRoot`):
//! reject absolute paths and any resolved path that escapes the root via `..`.
//! Lexical only — no symlink resolution — and fails closed.

use super::err;
use crate::tools::jev_reasoning::JevProviderError;
use std::path::{Component, Path, PathBuf};

/// Lexical (no-symlink) normalization matching Node `path.resolve` semantics:
/// `.` is dropped, `..` pops the previous segment and is a no-op at the root.
pub(super) fn lexical_normalize(path: &Path) -> PathBuf {
    let mut root = PathBuf::new();
    let mut stack: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => root.push(prefix.as_os_str()),
            Component::RootDir => root.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                stack.pop();
            }
            Component::Normal(segment) => stack.push(segment.to_os_string()),
        }
    }
    let mut out = root;
    for segment in stack {
        out.push(segment);
    }
    out
}

pub(super) fn bounded_root(root_dir: &Path, candidate: &str) -> Result<PathBuf, JevProviderError> {
    if candidate.trim().is_empty() {
        return Err(err(
            "invalidJevRequest",
            "scout candidate path must be a nonempty rootDir-relative string.",
            "Provide a path inside the sandbox root.",
        ));
    }
    if Path::new(candidate).is_absolute() {
        return Err(err(
            "invalidJevRequest",
            format!(
                "scout candidate path must be rootDir-relative; received absolute {candidate:?}."
            ),
            "Use a path inside the sandbox root.",
        ));
    }
    let abs = lexical_normalize(&root_dir.join(candidate));
    if abs != *root_dir && abs.starts_with(root_dir) {
        Ok(abs)
    } else {
        Err(err(
            "invalidJevRequest",
            format!("scout candidate path escapes the sandbox root: {candidate:?}."),
            "Use a path inside the sandbox root.",
        ))
    }
}
