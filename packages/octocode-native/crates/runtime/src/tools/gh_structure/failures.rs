//! How a listing states a failure: a missing file, path, commit or repository, with its recovery lead.
use super::*;

/// A full 40-hex `ref` skips ref resolution, so a SHA GitHub does not have
/// surfaces as the tree read's bare 404. On that failure only, one commit
/// lookup tells a missing commit (named like any missing ref, with
/// `hints.viewRefs`) from a missing repository; any other outcome keeps the
/// original error.
pub(super) async fn classify_unresolved_sha(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    context: &RequestContext,
    error: ProviderError,
) -> ProviderError {
    let Some(reference) = query.ref_.as_deref() else {
        return error;
    };
    if error.kind != ProviderErrorKind::NotFound
        || error.reason.is_some()
        || !octocode_github::is_full_sha(reference)
    {
        return error;
    }
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    match provider
        .transport
        .commit_sha(owner, repo, reference, context)
        .await
    {
        Err(probe)
            if probe.reason == Some(ProviderErrorReason::RefNotFound)
                || probe.reason == Some(ProviderErrorReason::RepositoryNotFound) =>
        {
            missing_ref(probe, owner, repo, reference)
        }
        _ => error,
    }
}

/// A listing of a file: the repair is that file's read at the same ref.
pub(super) fn file_failure(error: ProviderError, at: &RepoPath<'_>) -> GhFailure {
    let mut read = json!({"owner": at.owner, "repo": at.repo, "path": at.path});
    if let Some(reference) = at.reference {
        read["ref"] = json!(reference);
    }
    let mut failure = search_failure(error);
    failure.next = Some(Box::new(json!({
        "read": crate::response::result::Continuation::new(ToolId::GhGetFileContent, read)
            .confidence(Confidence::Exact)
            .build()
    })));
    failure
}

pub(super) fn path_failure(
    error: ProviderError,
    at: &RepoPath<'_>,
    max_depth: Option<std::num::NonZeroU64>,
    found: Option<PathRecovery>,
) -> GhFailure {
    let mut failure = search_failure(error);
    let requested = at.path;
    let (directory, confidence, hint) = match &found {
        Some(found) => {
            failure.message = format!("Path not found in {}/{}: {requested}", at.owner, at.repo);
            let directory = if found.directory.is_empty() {
                ".".to_owned()
            } else {
                found.directory.clone()
            };
            // The listing lead targets the deepest existing prefix; only a
            // case-only difference is a fact the lead does not state.
            let hint = if found.file.is_some() {
                Some("The path names a file (its case differs).")
            } else if found.directory.eq_ignore_ascii_case(requested) {
                Some("Only the path's case differs.")
            } else {
                None
            };
            (directory, Confidence::Exact, hint)
        }
        // Without a walk answer the parent listing is the whole recovery.
        None => (parent_dir(requested), Confidence::Low, None),
    };
    let depth = max_depth.map(|depth| i64::try_from(depth.get()).unwrap_or(i64::MAX));
    let listing = RepoPath {
        path: &directory,
        ..*at
    };
    failure.next = Some(Box::new(
        json!({ "viewTree": tree_recovery(&listing, depth, confidence) }),
    ));
    match hint {
        Some(hint) => failure.hint(hint),
        None => failure,
    }
}
