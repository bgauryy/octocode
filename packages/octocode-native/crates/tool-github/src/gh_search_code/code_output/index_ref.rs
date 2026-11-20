//! A search pinned to a ref the default-branch index does not cover.
use super::requested_ref;
use octocode_contract_types::{
    GhSearchCodeQuery, GhStructureQuery, GhStructureQueryTree, GhStructureQueryTreeOperation,
};
use octocode_github::ProviderError;
use octocode_github::{ProviderErrorKind, RequestContext};
use octocode_runtime_base::response::paging::push_warnings;
use octocode_runtime_base::response::result::Continuation;
use octocode_tools_shared::gh_shared::ensure_calls;
use serde_json::{Value, json};
use std::num::NonZeroU64;

/// The default-branch head a ref search's index warning names: the one the
/// walk's first page read (`indexHead`, carried by the continuation), else
/// asked now. `None` for a search without a ref or a head GitHub lacks.
pub(in crate::gh_search_code) async fn index_head(
    provider: &octocode_github::GitHubProvider,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    let (Some(_), Some(repo)) = (requested_ref(query), query.repo.as_deref()) else {
        return Ok(None);
    };
    if let Some(head) = &query.index_head {
        return Ok(Some(head.to_string()));
    }
    commit_or_none(provider, query.owner.as_str(), repo, None, context).await
}

/// A search pinned to a ref other than the default-branch head: its files
/// come from the default-branch index, so files only at the ref are missing
/// and unresolved rows show default-branch text. Warn with the index commit
/// and lead first to the ref's own listing. `resolved_sha` is the ref's
/// commit when the hit lines were read at it; `head` is the default-branch
/// head ([`index_head`]).
pub(in crate::gh_search_code) async fn disclose_index_ref(
    provider: &octocode_github::GitHubProvider,
    value: &mut Value,
    query: &GhSearchCodeQuery,
    resolved_sha: Option<String>,
    head: Option<String>,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let (Some(reference), Some(repo)) = (requested_ref(query), query.repo.as_deref()) else {
        return Ok(());
    };
    let owner = query.owner.as_str();
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
        format!("; `matches` read at {reference}")
    } else {
        String::new()
    };
    // The index commit, the ref, and what that leaves unsearched: a file that
    // exists only at the ref is not a candidate, so its absence proves
    // nothing. `viewRepo` lists the ref's own files.
    let warning = format!(
        "Candidate files come from the default-branch index{index}, not {reference}: a file whose keywords match only at {reference} (it exists only there, or its text changed since) cannot appear, so absence proves nothing; list the files at {reference} and read them with `matchString`{lines}."
    );
    push_warnings(value, [warning]);
    // Rows not at the ref were moved or renamed there: search their names
    // across the whole tree (a moved file may leave the searched path).
    let moved = moved_names(value);
    let path = if moved.is_empty() {
        query.path.as_deref().map_or("", |path| path.as_str())
    } else {
        ""
    };
    let listing = moved
        .iter()
        .map(|name| name.as_str().try_into().ok())
        .collect::<Option<Vec<_>>>()
        .zip(owner.try_into().ok())
        .zip(repo.as_str().try_into().ok())
        .zip(at_ref.as_deref().unwrap_or(reference).try_into().ok())
        // An empty path lists the root; a path the schema refuses drops the lead.
        .zip(if path.is_empty() {
            Some(None)
        } else {
            path.try_into().ok().map(Some)
        })
        .map(
            |((((include, owner), repo), reference), path)| GhStructureQueryTree {
                include,
                limit: NonZeroU64::new(300).unwrap_or(NonZeroU64::MIN),
                main_goal: None,
                materialize: None,
                materialize_offset: None,
                max_depth: None,
                operation: GhStructureQueryTreeOperation::Tree,
                owner,
                page: None,
                path,
                reasoning: None,
                ref_: Some(reference),
                repo,
                sort: octocode_contract_types::GhStructureTreeSort {
                    by: octocode_contract_types::GhStructureTreeSortBy::Path,
                    order: octocode_contract_types::SortOrder::Asc,
                },
                walk_offset: None,
            },
        );
    if let Some(listing) = listing {
        let lead = Continuation::related_query(GhStructureQuery::from(listing)).build();
        ensure_calls(value);
        if let Some(next) = value.get_mut("calls").and_then(Value::as_object_mut) {
            next.shift_insert(0, "viewRepo".to_owned(), lead);
        }
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
    names.truncate(octocode_runtime_base::id::MAX_INPUT_ARRAY_ITEMS);
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
        .resolve_reference(owner, repo, reference, context)
        .await
    {
        Ok(sha) => Ok(Some(sha)),
        Err(error) if error.kind == ProviderErrorKind::Cancelled => Err(error),
        Err(_) => Ok(None),
    }
}

/// Some row lists `matches` read at the resolved commit.
fn resolved_sha_shown(value: &Value) -> bool {
    value["files"]
        .as_array()
        .is_some_and(|files| files.iter().any(|row| row.get("matches").is_some()))
}
