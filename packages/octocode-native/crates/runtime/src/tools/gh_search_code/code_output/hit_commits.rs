//! The commits a page's hits are read at, and the line hits read there.
use super::reads_lines;
use crate::security::scan::ContentScan;
use crate::tools::gh_search_code::GhSearchCodeQuery;
use octocode_github::{CodeSearchItem, ProviderError};
use octocode_github::{ProviderErrorKind, RequestContext};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Line hits for the files of a `match:"file"` page: a ref search is read
/// at the requested ref's commit; a default-branch search (repo-scoped or
/// owner-wide) at each hit's indexed commit (from its `html_url`), the
/// commit the index matched.
pub(in crate::tools::gh_search_code) struct Resolution {
    /// The one commit every row was read at; `None` when rows differ (each
    /// row then names its own `commitSha`).
    pub(in crate::tools::gh_search_code) sha: Option<String>,
    pub(in crate::tools::gh_search_code) reference: Option<String>,
    /// Per row: the commit it was read at, and its hits.
    pub(in crate::tools::gh_search_code) hits:
        Vec<(String, crate::tools::gh_search_code::lines::FileHits)>,
    /// Files read for the page (core API reads, through the contents
    /// cache): identical blobs at one commit are read once.
    pub(in crate::tools::gh_search_code) reads: usize,
}

/// The owner and repository a row's reads target: the query's when it is
/// repo-scoped, else the row's own.
pub(in crate::tools::gh_search_code) fn row_repo<'a>(
    query: &'a GhSearchCodeQuery,
    row: &'a serde_json::Map<String, Value>,
) -> Option<(&'a str, &'a str)> {
    match query.repo.as_deref() {
        Some(repo) => Some((query.owner.as_str(), repo.as_str())),
        None => Some((row.get("owner")?.as_str()?, row.get("repo")?.as_str()?)),
    }
}

/// `(owner, repo, commit)` a group of rows is read at.
type RepoCommit = (String, String, String);

/// The commit a code-search hit was indexed at: `html_url` is
/// `…/blob/<sha>/<path>`.
fn indexed_commit(html_url: &str) -> Option<String> {
    let (_, rest) = html_url.split_once("/blob/")?;
    let sha = rest.split('/').next()?;
    octocode_github::is_full_sha(sha).then(|| sha.to_ascii_lowercase())
}

/// The commit each row of a page that read no lines (a path-only search, a
/// `fields` pick without `lines`) was indexed at, from its hit's `html_url`:
/// `data.commitSha` when every row shares one, else each row's own, so a
/// read of a listed path is pinned to the commit the index matched. A `ref`
/// search's rows name no index commit (the index is the default branch's).
pub(in crate::tools::gh_search_code) fn index_commits(
    value: &mut Value,
    items: &mut [Value],
    found: &[CodeSearchItem],
    query: &GhSearchCodeQuery,
) {
    if requested_ref(query).is_some() {
        return;
    }
    let commit_of = |row: &serde_json::Map<String, Value>| {
        let (owner, repo) = row_repo(query, row)?;
        let path = row.get("path")?.as_str()?;
        let full_name = format!("{owner}/{repo}");
        found
            .iter()
            .find(|item| {
                item.path == path && item.repository.full_name.eq_ignore_ascii_case(&full_name)
            })
            .and_then(|item| indexed_commit(&item.html_url))
    };
    let commits = items
        .iter()
        .map(|row| row.as_object().and_then(commit_of))
        .collect::<Vec<_>>();
    match commits.first() {
        Some(Some(first)) if commits.iter().all(|commit| commit.as_ref() == Some(first)) => {
            value["commitSha"] = json!(first);
        }
        _ => {
            for (row, commit) in items.iter_mut().zip(commits) {
                if let (Some(row), Some(commit)) = (row.as_object_mut(), commit) {
                    row.insert("commitSha".into(), json!(commit));
                }
            }
        }
    }
}

/// The ref hits are verified at; `None` is the default branch.
pub(in crate::tools::gh_search_code) fn requested_ref(query: &GhSearchCodeQuery) -> Option<&str> {
    query
        .ref_
        .as_deref()
        .map(|branch| branch.trim())
        .filter(|branch| !branch.is_empty())
}

