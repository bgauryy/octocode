//! An empty code search: the scope check, its leads and its default hints.
use super::requested_ref;
use crate::id::ToolId;
use crate::response::result::Continuation;
use crate::tools::gh_search_code::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem};
use crate::tools::gh_shared::{RepoPath, ensure_calls};
use octocode_github::ProviderError;
use octocode_github::{GitHubTransport, ProviderErrorKind, ProviderErrorReason, RequestContext};
use serde_json::{Value, json};

/// GitHub answers a code search of a login that does not exist with an empty,
/// incomplete page. An empty, incomplete owner-wide search therefore checks
/// the login: a missing one is a not-found failure, not an empty search.
pub(in crate::tools::gh_search_code) async fn owner_exists(
    owner: &str,
    transport: &GitHubTransport,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    match transport
        .list_owner_repositories(owner, "pushed", "desc", 1, 1, context)
        .await
    {
        Err(error) if error.kind == ProviderErrorKind::NotFound => {
            Err(error.with_reason(ProviderErrorReason::UserNotFound))
        }
        Err(error) if error.is_cancelled_or_timed_out() => Err(error),
        _ => Ok(()),
    }
}

/// The scope check of an empty search: a missing repository fails, an
/// archived one is named, and an existing one leads to what verifies the
/// absence; an owner-wide search leads to the owner's repositories.
/// `broadened`: the search offers `broadenSearch`, which takes the root
/// listing's place. Returns the repository's default branch when its
/// metadata was read: the branch the code-search index covers.
pub(in crate::tools::gh_search_code) async fn empty_scope(
    value: &mut Value,
    diagnostics: &mut crate::response::result::ToolDiagnostics,
    query: &GhSearchCodeQuery,
    broadened: bool,
    transport: &GitHubTransport,
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    let GhSearchCodeQuery {
        owner,
        repo: Some(repo),
        ..
    } = query
    else {
        add_owner_lead(value, diagnostics, query.owner.as_str());
        return Ok(None);
    };
    let metadata = transport.repository_metadata(owner, repo, context).await;
    let default_branch = metadata
        .as_ref()
        .ok()
        .map(|metadata| metadata.default_branch.clone())
        .filter(|branch| !branch.is_empty());
    let (name, tool, mut next_query, code) = match metadata {
        Err(error) if error.is_cancelled_or_timed_out() => {
            return Err(error);
        }
        // A missing repository is a not-found failure, not an empty
        // search: the shared search failure names repository access.
        Err(error) if error.kind == ProviderErrorKind::NotFound => {
            return Err(error.with_reason(ProviderErrorReason::RepositoryNotFound));
        }
        Ok(metadata) if metadata.archived => (
            "viewTree",
            ToolId::GhStructure,
            json!({"operation":"tree","owner":owner,"repo":repo,"path":""}),
            "ghRepoArchived",
        ),
        // The repository exists: a scoped path is listed; a repo-wide search
        // gets a local copy and the root layout to verify against.
        _ => {
            let hint = "No indexed match is unproven absence: check the source with the related[] ghStructure or ghCloneRepo request.";
            diagnostics.hint(hint);
            let Some(scope) = query
                .path_prefix
                .as_deref()
                .map(std::string::String::as_str)
            else {
                // A path search of code leads to the same search of file
                // contents (`findCode`): that targeted lead keeps the slots.
                if !path_mode_given_code(query) {
                    add_clone_lead(value, query, owner.as_str(), repo.as_str());
                    if !broadened {
                        add_root_lead(value, query, owner.as_str(), repo.as_str());
                    }
                }
                return Ok(default_branch);
            };
            // `pathPrefix` is a directory prefix: list the directory, or for a
            // file's path its parent, and read the file for the keywords.
            let listed = match file_scope(scope) {
                Some(file) => {
                    ensure_calls(value);
                    if let Some(read) =
                        crate::tools::gh_search_code::leads::file_scope_read(query, file.path)
                    {
                        value["calls"]["read"] = read;
                    }
                    file.parent
                }
                None => scope.trim_matches('/'),
            };
            (
                "viewTree",
                ToolId::GhStructure,
                json!({"operation":"tree","owner":owner,"repo":repo,"path":listed}),
                "ghScopedZeroUnproven",
            )
        }
    };
    let hint = match code {
        "ghRepoArchived" => {
            Some("Repository is archived, so its code-search index may lag: list its files with the related[] ghStructure request.".to_owned())
        }
        _ => None,
    };
    // Structure checks inspect the ref the caller asked about, not the
    // default branch the code-search index covers.
    if tool == ToolId::GhStructure
        && let Some(reference) = requested_ref(query)
    {
        next_query["ref"] = json!(reference);
    }
    // A missing, renamed, or archived repository is the answer: say so
    // instead of the generic default-branch note.
    if let Some(hint) = hint {
        diagnostics.hint(&hint);
        value["hints"] = json!([hint]);
    }
    value["calls"][name] = Continuation::related(tool, next_query).build();
    Ok(default_branch)
}

