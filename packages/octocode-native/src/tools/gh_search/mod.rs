mod code_output;
mod fragments;
mod queries;
mod ranking;
mod tree;
use crate::tools::result::ToolData;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::providers::github::{
    CodeSearchRequest, CredentialResolver, ProviderError, ProviderErrorKind, RepositorySearchPage,
    RepositorySearchRequest, RequestContext,
};
use crate::tools::local_fetch::ContentScan;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GhSearchQuery {
    Code {
        keywords: Option<Vec<String>>,
        owner: Option<String>,
        repo: Option<String>,
        language: Option<String>,
        path: Option<String>,
        extension: Option<String>,
        filename: Option<String>,
        #[serde(rename = "match")]
        match_kind: Option<String>,
        concise: Option<bool>,
        page: Option<usize>,
        page_size: Option<usize>,
    },
    Repositories {
        keywords: Option<Vec<String>>,
        owner: Option<String>,
        language: Option<String>,
        stars: Option<String>,
        forks: Option<String>,
        good_first_issues: Option<String>,
        updated: Option<String>,
        created: Option<String>,
        size: Option<String>,
        #[serde(rename = "match")]
        match_kind: Option<Vec<String>>,
        sort: Option<String>,
        archived: Option<bool>,
        visibility: Option<String>,
        license: Option<String>,
        topics: Option<Vec<String>>,
        concise: Option<bool>,
        page: Option<usize>,
        page_size: Option<usize>,
    },
    Tree {
        owner: String,
        repo: String,
        branch: Option<String>,
        path: Option<String>,
        max_depth: Option<usize>,
        page: Option<usize>,
        page_size: Option<usize>,
        metadata_page: Option<usize>,
        include: Option<Vec<String>>,
        materialize: Option<bool>,
        materialize_offset: Option<usize>,
    },
}