/// The commit a ref search's hit lines are read at, resolved while the
/// index search runs (`None`: a default-branch search, read at its indexed
/// commits, or a page that lists no lines — owner-wide, path-only, or a
/// `fields` pick without `lines`).
pub(in crate::tools::gh_search_code) async fn line_commit(
    provider: &octocode_github::GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
) -> Option<Result<String, ProviderError>> {
    let repo = query.repo.as_deref()?;
    let reference = requested_ref(query)?;
    if !reads_lines(query) {
        return None;
    }
    Some(
        provider
            .resolve_reference(&query.owner, repo, Some(reference), false, context)
            .await,
    )
}

/// The page's line hits, read at the requested ref's commit (a ref search)
/// or at each hit's indexed commit; `None` when the page lists no lines.
pub(in crate::tools::gh_search_code) async fn resolve_lines(
    provider: &octocode_github::GitHubProvider,
    query: &GhSearchCodeQuery,
    items: &[Value],
    found: &[CodeSearchItem],
    commit: Option<Result<String, ProviderError>>,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Option<Resolution>, ProviderError> {
    if items.is_empty() || !reads_lines(query) {
        return Ok(None);
    }
    let Some(reference) = requested_ref(query).filter(|_| query.repo.is_some()) else {
        return indexed_lines(provider, query, items, found, context, security).await;
    };
    let Some(commit) = commit else {
        return Ok(None);
    };
    // A named ref that does not resolve is the caller's mistake: never fall
    // back to default-branch lines labeled as that ref.
    let sha = commit?;
    // Every row is read at the ref (through the contents cache): an index
    // fragment carries no line numbers, and it is default-branch text.
    let reads = items
        .iter()
        .enumerate()
        .filter_map(|(index, row)| Some((index, row.get("path")?.as_str()?.to_owned(), None)))
        .collect::<Vec<_>>();
    let (owner, repo) = (
        query.owner.as_str(),
        query.repo.as_deref().map_or("", |repo| repo.as_str()),
    );
    let key = (owner.to_owned(), repo.to_owned(), sha.clone());
    let mut resolution = read_groups(
        provider,
        query,
        items.len(),
        vec![(key, reads)],
        context,
        security,
    )
    .await?;
    resolution.sha = Some(sha);
    resolution.reference = Some(reference.to_owned());
    Ok(Some(resolution))
}

/// Line hits of a default-branch `match:"file"` page: each row is read at
/// the commit its hit was indexed at, one concurrent batch per repository
/// and commit. A repo-scoped row without an indexed commit is read at the
/// default-branch head (resolved only then); an owner-wide one keeps its
/// fragments.
async fn indexed_lines(
    provider: &octocode_github::GitHubProvider,
    query: &GhSearchCodeQuery,
    items: &[Value],
    found: &[CodeSearchItem],
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Option<Resolution>, ProviderError> {
    // (repository, path) -> (indexed commit, blob SHA).
    let indexed: HashMap<(String, &str), (Option<String>, &str)> = found
        .iter()
        .map(|item| {
            (
                (item.repository.full_name.to_lowercase(), item.path.as_str()),
                (indexed_commit(&item.html_url), item.sha.as_str()),
            )
        })
        .collect();
    let located = |row| locate(&indexed, row);
    let head = match query.repo.as_deref() {
        Some(repo)
            if items
                .iter()
                .any(|row| located(row).is_none_or(|(.., commit, _)| commit.is_none())) =>
        {
            match provider
                .resolve_reference(&query.owner, repo, None, false, context)
                .await
            {
                Ok(sha) => Some(sha),
                Err(error) if error.kind == ProviderErrorKind::Cancelled => return Err(error),
                Err(_) => None,
            }
        }
        _ => None,
    };
    let mut groups: Vec<(RepoCommit, Vec<Read>)> = Vec::new();
    for (index, row) in items.iter().enumerate() {
        let (owner, repo, path, commit, blob) = match located(row) {
            Some(found) => found,
            None => match (
                query.repo.as_deref(),
                row.get("path").and_then(Value::as_str),
            ) {
                (Some(repo), Some(path)) => (query.owner.as_str(), repo.as_str(), path, None, ""),
                _ => continue,
            },
        };
        // One indexed blob is read once, whichever paths hold it; a row
        // read at the head may hold other content than the index saw.
        let extension = path.rsplit_once('.').map_or("", |(_, extension)| extension);
        let blob = (commit.is_some() && !blob.is_empty()).then(|| format!("{blob}.{extension}"));
        let Some(sha) = commit.or_else(|| head.clone()) else {
            continue;
        };
        let key = (owner.to_owned(), repo.to_owned(), sha);
        let read = (index, path.to_owned(), blob);
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, reads)) => reads.push(read),
            None => groups.push((key, vec![read])),
        }
    }
    if groups.is_empty() {
        return Ok(None);
    }
    let mut resolution =
        read_groups(provider, query, items.len(), groups, context, security).await?;
    let mut shas = resolution
        .hits
        .iter()
        .map(|(sha, _)| sha.as_str())
        .filter(|sha| !sha.is_empty());
    let first = shas.next().map(str::to_owned);
    resolution.sha = first.filter(|sha| shas.all(|other| other == sha));
    Ok(Some(resolution))
}

