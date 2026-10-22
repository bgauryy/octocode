use super::leads::{declaration_read, hits_read, line_read, read_top_match, scoped_fragment_read};
use super::{GhSearchCodeQuery, GhSearchCodeQueryMatchIn};
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Confidence;
use crate::response::result::Continuation;
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::{RepoPath, ensure_next};
use octocode_github::{CodeSearchItem, ProviderError};
use octocode_github::{GitHubTransport, ProviderErrorKind, ProviderErrorReason, RequestContext};
use serde_json::{Value, json};
use std::collections::HashMap;

/// GitHub answers a code search of a login that does not exist with an empty,
/// incomplete page. An empty, incomplete owner-wide search therefore checks
/// the login: a missing one is a not-found failure, not an empty search.
pub(super) async fn owner_exists(
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
pub(super) async fn empty_scope(
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
                    if let Some(read) = super::leads::file_scope_read(query, file.path) {
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
    value["next"][name] = Continuation::new(tool, next_query)
        .confidence(Confidence::Exact)
        .build();
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
    value["next"]["cloneRepo"] = at
        .lead(ToolId::GhCloneRepo)
        .confidence(Confidence::Exact)
        .build();
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
    value["next"]["viewTree"] = at
        .lead(ToolId::GhStructure)
        .confidence(Confidence::Medium)
        .build();
}

/// A `match:"path"` search whose keywords read as code (spaces or syntax
/// characters) rather than path segments.
pub(super) fn path_mode_given_code(query: &GhSearchCodeQuery) -> bool {
    query.match_in == GhSearchCodeQueryMatchIn::Path
        && query.keywords.iter().any(|keyword| {
            keyword
                .trim()
                .chars()
                .any(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')))
        })
}

/// Line hits for the files of a `match:"file"` page: a ref search is read
/// at the requested ref's commit; a default-branch search (repo-scoped or
/// owner-wide) at each hit's indexed commit (from its `html_url`), the
/// commit the index matched.
pub(super) struct Resolution {
    /// The one commit every row was read at; `None` when rows differ (each
    /// row then names its own `commitSha`).
    pub(super) sha: Option<String>,
    pub(super) reference: Option<String>,
    /// Per row: the commit it was read at, and its hits.
    pub(super) hits: Vec<(String, super::lines::FileHits)>,
    /// Files read for the page (core API reads, through the contents
    /// cache): identical blobs at one commit are read once.
    pub(super) reads: usize,
}

/// The owner and repository a row's reads target: the query's when it is
/// repo-scoped, else the row's own.
pub(super) fn row_repo<'a>(
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
pub(super) fn index_commits(
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
pub(super) fn requested_ref(query: &GhSearchCodeQuery) -> Option<&str> {
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
pub(super) async fn line_commit(
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
pub(super) async fn resolve_lines(
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
            super::lines::resolve_files(
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
    let mut hits: Vec<Option<(String, super::lines::FileHits)>> = (0..rows).map(|_| None).collect();
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
            .map(|hit| hit.unwrap_or_else(|| (String::new(), super::lines::FileHits::Unavailable)))
            .collect(),
        reads: read,
    })
}

/// A search pinned to a ref other than the default-branch head: its files
/// come from the default-branch index, so files only at the ref are missing
/// and unresolved rows show default-branch text. Warn with the index commit
/// and lead first to the ref's own listing. `resolved_sha` is the ref's
/// commit when the hit lines were read at it.
pub(super) async fn disclose_index_ref(
    provider: &octocode_github::GitHubProvider,
    value: &mut Value,
    query: &GhSearchCodeQuery,
    resolved_sha: Option<String>,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let (Some(reference), Some(repo)) = (requested_ref(query), query.repo.as_deref()) else {
        return Ok(());
    };
    let owner = query.owner.as_str();
    let head = commit_or_none(provider, owner, repo, None, context).await?;
    let at_ref = match resolved_sha {
        Some(sha) => Some(sha),
        None => commit_or_none(provider, owner, repo, Some(reference), context).await?,
    };
    if head.is_some() && head == at_ref {
        return Ok(());
    }
    let index = head.as_deref().map_or_else(String::new, |sha| {
        format!(" at {}", &sha[..sha.len().min(7)])
    });
    let lines = if resolved_sha_shown(value) {
        format!("; `lines` read at {reference}")
    } else {
        String::new()
    };
    // The per-call facts only: the index commit and the ref. That files
    // only at the ref are missing is the description's, and `viewRepo`
    // lists them.
    let warning = format!("Files from the default-branch index{index}, not {reference}{lines}.");
    push_warnings(value, [warning]);
    // Rows not at the ref were moved or renamed there: search their names
    // across the whole tree (a moved file may leave the searched path).
    let moved = moved_names(value);
    let at = RepoPath {
        owner,
        repo: repo.as_str(),
        path: if moved.is_empty() {
            query.path.as_deref().map_or("", |path| path.as_str())
        } else {
            ""
        },
        reference: Some(at_ref.as_deref().unwrap_or(reference)),
    };
    let mut listing = at.row();
    if !moved.is_empty() {
        listing["include"] = json!(moved);
    }
    let lead = Continuation::new(ToolId::GhStructure, listing).build();
    ensure_next(value);
    if let Some(next) = value.get_mut("next").and_then(Value::as_object_mut) {
        next.shift_insert(0, "viewRepo".to_owned(), lead);
    }
    Ok(())
}

/// File names of the rows absent at the requested ref (`atRef:false`),
/// once each, within the input array limit.
fn moved_names(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for row in value["files"].as_array().into_iter().flatten() {
        if row.get("atRef") != Some(&json!(false)) {
            continue;
        }
        let Some(path) = row.get("path").and_then(Value::as_str) else {
            continue;
        };
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names.truncate(crate::id::MAX_INPUT_ARRAY_ITEMS);
    names
}

/// The commit `reference` (`None`: the default-branch head) names, or
/// `None` when it does not resolve; only cancellation fails.
async fn commit_or_none(
    provider: &octocode_github::GitHubProvider,
    owner: &str,
    repo: &str,
    reference: Option<&str>,
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    match provider
        .resolve_reference(owner, repo, reference, false, context)
        .await
    {
        Ok(sha) => Ok(Some(sha)),
        Err(error) if error.kind == ProviderErrorKind::Cancelled => Err(error),
        Err(_) => Ok(None),
    }
}

/// Some row lists `lines` read at the resolved commit.
fn resolved_sha_shown(value: &Value) -> bool {
    value["files"]
        .as_array()
        .is_some_and(|files| files.iter().any(|row| row.get("lines").is_some()))
}

/// Reads a shaped page carries: the top hit's line range, and one read of
/// every keyword line for each file whose listed hits were capped or cut.
pub(super) struct PageReads {
    pub(super) top: Option<Value>,
    pub(super) hits: Vec<Value>,
}

/// Shape file rows: resolved files list numbered `lines` instead of index
/// fragments; fragment `matchIndices` are verbose (core field class); a repo-scoped
/// page names owner/repo once. The top read is a line range of the top
/// resolved hit, else `fragment_read` kept inside the verified source scope.
/// Content classification needs the normal hit evidence even when the host
/// requests only paths. Ordinary path projections still avoid line reads.
fn reads_lines(query: &GhSearchCodeQuery) -> bool {
    query.match_in == GhSearchCodeQueryMatchIn::File
        && (query.clasify_context.is_some()
            || query.fields.is_empty()
            || picks(query, "lines")
            || picks(query, "matchCount"))
}

/// A `fields` pick names `field`.
fn picks(query: &GhSearchCodeQuery, field: &str) -> bool {
    query.fields.iter().any(|picked| picked.as_str() == field)
}

pub(super) fn shape_files(
    value: &mut Value,
    items: &mut [Value],
    query: &GhSearchCodeQuery,
    resolution: Option<Resolution>,
) -> PageReads {
    let mut hit_reads = Vec::new();
    // One read lead: the top resolved hit's lines; only without them, the
    // index fragment (computed from the top row before it is reshaped).
    let fragment = items.first().cloned();
    let mut top = None;
    if let Some(resolution) = &resolution {
        if let Some(sha) = &resolution.sha {
            value["commitSha"] = json!(sha);
        }
        for (row, (sha, hits)) in items.iter_mut().zip(&resolution.hits) {
            let Some(row) = row.as_object_mut() else {
                continue;
            };
            // Rows read at different commits each name theirs.
            if resolution.sha.is_none()
                && !sha.is_empty()
                && matches!(hits, super::lines::FileHits::Lines { .. })
            {
                row.insert("commitSha".into(), json!(sha));
            }
            match hits {
                super::lines::FileHits::Lines {
                    lines,
                    clipped,
                    first,
                    last,
                    best,
                    total,
                    line_count,
                    declaration,
                    resume_after,
                } => {
                    if !query
                        .extras
                        .contains(&octocode_contract_types::ExtraKind::Details)
                    {
                        row.shift_remove("matches");
                    }
                    row.insert("lines".into(), json!(lines));
                    // A file states its count when it has unlisted hits, or
                    // when a `fields` pick asks for it.
                    if *total > lines.len() || picks(query, "matchCount") {
                        row.insert("matchCount".into(), json!(total));
                    }
                    if (*total > lines.len() || *clipped)
                        && let Some(read) = hits_read(
                            query,
                            row,
                            sha,
                            resume_after.map(|after| (after, *line_count)),
                        )
                    {
                        hit_reads.push(read);
                    }
                    if top.is_none() {
                        top = Some(match declaration {
                            Some(head) => declaration_read(query, row, head, sha),
                            None => line_read(query, row, (*first, *last, *best), *line_count, sha),
                        });
                    }
                }
                super::lines::FileHits::Missing | super::lines::FileHits::Unmatched
                    if resolution.reference.is_some() =>
                {
                    // The path, or its hit, is not at the ref; the
                    // default-branch snippet is not this ref's content.
                    row.shift_remove("matches");
                    row.insert("atRef".into(), json!(false));
                }
                super::lines::FileHits::Unavailable if resolution.reference.is_some() => {
                    // Not read at the ref: drop the default-branch snippet and
                    // lead to the keyword lines at the ref.
                    row.shift_remove("matches");
                    row.insert("lineResolved".into(), json!(false));
                    if let Some(read) = hits_read(query, row, sha, None) {
                        hit_reads.push(read);
                    }
                }
                _ => {
                    row.insert("lineResolved".into(), json!(false));
                }
            }
        }
    }
    for row in items.iter_mut().filter_map(Value::as_object_mut) {
        if query.repo.is_some() {
            row.shift_remove("owner");
            row.shift_remove("repo");
        }
    }
    let top = top.flatten().or_else(|| {
        let read = fragment.as_ref().and_then(read_top_match);
        scoped_fragment_read(read, query, resolution.as_ref())
    });
    PageReads {
        top,
        hits: hit_reads,
    }
}

/// Fields an owner-wide page names once, on the row, when every file row
/// carries the same value: the repository and the commit its lines were
/// read at.
const HOISTED_FIELDS: [&str; 3] = ["owner", "repo", "commitSha"];

/// Move each of [`HOISTED_FIELDS`] that every file row shares to the row
/// itself (a repo-scoped page names owner/repo there already).
pub(super) fn hoist_shared(value: &mut Value, items: &mut [Value]) {
    for field in HOISTED_FIELDS {
        let mut rows = items.iter().map(|row| row.get(field));
        let Some(Some(first)) = rows.next() else {
            continue;
        };
        if value.get(field).is_some() || !rows.all(|other| other == Some(first)) {
            continue;
        }
        let first = first.clone();
        for row in items.iter_mut().filter_map(Value::as_object_mut) {
            row.shift_remove(field);
        }
        value[field] = first;
    }
}

/// Rows whose listed evidence (everything but `path`) is identical, such as
/// the same README in several packages, are listed once: the first keeps the
/// evidence and names every other path in `alsoAt`, in page order.
pub(super) fn merge_identical(items: &mut Vec<Value>) {
    let mut first_of = std::collections::HashMap::new();
    let mut kept: Vec<Value> = Vec::with_capacity(items.len());
    for item in items.drain(..) {
        let evidence = item
            .as_object()
            .filter(|row| row.contains_key("lines") || row.contains_key("matches"))
            .and_then(|row| {
                let mut evidence = row.clone();
                evidence.shift_remove("path")?;
                serde_json::to_string(&evidence).ok()
            });
        let Some(evidence) = evidence else {
            kept.push(item);
            continue;
        };
        match first_of.get(&evidence) {
            Some(&index) => {
                let row: &mut Value = &mut kept[index];
                let path = item["path"].clone();
                match row.get_mut("alsoAt").and_then(Value::as_array_mut) {
                    Some(paths) => paths.push(path),
                    None => {
                        if let Value::Object(fields) = row {
                            let mut rest = std::mem::take(fields);
                            let own = rest.shift_remove("path");
                            fields.extend(own.map(|own| ("path".to_owned(), own)));
                            fields.insert("alsoAt".into(), json!([path]));
                            fields.extend(rest);
                        }
                    }
                }
            }
            None => {
                first_of.insert(evidence, kept.len());
                kept.push(item);
            }
        }
    }
    *items = kept;
}

/// A `path` scope that names one file rather than a directory.
pub(super) struct FileScope<'a> {
    /// The scope without surrounding slashes.
    pub(super) path: &'a str,
    pub(super) parent: &'a str,
    pub(super) name: &'a str,
}

/// The index matches `path` as a directory prefix, so a scope whose last
/// segment looks like a file (an extension that holds a letter, or a known
/// extensionless or dot file such as `Makefile` or `.gitignore`) matches
/// nothing. A dotted directory such as `docs/v1.2` is not a file.
pub(super) fn file_scope(scope: &str) -> Option<FileScope<'_>> {
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
pub(super) fn empty_hints(query: &GhSearchCodeQuery) -> Vec<String> {
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

pub(super) fn files(
    items: &[CodeSearchItem],
    query: &GhSearchCodeQuery,
    security: &impl ContentScan,
) -> Result<Vec<Value>, ProviderError> {
    let GhSearchCodeQuery {
        match_in, keywords, ..
    } = query;
    let path_only = *match_in == GhSearchCodeQueryMatchIn::Path;
    let terms = super::ranking::terms(keywords)?;
    let mut groups: Vec<super::ranking::Group> = Vec::new();
    let mut group_indices = HashMap::new();
    for item in items {
        let group_index = *group_indices
            .entry(item.repository.full_name.clone())
            .or_insert_with(|| {
                groups.push(super::ranking::Group {
                    id: item.repository.full_name.clone(),
                    matches: Vec::new(),
                });
                groups.len() - 1
            });
        let mut matches = Vec::new();
        if !path_only {
            for fragment in &item.text_matches {
                if let Some(value) = super::fragments::project(fragment, &item.path, security)? {
                    matches.push(value);
                }
            }
            if matches.is_empty() {
                matches.push(json!({"pathOnly":true}));
            }
        } else {
            matches.push(json!({}));
        }
        for value in matches {
            let score = super::ranking::score(
                &item.path,
                value["value"].as_str().unwrap_or_default(),
                &terms,
            );
            groups[group_index].matches.push(super::ranking::Match {
                path: item.path.clone(),
                value,
                score,
            });
        }
    }
    super::ranking::sort(&mut groups);
    let mut files: Vec<Value> = Vec::new();
    let mut indices = HashMap::new();
    for group in groups {
        let (owner, repo) = group.id.split_once('/').unwrap_or(("", &group.id));
        for matched in group.matches {
            let key = (group.id.clone(), matched.path.clone());
            let values = if path_only {
                Vec::new()
            } else {
                vec![matched.value]
            };
            if let Some(&index) = indices.get(&key) {
                if let Some(existing) = files
                    .get_mut(index)
                    .and_then(|v: &mut Value| v["matches"].as_array_mut())
                {
                    existing.extend(values);
                }
            } else {
                indices.insert(key, files.len());
                let mut row = json!({"owner":owner,"repo":repo,"path":matched.path});
                // Path matches carry no snippets.
                if !path_only {
                    row["matches"] = json!(values);
                }
                files.push(row);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::super::leads::unsearched_ref_read;
    use super::*;

    /// An owner-wide page whose rows share a repository and commit names
    /// them once on the row; a field the rows disagree on stays per row.
    #[test]
    fn owner_wide_rows_hoist_shared_repository_and_commit() {
        let mut value = json!({});
        let mut items = vec![
            json!({"owner":"o","repo":"r","commitSha":"c1","path":"a.rs"}),
            json!({"owner":"o","repo":"r","commitSha":"c1","path":"b.rs"}),
        ];
        hoist_shared(&mut value, &mut items);
        assert_eq!(value, json!({"owner":"o","repo":"r","commitSha":"c1"}));
        assert_eq!(items, vec![json!({"path":"a.rs"}), json!({"path":"b.rs"})]);
        let mut value = json!({});
        let mut items = vec![
            json!({"owner":"o","repo":"r","commitSha":"c1","path":"a.rs"}),
            json!({"owner":"o","repo":"s","commitSha":"c2","path":"b.rs"}),
            json!({"owner":"o","repo":"s","path":"c.rs"}),
        ];
        hoist_shared(&mut value, &mut items);
        assert_eq!(value, json!({"owner":"o"}));
        assert_eq!(items[0], json!({"repo":"r","commitSha":"c1","path":"a.rs"}));
        assert_eq!(items[2], json!({"repo":"s","path":"c.rs"}));
    }

    /// A repo-scoped page names no owner/repo: they are the query's, and the
    /// shared minimizer would drop the echo.
    #[test]
    fn repo_scoped_pages_carry_no_owner_repo_echo() {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "mainGoal":"test","reasoning":"test","owner":"o","repo":"r","keywords":["x"],"ref":"dev"
        }))
        .expect("query");
        let mut value = json!({});
        let mut items = vec![json!({"owner":"o","repo":"r","path":"src/a.rs","matches":[]})];
        shape_files(&mut value, &mut items, &query, None);
        assert!(value.get("owner").is_none(), "{value}");
        assert!(value.get("repo").is_none(), "{value}");
        // The index is always the default branch (the description says
        // so): no constant `indexRef` echo.
        assert!(value.get("indexRef").is_none(), "{value}");
        assert!(items[0].get("owner").is_none(), "{:?}", items[0]);
        // The empty row's one fact is the ref the index did not cover; a
        // named file at that ref is read there.
        assert_eq!(
            empty_hints(&query),
            ["Code search indexes only the default branch; `dev` was not searched."]
        );
        assert!(unsearched_ref_read(&query).is_none());
        let mut named = query.clone();
        named.filename = Some("Cargo.toml".parse().expect("filename"));
        named.path = Some("crates/x".parse().expect("path"));
        let read = unsearched_ref_read(&named).expect("read at the ref");
        assert_eq!(read["tool"], "ghGetFileContent", "{read}");
        assert_eq!(
            read["query"]["queries"][0]["path"], "crates/x/Cargo.toml",
            "{read}"
        );
        assert_eq!(read["query"]["queries"][0]["ref"], "dev", "{read}");
        let plain: GhSearchCodeQuery = serde_json::from_value(json!({
            "mainGoal":"test","reasoning":"test","owner":"o","keywords":["x"]
        }))
        .expect("query");
        assert!(empty_hints(&plain).is_empty());
        // A requested ref plus a path scope are two hints, never one composed
        // sentence.
        let mut both = query.clone();
        both.path = Some("crates/x".parse().expect("path"));
        let hints = empty_hints(&both);
        assert_eq!(hints.len(), 2, "{hints:?}");
        assert!(
            hints[0].starts_with("Code search indexes only"),
            "{hints:?}"
        );
        assert!(hints[1].starts_with("No indexed match under"), "{hints:?}");
    }

    /// Files whose listed evidence is identical are listed once, naming
    /// every path that shares it; path-only rows and differing rows stay.
    #[test]
    fn identical_files_are_listed_once_with_every_path() {
        let header = json!(["1\t# Octocode"]);
        let mut items = vec![
            json!({"path": "a/README.md", "lines": header}),
            json!({"path": "src/x.rs", "lines": ["3\tfn octocode()"]}),
            json!({"path": "b/README.md", "lines": header}),
            json!({"path": "c/README.md", "lines": header, "matchCount": 40}),
            json!({"path": "d/README.md", "lines": header}),
            json!({"path": "p1"}),
            json!({"path": "p2"}),
        ];
        merge_identical(&mut items);
        assert_eq!(
            items,
            vec![
                json!({"path": "a/README.md", "alsoAt": ["b/README.md", "d/README.md"], "lines": header}),
                json!({"path": "src/x.rs", "lines": ["3\tfn octocode()"]}),
                json!({"path": "c/README.md", "lines": header, "matchCount": 40}),
                json!({"path": "p1"}),
                json!({"path": "p2"}),
            ]
        );
    }
}
