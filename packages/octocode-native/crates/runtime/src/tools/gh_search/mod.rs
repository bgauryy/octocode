mod code_output;
mod fragments;
mod queries;
mod ranking;
mod tree;
use crate::tools::result::ToolData;
use serde_json::{Value, json};

use crate::providers::github::{
    CodeSearchRequest, CredentialResolver, ProviderError, ProviderErrorKind, RepositorySearchPage,
    RepositorySearchRequest, RequestContext,
};
use crate::tools::local_fetch::ContentScan;
use crate::tools::result::remove_null_fields;

pub use crate::contracts::tool_types::{
    GhSearchCodeQuery, GhSearchCodeQueryMatch, GhSearchRepoQuery, GhSearchRepoQueryMatchItem,
    GhSearchRepoQuerySort, GhSearchRepoQueryVisibility, GhStructureQuery,
    GhStructureQueryIncludeItem,
};

/// The provider pages in `usize`; the wire contract owns the integer types.
pub(crate) fn usize_of(value: std::num::NonZeroU64) -> usize {
    usize::try_from(value.get()).unwrap_or(usize::MAX)
}

/// `ghStructure`: browse a known repository's tree.
pub async fn execute_structure<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhStructureQuery,
    context: &RequestContext,
    home: &std::path::Path,
) -> Result<ToolData, ProviderError> {
    tree::execute(provider, query, context, home).await
}

/// `ghSearchCode`: indexed default-branch code or path search.
pub async fn execute_code<R: CredentialResolver, C: crate::providers::github::ConditionalCache>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchCodeQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    queries::validate_code_scope(query)?;
    let GhSearchCodeQuery {
        match_,
        page,
        page_size,
        ..
    } = query;
    if !queries::code_has_narrowing_selector(query) {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "Code search requires non-empty keywords, path, extension, filename, or language; owner/repo alone is not a bounded code search.",
        ));
    }
    let q = queries::code(query);
    let current = usize_of(*page);
    let per = usize_of(*page_size).min(100);
    reject_window(current, per)?;
    let data = transport
        .search_code(
            &CodeSearchRequest {
                query: q,
                page: current,
                per_page: per,
                include_fragments: *match_ != GhSearchCodeQueryMatch::Path,
            },
            context,
        )
        .await?;
    let total = data.total_count.min(1000);
    let pages = total.div_ceil(per);
    let more = current < pages;
    let items = code_output::files(&data.items, query, security)?;
    let mut value = json!({});
    if !items.is_empty() {
        value["files"] = json!(items);
    }
    if pages > 1 {
        value["pagination"] = json!({"currentPage":current,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":data.total_count>total,"uniqueFileCount":code_output::unique_file_count(&data.items),"hasMore":more,"nextPage":more.then_some(current+1)});
    }
    if !more && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut) {
        page.remove("nextPage");
    }
    add_next(&mut value, "ghSearchCode", query, current, more);
    if let Some(read) = code_output::read_top_match(&value) {
        value["next"]["readTopMatch"] = read;
    }
    apply_partial(
        &mut value,
        "ghSearchCode",
        query,
        data.incomplete_results && data.items.is_empty(),
        data.total_count > 1000,
        current,
        more,
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
        let mut retry = serde_json::to_value(query)
            .map_err(|error| ProviderError::new(ProviderErrorKind::Decode, error.to_string()))?;
        remove_null_fields(&mut retry);
        value["next"]["retry"] = json!({"tool":"ghSearchCode","query":retry,"confidence":"exact"});
        if data.items.is_empty() {
            value["next"]["retry"]["why"] =
                json!("Retry the same query because GitHub marked the result incomplete.");
        }
    }
    if data.items.is_empty() {
        output.status = Some("empty");
        code_output::empty_scope(value, &mut output.diagnostics, query, transport, context).await?;
        if value.get("next").is_none() {
            value["hints"] = json!(["Broaden keywords or remove filters."]);
        }
    }
    Ok(output)
}