/// A row's repository and path, with the commit its hit was indexed at
/// and the blob it holds.
fn locate<'r, 'f>(
    indexed: &HashMap<(String, &'f str), (Option<String>, &'f str)>,
    row: &'r Value,
) -> Option<(&'r str, &'r str, &'r str, Option<String>, &'f str)> {
    let (owner, repo, path) = (
        row.get("owner")?.as_str()?,
        row.get("repo")?.as_str()?,
        row.get("path")?.as_str()?,
    );
    let (commit, blob) = indexed.get(&(format!("{owner}/{repo}").to_lowercase(), path))?;
    Some((owner, repo, path, commit.clone(), *blob))
}

/// One row's read: its index on the page, its path, and the key of the
/// blob it holds (`<blob sha>.<extension>`) when the index named one.
type Read = (usize, String, Option<String>);

/// Reads each group's files at its commit, concurrently, once per distinct
/// blob, and gives every row its hits (a row no group reads keeps its
/// fragments: `Unavailable` at no commit).
async fn read_groups(
    provider: &octocode_github::GitHubProvider,
    query: &GhSearchCodeQuery,
    rows: usize,
    groups: Vec<(RepoCommit, Vec<Read>)>,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<Resolution, ProviderError> {
    let goal = query.main_goal.as_ref().map_or("", |goal| goal.as_str());
    // Per group: the distinct paths read, and for each row the read it shares.
    let plans = groups
        .iter()
        .map(|(_, reads)| {
            let mut paths: Vec<String> = Vec::new();
            let mut blobs: Vec<Option<&str>> = Vec::new();
            let shared = reads
                .iter()
                .map(|(index, path, blob)| {
                    let at = blob
                        .as_deref()
                        .and_then(|blob| blobs.iter().position(|seen| *seen == Some(blob)))
                        .unwrap_or_else(|| {
                            paths.push(path.clone());
                            blobs.push(blob.as_deref());
                            paths.len() - 1
                        });
                    (*index, at)
                })
                .collect::<Vec<_>>();
            (paths, shared)
        })
        .collect::<Vec<_>>();
    let reads = groups
        .iter()
        .zip(&plans)
        .map(|(((owner, repo, sha), _), (paths, _))| {
            crate::tools::gh_search_code::lines::resolve_files(
                provider,
                owner,
                repo,
                sha,
                paths,
                &query.keywords,
                // Without a mainGoal the keywords alone pick the best hit.
                goal,
                context,
                security,
            )
        });
    let resolved = futures_util::future::join_all(reads).await;
    let mut hits: Vec<Option<(String, crate::tools::gh_search_code::lines::FileHits)>> =
        (0..rows).map(|_| None).collect();
    let mut read = 0;
    for (((_, _, sha), _), ((paths, shared), result)) in
        groups.iter().zip(plans.iter().zip(resolved))
    {
        let files = result?;
        read += paths.len();
        for (index, at) in shared {
            hits[*index] = Some((sha.clone(), files[*at].clone()));
        }
    }
    Ok(Resolution {
        sha: None,
        reference: None,
        hits: hits
            .into_iter()
            .map(|hit| {
                hit.unwrap_or_else(|| {
                    (
                        String::new(),
                        crate::tools::gh_search_code::lines::FileHits::Unavailable,
                    )
                })
            })
            .collect(),
        reads: read,
    })
}
