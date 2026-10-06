//! `operation:"refs"` (branches and tags) and `operation:"languages"`.
use super::*;
use crate::contracts::tool_types::GhStructureQueryOperation;
use crate::providers::github::{NamedRef, RefKind};

/// GitHub's largest `per_page` for branches and tags.
const MAX_REFS_PER_PAGE: usize = 100;

/// Run a non-tree operation; `None` for the tree listing.
pub(super) async fn execute_operation<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
) -> Option<Result<ToolData, ProviderError>> {
    match query.operation? {
        GhStructureQueryOperation::Tree => None,
        GhStructureQueryOperation::Refs => Some(refs(provider, query, context).await),
        GhStructureQueryOperation::Languages => Some(languages(provider, query, context).await),
    }
}

/// One page of branches and of tags (the same page number of each), with
/// the default branch named. A page continues while either list has more,
/// so every branch and tag is listed exactly once.
async fn refs<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    let page = crate::tools::num::usize_of(query.page);
    let per_page = crate::tools::num::usize_of(query.page_size).clamp(1, MAX_REFS_PER_PAGE);
    let transport = &provider.transport;
    let (default_branch, branches, tags) = tokio::try_join!(
        default_branch(provider, owner, repo, context),
        transport.repository_refs(owner, repo, RefKind::Branches, page, per_page, context),
        transport.repository_refs(owner, repo, RefKind::Tags, page, per_page, context),
    )?;
    let rows = |refs: &[NamedRef]| -> Vec<Value> {
        refs.iter()
            .map(|named| json!({"name": named.name, "sha": named.sha}))
            .collect()
    };
    let mut value = json!({
        "defaultBranch": default_branch,
        "branches": rows(&branches.refs),
        "tags": rows(&tags.refs),
    });
    if branches.has_more || tags.has_more {
        value["pagination"] = json!({"currentPage": page, "hasMore": true, "pageSize": per_page});
        if page >= crate::tools::id::query_limits::gh_structure::PAGE_MAXIMUM {
            value["terminalLimit"] = json!(true);
        } else {
            let mut next = public_query(query)?;
            next["page"] = json!(page + 1);
            next["pageSize"] = json!(per_page);
            value["next"]["nextPage"] = continuation(
                next,
                format!("Continue branches and tags on page {}.", page + 1),
                "exact",
            );
        }
    }
    Ok(ToolData::from(value))
}

/// Bytes of code per language on the default branch, largest first.
async fn languages<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let languages = provider
        .transport
        .repository_languages(query.owner.as_str(), query.repo.as_str(), context)
        .await?;
    let languages: Map<String, Value> = languages
        .into_iter()
        .map(|(name, bytes)| (name, json!(bytes)))
        .collect();
    Ok(ToolData::from(json!({ "languages": languages })))
}
