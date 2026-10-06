use super::{GhSearchCodeQuery, GhSearchCodeQueryMatch};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::{Continuation, continuation_row_mut};
use crate::{
    providers::github::{CodeSearchItem, ProviderError},
    security::scan::ContentScan,
};
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) async fn empty_scope<R: CredentialResolver>(
    value: &mut Value,
    diagnostics: &mut crate::tools::result::ToolDiagnostics,
    query: &GhSearchCodeQuery,
    transport: &GitHubTransport<R>,
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
    let (name, tool, mut next_query, why, confidence, code) = match metadata {
        Err(error)
            if matches!(
                error.kind,
                ProviderErrorKind::Cancelled | ProviderErrorKind::Timeout
            ) =>
        {
            return Err(error);
        }
        // A missing repository is a not-found failure, not an empty
        // search: the shared search failure names repository access.
        Err(error) if error.kind == ProviderErrorKind::NotFound => {
            return Err(error.with_reason(ProviderErrorReason::RepositoryNotFound));
        }
        Ok(metadata) if metadata.archived => (
            "viewStructure",
            ToolId::GhStructure,
            json!({"owner":owner,"repo":repo,"path":""}),
            "Inspect the archived repository outside the code-search index.",
            "exact",
            "ghRepoArchived",
        ),
        // The repository exists: only a scoped path is left to verify.
        _ => {
            let hint = "No indexed matches is unproven absence; verify the repository structure and search a bounded local copy before concluding.";
            diagnostics.add("ghScopedZeroUnproven", hint, false);
            let scope = query.path.as_deref().map_or("", |path| path.as_str());
            (
                "viewStructure",
                ToolId::GhStructure,
                json!({"owner":owner,"repo":repo,"path":scope}),
                "Verify the structure the search covered before concluding absence.",
                "exact",
                "ghScopedZeroUnproven",
            )
        }
    };
    let hint = match code {
        "ghRepoArchived" => Some("The repository is archived, so its code-search index may lag; verify its structure and search locally.".to_owned()),
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
        .why(why)
        .confidence(confidence)
        .build();
    Ok(())
}

/// A `match:"path"` search whose keywords read as code (spaces or syntax
/// characters) rather than path segments.
pub(super) fn path_mode_given_code(query: &GhSearchCodeQuery) -> bool {
    query.match_ == GhSearchCodeQueryMatch::Path
        && query.keywords.iter().any(|keyword| {
            keyword
                .trim()
                .chars()
                .any(|c| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')))
        })
}

/// Code-search fragments carry no line numbers. Point the top hit at an exact
/// ghGetFileContent match read, whose sourceLineRanges carry line numbers.
/// Only a code hit (not docs, changelogs, or tests, which GitHub often ranks
/// first) earns the lead.
fn read_top_match(top: &Value) -> Option<Value> {
    let file = top.as_object()?;
    let matched = file.get("matches")?.as_array()?.first()?;
    let anchor = &matched["matchIndices"][0];
    let start = usize::try_from(anchor["start"].as_u64()?).ok()?;
    let end = usize::try_from(anchor["end"].as_u64()?).ok()?;
    let token = crate::content::utf16_slice(matched["value"].as_str()?, start, end)?;
    if token.trim().is_empty() {
        return None;
    }
    let path = file.get("path")?.as_str()?;
    if !code_hit(path) {
        return None;
    }
    // A cross-tool read states its reason in `why`; its query carries only
    // the brief the caller sent, which the response stage copies.
    Some(
        Continuation::new(
            ToolId::GhGetFileContent,
            json!({
                "owner": file.get("owner")?,
                "repo": file.get("repo")?,
                "path": file.get("path")?,
                "matchString": token,
                "contextLines": 5,
            }),
        )
        .why("Read the top hit's matched region; its content is numbered with source lines.")
        .confidence("medium")
        .build(),
    )
}

/// A source file outside tests: the only hit a top-match read is worth.
fn code_hit(path: &str) -> bool {
    matches!(
        crate::content::classify_file_type(path),
        Some(crate::content::FileType::Code)
    ) && !crate::content::is_test_path(path)
}

/// Line hits for the files of a `match:"file"` page: a repo-scoped page is
/// read at the requested ref or the default-branch HEAD; an owner-wide page
/// at each hit's indexed commit (from its `html_url`).
pub(super) struct Resolution {
    /// The one commit every row was read at; `None` when rows differ (each
    /// row then names its own `commitSha`).
    pub(super) sha: Option<String>,
    reference: Option<String>,
    /// Per row: the commit it was read at, and its hits.
    hits: Vec<(String, super::lines::FileHits)>,
}

/// The owner and repository a row's reads target: the query's when it is
/// repo-scoped, else the row's own.
fn row_repo<'a>(
    query: &'a GhSearchCodeQuery,
    row: &'a serde_json::Map<String, Value>,
) -> Option<(&'a str, &'a str)> {
    match query.repo.as_deref() {
        Some(repo) => Some((query.owner.as_str(), repo.as_str())),
        None => Some((row.get("owner")?.as_str()?, row.get("repo")?.as_str()?)),
    }
}

/// `(owner, repo, commit)` an owner-wide row is read at.
type RepoCommit = (String, String, String);

/// The commit a code-search hit was indexed at: `html_url` is
/// `…/blob/<sha>/<path>`.
fn indexed_commit(html_url: &str) -> Option<String> {
    let (_, rest) = html_url.split_once("/blob/")?;
    let sha = rest.split('/').next()?;
    (sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| sha.to_ascii_lowercase())
}

/// The ref hits are verified at; `None` is the default branch.
fn requested_ref(query: &GhSearchCodeQuery) -> Option<&str> {
    query
        .ref_
        .as_deref()
        .map(|branch| branch.trim())
        .filter(|branch| !branch.is_empty())
}

/// The commit a page's hit lines are read at, resolved while the index
/// search runs (`None`: the page lists no lines — owner-wide, path-only,
/// or concise).
pub(super) async fn line_commit<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
) -> Option<Result<String, ProviderError>> {
    let repo = query.repo.as_deref()?;
    if query.match_ != GhSearchCodeQueryMatch::File || query.concise == Some(true) {
        return None;
    }
    Some(
        super::lines::resolve_commit(provider, &query.owner, repo, requested_ref(query), context)
            .await,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve_lines<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchCodeQuery,
    items: &[Value],
    found: &[CodeSearchItem],
    commit: Option<Result<String, ProviderError>>,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Option<Resolution>, ProviderError> {
    if items.is_empty() {
        return Ok(None);
    }
    let Some(repo) = query.repo.as_deref() else {
        return owner_wide_lines(provider, query, items, found, context, security).await;
    };
    let Some(commit) = commit else {
        return Ok(None);
    };
    let reference = requested_ref(query);
    let sha = match commit {
        Ok(sha) => sha,
        Err(error) if error.kind == ProviderErrorKind::Cancelled => return Err(error),
        // A named ref that does not resolve is the caller's mistake: never
        // fall back to default-branch lines labeled as that ref.
        Err(error) if reference.is_some() => return Err(error),
        Err(_) => return Ok(None),
    };
    // Every row is read (through the contents cache): an index fragment
    // carries no line numbers, and at a ref it is default-branch text.
    let paths = items
        .iter()
        .filter_map(|row| row.get("path").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    let hits = super::lines::resolve_files(
        provider,
        &query.owner,
        repo,
        &sha,
        &paths,
        &query.keywords,
        // Without a mainGoal the keywords alone pick the best hit.
        query.main_goal.as_ref().map_or("", |goal| goal.as_str()),
        context,
        security,
    )
    .await?;
    Ok(Some(Resolution {
        hits: hits.into_iter().map(|hits| (sha.clone(), hits)).collect(),
        sha: Some(sha),
        reference: reference.map(str::to_owned),
    }))
}

/// Line hits of an owner-wide `match:"file"` page: each row is read at the
/// commit its hit was indexed at, one concurrent batch per repository and
/// commit. A row without an indexed commit keeps its fragments.
async fn owner_wide_lines<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchCodeQuery,
    items: &[Value],
    found: &[CodeSearchItem],
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Option<Resolution>, ProviderError> {
    if query.match_ != GhSearchCodeQueryMatch::File || query.concise == Some(true) {
        return Ok(None);
    }
    let commits: HashMap<(&str, &str), String> = found
        .iter()
        .filter_map(|item| {
            let sha = indexed_commit(&item.html_url)?;
            Some((
                (item.repository.full_name.as_str(), item.path.as_str()),
                sha,
            ))
        })
        .collect();
    // (owner, repo, sha) -> the rows (index, path) read there.
    let mut groups: Vec<(RepoCommit, Vec<(usize, String)>)> = Vec::new();
    for (index, row) in items.iter().enumerate() {
        let (Some(owner), Some(repo), Some(path)) = (
            row.get("owner").and_then(Value::as_str),
            row.get("repo").and_then(Value::as_str),
            row.get("path").and_then(Value::as_str),
        ) else {
            continue;
        };
        let full_name = format!("{owner}/{repo}");
        let Some(sha) = commits.get(&(full_name.as_str(), path)) else {
            continue;
        };
        let key = (owner.to_owned(), repo.to_owned(), sha.clone());
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, rows)) => rows.push((index, path.to_owned())),
            None => groups.push((key, vec![(index, path.to_owned())])),
        }
    }
    if groups.is_empty() {
        return Ok(None);
    }
    let goal = query.main_goal.as_ref().map_or("", |goal| goal.as_str());
    let reads = groups.iter().map(|((owner, repo, sha), rows)| async move {
        let paths = rows
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        super::lines::resolve_files(
            provider,
            owner,
            repo,
            sha,
            &paths,
            &query.keywords,
            goal,
            context,
            security,
        )
        .await
    });
    let resolved = futures_util::future::join_all(reads).await;
    let mut hits: Vec<Option<(String, super::lines::FileHits)>> =
        (0..items.len()).map(|_| None).collect();
    for (((_, _, sha), rows), result) in groups.iter().zip(resolved) {
        for ((index, _), file) in rows.iter().zip(result?) {
            hits[*index] = Some((sha.clone(), file));
        }
    }
    let hits: Vec<(String, super::lines::FileHits)> = hits
        .into_iter()
        .map(|hit| hit.unwrap_or_else(|| (String::new(), super::lines::FileHits::Unavailable)))
        .collect();
    let mut shas = hits
        .iter()
        .map(|(sha, _)| sha.as_str())
        .filter(|sha| !sha.is_empty());
    let first = shas.next().map(str::to_owned);
    let uniform = first.filter(|sha| shas.all(|other| other == sha));
    Ok(Some(Resolution {
        sha: uniform,
        reference: None,
        hits,
    }))
}

