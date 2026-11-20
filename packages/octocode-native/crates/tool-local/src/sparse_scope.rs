//! A path a sparse ghCloneRepo checkout leaves out, and the coverage of a row
//! that reads such a checkout ([`octocode_tools_shared::gh_git::clone_scope`]):
//! a miss there says nothing of the repository, so the error names the scope
//! and leads to the clone that checks the path out.
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::gh_git::clone_scope::{CloneScope, clone_scope};
use std::path::Path;

/// `error`, a refusal of `requested`, as the miss of a path the sparse
/// checkout holding it leaves out: its message names the checkout's paths and
/// its leads gain the ghCloneRepo call that checks it out beside them. Any
/// other error, and a miss inside the checkout's paths, stays as it is.
pub(crate) fn outside_checkout(
    mut error: ToolError,
    requested: &str,
    paths: &PathPolicy,
) -> ToolError {
    if error.code != ErrorCode::NotFound {
        return error;
    }
    let resolved = paths.expand_and_resolve(Path::new(requested));
    let Some((scope, reclone)) = clone_scope(&resolved)
        .filter(|scope| scope.excludes(&resolved))
        .and_then(|scope| {
            let reclone = scope.reclone(&resolved)?;
            Some((scope, reclone))
        })
    else {
        return error;
    };
    error.message = scope.outside_message(requested);
    let mut calls = error
        .calls
        .take()
        .map_or_else(|| serde_json::json!({}), |calls| *calls);
    if let Some(calls) = calls.as_object_mut() {
        calls.insert("reclone".to_owned(), reclone);
    }
    error.calls = Some(Box::new(calls));
    error
}

/// The coverage warning of a row whose `root` (a path on disk) lies in a
/// sparse checkout: what of the repository is on disk there.
pub(crate) fn coverage(root: &Path) -> Option<String> {
    clone_scope(root).map(|scope: CloneScope| scope.coverage_warning())
}

#[cfg(test)]
mod tests;
