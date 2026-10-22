//! ghStructure: a known repository's tree at one commit, optionally written
//! to a local directory (`materialize`).
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::ToolData;
use crate::response::result::query_row;
use crate::tools::gh_shared::{
    GhFailure, PathRecovery, RepoPath, default_branch, locate_path, missing_path, parent_dir,
    push_warning_front, repo_key, search_failure, tree_recovery, with_default_branch,
    with_ref_recovery,
};
use octocode_contract_types::ExtraKind;
use octocode_github::{
    ContentsEntry, GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext, TreeRequest,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::Path;

pub use octocode_contract_types::GhStructureQuery;
use octocode_contract_types::{
    GhStructureQueryLanguages, GhStructureQueryRefs, GhStructureQueryTree,
};

mod dates;
mod listing;
pub(crate) mod materialize;
mod refs;
mod traverse;
use listing::*;
use materialize::*;
use traverse::*;

pub async fn run(
    provider: &GitHubProvider,
    query: &GhStructureQuery,
    request: Result<&RequestContext, ProviderError>,
    home: &Path,
) -> Result<ToolData, GhFailure> {
    if let GhStructureQuery::Tree(query) = query {
        return run_tree(provider, query, request, home).await;
    }
    let context = request.map_err(|error| search_failure(error, None))?;
    let error = match execute(provider, query, context, home).await {
        Ok(output) => return Ok(output),
        Err(error) => error,
    };
    let (owner, repo) = repository(query);
    let renamed = crate::tools::gh_shared::canonical_query(provider, query, owner, repo, context)
        .await
        .ok()
        .flatten();
    let query = renamed.as_ref().map_or(query, |(_, canonical)| canonical);
    let (owner, repo) = repository(query);
    let failure = with_ref_recovery(search_failure(error, None), owner, repo);
    let failure = with_default_branch(provider, failure, ToolId::GhStructure, query, context).await;
    Err(crate::tools::gh_shared::with_search_repair(
        failure,
        ToolId::GhStructure,
        query,
        1,
        Some(repo),
    ))
}

fn repository(query: &GhStructureQuery) -> (&str, &str) {
    match query {
        GhStructureQuery::Tree(query) => (query.owner.as_str(), query.repo.as_str()),
        GhStructureQuery::Refs(query) => (query.owner.as_str(), query.repo.as_str()),
        GhStructureQuery::Languages(query) => (query.owner.as_str(), query.repo.as_str()),
    }
}

pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhStructureQuery,
    context: &RequestContext,
    home: &Path,
) -> Result<ToolData, ProviderError> {
    match query {
        GhStructureQuery::Tree(query) => execute_tree(provider, query, context, home).await,
        GhStructureQuery::Refs(query) => refs::refs(provider, query, context).await,
        GhStructureQuery::Languages(query) => refs::languages(provider, query, context).await,
    }
}

/// Run one ghStructure row. A listing of a missing path names the path and
/// lists the nearest existing directory (case-corrected) with
/// `hints.viewTree`; without a located directory it lists the parent.
async fn run_tree(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    request: Result<&RequestContext, ProviderError>,
    home: &Path,
) -> Result<ToolData, GhFailure> {
    let context = request.map_err(|error| search_failure(error, None))?;
    let listed = listed_at(query).map_err(|error| search_failure(error, None))?;
    let query = listed.as_ref().unwrap_or(query);
    let error = match execute_tree(provider, query, context, home).await {
        Ok(output) => return Ok(output),
        Err(error) => error,
    };
    let error = classify_unresolved_sha(provider, query, context, error).await;
    let path = query.path.as_deref().unwrap_or_default().trim_matches('/');
    let requested = query.ref_.as_ref().map(|value| value.as_str());
    // A path failure comes after its ref resolved: its leads pin that commit,
    // and name a renamed repository as it is now.
    let scope = crate::tools::gh_shared::failure_scope(
        provider,
        query.owner.as_str(),
        query.repo.as_str(),
        requested,
        error.reason == Some(ProviderErrorReason::PathIsFile) || missing_path(&error, Some(path)),
        context,
    )
    .await;
    let renamed = scope
        .canonical
        .as_ref()
        .and_then(|(owner, repo)| crate::tools::gh_shared::with_repo(query, owner, repo).ok());
    let query = renamed.as_ref().unwrap_or(query);
    let at = RepoPath {
        owner: query.owner.as_str(),
        repo: query.repo.as_str(),
        path,
        reference: scope.commit.as_deref().or(requested),
    };
    if error.reason == Some(ProviderErrorReason::PathIsFile) {
        return Err(file_failure(error, &at));
    }
    if error.reason == Some(ProviderErrorReason::Submodule) {
        let failure = search_failure(error, None);
        return Err(
            crate::tools::gh_shared::submodule_failure(provider, &at, context, failure).await,
        );
    }
    if !missing_path(&error, Some(at.path)) {
        // A repository that did not resolve leads to a search for its name.
        let failure = with_ref_recovery(search_failure(error, None), at.owner, at.repo);
        let failure =
            with_default_branch(provider, failure, ToolId::GhStructure, query, context).await;
        return Err(crate::tools::gh_shared::with_search_repair(
            failure,
            ToolId::GhStructure,
            query,
            1,
            Some(at.repo),
        ));
    }
    let found = locate_path(provider, &at, context).await;
    Err(path_failure(error, &at, query.max_depth, found))
}

