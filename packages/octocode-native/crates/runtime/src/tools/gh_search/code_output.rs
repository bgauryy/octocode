use super::{GhSearchCodeQuery, GhSearchCodeQueryMatch};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderErrorKind, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::remove_null_fields;
use crate::{
    providers::github::{CodeSearchItem, ProviderError},
    security::scan::ContentScan,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub(super) fn unique_file_count(items: &[CodeSearchItem]) -> usize {
    items
        .iter()
        .map(|item| (&item.repository.full_name, &item.path))
        .collect::<HashSet<_>>()
        .len()
}

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
        Err(error) if error.kind == ProviderErrorKind::NotFound => (
            "findRepository",
            ToolId::GhSearchRepo,
            json!({"keywords":[repo]}),
            "Find the repository by name in case it moved or was renamed.",
            "low",
            "ghRepoNotFound",
        ),
        Ok(metadata)
            if metadata
                .full_name
                .as_ref()
                .is_some_and(|name| !name.eq_ignore_ascii_case(&format!("{owner}/{repo}"))) =>
        {
            let name = metadata.full_name.as_deref().unwrap_or_default();
            let (new_owner, new_repo) = name.split_once('/').unwrap_or((name, ""));
            // Re-run the same normalized query (every filter) against the new
            // name, from page 1.
            let mut next = serde_json::to_value(query).map_err(|error| {
                ProviderError::new(ProviderErrorKind::Decode, error.to_string())
            })?;
            remove_null_fields(&mut next);
            next["owner"] = json!(new_owner);
            next["repo"] = json!(new_repo);
            // From the first page: the schema default is stamped below.
            if let Some(object) = next.as_object_mut() {
                object.remove("page");
            }
            (
                "retryRenamed",
                ToolId::GhSearchCode,
                next,
                "Re-run the same search against the renamed repository.",
                "exact",
                "ghRepoRenamed",
            )
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
            let Some(scope) = query.path.as_deref() else {
                return Ok(());
            };
            (
                "viewStructure",
                ToolId::GhStructure,
                json!({"owner":owner,"repo":repo,"path":scope.as_str()}),
                "Verify that the scoped path exists before concluding absence.",
                "exact",
                "ghScopedZeroUnproven",
            )
        }
    };
    let hint = match code {
        "ghRepoNotFound" => Some("The repository is missing, private, or hidden from this token; check owner/repo spelling and token access.".to_owned()),
        "ghRepoRenamed" => Some(format!("The repository was renamed to {}/{}; run the retryRenamed continuation.", next_query["owner"].as_str().unwrap_or_default(), next_query["repo"].as_str().unwrap_or_default())),
        "ghRepoArchived" => Some("The repository is archived, so its code-search index may lag; verify its structure and search locally.".to_owned()),
        _ => None,
    };
    // A missing, renamed, or archived repository is the answer: say so
    // instead of the generic default-branch note.
    if let Some(hint) = hint {
        diagnostics.add(code, &hint, false);
        value["hints"] = json!([hint]);
    }
    // These are advisory "start a fresh query" actions (renamed repo / a
    // different tool), not next-page continuations of the original search,
    // so stamp page 1 rather than `page + 1`. The canonical continuation
    // contract still requires each tool's defaulted fields, which the
    // hand-built queries above omit: stamp them from the target's schema.
    if let Some(object) = next_query.as_object_mut() {
        crate::contracts::stamp_schema_defaults(
            tool,
            None,
            object,
            &["page", "pageSize", "match", "sort"],
        );
    }
    // Re-running the stale name cannot recover results the renamed repository
    // holds: the renamed query is the same search, so it supersedes `retry`.
    // A repository the token cannot see stays unsearchable on retry.
    if matches!(name, "retryRenamed" | "findRepository")
        && let Some(next) = value.get_mut("next").and_then(Value::as_object_mut)
    {
        next.remove("retry");
    }
    value["next"][name] =
        json!({"tool":tool.as_str(),"query":next_query,"confidence":confidence,"why":why});
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
pub(super) fn read_top_match(value: &Value) -> Option<Value> {
    let file = value["files"].as_array()?.first()?.as_object()?;
    let matched = file.get("matches")?.as_array()?.first()?;
    let text: Vec<u16> = matched["value"].as_str()?.encode_utf16().collect();
    let anchor = &matched["matchIndices"][0];
    let start = usize::try_from(anchor["start"].as_u64()?).ok()?;
    let end = usize::try_from(anchor["end"].as_u64()?).ok()?;
    let token = String::from_utf16(text.get(start..end)?).ok()?;
    if token.trim().is_empty() {
        return None;
    }
    let path = file.get("path")?.as_str()?;
    // GitHub ranking often puts docs, changelogs, and tests first; only a
    // code hit earns medium confidence, and none earns more.
    let confidence = match crate::content::classify_file_type(path) {
        Some(crate::content::FileType::Code) if !crate::content::is_test_path(path) => "medium",
        _ => "low",
    };
    // A cross-tool read states its own reason rather than reusing the
    // search's.
    Some(json!({
        "tool": ToolId::GhGetFileContent.as_str(),
        "confidence": confidence,
        "why": "Read the top hit's matched region; its content is numbered with source lines.",
        "query": {
            "owner": file.get("owner")?,
            "repo": file.get("repo")?,
            "path": file.get("path")?,
            "matchString": token,
            "contextLines": 5,
            "reasoning": "Read the top code hit's matched region.",
        },
    }))
}