/// A search pinned to a ref other than the default-branch head: its files
/// come from the default-branch index, so files only at the ref are missing
/// and unresolved rows show default-branch text. Warn with the index commit
/// and lead first to the ref's own listing. `resolved_sha` is the ref's
/// commit when the hit lines were read at it.
pub(super) async fn disclose_index_ref<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
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
        format!("; only `lines` were read at {reference}")
    } else {
        String::new()
    };
    let warning = format!(
        "Results come from the default-branch index{index}, not {reference}{lines}. Files only at {reference} are missing: hints.viewRepo lists it."
    );
    match value.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.push(json!(warning)),
        None => value["warnings"] = json!([warning]),
    }
    let mut listing = json!({
        "owner": owner,
        "repo": repo.as_str(),
        "path": query.path.as_deref().map_or("", |path| path.as_str()),
        "ref": at_ref.as_deref().unwrap_or(reference),
    });
    // Rows not at the ref were moved or renamed there: search their names
    // across the whole tree (a moved file may leave the searched path).
    let moved = moved_names(value);
    if !moved.is_empty() {
        listing["include"] = json!(moved);
        if let Some(listing) = listing.as_object_mut() {
            listing.shift_remove("path");
        }
    }
    let lead = Continuation::new(ToolId::GhStructure, listing)
        .why("List the requested ref; the code index covers only the default branch.")
        .build();
    if !value.get("next").is_some_and(Value::is_object) {
        value["next"] = json!({});
    }
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
    names.truncate(crate::tools::id::MAX_INPUT_ARRAY_ITEMS);
    names
}

