//! Pieces every GitHub tool shares: the failure a tool states, search paging,
//! a renamed repository's rescope, and the missing-path recovery of
//! ghStructure and ghGetFileContent. An empty search's broadening is
//! [`crate::tools::leads::broaden`].
pub(crate) mod commit;
pub(crate) mod dates;
mod default_branch;
mod failed_read;
mod failure;
mod fields;
pub(crate) mod history;
mod paths;
pub(crate) mod query;
pub(crate) mod ranges;
mod rename;
mod search;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
pub(crate) mod text;
pub(crate) use default_branch::{
    DefaultBranchRetry, default_branch, default_branch_retry, repo_key, unresolved_refs,
    with_default_branch,
};
#[cfg(test)]
pub(crate) use failed_read::hosted_repository;
pub(crate) use failed_read::{failure_scope, submodule_failure};
pub use failure::GhFailure;
pub(crate) use failure::{
    REPOSITORY_ACCESS_HINT, owner_listing, owner_not_found_message, provider_hint,
    provider_message, ref_recovery, repository_not_found, repository_search, search_failure,
    validation_message, with_owner_listing, with_ref_recovery,
};
#[cfg(test)]
pub(crate) use fields::pick_fields;
pub(crate) use fields::{ensure_calls, field_names, finish_calls, pick_rows, push_warning_front};
#[cfg(test)]
pub(crate) use paths::repo_path;
pub(crate) use paths::{
    PathRecovery, RepoPath, is_repo_relative, listing_path, locate_path, missing_path, parent_dir,
    tree_recovery,
};
pub(crate) use rename::{
    RenameCheck, Rescope, canonical_query, canonical_repo, remember_canonical, renamed_warning,
    rerun_renamed, rescope_on_rename, search_may_be_renamed, with_repo,
};
pub(crate) use search::{
    SearchCompleteness, SearchWindow, add_next, apply_partial, path_check, reject_boolean_keywords,
    with_search_repair,
};