/// Line hits for the top files of a repo-scoped `match:"file"` page, read at
/// the requested `branch` or the default-branch HEAD.
pub(super) struct Resolution {
    sha: String,
    reference: Option<String>,
    hits: Vec<super::lines::FileHits>,
}

/// The ref hits are verified at; `None` is the default branch.
fn requested_ref(query: &GhSearchCodeQuery) -> Option<&str> {
    query
        .branch
        .as_deref()
        .map(|branch| branch.trim())
        .filter(|branch| !branch.is_empty())
}

pub(super) async fn resolve_lines<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchCodeQuery,
    items: &[Value],
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Option<Resolution>, ProviderError> {
    let Some(repo) = query.repo.as_deref() else {
        return Ok(None);
    };
    if query.match_ != GhSearchCodeQueryMatch::File
        || query.concise == Some(true)
        || items.is_empty()
    {
        return Ok(None);
    }
    let reference = requested_ref(query);
    let sha = match super::lines::resolve_commit(provider, &query.owner, repo, reference, context)
        .await
    {
        Ok(sha) => sha,
        Err(error) if error.kind == ProviderErrorKind::Cancelled => return Err(error),
        // A named ref that does not resolve is the caller's mistake: never
        // fall back to default-branch lines labeled as that ref.
        Err(error) if reference.is_some() => return Err(error),
        Err(_) => return Ok(None),
    };
    let paths = items
        .iter()
        .take(super::lines::MAX_RESOLVED_FILES)
        .filter_map(|row| row.get("path").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    let hits = super::lines::resolve_files(
        provider,
        &query.owner,
        repo,
        &sha,
        &paths,
        &query.keywords,
        &query.goal,
        context,
        security,
    )
    .await?;
    Ok(Some(Resolution {
        sha,
        reference: reference.map(str::to_owned),
        hits,
    }))
}

/// Shape file rows: resolved files list numbered `lines` instead of index
/// fragments; fragment `matchIndices` stay only under `debug`; a repo-scoped
/// page names owner/repo once. Returns the top hit's read: a line range of
/// the top resolved hit, else `fragment_read` kept inside the verified
/// source scope.
pub(super) fn shape_files(
    value: &mut Value,
    items: &mut [Value],
    query: &GhSearchCodeQuery,
    resolution: Option<Resolution>,
    fragment_read: Option<Value>,
) -> Option<Value> {
    if query.concise == Some(true) {
        return None;
    }
    let fragment_read = scoped_fragment_read(fragment_read, query, resolution.as_ref());
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
    let mut top = None;
    if let Some(resolution) = &resolution {
        value["commitSha"] = json!(resolution.sha);
        if let Some(reference) = &resolution.reference {
            value["ref"] = json!(reference);
        }
        for (row, hits) in items.iter_mut().zip(&resolution.hits) {
            let Some(row) = row.as_object_mut() else {
                continue;
            };
            match hits {
                super::lines::FileHits::Lines {
                    lines,
                    first,
                    last,
                    best,
                    total,
                    line_count,
                } => {
                    if !query.debug {
                        row.shift_remove("matches");
                    }
                    row.insert("lines".into(), json!(lines));
                    if *total > lines.len() {
                        row.insert("hitCount".into(), json!(total));
                    }
                    if top.is_none() {
                        top = Some(line_read(
                            query,
                            row,
                            (*first, *last, *best),
                            *line_count,
                            &resolution.sha,
                        ));
                    }
                }
                super::lines::FileHits::Missing if resolution.reference.is_some() => {
                    // The default-branch snippet is not this ref's content.
                    row.shift_remove("matches");
                    row.insert("atRef".into(), json!(false));
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
        if !query.debug
            && let Some(matches) = row.get_mut("matches").and_then(Value::as_array_mut)
        {
            for matched in matches.iter_mut().filter_map(Value::as_object_mut) {
                matched.shift_remove("matchIndices");
            }
        }
    }
    top.flatten().or(fragment_read)
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
            Some(super::lines::FileHits::Missing) => return None,
            _ => resolution.sha.as_str(),
        },
        None => match requested_ref(query) {
            Some(reference) => reference,
            None => return Some(read),
        },
    };
    read["query"]["branch"] = json!(branch);
    Some(read)
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
    let repo = query.repo.as_deref()?;
    let anchor = if best == 0 { first } else { best };
    let start = anchor.saturating_sub(5).max(1);
    let mut end = anchor.saturating_add(17);
    if anchor == first && last.saturating_sub(first) <= 40 {
        end = end.max(last.saturating_add(3));
    }
    let end = end.min(u32::try_from(line_count).unwrap_or(u32::MAX).max(start));
    let confidence = match crate::content::classify_file_type(path) {
        Some(crate::content::FileType::Code) if !crate::content::is_test_path(path) => "medium",
        _ => "low",
    };
    let read = json!({
        "owner": query.owner.as_str(),
        "repo": repo.as_str(),
        "path": path,
        "startLine": start,
        "endLine": end,
        // The commit the lines were read at: a branch push cannot shift
        // the window before the read runs.
        "branch": sha,
        "reasoning": "Read the top code hit's lines.",
    });
    Some(json!({
        "tool": ToolId::GhGetFileContent.as_str(),
        "confidence": confidence,
        "why": "Read the top hit's lines in context.",
        "query": read,
    }))
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
        ("extension", query.extension.is_some()),
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
        let value = json!({"files": [{"owner": "o", "repo": "r", "path": "src/a.rs",
            "matches": [{"value": "x\nfn find_all() {}", "matchIndices": [{"start": 5, "end": 13, "lineOffset": 1}]}]}]});
        let read = read_top_match(&value).expect("continuation");
        assert_eq!(read["tool"], "ghGetFileContent");
        assert_eq!(read["query"]["matchString"], "find_all");
        assert_eq!(read["query"]["path"], "src/a.rs");
        let concise = json!({"files": ["o/r:src/a.rs"]});
        assert!(read_top_match(&concise).is_none());
    }

    #[test]
    fn read_top_match_confidence_tracks_path_kind() {
        let top = |path: &str| {
            let value = json!({"files": [{"owner": "o", "repo": "r", "path": path,
                "matches": [{"value": "needle", "matchIndices": [{"start": 0, "end": 6}]}]}]});
            read_top_match(&value).expect("continuation")
        };
        assert_eq!(top("src/a.rs")["confidence"], "medium");
        for path in [
            "GUIDE.md",
            "CHANGELOG.md",
            "tests/a.rs",
            "src/a.test.ts",
            "package.json",
        ] {
            assert_eq!(top(path)["confidence"], "low", "{path}");
        }
        assert!(
            !top("src/a.rs")["why"]
                .as_str()
                .unwrap_or_default()
                .contains("with line numbers")
        );
    }
}