/// `ghSearchRepo`: repository search, or an owner-only repository listing.
pub async fn execute_repositories<
    R: CredentialResolver,
    C: crate::providers::github::ConditionalCache,
>(
    provider: &crate::providers::github::GitHubProvider<R, C>,
    query: &GhSearchRepoQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    queries::validate_repo_scope(query)?;
    let GhSearchRepoQuery {
        keywords,
        owner,
        language,
        stars,
        forks,
        good_first_issues,
        updated,
        created,
        size,
        match_,
        sort,
        archived,
        visibility,
        license,
        topics,
        concise,
        page,
        page_size,
        ..
    } = query;
    let mut terms = keywords.clone();
    terms.extend(topics.iter().map(|x| format!("topic:{x}")));
    let q = queries::repositories(query);
    let current = usize_of(*page);
    let per = usize_of(*page_size).min(100);
    let owner_only = terms.is_empty()
        && owner.is_some()
        && language.is_none()
        && stars.is_none()
        && forks.is_none()
        && good_first_issues.is_none()
        && updated.is_none()
        && created.is_none()
        && size.is_none()
        && match_.is_empty()
        && archived.is_none()
        && visibility.is_none()
        && license.is_none()
        && matches!(
            sort,
            GhSearchRepoQuerySort::BestMatch | GhSearchRepoQuerySort::Updated
        );
    // The owner listing pages through the REST list API, which has no
    // 1,000-result search window; only search is bounded by it.
    if !owner_only {
        reject_window(current, per)?;
    }
    let mut listing: Option<OwnerListing> = None;
    let data = if owner_only {
        let owner = owner.as_deref().map_or("", String::as_str);
        let sort = (*sort == GhSearchRepoQuerySort::Updated).then_some("updated");
        // Search excludes archived repositories by default
        // (`archived:false`); the owner listing API cannot, so filter
        // and keep reading provider pages until a page of kept rows,
        // the end of the listing (no Link next), or the page budget.
        let mut items = Vec::new();
        let mut provider_page = current;
        let more = loop {
            let (batch, has_next) = transport
                .list_owner_repositories(owner, sort, provider_page, per, context)
                .await?;
            items.extend(batch.into_iter().filter(|item| !item.archived));
            if !has_next
                || items.len() >= per
                || provider_page + 1 - current >= MAX_OWNER_LISTING_PAGES
            {
                break has_next;
            }
            provider_page += 1;
        };
        listing = Some(OwnerListing {
            last_page: provider_page,
            more,
        });
        RepositorySearchPage {
            total_count: items.len(),
            incomplete_results: false,
            items,
        }
    } else {
        transport
            .search_repositories(
                &RepositorySearchRequest {
                    query: q,
                    sort: (*sort != GhSearchRepoQuerySort::BestMatch).then(|| sort.to_string()),
                    page: current,
                    per_page: per,
                },
                context,
            )
            .await?
    };
    let total = data.total_count.min(1000);
    let pages = total.div_ceil(per);
    let more = match &listing {
        Some(listing) => listing.more,
        None => current < pages,
    };
    let provider_incomplete = data.incomplete_results;
    let provider_capped = listing.is_none() && data.total_count > 1000;
    let repositories = if *concise == Some(true) {
        data.items
            .into_iter()
            .map(|r| json!(r.full_name))
            .collect::<Vec<_>>()
    } else {
        data.items.into_iter().map(|r| { let (o,n)=r.full_name.split_once('/').unwrap_or(("",&r.name)); json!({"owner":o,"repo":n,"stars":r.stargazers_count,"forks":r.forks_count,"language":r.language,"license":r.license.and_then(|v|v.spdx_id),"description":r.description,"pushedAt":date(r.pushed_at),"createdAt":date(r.created_at),"updatedAt":date(r.updated_at),"topics":r.topics}) }).collect::<Vec<_>>()
    };
    let repositories_empty = repositories.is_empty();
    let mut value = match &listing {
        // The REST listing reports no total: `page` is the provider
        // page cursor and `nextPage` follows the real Link header.
        Some(listing) => {
            json!({"repositories":repositories,"pagination":{
                "currentPage":current,"perPage":per,"hasMore":more,
                "nextPage":more.then_some(listing.last_page + 1),
                "providerPagesRead":listing.last_page + 1 - current,
                "countScope":"unknown"
            }})
        }
        None => {
            json!({"repositories":repositories,"pagination":{"currentPage":current,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":provider_capped,"hasMore":more,"nextPage":more.then_some(current+1)}})
        }
    };
    if !more && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut) {
        page.remove("nextPage");
    }
    let next_from = listing
        .as_ref()
        .map_or(current, |listing| listing.last_page);
    add_next(&mut value, "ghSearchRepo", query, next_from, more);
    apply_partial(
        &mut value,
        "ghSearchRepo",
        query,
        provider_incomplete,
        provider_capped,
        current,
        more,
        "repositories",
    );
    Ok(repository_output(
        value,
        repositories_empty && !more,
        provider_incomplete,
        provider_capped,
    ))
}

/// Provider pages read by one owner-only listing call before it stops.
const MAX_OWNER_LISTING_PAGES: usize = 5;

struct OwnerListing {
    /// Last provider page read; the next cursor is the page after it.
    last_page: usize,
    /// The last page carried a Link `rel="next"`.
    more: bool,
}

fn repository_output(
    value: Value,
    repositories_empty: bool,
    provider_incomplete: bool,
    provider_capped: bool,
) -> ToolData {
    let mut output = ToolData::from(value);
    if repositories_empty && !provider_incomplete && !provider_capped {
        output.status = Some("empty");
        output.data["hints"] = json!(["Broaden keywords or remove repository filters."]);
    }
    output
}

fn reject_window(page: usize, per: usize) -> Result<(), ProviderError> {
    if (page - 1).saturating_mul(per) >= 1000 {
        Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "GitHub search page exceeds the 1,000-result search window",
        )
        .with_reason(crate::providers::github::ProviderErrorReason::SearchWindowExceeded))
    } else {
        Ok(())
    }
}
fn add_next(
    value: &mut Value,
    tool: &str,
    query: &impl serde::Serialize,
    page: usize,
    has_more: bool,
) {
    if !has_more {
        return;
    }
    let mut next = serde_json::to_value(query).unwrap_or_default();
    remove_null_fields(&mut next);
    next["page"] = json!(page + 1);
    value["next"] = json!({"nextPage":{"tool":tool,"query":next,"confidence":"exact"}});
}
#[allow(clippy::too_many_arguments)]
fn apply_partial(
    value: &mut Value,
    tool: &str,
    query: &impl serde::Serialize,
    incomplete: bool,
    capped: bool,
    page: usize,
    has_more: bool,
    subject: &str,
) {
    let mut reasons = Vec::new();
    if capped {
        reasons.push("providerResultCap");
        // terminalLimit means no executable continuation remains: only the
        // last reachable page of a capped search ends coverage.
        if !has_more {
            value["terminalLimit"] = json!(true);
        }
        value["providerLimit"] = json!({"reason":"providerResultCap","maxResults":1000});
    }
    if incomplete {
        reasons.push("providerIncompleteResults");
        let mut retry = serde_json::to_value(query).unwrap_or_default();
        remove_null_fields(&mut retry);
        retry["page"] = json!(page);
        value["next"]["retry"] = json!({"tool":tool,"query":retry,"why":format!("Retry the same {subject} provider page because the provider reported incomplete results."),"confidence":"exact"});
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
        for raw in [
            r#"{"goal":"test","reasoning":"test","owner":"o"}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","keywords":[]}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","keywords":["   "]}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","repo":"r"}"#,
        ] {
            let query: GhSearchCodeQuery =
                serde_json::from_str(raw).expect("code search fixture should deserialize");
            assert!(!queries::code_has_narrowing_selector(&query), "{raw}");
        }
        for raw in [
            r#"{"goal":"test","reasoning":"test","owner":"o","keywords":["needle"]}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","path":"src"}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","extension":"rs"}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","filename":"Cargo.toml"}"#,
            r#"{"goal":"test","reasoning":"test","owner":"o","language":"rust"}"#,
        ] {
            let query: GhSearchCodeQuery =
                serde_json::from_str(raw).expect("bounded code search fixture should deserialize");
            assert!(queries::code_has_narrowing_selector(&query), "{raw}");
        }
    }
    #[test]
    fn parses_each_public_tool_query() {
        serde_json::from_str::<GhSearchCodeQuery>(
            r#"{"goal":"test","reasoning":"test","owner":"o","keywords":["x"]}"#,
        )
        .expect("ghSearchCode query");
        serde_json::from_str::<GhSearchRepoQuery>(
            r#"{"goal":"test","reasoning":"test","owner":"o"}"#,
        )
        .expect("ghSearchRepo query");
        serde_json::from_str::<GhStructureQuery>(
            r#"{"goal":"test","reasoning":"test","owner":"o","repo":"r"}"#,
        )
        .expect("ghStructure query");
    }
    #[test]
    fn complete_repository_zero_is_empty_but_partial_zero_is_not() {
        let complete = repository_output(json!({"repositories":[]}), true, false, false);
        assert_eq!(complete.status, Some("empty"));
        assert!(complete.data["hints"][0].is_string());

        for (incomplete, capped) in [(true, false), (false, true)] {
            let partial = repository_output(json!({"repositories":[]}), true, incomplete, capped);
            assert_eq!(partial.status, None);
            assert!(partial.data.get("hints").is_none());
        }
    }

    mod provider_backed {
        use super::super::*;
        use crate::providers::github::{
            CredentialSource, GitHubEndpoint, GitHubProvider, GitHubTransport, NoCache,
            RetryPolicy, StaticCredentialResolver,
        };
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use std::{sync::Arc, time::Duration};
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path, query_param},
        };

        struct Passthrough;
        impl ContentScan for Passthrough {
            fn sanitize(
                &self,
                text: &str,
                _: &std::path::Path,
            ) -> Result<(String, Vec<String>), (String, String)> {
                Ok((text.to_owned(), vec![]))
            }
        }

        fn provider(server: &MockServer) -> GitHubProvider<StaticCredentialResolver, NoCache> {
            let endpoint = GitHubEndpoint::new(
                url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"),
            )
            .expect("endpoint");
            GitHubProvider {
                transport: GitHubTransport::new(
                    endpoint,
                    Arc::new(StaticCredentialResolver::new(
                        "fixture",
                        CredentialSource::Override,
                    )),
                    RetryPolicy {
                        max_attempts: 1,
                        ..Default::default()
                    },
                )
                .expect("transport"),
                cache: NoCache,
            }
        }

        /// Fixtures name their tool with a test-only `operation` key:
        /// code → ghSearchCode, repositories → ghSearchRepo, tree → ghStructure.
        async fn run(server: &MockServer, mut query: Value) -> Result<ToolData, ProviderError> {
            let operation = query
                .as_object_mut()
                .and_then(|object| object.remove("operation"))
                .expect("fixture operation");
            let home = std::env::temp_dir().join(format!(
                "gh-search-test-{}-{}",
                std::process::id(),
                server.address().port()
            ));
            let provider = provider(server);
            let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
            match operation.as_str() {
                Some("code") => {
                    let query = serde_json::from_value(query).expect("code query");
                    execute_code(&provider, &query, &context, &Passthrough).await
                }
                Some("repositories") => {
                    let query = serde_json::from_value(query).expect("repositories query");
                    execute_repositories(&provider, &query, &context).await
                }
                Some("tree") => {
                    let query = serde_json::from_value(query).expect("tree query");
                    execute_structure(&provider, &query, &context, &home).await
                }
                other => panic!("unknown fixture operation {other:?}"),
            }
        }

        fn repo_item(name: &str, archived: bool) -> Value {
            json!({
                "full_name": format!("o/{name}"), "name": name,
                "html_url": "https://x", "default_branch": "main",
                "archived": archived
            })
        }

        #[tokio::test]
        async fn renamed_repository_retry_keeps_every_filter() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(
                        json!({"total_count":0,"incomplete_results":false,"items":[]}),
                    ),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","goal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],
                       "extension":"rs","path":"src","language":"rust","page":2,"pageSize":10}),
            )
            .await
            .expect("search");
            let retry = &out.data["next"]["retryRenamed"]["query"];
            assert_eq!(retry["owner"], "c", "{}", out.data);
            assert_eq!(retry["repo"], "d");
            assert_eq!(retry["extension"], "rs");
            assert_eq!(retry["path"], "src");
            assert_eq!(retry["language"], "rust");
            assert_eq!(retry["pageSize"], 10);
            assert_eq!(retry["page"], 1);
        }

        #[tokio::test]
        async fn renamed_repository_supersedes_incomplete_retry() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(
                        json!({"total_count":0,"incomplete_results":true,"items":[]}),
                    ),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","goal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            )
            .await
            .expect("search");
            assert!(out.data["next"].get("retry").is_none(), "{}", out.data);
            assert_eq!(out.data["next"]["retryRenamed"]["query"]["repo"], "d");
        }

        #[tokio::test]
        async fn concise_repositories_are_flat_rows() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/repositories"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":false,"items":[repo_item("r", false)]
                })))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"repositories","goal": "test", "reasoning":"test","keywords":["x"],"concise":true}),
            )
            .await
            .expect("search");
            assert_eq!(out.data["repositories"], json!(["o/r"]));
        }

        #[tokio::test]
        async fn owner_listing_honors_sort_and_excludes_archived() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/orgs/o/repos"))
                .and(query_param("sort", "updated"))
                .and(query_param("direction", "desc"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!([repo_item("live", false), repo_item("old", true)])),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"repositories","goal": "test", "reasoning":"test","owner":"o","sort":"updated"}),
            )
            .await
            .expect("owner listing");
            let names = out.data["repositories"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(names, vec!["live".to_owned()], "{}", out.data);
        }

        #[tokio::test]
        async fn owner_listing_reads_past_archived_pages_and_follows_the_link_cursor() {
            // A full provider page of mostly archived repositories plus a Link
            // next must keep paging instead of ending with a fabricated total.
            let server = MockServer::start().await;
            let link = |page: usize| {
                format!(
                    "<{}/api/v3/orgs/o/repos?page={page}&per_page=3>; rel=\"next\"",
                    server.uri()
                )
            };
            for (page, rows, next) in [
                (
                    1,
                    vec![
                        repo_item("a", false),
                        repo_item("x1", true),
                        repo_item("x2", true),
                    ],
                    Some(2),
                ),
                (
                    2,
                    vec![
                        repo_item("x3", true),
                        repo_item("b", false),
                        repo_item("c", false),
                    ],
                    Some(3),
                ),
                (3, vec![repo_item("d", false)], None),
            ] {
                let mut response = ResponseTemplate::new(200).set_body_json(json!(rows));
                if let Some(next) = next {
                    response = response.insert_header("link", link(next).as_str());
                }
                Mock::given(method("GET"))
                    .and(path("/api/v3/orgs/o/repos"))
                    .and(query_param("page", page.to_string()))
                    .respond_with(response)
                    .mount(&server)
                    .await;
            }
            let names = |out: &ToolData| {
                out.data["repositories"]
                    .as_array()
                    .expect("rows")
                    .iter()
                    .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
                    .collect::<Vec<_>>()
            };
            let first = run(
                &server,
                json!({"operation":"repositories","goal": "test", "reasoning":"test","owner":"o","pageSize":3}),
            )
            .await
            .expect("owner listing");
            assert_eq!(names(&first), vec!["a", "b", "c"], "{}", first.data);
            let pagination = &first.data["pagination"];
            assert_eq!(pagination["hasMore"], true, "{}", first.data);
            assert_eq!(pagination["nextPage"], 3, "{}", first.data);
            assert!(pagination.get("totalMatches").is_none(), "{}", first.data);
            assert!(pagination.get("totalPages").is_none(), "{}", first.data);
            assert_eq!(first.data["next"]["nextPage"]["query"]["page"], 3);
            assert_ne!(first.status, Some("empty"));

            let last = run(
                &server,
                json!({"operation":"repositories","goal": "test", "reasoning":"test","owner":"o","pageSize":3,"page":3}),
            )
            .await
            .expect("last page");
            assert_eq!(names(&last), vec!["d"], "{}", last.data);
            assert_eq!(last.data["pagination"]["hasMore"], false);
            assert!(last.data["next"].get("nextPage").is_none(), "{}", last.data);
        }

        #[tokio::test]
        async fn capped_search_is_terminal_only_on_the_last_reachable_page() {
            let server = MockServer::start().await;
            let item = json!({"name":"a.rs","path":"a.rs","sha":"s","html_url":"h",
                "repository":{"full_name":"o/r","html_url":"h","url":"u"}});
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":5000,"incomplete_results":false,"items":[item]
                })))
                .mount(&server)
                .await;
            let first = run(
                &server,
                json!({"operation":"code","goal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":1,"pageSize":100}),
            )
            .await
            .expect("page 1");
            assert!(first.data["next"]["nextPage"].is_object());
            assert!(first.data.get("terminalLimit").is_none(), "{}", first.data);
            let last = run(
                &server,
                json!({"operation":"code","goal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}),
            )
            .await
            .expect("page 10");
            assert_eq!(last.data["terminalLimit"], true, "{}", last.data);
            assert!(last.data["next"].get("nextPage").is_none());
        }

        #[tokio::test]
        async fn tree_missing_path_on_existing_branch_does_not_fall_back() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/missing"))
                .and(query_param("ref", "dev"))
                .respond_with(
                    ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})),
                )
                .mount(&server)
                .await;
            // The path exists on the default branch: the old heuristic silently
            // showed it instead of reporting the path missing on `dev`.
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/missing"))
                .and(query_param("ref", "main"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"x.rs","path":"missing/x.rs","type":"file","size":1}
                ])))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/commits/dev"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"sha":"0".repeat(40)})),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})),
                )
                .mount(&server)
                .await;
            let error = run(
                &server,
                json!({"operation":"tree","goal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"dev","path":"missing"}),
            )
            .await
            .expect_err("path missing on an existing branch");
            assert_eq!(error.kind, ProviderErrorKind::NotFound);
        }

        #[tokio::test]
        async fn tree_missing_branch_still_falls_back_to_default() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .and(query_param("ref", "gone"))
                .respond_with(
                    ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/commits/gone"))
                .respond_with(
                    ResponseTemplate::new(422)
                        .set_body_json(json!({"message":"No commit found for SHA: gone"})),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .and(query_param("ref", "main"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"x.rs","path":"x.rs","type":"file","size":1}
                ])))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","goal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"gone"}),
            )
            .await
            .expect("fallback");
            assert_eq!(out.data["branchFallback"]["actualBranch"], "main");
        }

        #[tokio::test]
        async fn tree_on_a_file_path_is_a_clear_error() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"
                })))
                .mount(&server)
                .await;
            let error = run(
                &server,
                json!({"operation":"tree","goal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main","path":"src/lib.rs"}),
            )
            .await
            .expect_err("file path");
            assert_eq!(error.kind, ProviderErrorKind::Validation);
            assert!(error.message.contains("is a file"), "{}", error.message);
        }

        #[tokio::test]
        async fn materialize_skips_binary_files_with_a_warning() {
            let server = MockServer::start().await;
            let sha = "0123456789abcdef0123456789abcdef01234567";
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"img.png","path":"img.png","type":"file","size":4},
                    {"name":"ok.rs","path":"ok.rs","type":"file","size":4}
                ])))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha":sha})))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/img.png"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode([0u8, 1, 2, 3])
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/ok.rs"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode("fn x(){}")
                })))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","goal": "test", "reasoning":"test","owner":"a","repo":"b","branch":sha,"materialize":true}),
            )
            .await
            .expect("materialize continues past binary files");
            let local = out.data["location"]["localPath"]
                .as_str()
                .expect("location");
            assert!(std::path::Path::new(local).join("ok.rs").exists());
            let warnings = out.data["warnings"].to_string();
            assert!(warnings.contains("img.png"), "{}", out.data);
            let _ = std::fs::remove_dir_all(local);
        }

        #[tokio::test]
        async fn tree_reports_omitted_entries_and_next_page_drops_completed_metadata() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"node_modules","path":"node_modules","type":"dir"},
                    {"name":".DS_Store","path":".DS_Store","type":"file","size":1},
                    {"name":"a.rs","path":"a.rs","type":"file","size":4},
                    {"name":"b.rs","path":"b.rs","type":"file","size":4}
                ])))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/branches"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"name":"main"}])))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","goal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main",
                    "pageSize":1,"include":["sizes","branches"]}),
            )
            .await
            .expect("tree");
            assert_eq!(
                out.data["omitted"]["entries"],
                json!({".DS_Store":1,"node_modules":1}),
                "{}",
                out.data
            );
            let next = &out.data["next"]["nextPage"]["query"];
            assert_eq!(next["page"], 2, "{}", out.data);
            assert_eq!(next["include"], json!(["sizes"]), "{}", out.data);
            assert!(next.get("metadataPage").is_none(), "{}", out.data);
        }

        #[tokio::test]
        async fn tree_fallback_walk_stops_at_the_directory_fetch_cap() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(wiremock::matchers::path_regex("/git/trees/"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&server)
                .await;
            let dirs = (0..250)
                .map(|n| json!({"name":format!("d{n}"),"path":format!("d{n}"),"type":"dir"}))
                .collect::<Vec<_>>();
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!(dirs)))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(wiremock::matchers::path_regex("/contents/d[0-9]+$"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","goal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main","maxDepth":2}),
            )
            .await
            .expect("tree");
            assert_eq!(out.data["terminalLimit"], true, "{}", out.data);
            assert_eq!(out.data["providerLimit"]["reason"], "treeFetchLimit");
            let requests = server.received_requests().await.unwrap_or_default();
            let contents = requests
                .iter()
                .filter(|request| request.url.path().contains("/contents"))
                .count();
            assert_eq!(contents, super::super::tree::MAX_DIRECTORY_FETCHES);
        }
    }

    #[test]
    fn incomplete_and_cap_are_losslessly_typed() {
        let query = serde_json::from_str::<GhSearchCodeQuery>(
            r#"{"goal":"test","reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}"#,
        )
        .expect("GitHub search test data should be valid");
        let mut value = json!({"pagination":{"hasMore":false}});
        apply_partial(
            &mut value,
            "ghSearchCode",
            &query,
            true,
            true,
            10,
            false,
            "code",
        );
        assert_eq!(value["terminalLimit"], true);
        assert_eq!(value["providerLimit"]["maxResults"], 1000);
        assert_eq!(
            value["partialReasons"],
            json!(["providerResultCap", "providerIncompleteResults"])
        );
        assert_eq!(value["next"]["retry"]["query"]["page"], 10);
    }
}