/// An owner-wide search has no one repository to list or clone: the
/// owner's repositories are where to search or read next.
fn add_owner_lead(
    value: &mut Value,
    diagnostics: &mut crate::response::result::ToolDiagnostics,
    owner: &str,
) {
    diagnostics.hint(
        "No indexed match across the owner is unproven absence: the index may not cover every repository; list them with the related[] ghSearchRepo request, then search or read one.",
    );
    ensure_calls(value);
    value["calls"]["ownerRepositories"] = crate::tools::gh_shared::owner_listing(owner);
}

/// The index proves nothing about absence: a local clone of the searched ref
/// is the check that does. Offered for a repository-wide search only: a path
/// scope already leads to its listing.
fn add_clone_lead(value: &mut Value, query: &GhSearchCodeQuery, owner: &str, repo: &str) {
    let at = RepoPath {
        reference: requested_ref(query),
        ..RepoPath::root(owner, repo)
    };
    ensure_calls(value);
    value["calls"]["cloneRepo"] = at.lead(ToolId::GhCloneRepo).build();
}

/// `cloneRepo` runs only where clone is available, so an empty repo-wide
/// search with nothing to broaden also gets the root listing at the
/// searched ref, which every surface runs.
fn add_root_lead(value: &mut Value, query: &GhSearchCodeQuery, owner: &str, repo: &str) {
    let at = RepoPath {
        reference: requested_ref(query),
        ..RepoPath::root(owner, repo)
    };
    ensure_calls(value);
    value["calls"]["viewTree"] = at.lead(ToolId::GhStructure).build();
}

/// A `match:"path"` search whose keywords read as code (spaces or syntax
/// characters) rather than path segments.
pub(in crate::tools::gh_search_code) fn path_mode_given_code(query: &GhSearchCodeQuery) -> bool {
    !query
        .match_in
        .contains(&GhSearchCodeQueryMatchInItem::Content)
        && query.keywords.iter().any(|keyword| {
            keyword
                .trim()
                .chars()
                .any(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')))
        })
}

/// A `pathPrefix` scope that names one file rather than a directory.
pub(in crate::tools::gh_search_code) struct FileScope<'a> {
    /// The scope without surrounding slashes.
    pub(in crate::tools::gh_search_code) path: &'a str,
    pub(in crate::tools::gh_search_code) parent: &'a str,
    pub(in crate::tools::gh_search_code) name: &'a str,
}

/// The index matches `pathPrefix` as a directory prefix, so a scope whose last
/// segment looks like a file (an extension that holds a letter, or a known
/// extensionless or dot file such as `Makefile` or `.gitignore`) matches
/// nothing. A dotted directory such as `docs/v1.2` is not a file.
pub(in crate::tools::gh_search_code) fn file_scope(scope: &str) -> Option<FileScope<'_>> {
    let path = scope.trim_matches('/');
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
    let has_extension = name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (1..=8).contains(&extension.len())
            && extension.chars().all(|c| c.is_ascii_alphanumeric())
            && extension.chars().any(|c| c.is_ascii_alphabetic())
    });
    let known = matches!(
        crate::content::classify_file_type(name),
        Some(crate::content::FileType::Config | crate::content::FileType::Lock)
    );
    (has_extension || known).then_some(FileScope { path, parent, name })
}

/// The requested ref the default-branch index did not cover: `None` when no
/// ref was requested or it names `default_branch` (the indexed branch, when
/// known).
pub(in crate::tools::gh_search_code) fn unsearched_ref<'q>(
    query: &'q GhSearchCodeQuery,
    default_branch: Option<&str>,
) -> Option<&'q str> {
    requested_ref(query).filter(|reference| default_branch != Some(*reference))
}

/// What an empty code search says in its default hint: a requested ref the
/// default-branch index did not cover ([`unsearched_ref`]), naming that
/// branch when known, and for a `pathPrefix` scope why that proves nothing
/// (a file's path matches no directory; a directory's index can lag). The
/// leads carry the routes that verify it. Each note is its own hint, so no
/// composed hint outgrows the guidance budget.
pub(in crate::tools::gh_search_code) fn empty_hints(
    query: &GhSearchCodeQuery,
    default_branch: Option<&str>,
) -> Vec<String> {
    let reference = unsearched_ref(query, default_branch).map(|reference| {
        let branch = default_branch.map_or_else(String::new, |branch| format!(" `{branch}`"));
        format!(
            "Code search indexes only the default branch{branch}; `{reference}` was not searched."
        )
    });
    let scope = query.path_prefix.as_deref().map(std::string::String::as_str).and_then(|scope| {
        match file_scope(scope) {
            Some(file) => {
                let under = if file.parent.is_empty() {
                    String::new()
                } else {
                    format!(" with `pathPrefix:\"{}\"`", file.parent)
                };
                Some(format!(
                    "`pathPrefix` is a directory prefix and `{}` looks like a file: search it by `nameContains:\"{}\"`{under}, or read it with matchString.",
                    file.path, file.name
                ))
            }
            // Absence under a directory is a repository fact: an owner-wide
            // search has no one directory to list.
            None => query.repo.as_ref().map(|_| {
                format!(
                    "`pathPrefix:\"{}\"` is a directory scope: to search one file there, set `nameContains` to its name and `pathPrefix` to its parent.",
                    scope.trim_matches('/')
                )
            }),
        }
    });
    reference.into_iter().chain(scope).collect()
}
