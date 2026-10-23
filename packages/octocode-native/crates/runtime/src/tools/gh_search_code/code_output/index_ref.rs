//! A search pinned to a ref the default-branch index does not cover.
use super::requested_ref;
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Continuation;
use crate::tools::gh_search_code::GhSearchCodeQuery;
use crate::tools::gh_shared::{RepoPath, ensure_next};
use octocode_github::ProviderError;
use octocode_github::{ProviderErrorKind, RequestContext};
use serde_json::{Value, json};

/// A search pinned to a ref other than the default-branch head: its files
/// come from the default-branch index, so files only at the ref are missing
/// and unresolved rows show default-branch text. Warn with the index commit
/// and lead first to the ref's own listing. `resolved_sha` is the ref's
/// commit when the hit lines were read at it.
pub(in crate::tools::gh_search_code) async fn disclose_index_ref(
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