pub async fn execute<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchQuery,
    context: &RequestContext,
    security: &impl ContentScan,
    home: &std::path::Path,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    match query {
        GhSearchQuery::Code {
            match_kind,
            page,
            page_size,
            ..
        } => {
            let q = queries::code(query);
            let current = page.unwrap_or(1);
            let per = page_size.unwrap_or(30).min(100);
            reject_window(current, per)?;
            let data = transport
                .search_code(
                    &CodeSearchRequest {
                        query: q,
                        page: current,
                        per_page: per,
                        include_fragments: match_kind.as_deref() != Some("path"),
                    },
                    context,
                )
                .await?;
            let total = data.total_count.min(1000);
            let pages = total.div_ceil(per);
            let more = current < pages;
            let items = code_output::files(&data.items, query, security)?;
            let mut value = json!({"operation":"code"});
            if !items.is_empty() {
                value["files"] = json!(items);
            }
            if pages > 1 {
                value["pagination"] = json!({"currentPage":current,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":data.total_count>total,"uniqueFileCount":code_output::unique_file_count(&data.items),"hasMore":more,"nextPage":more.then_some(current+1)});
            }
            if !more && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut)
            {
                page.remove("nextPage");
            }
            add_next(&mut value, query, current, more, "code");
            apply_partial(
                &mut value,
                query,
                data.incomplete_results && data.items.is_empty(),
                data.total_count > 1000,
                current,
                "code",
            );
            let mut output = ToolData::from(value);
            let value = &mut output.data;
            if data.incomplete_results {
                if data.items.is_empty() {
                    value["incompleteResults"] = json!(true);
                }
                output.diagnostics.add(
                    "ghIncompleteResults",
                    "GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.",
                    more || data.items.is_empty(),
                );
                let mut retry = serde_json::to_value(query).map_err(|error| {
                    ProviderError::new(ProviderErrorKind::Decode, error.to_string())
                })?;
                remove_nulls(&mut retry);
                value["next"]["retry"] =
                    json!({"tool":"ghSearch","query":retry,"confidence":"exact"});
                if data.items.is_empty() {
                    value["next"]["retry"]["why"] =
                        json!("Retry the same query because GitHub marked the result incomplete.");
                }
            }
            if data.items.is_empty() {
                output.status = Some("empty");
                code_output::empty_scope(value, &mut output.diagnostics, query, transport, context)
                    .await?;
                if value.get("next").is_none() {
                    value["hints"] = json!(["Broaden keywords or remove filters."]);
                }
            }
            Ok(output)
        }
        GhSearchQuery::Repositories {
            keywords,
            owner,
            language,
            stars,
            forks,
            good_first_issues,
            updated,
            created,
            size,
            match_kind,
            sort,
            archived,
            visibility,
            license,
            topics,
            page,
            page_size,
            ..
        } => {
            let mut terms = keywords.clone().unwrap_or_default();
            if let Some(v) = topics {
                terms.extend(v.iter().map(|x| format!("topic:{x}")));
            }
            let q = queries::repositories(query);
            let current = page.unwrap_or(1);
            let per = page_size.unwrap_or(30).min(100);
            reject_window(current, per)?;
            let owner_only = terms.is_empty()
                && owner.is_some()
                && language.is_none()
                && stars.is_none()
                && forks.is_none()
                && good_first_issues.is_none()
                && updated.is_none()
                && created.is_none()
                && size.is_none()
                && match_kind.is_none()
                && archived.is_none()
                && visibility.is_none()
                && license.is_none()
                && sort
                    .as_deref()
                    .is_none_or(|v| v == "best-match" || v == "updated");
            let data = if owner_only {
                let (items, more) = transport
                    .list_owner_repositories(
                        owner.as_deref().unwrap_or_default(),
                        current,
                        per,
                        context,
                    )
                    .await?;
                let seen = (current - 1) * per + items.len();
                RepositorySearchPage {
                    total_count: seen + usize::from(more),
                    incomplete_results: false,
                    items,
                }
            } else {
                transport
                    .search_repositories(
                        &RepositorySearchRequest {
                            query: q,
                            sort: sort
                                .as_ref()
                                .filter(|v| v.as_str() != "best-match")
                                .cloned(),
                            page: current,
                            per_page: per,
                        },
                        context,
                    )
                    .await?
            };
            let total = data.total_count.min(1000);
            let pages = total.div_ceil(per);
            let more = current < pages;
            let repositories=data.items.into_iter().map(|r| { let (o,n)=r.full_name.split_once('/').unwrap_or(("",&r.name)); json!({"owner":o,"repo":n,"stars":r.stargazers_count,"forks":r.forks_count,"openIssuesCount":r.open_issues_count,"language":r.language,"license":r.license.and_then(|v|v.spdx_id),"description":r.description,"pushedAt":date(r.pushed_at),"createdAt":date(r.created_at),"updatedAt":date(r.updated_at),"topics":r.topics}) }).collect::<Vec<_>>();
            let mut value = json!({"operation":"repositories","pagination":{"currentPage":current,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":data.total_count>total,"hasMore":more,"nextPage":more.then_some(current+1)},"repositories":repositories});
            if !more && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut)
            {
                page.remove("nextPage");
            }
            if owner_only
                && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut)
            {
                page.remove("totalMatchesCapped");
            }
            if let Some(top) = repositories.first()
                && let (Some(owner), Some(repo)) = (
                    top.get("owner").and_then(Value::as_str),
                    top.get("repo").and_then(Value::as_str),
                )
            {
                value["next"]["viewStructure"] = json!({
                    "tool": "ghSearch",
                    "query": {"operation":"tree","owner":owner,"repo":repo,"path":""},
                    "confidence": "low"
                });
                value["next"]["searchCode"] = json!({
                    "tool": "ghSearch",
                    "query": {"operation":"code","owner":owner,"repo":repo},
                    "confidence": "low"
                });
            }
            add_next(&mut value, query, current, more, "repositories");
            apply_partial(
                &mut value,
                query,
                data.incomplete_results,
                data.total_count > 1000,
                current,
                "repositories",
            );
            Ok(value.into())
        }
        GhSearchQuery::Tree { .. } => tree::execute(provider, query, context, home).await,
    }
}

fn reject_window(page: usize, per: usize) -> Result<(), ProviderError> {
    if (page - 1).saturating_mul(per) >= 1000 {
        Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "GitHub search page exceeds the 1,000-result search window",
        ))
    } else {
        Ok(())
    }
}
fn add_next(
    value: &mut Value,
    query: &GhSearchQuery,
    page: usize,
    has_more: bool,
    operation: &str,
) {
    if !has_more {
        return;
    }
    let mut next = serde_json::to_value(query).unwrap_or_default();
    remove_nulls(&mut next);
    if operation == "code" && next.get("match").is_none() {
        next["match"] = json!("file");
    }
    next["page"] = json!(page + 1);
    value["next"] = json!({"nextPage":{"tool":"ghSearch","query":next,"confidence":"exact"}});
}
fn remove_nulls(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, v| !v.is_null());
        for v in map.values_mut() {
            remove_nulls(v)
        }
    }
}
fn apply_partial(
    value: &mut Value,
    query: &GhSearchQuery,
    incomplete: bool,
    capped: bool,
    page: usize,
    operation: &str,
) {
    let mut reasons = Vec::new();
    if capped {
        reasons.push("providerResultCap");
        value["terminalLimit"] = json!(true);
        value["providerLimit"] = json!({"reason":"providerResultCap","maxResults":1000});
    }
    if incomplete {
        reasons.push("providerIncompleteResults");
        let mut retry = serde_json::to_value(query).unwrap_or_default();
        remove_nulls(&mut retry);
        retry["page"] = json!(page);
        value["next"]["retry"] = json!({"tool":"ghSearch","query":retry,"why":format!("Retry the same {operation} provider page because the provider reported incomplete results."),"confidence":"exact"});
    }
    if !reasons.is_empty() {
        value["isPartial"] = json!(true);
        value["partialReasons"] = json!(reasons);
    }
}
fn date(value: Option<String>) -> Option<String> {
    value.map(|v| v.chars().take(10).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_and_unreachable_searches() {
        assert!(reject_window(11, 100).is_err());
        assert!(reject_window(10, 100).is_ok());
    }
    #[test]
    fn parses_each_public_variant() {
        for raw in [
            r#"{"operation":"code","keywords":["x"]}"#,
            r#"{"operation":"repositories","owner":"o"}"#,
            r#"{"operation":"tree","owner":"o","repo":"r"}"#,
        ] {
            serde_json::from_str::<GhSearchQuery>(raw)
                .expect("GitHub search test data should be valid");
        }
    }
    #[test]
    fn incomplete_and_cap_are_losslessly_typed() {
        let query = serde_json::from_str::<GhSearchQuery>(
            r#"{"operation":"code","keywords":["x"],"page":10,"pageSize":100}"#,
        )
        .expect("GitHub search test data should be valid");
        let mut value = json!({"operation":"code","pagination":{"hasMore":false}});
        apply_partial(&mut value, &query, true, true, 10, "code");
        assert_eq!(value["terminalLimit"], true);
        assert_eq!(value["providerLimit"]["maxResults"], 1000);
        assert_eq!(
            value["partialReasons"],
            json!(["providerResultCap", "providerIncompleteResults"])
        );
        assert_eq!(value["next"]["retry"]["query"]["page"], 10);
    }
}
