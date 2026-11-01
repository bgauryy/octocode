//! A failed ghGetFileContent read: one message, one hint and one recovery
//! per row, chosen from the provider error and the missing-path walk.
use super::access;
use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};
use crate::tools::gh_shared::{
    GhFailure, PathRecovery, RepoPath, parent_dir, provider_hint, provider_message,
    repository_not_found, repository_not_found_message, repository_recovery,
    repository_search_hint, tree_recovery, validation_message, with_ref_recovery,
};
use octocode_contract_types::GhGetFileContentQuery;
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
use serde_json::{Value, json};

pub(super) fn repo_path(query: &GhGetFileContentQuery) -> RepoPath<'_> {
    RepoPath {
        owner: access::owner(query),
        repo: access::repo(query),
        path: access::path(query),
        reference: access::reference(query),
    }
}

/// The failure row of a read. `found` is the missing-path walk's answer
/// (only for a path that did not resolve).
pub(crate) fn failure(
    error: ProviderError,
    query: &GhGetFileContentQuery,
    found: Option<PathRecovery>,
) -> GhFailure {
    let at = repo_path(query);
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
        ProviderErrorKind::NotFound if found.is_some() => path_not_found(&at, at.reference),
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
        label = Some(repository_search_hint(Some(owner), repo));
        (None, Some(repository_recovery(Some(owner), repo)))
    } else if error.kind == ProviderErrorKind::NotFound {
        path_recovery(query, &at, found)
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

/// The missing-path message, naming the ref the caller asked for and,
/// when that named a branch or tag, the commit it resolved to (`at`'s
/// ref, which the recovery leads pin).
pub(super) fn path_not_found(at: &RepoPath<'_>, asked: Option<&str>) -> String {
    let (owner, repo, path) = (at.owner, at.repo, at.path);
    let Some(asked) = asked.filter(|value| !value.is_empty()) else {
        return format!("Path not found in {owner}/{repo}: {path}");
    };
    match at.reference.filter(|commit| *commit != asked) {
        Some(commit) => {
            let short = commit.get(..7).unwrap_or(commit);
            format!("Path not found in {owner}/{repo} at {asked} ({short}): {path}")
        }
        None => format!("Path not found in {owner}/{repo} at {asked}: {path}"),
    }
}

/// A path that did not resolve: the case-corrected file, the nearest
/// existing directory, or (without a walk answer) the parent directory.
fn path_recovery(
    query: &GhGetFileContentQuery,
    at: &RepoPath<'_>,
    found: Option<PathRecovery>,
) -> (Option<&'static str>, Option<Value>) {
    let Some(found) = found else {
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
    // One repair: the case-corrected file's read, else the listing of the
    // deepest existing directory (the lead's path is the fact).
    match found.file {
        Some(path) => {
            let mut read = query_row(query, &[]);
            read["path"] = json!(path);
            let read = Continuation::related(ToolId::GhGetFileContent, read).build();
            (
                Some("Only the path's case differs."),
                Some(json!({ "read": read })),
            )
        }
        None => (
            None,
            Some(json!({ "viewTree": tree_recovery(&listing, None) })),
        ),
    }
}
