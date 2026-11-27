//! An empty code search: the scope check, its leads and its default hints.
use super::requested_ref;
use crate::gh_search_code::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem};
use crate::qualifier_keywords::{Qualifier, qualifier, set_to};
use octocode_github::ProviderError;
use octocode_github::{
    GitHubTransport, ProviderErrorKind, ProviderErrorReason, RepositoryMetadata, RequestContext,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use octocode_tools_shared::gh_shared::ensure_calls;
use serde_json::{Value, json};

/// GitHub answers a code search of a login that does not exist with an empty,
/// incomplete page. An empty, incomplete owner-wide search therefore checks
/// the login: a missing one is a not-found failure, not an empty search.
pub(in crate::gh_search_code) async fn owner_exists(
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

/// What an empty search's scope check found ([`empty_scope`]).
#[derive(Default)]
pub(in crate::gh_search_code) struct EmptyScope {
    /// The repository's default branch when its metadata was read: the
    /// branch the code-search index covers.
    pub(in crate::gh_search_code) default_branch: Option<String>,
    /// The repository is archived, which the index does not cover at all:
    /// `coverage` says so and the listing is the row's recovery.
    pub(in crate::gh_search_code) archived: bool,
}

/// The scope check of an empty search: a missing repository fails, an
/// archived one is named in `coverage`, and a scoped one leads to what
/// verifies the absence (the folder, or a file's read and its parent); an
/// owner-wide search leads to the owner's repositories. A repo-wide miss
/// gets no listing: the tool description routes unindexed files to
/// ghStructure. `checked`: the metadata this request's rename check already
/// read, so it is not read again.
pub(in crate::gh_search_code) async fn empty_scope(
    value: &mut Value,
    diagnostics: &mut octocode_runtime_base::response::result::ToolDiagnostics,
    query: &GhSearchCodeQuery,
    transport: &GitHubTransport,
    checked: Option<RepositoryMetadata>,
    context: &RequestContext,
) -> Result<EmptyScope, ProviderError> {
    let GhSearchCodeQuery {
        owner,
        repo: Some(repo),
        ..
    } = query
    else {
        add_owner_lead(value, diagnostics, query.owner.as_str());
        return Ok(EmptyScope::default());
    };
    let metadata = match checked {
        Some(metadata) => Ok(metadata),
        None => transport.repository_metadata(owner, repo, context).await,
    };
    let default_branch = metadata
        .as_ref()
        .ok()
        .map(|metadata| metadata.default_branch.clone())
        .filter(|branch| !branch.is_empty());
    let (mut next_query, archived) = match metadata {
        Err(error) if error.is_cancelled_or_timed_out() => {
            return Err(error);
        }
        // A missing repository is a not-found failure, not an empty
        // search: the shared search failure names repository access.
        Err(error) if error.kind == ProviderErrorKind::NotFound => {
            return Err(error.with_reason(ProviderErrorReason::RepositoryNotFound));
        }
        Ok(metadata) if metadata.archived => (
            json!({"operation":"tree","owner":owner,"repo":repo,"path":""}),
            true,
        ),
        // The repository exists: a scoped path is listed. No hint: the tool
        // description says an empty search never rules out an unindexed
        // file and names the next steps (browse ghStructure, read with
        // matchString); the leads are the row's.
        _ => {
            let Some(scope) = query.path.as_deref().map(std::string::String::as_str) else {
                return Ok(EmptyScope {
                    default_branch,
                    archived: false,
                });
            };
            // List the directory, or for a file's path its parent, and read
            // the file for the keywords: the index may not hold it.
            let listed = match file_scope(scope) {
                Some(file) => {
                    ensure_calls(value);
                    if let Some(read) =
                        crate::gh_search_code::leads::file_scope_read(query, file.path)
                    {
                        value["calls"]["read"] = read;
                    }
                    file.parent
                }
                None => scope.trim_matches('/'),
            };
            (
                json!({"operation":"tree","owner":owner,"repo":repo,"path":listed}),
                false,
            )
        }
    };
    // Structure checks inspect the ref the caller asked about, not the
    // default branch the code-search index covers.
    if let Some(reference) = requested_ref(query) {
        next_query["ref"] = json!(reference);
    }
    // The index holds nothing of an archived repository: its empty answer
    // proves no absence, whatever the search itself completed. The reason
    // names the cause and the listing is the recovery: no hint restates them.
    if archived {
        value["coverage"] = json!({
            "state": "partial",
            "reason": "archived: GitHub's code search does not index archived repositories"
        });
    }
    ensure_calls(value);
    value["calls"]["viewTree"] = Continuation::related(ToolId::GhStructure, next_query).build();
    Ok(EmptyScope {
        default_branch,
        archived,
    })
}

/// An owner-wide search has no one repository to list or clone: the
/// owner's repositories are where to search or read next.
fn add_owner_lead(
    value: &mut Value,
    diagnostics: &mut octocode_runtime_base::response::result::ToolDiagnostics,
    owner: &str,
) {
    diagnostics.hint(
        "No indexed match across the owner is unproven absence: the index may not cover every repository; list them with the related[] ghSearchRepo request, then search or read one.",
    );
    ensure_calls(value);
    value["calls"]["ownerRepositories"] = octocode_tools_shared::gh_shared::owner_listing(owner);
}

/// A `match:"path"` search whose keywords read as code (spaces or syntax
/// characters) rather than path segments.
pub(in crate::gh_search_code) fn path_mode_given_code(query: &GhSearchCodeQuery) -> bool {
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

/// A `path` scope that names one file rather than a directory.
pub(in crate::gh_search_code) struct FileScope<'a> {
    /// The scope without surrounding slashes.
    pub(in crate::gh_search_code) path: &'a str,
    pub(in crate::gh_search_code) parent: &'a str,
}

/// A scope whose last segment looks like a file (an extension that holds a
/// letter, or a known extensionless or dot file such as `Makefile` or
/// `.gitignore`): its empty search leads to the file's read and its parent's
/// listing. A dotted directory such as `docs/v1.2` is not a file.
pub(in crate::gh_search_code) fn file_scope(scope: &str) -> Option<FileScope<'_>> {
    let path = scope.trim_matches('/');
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
    let has_extension = name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && (1..=8).contains(&extension.len())
            && extension.chars().all(|c| c.is_ascii_alphanumeric())
            && extension.chars().any(|c| c.is_ascii_alphabetic())
    });
    let known = matches!(
        octocode_runtime_base::content::classify_file_type(name),
        Some(
            octocode_runtime_base::content::FileType::Config
                | octocode_runtime_base::content::FileType::Lock
        )
    );
    (has_extension || known).then_some(FileScope { path, parent })
}

