//! An empty code search: the scope check, its leads and its default hints.
use super::requested_ref;
use crate::id::ToolId;
use crate::response::result::Continuation;
use crate::tools::gh_search_code::{GhSearchCodeQuery, GhSearchCodeQueryMatchIn};
use crate::tools::gh_shared::{RepoPath, ensure_next};
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
        .list_owner_repositories(owner, "pushed", 1, 1, context)
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
/// absence. `broadened`: the search offers `broadenSearch`, which takes the
/// root listing's place.
pub(in crate::tools::gh_search_code) async fn empty_scope(
    value: &mut Value,
    diagnostics: &mut crate::response::result::ToolDiagnostics,
    query: &GhSearchCodeQuery,
    broadened: bool,
    transport: &GitHubTransport,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let GhSearchCodeQuery {
        owner,
        repo: Some(repo),
        ..
    } = query
    else {
        return Ok(());
    };
    let metadata = transport.repository_metadata(owner, repo, context).await;
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
            json!({"owner":owner,"repo":repo,"path":""}),
            "ghRepoArchived",
        ),
        // The repository exists: a scoped path is listed; a repo-wide search
        // gets a local copy and the root layout to verify against.
        _ => {
            let hint = "No indexed matches is unproven absence; verify the repo structure and search a bounded local copy before concluding.";
            diagnostics.add("ghScopedZeroUnproven", hint, false);
            let Some(scope) = query.path.as_deref().map(|path| path.as_str()) else {
                // A path search of code leads to the same search of file
                // contents (`findCode`): that targeted lead keeps the slots.
                if !path_mode_given_code(query) {
                    add_clone_lead(value, query, owner.as_str(), repo.as_str());
                    if !broadened {
                        add_root_lead(value, query, owner.as_str(), repo.as_str());
                    }
                }
                return Ok(());
            };
            // `path` is a directory prefix: list the directory, or for a
            // file's path its parent, and read the file for the keywords.
            let listed = match file_scope(scope) {
                Some(file) => {
                    ensure_next(value);
                    if let Some(read) =
                        crate::tools::gh_search_code::leads::file_scope_read(query, file.path)
                    {
                        value["next"]["read"] = read;
                    }
                    file.parent
                }
                None => scope.trim_matches('/'),
            };
            (
                "viewTree",
                ToolId::GhStructure,
                json!({"owner":owner,"repo":repo,"path":listed}),
                "ghScopedZeroUnproven",
            )
        }
    };
    let hint = match code {
        "ghRepoArchived" => {
            Some("The repository is archived; its code-search index may lag.".to_owned())
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
        diagnostics.add(code, &hint, false);
        value["hints"] = json!([hint]);
    }
    value["next"][name] = Continuation::new(tool, next_query).build();
    Ok(())
}

/// The index proves nothing about absence: a local clone of the searched ref
/// is the check that does. Offered for a repository-wide search only: a path
/// scope already leads to its listing.
fn add_clone_lead(value: &mut Value, query: &GhSearchCodeQuery, owner: &str, repo: &str) {
    let at = RepoPath {
        reference: requested_ref(query),
        ..RepoPath::root(owner, repo)
    };
    ensure_next(value);
    value["next"]["cloneRepo"] = at.lead(ToolId::GhCloneRepo).build();
}

/// `cloneRepo` runs only where clone is available, so an empty repo-wide
/// search with nothing to broaden also gets the root listing at the
/// searched ref, which every surface runs.
fn add_root_lead(value: &mut Value, query: &GhSearchCodeQuery, owner: &str, repo: &str) {
    let at = RepoPath {
        reference: requested_ref(query),
        ..RepoPath::root(owner, repo)
    };
    ensure_next(value);
    value["next"]["viewTree"] = at.lead(ToolId::GhStructure).build();
}

/// A `match:"path"` search whose keywords read as code (spaces or syntax
/// characters) rather than path segments.
pub(in crate::tools::gh_search_code) fn path_mode_given_code(query: &GhSearchCodeQuery) -> bool {
    query.match_in == GhSearchCodeQueryMatchIn::Path
        && query.keywords.iter().any(|keyword| {
            keyword
                .trim()
                .chars()
                .any(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')))
        })
}

/// A `path` scope that names one file rather than a directory.
pub(in crate::tools::gh_search_code) struct FileScope<'a> {
    /// The scope without surrounding slashes.
    pub(in crate::tools::gh_search_code) path: &'a str,
    pub(in crate::tools::gh_search_code) parent: &'a str,
    pub(in crate::tools::gh_search_code) name: &'a str,
}

/// The index matches `path` as a directory prefix, so a scope whose last
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

/// What an empty code search says in its default hint: a requested ref the
/// default-branch index did not cover, and for a `path` scope why that proves
/// nothing (a file's path matches no directory; a directory's index can lag).
/// The leads carry the routes that verify it. Each note is its own hint, so
/// no composed hint outgrows the guidance budget.
pub(in crate::tools::gh_search_code) fn empty_hints(query: &GhSearchCodeQuery) -> Vec<String> {
    let reference = requested_ref(query).map(|reference| {
        format!("Code search indexes only the default branch; `{reference}` was not searched.")
    });
    let scope = query.path.as_deref().map(|path| path.as_str()).and_then(|scope| {
        match file_scope(scope) {
            Some(file) => {
                let under = if file.parent.is_empty() {
                    String::new()
                } else {
                    format!(" with `path:\"{}\"`", file.parent)
                };
                Some(format!(
                    "`path` is a directory prefix and `{}` looks like a file: search it by `filename:\"{}\"`{under}, or read it with matchString.",
                    file.path, file.name
                ))
            }
            // Absence under a directory is a repository fact: an owner-wide
            // search has no one directory to list.
            None => query.repo.as_ref().map(|_| {
                format!(
                    "No indexed match under `{}`: unproven absence; a file is `filename` + its parent as `path`; ghStructure lists the path.",
                    scope.trim_matches('/')
                )
            }),
        }
    });
    reference.into_iter().chain(scope).collect()
}
