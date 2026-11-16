//! Query and endpoint validation: repository segments, sparse paths, and the git web root clones come from.
use super::{CloneError, GhCloneRepoQuery, Sparse};
use octocode_contract_types::ErrorCode;

pub(super) fn validate_query(query: &GhCloneRepoQuery) -> Result<(), CloneError> {
    for (name, value) in [
        ("owner", query.owner.as_str()),
        ("repo", query.repo.as_str()),
    ] {
        if value.trim().is_empty()
            || value.contains('/')
            || value.contains('\\')
            || value == "."
            || value == ".."
        {
            return Err(CloneError::new(
                ErrorCode::InvalidInput,
                format!("{name} must be a non-empty GitHub path segment"),
            ));
        }
    }
    if query
        .ref_
        .as_deref()
        .is_some_and(|branch| branch.trim().is_empty())
    {
        return Err(CloneError::new(
            ErrorCode::InvalidInput,
            "ref must not be empty",
        ));
    }
    let sparse = Sparse::of(query);
    for path in sparse.as_ref().map_or(&[][..], Sparse::paths) {
        if !is_sparse_path(path) {
            return Err(CloneError::new(
                ErrorCode::InvalidInput,
                "paths must contain a non-empty repo-relative path without traversal",
            ));
        }
    }
    Ok(())
}

/// A sparse checkout path that stays inside the repository: not blank, no
/// backslash, no root or drive prefix (`/x`, `C:x`), and no `.` or `..`
/// segment (an interior `a/./b` included, which `Path::components`
/// would silently normalize away). A trailing `/` names a directory.
pub(super) fn is_sparse_path(path: &str) -> bool {
    let drive = path.as_bytes().get(1) == Some(&b':')
        && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    !path.trim().is_empty()
        && !path.contains(['\\', '\r', '\n', '\0'])
        && !path.starts_with('/')
        && !drive
        && path
            .strip_suffix('/')
            .unwrap_or(path)
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."))
}