/// The requested ref the default-branch index did not cover: `None` when no
/// ref was requested or it names `default_branch` (the indexed branch, when
/// known).
pub(in crate::gh_search_code) fn unsearched_ref<'q>(
    query: &'q GhSearchCodeQuery,
    default_branch: Option<&str>,
) -> Option<&'q str> {
    requested_ref(query).filter(|reference| default_branch != Some(*reference))
}

/// The code-search qualifiers a keyword can spell, with the field that
/// carries each: a keyword matches text, so `language:rust` finds nothing
/// `extensions` would.
const QUALIFIER_FIELDS: &[Qualifier] = &[
    Qualifier {
        names: &["language", "lang"],
        field: |value| {
            format!(
                "set `extensions` to the file extensions of {value} (no leading dot, e.g. `rs`, `py`; code search has no language field)"
            )
        },
    },
    Qualifier {
        names: &["extension", "ext"],
        field: |value| {
            format!(
                "set `extensions` to `[\"{}\"]` (no leading dot)",
                value.trim_start_matches('.')
            )
        },
    },
    qualifier!(["path"] => "path"),
    qualifier!(["filename"] => "nameContains"),
    qualifier!(["user", "org"] => "owner"),
    Qualifier {
        names: &["repo"],
        field: |value| match value.split_once('/') {
            Some((owner, repo)) => format!("set `owner` to `{owner}` and `repo` to `{repo}`"),
            None => set_to("repo", value),
        },
    },
    Qualifier {
        names: &["in"],
        field: |value| format!("set `matchIn` to `[\"{value}\"]` (`content` or `path`)"),
    },
];

/// A hint per keyword spelled as a code-search qualifier that names the
/// field carrying it.
pub(in crate::gh_search_code) fn qualifier_keyword_hints(keywords: &[&str]) -> Vec<String> {
    crate::qualifier_keywords::hints(keywords, QUALIFIER_FIELDS)
}

/// What an empty code search says in its default hint: a requested ref the
/// default-branch index did not cover ([`unsearched_ref`]), naming that
/// branch when known. The leads carry the routes that verify it.
pub(in crate::gh_search_code) fn empty_hints(
    query: &GhSearchCodeQuery,
    default_branch: Option<&str>,
) -> Vec<String> {
    unsearched_ref(query, default_branch)
        .map(|reference| {
            let branch =
                default_branch.map_or_else(String::new, |branch| format!(" `{branch}`"));
            format!(
                "Code search indexes only the default branch{branch}; `{reference}` was not searched."
            )
        })
        .into_iter()
        .collect()
}
