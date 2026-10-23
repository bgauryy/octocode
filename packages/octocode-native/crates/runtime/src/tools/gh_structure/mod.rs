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
mod failures;
mod listing;
pub(crate) mod materialize;
mod refs;
mod tag_order;
mod traverse;
mod tree;
use failures::*;
use listing::*;
use materialize::*;
use traverse::*;
use tree::*;

pub async fn run(
    provider: &GitHubProvider,
    query: &GhStructureQuery,
    request: Result<&RequestContext, ProviderError>,
    home: &Path,
) -> Result<ToolData, GhFailure> {
    if let GhStructureQuery::Tree(query) = query {
        return run_tree(provider, query, request, home).await;
    }
    let context = request.map_err(search_failure)?;
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
    let failure = with_ref_recovery(search_failure(error), owner, repo);
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
    let context = request.map_err(search_failure)?;
    let listed = listed_at(query).map_err(search_failure)?;
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
        let failure = search_failure(error);
        return Err(
            crate::tools::gh_shared::submodule_failure(provider, &at, context, failure).await,
        );
    }
    if !missing_path(&error, Some(at.path)) {
        // A repository that did not resolve leads to a search for its name.
        let failure = with_ref_recovery(search_failure(error), at.owner, at.repo);
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
    omitted: std::collections::BTreeSet<String>,
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
            traversal.omitted.retain(|named| {
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
