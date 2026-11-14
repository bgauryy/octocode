//! ghStructure: a known repository's tree at one commit, optionally written
//! to a local directory (`materialize`).
use crate::id::ToolId;
use crate::response::result::ToolData;
use crate::response::result::query_row;
use crate::tools::gh_shared::{
    GhFailure, PathRecovery, RepoPath, default_branch, locate_path, missing_path, parent_dir,
    push_warning_front, read_failure_scope, search_failure, tree_recovery, with_read_recovery,
};
use octocode_contract_types::{
    GhStructureQuery, GhStructureQueryLanguages, GhStructureQueryRefs, GhStructureQueryTree,
};
use octocode_github::{
    ContentsEntry, GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext, TreeRequest,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::Path;

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
    Err(with_read_recovery(
        provider,
        search_failure(error),
        ToolId::GhStructure,
        query,
        owner,
        repo,
        context,
    )
    .await)
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
        GhStructureQuery::Tree(query) => {
            execute_tree(provider, query, context, home, &mut None).await
        }
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
    let mut read_commit = None;
    let error = match execute_tree(provider, query, context, home, &mut read_commit).await {
        Ok(output) => return Ok(output),
        Err(error) => error,
    };
    let error = classify_unresolved_sha(provider, query, context, error).await;
    let path = query
        .path
        .as_deref()
        .map_or("", String::as_str)
        .trim_matches('/');
    let requested = query.ref_.as_ref().map(|value| value.as_str());
    // A path failure comes after its ref resolved: its leads pin that commit,
    // and name a renamed repository as it is now.
    let scope = read_failure_scope(
        provider,
        query.owner.as_str(),
        query.repo.as_str(),
        path,
        read_commit,
        error,
        context,
    )
    .await;
    let error = scope.error;
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
        return Err(with_read_recovery(
            provider,
            search_failure(error),
            ToolId::GhStructure,
            query,
            at.owner,
            at.repo,
            context,
        )
        .await);
    }
    let found = locate_path(provider, &at, context).await;
    Err(path_failure(error, &at, query.max_depth, found))
}

/// The query with its `path` as the repository names it
/// ([`crate::tools::gh_shared::listing_path`]), so the listing, its
/// continuations and its recovery leads all name one path; `None` when it
/// already does. A path climbing above the root is invalid input.
fn listed_at(query: &GhStructureQueryTree) -> Result<Option<GhStructureQueryTree>, ProviderError> {
    let Some(requested) = query.path.as_deref().map(String::as_str) else {
        return Ok(None);
    };
    let path = crate::tools::gh_shared::listing_path(requested)?;
    if path == requested {
        return Ok(None);
    }
    let path = path
        .parse()
        .map_err(|error: octocode_contract_types::error::ConversionError| {
            ProviderError::new(ProviderErrorKind::Validation, error.to_string())
        })?;
    Ok(Some(GhStructureQueryTree {
        path: Some(path),
        ..query.clone()
    }))
}

const CONTENTS_LIMIT: usize = 1000;
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
    /// What the listing does not show, by kind (`symlink`, `submodule`):
    /// the repository-relative paths of entries with no listable content,
    /// each kind stated once beside its paths. Every
    /// committed directory is listed and walked: a tree holds only tracked
    /// content, so the local build-output prune names (`build/`,
    /// `coverage/`, `node_modules/`) do not apply to it.
    #[serde(default)]
    omitted: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
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
    /// (matched against repository-relative paths), directory by
    /// directory in path order, so a page of a deep listing carries each
    /// directory's files with its folders instead of every folder of the
    /// tree before the first file. An omitted entry stays named only when
    /// the globs keep its path too.
    fn select(&self, traversal: &mut Traversal) {
        if let Some(filter) = &self.filter {
            let kept = |path: &str| filter.iter().any(|glob| glob.matches(path));
            traversal.entries.retain(|entry| kept(&entry.path));
            for paths in traversal.omitted.values_mut() {
                paths.retain(|path| kept(path));
            }
            traversal.omitted.retain(|_, paths| !paths.is_empty());
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
        let path = crate::tools::gh_shared::listing_path(
            query.path.as_deref().map_or("", String::as_str),
        )?;
        // A name search, a materialize and a size rank take every level unless
        // the caller bounds them: each selects files, and a folder is not one.
        let depth = match query.max_depth {
            Some(depth) => octocode_engine::num::saturating_usize(depth.get()),
            None if filter.is_some() || query.materialize == Some(true) || ranks_by_size(query) => {
                max_listing_depth()
            }
            None => 1,
        };
        Ok(Self {
            path,
            depth,
            filter,
        })
    }
}

#[cfg(test)]
mod tests;

/// This tool's recovery for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {}
