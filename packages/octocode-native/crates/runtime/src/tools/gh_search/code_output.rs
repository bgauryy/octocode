use super::{GhSearchCodeQuery, GhSearchCodeQueryMatch};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderErrorKind, RequestContext,
};
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
            "ghSearchRepo",
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
            next["page"] = json!(1);
            (
                "retryRenamed",
                "ghSearchCode",
                next,
                "Re-run the same search against the renamed repository.",
                "exact",
                "ghRepoRenamed",
            )
        }
        Ok(metadata) if metadata.archived => (
            "viewStructure",
            "ghStructure",
            json!({"owner":owner,"repo":repo,"path":""}),
            "Inspect the archived repository outside the code-search index.",
            "exact",
            "ghRepoArchived",
        ),
        _ => (
            "viewStructure",
            "ghStructure",
            json!({"owner":owner,"repo":repo,"path":""}),
            "Verify that the scoped repository and path exist before concluding absence.",
            "exact",
            "ghScopedZeroUnproven",
        ),
    };
    let hint = match code {
        "ghRepoNotFound" => "The repository is missing, private, or hidden from this token; check owner/repo spelling and token access.".to_owned(),
        "ghRepoRenamed" => format!("The repository was renamed to {}/{}; retry against the renamed repository.", next_query["owner"].as_str().unwrap_or_default(), next_query["repo"].as_str().unwrap_or_default()),
        "ghRepoArchived" => "The repository is archived, so its code-search index may lag or be incomplete; verify its structure and search locally.".to_owned(),
        _ => "No indexed matches is unproven absence; verify the repository structure and search a bounded local copy before concluding.".to_owned(),
    };
    diagnostics.add(code, &hint, false);
    // A repository the token cannot see is the answer: say so instead of the
    // generic default-branch note.
    if code == "ghRepoNotFound" {
        value["hints"] = json!([hint]);
    }
    // These are advisory "start a fresh query" actions (renamed repo / a
    // different tool), not next-page continuations of the original search,
    // so stamp page 1 rather than `page + 1`. The canonical continuation
    // contract still requires each tool's defaulted fields, which the
    // hand-built queries above omit.
    if let Some(object) = next_query.as_object_mut() {
        object.entry("page").or_insert_with(|| json!(1));
        match tool {
            "ghSearchCode" => {
                object.entry("pageSize").or_insert_with(|| json!(30));
                object.entry("match").or_insert_with(|| json!("file"));
            }
            "ghSearchRepo" => {
                object.entry("pageSize").or_insert_with(|| json!(30));
                object.entry("sort").or_insert_with(|| json!("best-match"));
            }
            _ => {
                object.entry("pageSize").or_insert_with(|| json!(100));
            }
        }
    }
    // Re-running the stale name cannot recover results the renamed repository
    // holds: the renamed query is the same search, so it supersedes `retry`.
    // A repository the token cannot see stays unsearchable on retry.
    if matches!(name, "retryRenamed" | "findRepository")
        && let Some(next) = value.get_mut("next").and_then(Value::as_object_mut)
    {
        next.remove("retry");
    }
    value["next"][name] = json!({"tool":tool,"query":next_query,"confidence":confidence,"why":why});
    Ok(())
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
        "tool": "ghGetFileContent",
        "confidence": confidence,
        "why": "Read the top hit's matched region; sourceLineRanges gives its line numbers.",
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
                let mut row =
                    json!({"owner":owner,"repo":repo,"path":matched.path,"matches":values});
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