/// The query with its `path` as the repository names it
/// ([`crate::tools::gh_shared::listing_path`]), so the listing, its
/// continuations and its recovery leads all name one path; `None` when it
/// already does. A path climbing above the root is invalid input.
fn listed_at(query: &GhStructureQueryTree) -> Result<Option<GhStructureQueryTree>, ProviderError> {
    let Some(requested) = query.path.as_deref() else {
        return Ok(None);
    };
    let path = crate::tools::gh_shared::listing_path(requested)?;
    Ok((path != requested).then(|| GhStructureQueryTree {
        path: Some(path),
        ..query.clone()
    }))
}

/// A full 40-hex `ref` skips ref resolution, so a SHA GitHub does not have
/// surfaces as the tree read's bare 404. On that failure only, one commit
/// lookup tells a missing commit (named like any missing ref, with
/// `hints.viewRefs`) from a missing repository; any other outcome keeps the
/// original error.
async fn classify_unresolved_sha(
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
fn file_failure(error: ProviderError, at: &RepoPath<'_>) -> GhFailure {
    let mut read = json!({"owner": at.owner, "repo": at.repo, "path": at.path});
    if let Some(reference) = at.reference {
        read["ref"] = json!(reference);
    }
    let mut failure = search_failure(error, None);
    failure.next = Some(Box::new(json!({
        "read": crate::response::result::Continuation::new(ToolId::GhGetFileContent, read)
            .confidence(Confidence::Exact)
            .build()
    })));
    failure
}

fn path_failure(
    error: ProviderError,
    at: &RepoPath<'_>,
    max_depth: Option<std::num::NonZeroU64>,
    found: Option<PathRecovery>,
) -> GhFailure {
    let mut failure = search_failure(error, None);
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

/// Largest listing page (contract `pageSize` maximum). Path-only rows stay
/// compact: 500 entries of a deep tree render in about 13k chars.
fn max_entries_per_page() -> usize {
    crate::contracts::query_schema_max(ToolId::GhStructure, None, "pageSize")
}
const CONTENTS_LIMIT: usize = 1000;
/// Tree listing page size when `pageSize` is omitted (contract
/// `ghStructure.semanticPolicy.treePageSize`).
pub(super) const DEFAULT_ENTRIES_PER_PAGE: u64 =
    crate::id::gh_structure_policy::TREE_PAGE_SIZE as u64;
/// Upper bound on Contents API directory reads for one fallback walk (git
/// trees API truncated or unavailable). Past it the listing is a typed
/// terminal limit instead of an unbounded request fan-out.
pub(crate) const MAX_DIRECTORY_FETCHES: usize = 200;
/// GitHub reads one listing step (a walk level, a materialize chunk) sends
/// at once; the global request semaphore still bounds the total.
const READ_CONCURRENCY: usize = 8;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct TreeEntry {
    path: String,
    kind: EntryKind,
    size: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
enum EntryKind {
    File,
    Dir,
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Traversal {
    entries: Vec<TreeEntry>,
    failed_subtrees: usize,
    contents_limit: bool,
    /// Fallback walk stopped at [`MAX_DIRECTORY_FETCHES`].
    #[serde(default)]
    fetch_limit: bool,
    /// What the listing does not show, as `"<repo-relative path> (<kind>)"`:
    /// symlinks and submodules, which have no listable content. Every
    /// committed directory is listed and walked: a tree holds only tracked
    /// content, so the local build-output prune names (`build/`,
    /// `coverage/`, `node_modules/`) do not apply to it.
    #[serde(default)]
    omitted: std::collections::BTreeMap<String, usize>,
}

async fn execute_tree(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    context: &RequestContext,
    home: &Path,
) -> Result<ToolData, ProviderError> {
    let scope = Scope::of(query)?;
    let (resolved_branch, commit_sha) = resolve_listing_ref(provider, query, context).await?;
    // A listing is read, dated and continued under the canonical name:
    // later requests skip the redirect, and every lead names the
    // repository as it is now.
    let (renamed_warning, canonical) = crate::tools::gh_shared::canonical_query(
        provider,
        query,
        query.owner.as_str(),
        query.repo.as_str(),
        context,
    )
    .await?
    .unzip();
    let query = canonical.as_ref().unwrap_or(query);
    let listing = Listing {
        owner: &query.owner,
        repo: &query.repo,
        branch: &commit_sha,
        root: &scope.path,
        max_depth: scope.depth,
    };
    let mut traversal = traverse(provider, &listing, context).await?;
    scope.select(&mut traversal);
    let page = ListingPage::of(query, &traversal.entries);
    let page_dates = date_page(provider, query, &commit_sha, &page, context).await;
    let dating = PageDating {
        undated: page_dates.undated,
        failed: page_dates.warning.is_some(),
    };
    let mut value = json!({
        "files": build_structure(page.entries, &page_dates.entries),
        "summary": summary(&traversal.entries),
        "resolvedRef": resolved_branch,
    });
    // A caller-supplied full SHA is not restated.
    if !resolved_branch.eq_ignore_ascii_case(&commit_sha) {
        value["commitSha"] = json!(commit_sha);
    }
    page.paginate(&mut value);
    dates::attach(&mut value, page_dates);
    let mut output = ToolData::from(Value::Null);
    let materialize_resume = if query.materialize == Some(true) {
        materialize_page(
            provider,
            query,
            &scope,
            &commit_sha,
            &page,
            home,
            context,
            &mut value,
        )
        .await?
    } else {
        None
    };
    disclose_limits(&traversal, &page, &mut value, &mut output);
    if traversal.contents_limit {
        value["next"]["cloneRepo"] = clone_lead(query, &scope, &commit_sha);
    }
    if let Some(warning) = renamed_warning {
        push_warning_front(&mut value, warning);
    }
    // Continuations read the same commit.
    let pinned = GhStructureQueryTree {
        ref_: Some(commit_sha.parse().map_err(
            |error: octocode_contract_types::error::ConversionError| {
                ProviderError::new(ProviderErrorKind::Decode, error.to_string())
            },
        )?),
        ..query.clone()
    };
    attach_continuations(
        &mut value,
        &pinned,
        &page,
        traversal.failed_subtrees > 0 || dating.failed,
        query.materialize == Some(true),
        materialize_resume,
    )?;
    dates::expand(&mut value, &pinned, &page, dating.undated)?;
    // A resumed materialize page (offset > 0) is anchored by `location`:
    // its first call already listed these entries.
    let resumed = query.materialize == Some(true)
        && query.materialize_offset.is_some_and(|offset| offset > 0)
        && value.get("location").is_some();
    if resumed && let Some(map) = value.as_object_mut() {
        map.remove("files");
    }
    if !resumed && structure_is_empty(&value) && value.get("isPartial").is_none() {
        output.status = Some("empty");
        if let Some(broader) = broaden_listing(&pinned) {
            value["next"]["broadenSearch"] = broader;
        }
    } else if value.pointer("/location/localPath").is_some() {
        // A materialized listing continues with the local tools at
        // `location.localPath`; re-listing the tree it just returned adds
        // nothing.
    } else if page.current == 1
        && let Some(read) = entry_read(&pinned, &traversal.entries, &commit_sha)
    {
        value["next"]["read"] = read;
    }
    output.data = value;
    Ok(output)
}

/// An empty filtered listing, broadened ([`crate::tools::gh_shared::broaden`]):
/// the same commit's listing without its `include` globs, so what the tree
/// holds names why they matched nothing. Never a materialization.
fn broaden_listing(pinned: &GhStructureQueryTree) -> Option<Value> {
    crate::tools::gh_shared::broaden(
        ToolId::GhStructure,
        &query_row(pinned, &[]),
        &crate::tools::gh_shared::Broaden {
            strip: &["materialize", "materializeOffset"],
            groups: &[&["include"]],
        },
        1,
    )
}

/// What dating a listing page left: entries no date reached, and whether
/// the dating request failed (the same page retries it).
#[derive(Default)]
struct PageDating {
    undated: usize,
    failed: bool,
}

/// With `extras: ["details"]` a listing states when each entry last changed
/// (one GraphQL request per page, so only when asked); a materialized page
/// hands its files to local tools and stays undated.
async fn date_page(
    provider: &GitHubProvider,
    query: &GhStructureQueryTree,
    commit_sha: &str,
    page: &ListingPage<'_>,
    context: &RequestContext,
) -> dates::PageDates {
    if !query.extras.contains(&ExtraKind::Details) || query.materialize == Some(true) {
        return dates::PageDates::default();
    }
    dates::page_dates(
        provider,
        &query.owner,
        &query.repo,
        commit_sha,
        page.entries,
        context,
    )
    .await
}

/// What one listing covers: the requested directory, the depth, and the
/// `include` globs (ORed: a path matching any is kept).
struct Scope {
    path: String,
    depth: usize,
    filter: Option<Vec<PathFilter>>,
}

impl Scope {
    /// The listed entries of the walk: those the `include` globs keep
    /// (matched relative to `path`, as the local tools match), directory by
    /// directory in path order, so a page of a deep listing carries each
    /// directory's files with its folders instead of every folder of the
    /// tree before the first file. An omitted entry stays named only when
    /// the globs keep its path too.
    fn select(&self, traversal: &mut Traversal) {
        if let Some(filter) = &self.filter {
            let kept = |path: &str| {
                let path = relative(path, &self.path);
                filter.iter().any(|glob| glob.matches(&path))
            };
            traversal.entries.retain(|entry| kept(&entry.path));
            traversal.omitted.retain(|named, _| {
                kept(
                    named
                        .rsplit_once(" (")
                        .map_or(named.as_str(), |(path, _)| path),
                )
            });
        }
        let entries = &mut traversal.entries;
        let parent = |path: &str| path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
        entries.sort_by(|left, right| {
            parent(&left.path)
                .cmp(&parent(&right.path))
                .then_with(|| left.path.cmp(&right.path))
        });
    }

    fn of(query: &GhStructureQueryTree) -> Result<Self, ProviderError> {
        let filters = query
            .include
            .iter()
            .map(|glob| PathFilter::new(glob))
            .collect::<Result<Vec<_>, _>>()?;
        let filter = (!filters.is_empty()).then_some(filters);
        let path = crate::tools::gh_shared::listing_path(query.path.as_deref().unwrap_or(""))?;
        // A name search looks at every level unless the caller bounds it.
        let depth = match (query.max_depth, &filter) {
            (Some(depth), _) => crate::num::usize_of(depth),
            (None, Some(_)) => max_listing_depth(),
            (None, None) => 1,
        };
        Ok(Self {
            path,
            depth,
            filter,
        })
    }
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn fallback_hint(&self, query: &serde_json::Value) -> &'static str {
        if crate::response::output::has_value(query, "include") {
            "Loosen the include glob, raise maxDepth, or list the tree without include."
        } else {
            "Check ref and path, raise maxDepth, or list the parent path."
        }
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
    /// The resolved branch is next-call input when the caller named none;
    /// it goes only when it repeats the requested ref.
    fn compact(
        &self,
        data: &mut serde_json::Map<String, serde_json::Value>,
        query: &serde_json::Value,
    ) {
        if data.get("resolvedRef").is_some() && data.get("resolvedRef") == query.get("ref") {
            data.remove("resolvedRef");
        }
    }
}

#[cfg(test)]
mod tests;
