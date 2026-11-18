//! Pieces every GitHub tool shares: the failure a tool states, search paging,
//! a renamed repository's rescope, and the missing-path recovery of
//! ghStructure and ghGetFileContent. An empty search's broadening is
//! [`crate::leads::broaden`].
pub mod commit;
pub mod dates;
pub mod default_branch;
pub mod failed_read;
pub mod failure;
pub mod fields;
pub mod history;
pub mod paths;
pub mod query;
pub mod ranges;
pub mod rename;
pub mod search;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;
pub mod text;
pub use default_branch::{
    DefaultBranchRetry, default_branch, default_branch_retry, unresolved_refs, with_default_branch,
};
#[cfg(any(test, feature = "test-support"))]
pub use failed_read::hosted_repository;
pub use failed_read::{failure_scope, read_failure_scope, submodule_failure, with_read_recovery};
pub use failure::GhFailure;
pub use failure::{
    CASE_DIFFERS_HINT, OWNER_SPELLING_HINT, RECOVERY_LIMIT, REPOSITORY_SEARCH_HINT, owner_listing,
    owner_not_found_message, provider_hint, provider_message, ref_recovery, repository_failure,
    repository_not_found, repository_not_found_message, repository_recovery, search_failure,
    validation_message, with_ref_recovery,
};
#[cfg(any(test, feature = "test-support"))]
pub use fields::pick_fields;
pub use fields::{ensure_calls, field_names, lean_rows, pick_rows, push_warning_front};
#[cfg(test)]
pub(crate) use paths::repo_path;
pub use paths::{
    CONTENTS_LIMIT, LISTING_MAY_BE_CUT_HINT, PathRecovery, RepoPath, is_repo_relative,
    listing_path, locate_path, missing_path, parent_dir, tree_recovery,
};
pub use rename::{
    RenameCheck, Rerun, Rescope, canonical_query, canonical_repo, name_canonically,
    renamed_warning, rerun_renamed, rescope_on_rename, search_may_be_renamed, with_repo,
};
pub use search::{
    SearchCompleteness, SearchWindow, add_next, apply_partial, path_check, reject_boolean_keywords,
    with_search_repair,
};