/// The commit `reference` (`None`: the default-branch head) names, or
/// `None` when it does not resolve; only cancellation fails.
async fn commit_or_none<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    reference: Option<&str>,
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    match super::lines::resolve_commit(provider, owner, repo, reference, context).await {
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
pub(super) fn shape_files(
    value: &mut Value,
    items: &mut [Value],
    query: &GhSearchCodeQuery,
    resolution: Option<Resolution>,
) -> PageReads {
    let mut hit_reads = Vec::new();
    if query.concise == Some(true) {
        return PageReads {
            top: None,
            hits: hit_reads,
        };
    }
    let reference = requested_ref(query);
    if query.repo.is_some() {
        value["owner"] = json!(query.owner.as_str());
        value["repo"] = json!(query.repo.as_deref().map(|repo| repo.as_str()));
    }
    if reference.is_some() {
        // Candidates come from the default-branch index; only `lines` were
        // read at the requested ref.
        value["indexRef"] = json!("defaultBranch");
    }
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
                } => {
                    if !query.debug {
                        row.shift_remove("matches");
                    }
                    row.insert("lines".into(), json!(lines));
                    if *total > lines.len() {
                        row.insert("hitCount".into(), json!(total));
                    }
                    if (*total > lines.len() || *clipped)
                        && let Some(read) = hits_read(query, row, sha)
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
                    if let Some(read) = hits_read(query, row, sha) {
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

/// ghGetFileContent read of every keyword line of one file at the resolved
/// commit, whole and numbered: the lossless continuation of a hit list that
/// was capped or cut to keyword windows. The same case-insensitive literal
/// any-of match as the listing.
fn hits_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    sha: &str,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    let keywords: Vec<&str> = query
        .keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .collect();
    let max = crate::tools::id::query_limits::gh_get_file_content::MATCH_STRING_MAX_ITEMS;
    let mut read = json!({
        "owner": owner,
        "repo": repo,
        "path": path,
        "ref": sha,
        "contextLines": 0,
    });
    match keywords.as_slice() {
        [] => return None,
        [one] => read["matchString"] = json!(one),
        many if many.len() <= max => read["matchString"] = json!(many),
        // More keywords than a matchString list holds: one escaped
        // alternation matches the same lines.
        many => {
            read["matchString"] = json!(
                many.iter()
                    .map(|keyword| regex::escape(keyword))
                    .collect::<Vec<_>>()
                    .join("|")
            );
            read["regex"] = json!("rust");
        }
    }
    Some(
        Continuation::new(ToolId::GhGetFileContent, read)
            .why("Read every keyword line of this file whole.")
            .build(),
    )
}

/// The index-fragment read of the top file, kept inside the verified source
/// scope: pinned to the resolved commit, dropped when the file is absent
/// there, and naming the requested ref when nothing was resolved. It never
/// silently reads the default branch for a requested ref.
fn scoped_fragment_read(
    read: Option<Value>,
    query: &GhSearchCodeQuery,
    resolution: Option<&Resolution>,
) -> Option<Value> {
    let mut read = read?;
    let branch = match resolution {
        Some(resolution) => match resolution.hits.first() {
            Some((_, super::lines::FileHits::Missing)) => return None,
            // At a ref, an unmatched or unread top file has no fragment read:
            // its fragment is default-branch text.
            Some((_, super::lines::FileHits::Unmatched | super::lines::FileHits::Unavailable))
                if resolution.reference.is_some() =>
            {
                return None;
            }
            // An owner-wide row without an indexed commit reads the
            // default branch.
            Some((sha, _)) if sha.is_empty() => return Some(read),
            Some((sha, _)) => sha.as_str(),
            None => return Some(read),
        },
        None => match requested_ref(query) {
            Some(reference) => reference,
            None => return Some(read),
        },
    };
    if let Some(row) = continuation_row_mut(&mut read) {
        row["ref"] = json!(branch);
    }
    Some(read)
}

/// ghGetFileContent read of the whole declaration the best hit names: its
/// unique head line as `matchString` with `block`, at the resolved commit.
/// A fixed line window would cut a function longer than the window.
fn declaration_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    head: &str,
    sha: &str,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    if !code_hit(path) {
        return None;
    }
    let read = json!({
        "owner": owner,
        "repo": repo,
        "path": path,
        "matchString": head,
        "block": true,
        "ref": sha,
    });
    Some(
        Continuation::new(ToolId::GhGetFileContent, read)
            .why("Read the top hit's whole declaration.")
            .confidence("medium")
            .build(),
    )
}

/// ghGetFileContent read of a resolved file's best hit for the goal (5 lines
/// before it); a read anchored on the first hit widens to the last hit when
/// that is close.
fn line_read(
    query: &GhSearchCodeQuery,
    row: &serde_json::Map<String, Value>,
    (first, last, best): (u32, u32, u32),
    line_count: usize,
    sha: &str,
) -> Option<Value> {
    let path = row.get("path")?.as_str()?;
    let (owner, repo) = row_repo(query, row)?;
    let anchor = if best == 0 { first } else { best };
    let start = anchor.saturating_sub(5).max(1);
    let mut end = anchor.saturating_add(17);
    if anchor == first && last.saturating_sub(first) <= 40 {
        end = end.max(last.saturating_add(3));
    }
    let end = end.min(u32::try_from(line_count).unwrap_or(u32::MAX).max(start));
    if !code_hit(path) {
        return None;
    }
    let read = json!({
        "owner": owner,
        "repo": repo,
        "path": path,
        // The published line-span spelling (startLine/endLine are accepted
        // but not published).
        "ranges": [format!("{start}-{end}")],
        // The commit the lines were read at: a branch push cannot shift
        // the window before the read runs.
        "ref": sha,
    });
    Some(
        Continuation::new(ToolId::GhGetFileContent, read)
            .why("Read the top hit's lines in context.")
            .confidence("medium")
            .build(),
    )
}

/// The cause of an empty code search: the index scope (default branch),
/// path matching, qualifiers, or owner-wide scope.
pub(super) fn empty_hint(query: &GhSearchCodeQuery) -> String {
    if let Some(reference) = requested_ref(query) {
        return format!(
            "Code search indexes only the default branch; `{reference}` was not searched. Read it with ghGetFileContent."
        );
    }
    if query.match_ == GhSearchCodeQueryMatch::Path {
        return "No path contains every keyword; use fewer keywords, or match:\"file\" for contents."
            .into();
    }
    let qualifiers = [
        ("path", query.path.is_some()),
        ("extensions", !query.extensions.is_empty()),
        ("filename", query.filename.is_some()),
        ("language", query.language.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, set)| set.then_some(name))
    .collect::<Vec<_>>();
    if !qualifiers.is_empty() {
        return format!(
            "No indexed match with {}; drop a qualifier or a keyword to broaden.",
            qualifiers.join("/")
        );
    }
    match query.repo.as_deref() {
        Some(repo) => format!(
            "No indexed match in {}/{}; try fewer or shorter keywords.",
            query.owner.as_str(),
            repo.as_str()
        ),
        None => format!(
            "No indexed match in any {} repository; check spelling, or set repo.",
            query.owner.as_str()
        ),
    }
}

pub(super) fn files(
    items: &[CodeSearchItem],
    query: &GhSearchCodeQuery,
    security: &impl ContentScan,
) -> Result<Vec<Value>, ProviderError> {
    let GhSearchCodeQuery {
        match_,
        concise,
        keywords,
        ..
    } = query;
    let path_only = *match_ == GhSearchCodeQueryMatch::Path;
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
    super::ranking::sort(&mut groups)?;
    let mut timestamps = HashMap::new();
    for item in items {
        timestamps
            .entry((item.repository.full_name.clone(), item.path.clone()))
            .or_insert_with(|| item.last_modified_at.clone());
    }
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
                if let Some(stamp) = timestamps
                    .get(&(group.id.clone(), matched.path.clone()))
                    .and_then(|value| value.as_ref())
                {
                    row["lastModifiedAt"] = json!(stamp);
                }
                files.push(row);
            }
        }
    }
    if *concise == Some(true) {
        return Ok(files
            .iter()
            .map(|file| {
                json!(format!(
                    "{}/{}:{}",
                    file["owner"].as_str().unwrap_or_default(),
                    file["repo"].as_str().unwrap_or_default(),
                    file["path"].as_str().unwrap_or_default()
                ))
            })
            .collect());
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_hit_reads_its_matched_token() {
        let top = json!({"owner": "o", "repo": "r", "path": "src/a.rs",
            "matches": [{"value": "x\nfn find_all() {}", "matchIndices": [{"start": 5, "end": 13, "lineOffset": 1}]}]});
        let read = read_top_match(&top).expect("continuation");
        assert_eq!(read["tool"], "ghGetFileContent");
        assert_eq!(read["query"]["queries"][0]["matchString"], "find_all");
        assert_eq!(read["query"]["queries"][0]["path"], "src/a.rs");
        assert!(read_top_match(&json!("o/r:src/a.rs")).is_none());
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
            json!({"path": "c/README.md", "lines": header, "hitCount": 40}),
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
                json!({"path": "c/README.md", "lines": header, "hitCount": 40}),
                json!({"path": "p1"}),
                json!({"path": "p2"}),
            ]
        );
    }

    /// Only a source hit earns the top-match read; docs, changelogs, tests
    /// and manifests (often ranked first) get none.
    #[test]
    fn read_top_match_is_offered_for_code_hits_only() {
        let top = |path: &str| {
            read_top_match(&json!({"owner": "o", "repo": "r", "path": path,
                "matches": [{"value": "needle", "matchIndices": [{"start": 0, "end": 6}]}]}))
        };
        assert!(top("src/a.rs").is_some());
        for path in [
            "GUIDE.md",
            "CHANGELOG.md",
            "tests/a.rs",
            "src/a.test.ts",
            "package.json",
        ] {
            assert!(top(path).is_none(), "{path}");
        }
    }
}
