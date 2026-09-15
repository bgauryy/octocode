use super::GhSearchQuery;
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderErrorKind, RequestContext,
};
use crate::{
    providers::github::{CodeSearchItem, ProviderError},
    tools::local_fetch::ContentScan,
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
    query: &GhSearchQuery,
    transport: &GitHubTransport<R>,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let GhSearchQuery::Code {
        owner: Some(owner),
        repo: Some(repo),
        keywords,
        ..
    } = query
    else {
        return Ok(());
    };
    let metadata = transport.repository_metadata(owner, repo, context).await;
    let (name, next_query, why, confidence, code) = match metadata {
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
            json!({"operation":"repositories","keywords":[repo]}),
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
            let mut next = json!({"operation":"code","owner":new_owner,"repo":new_repo});
            if let Some(keywords) = keywords {
                next["keywords"] = json!(keywords);
            }
            (
                "retryRenamed",
                next,
                "Re-run the same search against the renamed repository.",
                "exact",
                "ghRepoRenamed",
            )
        }
        Ok(metadata) if metadata.archived => (
            "viewStructure",
            json!({"operation":"tree","owner":owner,"repo":repo,"path":""}),
            "Inspect the archived repository outside the code-search index.",
            "exact",
            "ghRepoArchived",
        ),
        _ => (
            "viewStructure",
            json!({"operation":"tree","owner":owner,"repo":repo,"path":""}),
            "Verify that the scoped repository and path exist before concluding absence.",
            "exact",
            "ghScopedZeroUnproven",
        ),
    };
    let (_, hint) = match code {
        "findRepository" => ("ghRepoNotFound", "The repository was not found or is private to this token; verify the spelling or discover the current repository name.".to_owned()),
        "retryRenamed" => ("ghRepoRenamed", format!("The repository was renamed to {}/{}; retry against the renamed repository.", next_query["owner"].as_str().unwrap_or_default(), next_query["repo"].as_str().unwrap_or_default())),
        "ghRepoArchived" => ("ghRepoArchived", "The repository is archived, so its code-search index may lag or be incomplete; verify its structure and search locally.".to_owned()),
        _ => ("ghScopedZeroUnproven", "No indexed matches is unproven absence; verify the repository structure and search a bounded local copy before concluding.".to_owned()),
    };
    diagnostics.add(code, &hint, false);
    value["next"][name] =
        json!({"tool":"ghSearch","query":next_query,"confidence":confidence,"why":why});
    Ok(())
}

pub(super) fn files(
    items: &[CodeSearchItem],
    query: &GhSearchQuery,
    security: &impl ContentScan,
) -> Result<Vec<Value>, ProviderError> {
    let GhSearchQuery::Code {
        match_kind,
        concise,
        keywords,
        ..
    } = query
    else {
        return Ok(Vec::new());
    };
    let path_only = match_kind.as_deref() == Some("path");
    let terms = super::ranking::terms(keywords.as_deref().unwrap_or_default())?;
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
