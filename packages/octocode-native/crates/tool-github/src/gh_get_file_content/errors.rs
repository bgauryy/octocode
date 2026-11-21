//! A failed ghGetFileContent read: one message, one hint and one recovery
//! per row, chosen from the provider error and the missing-path walk.
use super::access;
use octocode_contract_types::GhGetFileContentQuery;
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::{Continuation, query_row};
use octocode_tools_shared::gh_shared::{
    GhFailure, PathRecovery, REPOSITORY_SEARCH_HINT, RepoPath, parent_dir, provider_hint,
    provider_message, repository_not_found, repository_not_found_message, tree_recovery,
    validation_message, with_ref_recovery,
};
use serde_json::{Value, json};

pub(super) fn repo_path(query: &GhGetFileContentQuery) -> RepoPath<'_> {
    RepoPath {
        owner: access::owner(query),
        repo: access::repo(query),
        path: access::path(query),
        reference: access::reference(query),
    }
}

/// What the missing-path walk found for a read that answered not found.
pub struct Located {
    pub(crate) found: PathRecovery,
    /// The read was sent under another name than `query`'s: the old name
    /// of a renamed repository, so a read under `query`'s is another call.
    pub(crate) renamed: bool,
}

/// The failure row of a read. `commit` is the commit the read's ref
/// resolved to, which the recovery leads pin in place of the ref; `found` is
/// the missing-path walk's answer (only for a path that did not resolve).
pub fn failure(
    error: ProviderError,
    query: &GhGetFileContentQuery,
    commit: Option<&str>,
    found: Option<Located>,
) -> GhFailure {
    let at = RepoPath {
        reference: commit.or(access::reference(query)),
        ..repo_path(query)
    };
    let (owner, repo, requested) = (at.owner, at.repo, at.path);
    let identity = |mut failure: GhFailure| {
        failure.fields = vec![
            ("owner", json!(owner)),
            ("repo", json!(repo)),
            ("path", json!(requested)),
        ];
        failure
    };
    // The requested branch/tag/SHA does not exist (the transport types
    // GitHub's "No commit found"): name it and list the refs it could name.
    if let Some(reference) = at.reference.filter(|value| !value.is_empty())
        && error.reason == Some(ProviderErrorReason::RefNotFound)
    {
        let failure = GhFailure::new(
            error,
            format!("Branch, tag, or SHA not found for {owner}/{repo}: \"{reference}\""),
        );
        return identity(with_ref_recovery(failure, owner, repo));
    }
    let message = match error.kind {
        ProviderErrorKind::NotFound if repository_not_found(&error) => {
            repository_not_found_message(owner, repo)
        }
        ProviderErrorKind::NotFound if found.is_some() => at.not_found_message(at.reference),
        // Provider-local validation (no HTTP status) carries a specific,
        // actionable message (directory/symlink/submodule path, bad name).
        ProviderErrorKind::Validation if error.status.is_none() => error.message.to_string(),
        // A read is not a search: GitHub's 422 detail names the bad field.
        ProviderErrorKind::Validation => validation_message(&error, "Invalid request parameters"),
        _ => provider_message(&error),
    };
    let mut label = None;
    let (hint, next): (Option<&str>, Option<Value>) = if error.reason
        == Some(ProviderErrorReason::BinaryFile)
    {
        (
            Some(
                "Binary content cannot be returned as text; retrying will not help. ghStructure lists its path and size.",
            ),
            None,
        )
    } else if error.kind == ProviderErrorKind::Validation
        && error.status.is_none()
        && error.reason == Some(ProviderErrorReason::PathIsDirectory)
    {
        // The provider confirmed this path is a directory: listing it is
        // exact, and the lead is the whole repair.
        (None, Some(json!({ "viewTree": tree_recovery(&at, None) })))
    } else if repository_not_found(&error) {
        // Listing a tree of the same repository cannot recover; a search
        // for its name names the owner that holds it (as on the search
        // tools).
        label = Some(REPOSITORY_SEARCH_HINT.to_owned());
        (
            None,
            Some(octocode_tools_shared::gh_shared::search::repository_repair(
                ToolId::GhGetFileContent,
                query,
                Some(owner),
                repo,
            )),
        )
    } else if error.kind == ProviderErrorKind::NotFound {
        let (hint, next) = path_recovery(query, &at, found);
        label = hint;
        (None, next)
    } else {
        (provider_hint(&error), None)
    };
    let mut failure = GhFailure::new(error, message);
    if let Some(hint) = hint.or(label.as_deref()) {
        failure = failure.hint(hint);
    }
    failure.calls = next.map(Box::new);
    identity(failure)
}

/// The hint of a read that answered not found for a path the tree lists
/// under that very spelling: its lead lists the directory that answered.
pub(crate) const LISTED_NOT_READ_HINT: &str =
    "The tree lists this path, but its read answered not found.";

/// [`LISTED_NOT_READ_HINT`] for a read under a renamed repository's old
/// name: its lead reads the path under the canonical name.
pub(crate) const OLD_NAME_NOT_READ_HINT: &str =
    "The tree lists this path, but its read under the repository's old name answered not found.";

/// A path that did not resolve: the case-corrected file, the nearest
/// existing directory, or (without a walk answer) the parent directory. A
/// file the walk found under the very path asked is no case difference:
/// the lead never repeats the read that failed.
fn path_recovery(
    query: &GhGetFileContentQuery,
    at: &RepoPath<'_>,
    found: Option<Located>,
) -> (Option<String>, Option<Value>) {
    let Some(Located { found, renamed }) = found else {
        let parent = parent_dir(at.path);
        let listing = RepoPath {
            path: &parent,
            ..*at
        };
        return (
            None,
            Some(json!({ "viewTree": tree_recovery(&listing, None) })),
        );
    };
    let directory = if found.directory.is_empty() {
        "."
    } else {
        found.directory.as_str()
    };
    let listing = RepoPath {
        path: directory,
        ..*at
    };
    let view_tree = || Some(json!({ "viewTree": tree_recovery(&listing, None) }));
    // One repair: the read of the file under the path the tree names it by
    // (case-corrected, or the same path under the canonical name), else the
    // listing of the deepest existing directory (the lead's path is the fact).
    let Some(path) = found.file else {
        return (
            found
                .truncated
                .then(|| octocode_tools_shared::gh_shared::LISTING_MAY_BE_CUT_HINT.to_owned()),
            view_tree(),
        );
    };
    let hint = match (path == at.path, renamed) {
        // The path given was a whole listing entry: the read is of its path.
        _ if found.listing_entry => octocode_tools_shared::root_read::listing_entry_note(&path),
        (false, _) => octocode_tools_shared::gh_shared::CASE_DIFFERS_HINT.to_owned(),
        (true, true) => OLD_NAME_NOT_READ_HINT.to_owned(),
        // The read led to would be the one that failed.
        (true, false) => return (Some(LISTED_NOT_READ_HINT.to_owned()), view_tree()),
    };
    let mut read = query_row(query, &[]);
    read["path"] = json!(path);
    // The lead pins the commit the failed read resolved.
    if let Some(reference) = at.reference {
        read["ref"] = json!(reference);
    }
    let read = Continuation::related(ToolId::GhGetFileContent, read).build();
    (Some(hint), Some(json!({ "read": read })))
}
