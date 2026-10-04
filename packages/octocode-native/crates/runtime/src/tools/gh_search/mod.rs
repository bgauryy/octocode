mod code_output;
mod fragments;
mod lines;
mod queries;
mod ranking;
mod tree;
use crate::tools::id::ToolId;
use crate::tools::result::ToolData;
use serde_json::{Value, json};

use crate::providers::github::{
    CodeSearchRequest, CredentialResolver, ProviderError, ProviderErrorKind, RepositorySearchPage,
    RepositorySearchRequest, RequestContext,
};
use crate::security::scan::ContentScan;
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
    let per = usize_of(*page_size).min(crate::contracts::query_schema_max(
        ToolId::GhSearchCode,
        None,
        "pageSize",
    ));
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
    let mut items = code_output::files(&data.items, query, security)?;
    // The fragment read is computed before rows are shaped (it reads the
    // per-row owner/repo and matchIndices).
    let fragment_read = code_output::read_top_match(&json!({"files": &items}));
    let mut value = json!({});
    let resolution = code_output::resolve_lines(provider, query, &items, context, security).await?;
    let resolved_sha = resolution.as_ref().map(|resolution| resolution.sha.clone());
    let reads = code_output::shape_files(&mut value, &mut items, query, resolution, fragment_read);
    if !items.is_empty() {
        value["files"] = json!(items);
    }
    if pages > 1 {
        value["pagination"] = json!({"currentPage":current,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":data.total_count>total,"uniqueFileCount":code_output::unique_file_count(&data.items),"hasMore":more,"nextPage":more.then_some(current+1)});
    }
    if !more && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut) {
        page.remove("nextPage");
    }
    add_next(&mut value, ToolId::GhSearchCode, query, current, more);
    if let Some(read) = reads.top {
        value["next"]["readTopMatch"] = read;
    }
    // `readHits`, `readHits2`, …: one per capped or cut file.
    for (position, read) in reads.hits.into_iter().enumerate() {
        let key = match position {
            0 => "readHits".to_owned(),
            n => format!("readHits{}", n + 1),
        };
        value["next"][key] = read;
    }
    if !items.is_empty() {
        code_output::disclose_index_ref(provider, &mut value, query, resolved_sha, context).await?;
    }
    // Provider-index completeness is reported on every page, apart from
    // whether another page exists.
    apply_partial(
        &mut value,
        ToolId::GhSearchCode,
        query,
        data.incomplete_results,
        data.total_count > 1000,
        current,
        more,
        "code",
    );
    let mut output = ToolData::from(value);
    let value = &mut output.data;
    if data.incomplete_results {
        value["incompleteResults"] = json!(true);
        output.diagnostics.add(
            "ghIncompleteResults",
            "GitHub reported an incomplete search index result; retry, narrow the scope, or verify locally before concluding absence.",
            true,
        );
    }
    if data.items.is_empty() {
        output.status = Some("empty");
        code_output::empty_scope(value, &mut output.diagnostics, query, transport, context).await?;
        if value.get("hints").is_none() && code_output::path_mode_given_code(query) {
            // Path matching never sees file contents: search them instead.
            let mut content = serde_json::to_value(query).map_err(|error| {
                ProviderError::new(ProviderErrorKind::Decode, error.to_string())
            })?;
            remove_null_fields(&mut content);
            content["match"] = json!("file");
            content["page"] = json!(1);
            value["next"]["searchContent"] =
                json!({"tool":ToolId::GhSearchCode.as_str(),"query":content,"confidence":"high"});
            value["hints"] = json!([
                "match:\"path\" matches file paths, not code; run searchContent to search file contents."
            ]);
        }
        if value.get("hints").is_none() {
            value["hints"] = json!([code_output::empty_hint(query)]);
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
    let per = usize_of(*page_size).min(crate::contracts::query_schema_max(
        ToolId::GhSearchRepo,
        None,
        "pageSize",
    ));
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
        && query.qualifiers.is_none()
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
        // The listing API cannot rank by relevance, and its own default
        // order is creation (oldest first): best-match lists the most
        // recently pushed repositories first.
        let sort = "pushed";
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
        let wanted = wanted_topics(query);
        data.items
            .into_iter()
            .map(|item| repository_row(item, &wanted))
            .collect::<Vec<_>>()
    };
    let repositories_empty = repositories.is_empty();
    let mut value = match &listing {
        // The REST listing reports no total: `page` is the provider
        // page cursor and `nextPage` follows the real Link header.
        // `next.nextPage` carries the cursor; pagination only says whether
        // more exists and how many matched. The listing is ordered by latest
        // push (the REST listing reports no total).
        Some(listing) => {
            // Page counters are verbose (core field class).
            json!({"repositories":repositories,"order":"pushed","pagination":{
                "hasMore":more,
                "currentPage":current,
                "providerPagesRead":listing.last_page + 1 - current
            }})
        }
        None => {
            let mut value = json!({"repositories":repositories,"pagination":{"totalMatches":total,"hasMore":more}});
            if provider_capped {
                value["pagination"]["totalMatchesCapped"] = json!(true);
            }
            value["pagination"]["currentPage"] = json!(current);
            value["pagination"]["totalPages"] = json!(pages);
            value
        }
    };
    let next_from = listing
        .as_ref()
        .map_or(current, |listing| listing.last_page);
    add_next(&mut value, ToolId::GhSearchRepo, query, next_from, more);
    apply_partial(
        &mut value,
        ToolId::GhSearchRepo,
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
    tool: ToolId,
    query: &impl serde::Serialize,
    page: usize,
    has_more: bool,
) {
    if !has_more {
        return;
    }
    if page >= 1000 {
        value["terminalLimit"] = json!(true);
        return;
    }
    let mut next = serde_json::to_value(query).unwrap_or_default();
    remove_null_fields(&mut next);
    next["page"] = json!(page + 1);
    value["next"] = json!({"nextPage":{"tool":tool.as_str(),"query":next,"confidence":"exact"}});
}
fn apply_partial(
    value: &mut Value,
    tool: ToolId,
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
        // `partialReasons` names the cap; the limit adds only its size.
        value["providerLimit"] = json!({"maxResults":1000});
    }
    if incomplete {
        reasons.push("providerIncompleteResults");
        let mut retry = serde_json::to_value(query).unwrap_or_default();
        remove_null_fields(&mut retry);
        retry["page"] = json!(page);
        value["next"]["retry"] = json!({"tool":tool.as_str(),"query":retry,"why":format!("Retry the same {subject} provider page because the provider reported incomplete results."),"confidence":"exact"});
    }
    if !reasons.is_empty() {
        value["isPartial"] = json!(true);
        value["partialReasons"] = json!(reasons);
    }
}
fn date(value: Option<String>) -> Option<String> {
    value.map(|v| v.chars().take(10).collect())
}

/// Lowercased query topics and keyword words: a row lists these topics first.
fn wanted_topics(query: &GhSearchRepoQuery) -> Vec<String> {
    query
        .topics
        .iter()
        .map(|topic| topic.to_lowercase())
        .chain(query.keywords.iter().flat_map(|keyword| {
            let keyword = keyword.to_lowercase();
            let hyphenated = keyword.split_whitespace().collect::<Vec<_>>().join("-");
            keyword
                .split_whitespace()
                .map(str::to_owned)
                .chain(std::iter::once(hyphenated))
                .collect::<Vec<_>>()
        }))
        .collect()
}

/// One compact repository row: `owner/repo`, the decision facts, the whole
/// description, and every topic (query matches first). Forks and the creation
/// and metadata-update dates are verbose (core field class).
fn repository_row(
    item: crate::providers::github::RepositorySearchItem,
    wanted: &[String],
) -> Value {
    let mut topics = item.topics;
    // Stable: matching topics keep GitHub's order, then the rest.
    topics.sort_by_key(|topic| !wanted.contains(&topic.to_lowercase()));
    let mut row = json!({
        "repo": item.full_name,
        "stars": item.stargazers_count,
        "language": item.language,
        "license": item.license.and_then(|license| license.spdx_id),
        "pushedAt": date(item.pushed_at),
        "description": item.description,
    });
    if !topics.is_empty() {
        row["topics"] = json!(topics);
    }
    row["forks"] = json!(item.forks_count);
    row["createdAt"] = json!(date(item.created_at));
    row["updatedAt"] = json!(date(item.updated_at));
    remove_null_fields(&mut row);
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_listing_page_ceiling_keeps_rows_and_discloses_remaining_results() {
        let query: GhSearchRepoQuery = serde_json::from_value(
            json!({"owner":"o","page":1000,"mainGoal":"test","reasoning":"test"}),
        )
        .expect("query");
        let mut page = json!({"repositories":[{"repo":"o/r"}],"pagination":{"hasMore":true}});
        add_next(&mut page, ToolId::GhSearchRepo, &query, 1000, true);
        assert_eq!(page["repositories"].as_array().expect("rows").len(), 1);
        assert!(page.get("next").is_none(), "{page}");
        assert_eq!(page["terminalLimit"], true, "{page}");
        let mut earlier = json!({"repositories":[],"pagination":{"hasMore":true}});
        add_next(&mut earlier, ToolId::GhSearchRepo, &query, 999, true);
        assert_eq!(earlier["next"]["nextPage"]["query"]["page"], 1000);
        assert!(earlier.get("terminalLimit").is_none());
    }

    #[test]
    fn rejects_empty_and_unreachable_searches() {
        assert!(reject_window(11, 100).is_err());
        assert!(reject_window(10, 100).is_ok());
        for raw in [
            r#"{"mainGoal":"test","reasoning":"test","owner":"o"}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","keywords":[]}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","keywords":["   "]}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","repo":"r"}"#,
        ] {
            let query: GhSearchCodeQuery =
                serde_json::from_str(raw).expect("code search fixture should deserialize");
            assert!(!queries::code_has_narrowing_selector(&query), "{raw}");
        }
        for raw in [
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","keywords":["needle"]}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","path":"src"}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","extension":"rs"}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","filename":"Cargo.toml"}"#,
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","language":"rust"}"#,
        ] {
            let query: GhSearchCodeQuery =
                serde_json::from_str(raw).expect("bounded code search fixture should deserialize");
            assert!(queries::code_has_narrowing_selector(&query), "{raw}");
        }
    }
    #[test]
    fn parses_each_public_tool_query() {
        serde_json::from_str::<GhSearchCodeQuery>(
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","keywords":["x"]}"#,
        )
        .expect("ghSearchCode query");
        serde_json::from_str::<GhSearchRepoQuery>(
            r#"{"mainGoal":"test","reasoning":"test","owner":"o"}"#,
        )
        .expect("ghSearchRepo query");
        serde_json::from_str::<GhStructureQuery>(
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","repo":"r"}"#,
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

        const TREE_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

        /// Resolve `reference` to [`TREE_SHA`], as GitHub's commits endpoint does.
        async fn mount_ref(server: &MockServer, reference: &str) {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
                .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
                .mount(server)
                .await;
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
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],
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
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            )
            .await
            .expect("search");
            assert!(out.data["next"].get("retry").is_none(), "{}", out.data);
            assert_eq!(out.data["next"]["retryRenamed"]["query"]["repo"], "d");
        }

        /// D9: an inaccessible repository gets no `retry` for its incomplete
        /// result (a retry cannot help) and says why the search is empty.
        #[tokio::test]
        async fn inaccessible_repository_drops_the_incomplete_retry() {
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
                    ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            )
            .await
            .expect("search");
            assert!(out.data["next"].get("retry").is_none(), "{}", out.data);
            assert!(
                out.data["next"].get("findRepository").is_some(),
                "{}",
                out.data
            );
            let hint = out.data["hints"][0].as_str().unwrap_or_default();
            assert!(hint.contains("private"), "{}", out.data);
        }

        /// Search items for `a/b` with one GitHub fragment each (GitHub caps
        /// fragments, so routing.py's second call is not in the index text).
        async fn mount_code_search(server: &MockServer) {
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":2,"incomplete_results":false,"items":[
                        {"name":"handler.py","path":"src/handler.py","sha":"1","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[{"fragment":"def wrap_app(app):","matches":[{"text":"wrap_app","indices":[4,12]}]}]},
                        {"name":"routing.py","path":"src/routing.py","sha":"2","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[{"fragment":"    await wrap_app(app)(scope)","matches":[{"text":"wrap_app","indices":[10,18]}]}]}
                    ]
                })))
                .mount(server)
                .await;
        }

        async fn mount_content(server: &MockServer, file: &str, body: &str) {
            Mock::given(method("GET"))
                // The contents route encodes the path as one segment.
                .and(path(format!(
                    "/api/v3/repos/a/b/contents/{}",
                    file.replace('/', "%2F")
                )))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode(body)
                })))
                .mount(server)
                .await;
        }

        /// Repo-scoped hits list every keyword line of the top files with its
        /// line number (`grep -n` over the fetched blob), name owner/repo
        /// once, and read the top hit by line range.
        #[tokio::test]
        async fn repo_scoped_hits_list_every_occurrence_with_line_numbers() {
            let server = MockServer::start().await;
            mount_code_search(&server).await;
            mount_ref(&server, "HEAD").await;
            mount_content(
                &server,
                "src/handler.py",
                "import x\n\ndef wrap_app(app):\n    return app\n",
            )
            .await;
            mount_content(
                &server,
                "src/routing.py",
                "from h import wrap_app\n\nasync def a():\n    await wrap_app(app)(scope)\n\nasync def b():\n    await wrap_app(s)(scope)\n",
            )
            .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
            let data = &out.data;
            assert_eq!(data["owner"], "a", "{data}");
            assert_eq!(data["repo"], "b", "{data}");
            assert_eq!(data["commitSha"], TREE_SHA, "{data}");
            let files = data["files"].as_array().expect("files");
            let routing = files
                .iter()
                .find(|row| row["path"] == "src/routing.py")
                .expect("routing row");
            assert_eq!(
                routing["lines"],
                json!([
                    "1\tfrom h import wrap_app",
                    "4\t    await wrap_app(app)(scope)",
                    "7\t    await wrap_app(s)(scope)"
                ]),
                "{data}"
            );
            for row in files {
                assert!(
                    row.get("owner").is_none() && row.get("repo").is_none(),
                    "{row}"
                );
                assert!(row.get("matches").is_none(), "{row}");
            }
            let read = &data["next"]["readTopMatch"]["query"];
            assert_eq!(read["owner"], "a", "{data}");
            assert!(read.get("matchString").is_none(), "{data}");
            assert!(
                read["startLine"].as_u64().is_some() && read["endLine"].as_u64().is_some(),
                "{data}"
            );
            crate::contracts::validate_query("ghGetFileContent", {
                let mut query = read.clone();
                query["mainGoal"] = json!("g");
                query
            })
            .expect("readTopMatch is a valid ghGetFileContent query");
        }

        /// A file whose hits are capped or whose hit lines are cut carries an
        /// executable read of every keyword line at the resolved commit, so
        /// no hit stays unreachable; a fully shown file gets none.
        #[tokio::test]
        async fn capped_or_clipped_hit_files_carry_a_read_of_every_hit() {
            let server = MockServer::start().await;
            mount_code_search(&server).await;
            mount_ref(&server, "HEAD").await;
            mount_content(
                &server,
                "src/handler.py",
                &format!("x = '{}' + wrap_app\n", "a".repeat(400)),
            )
            .await;
            mount_content(&server, "src/routing.py", &"wrap_app()\n".repeat(25)).await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
            let data = &out.data;
            let reads: Vec<&Value> = ["readHits", "readHits2"]
                .iter()
                .filter_map(|key| data["next"].get(*key))
                .collect();
            assert_eq!(reads.len(), 2, "{data}");
            assert!(data["next"].get("readHits3").is_none(), "{data}");
            let mut paths: Vec<&str> = reads
                .iter()
                .map(|read| read["query"]["path"].as_str().expect("path"))
                .collect();
            paths.sort_unstable();
            assert_eq!(paths, ["src/handler.py", "src/routing.py"], "{data}");
            for read in reads {
                assert_eq!(read["tool"], "ghGetFileContent", "{data}");
                let query = &read["query"];
                assert_eq!(query["branch"], TREE_SHA, "{data}");
                assert_eq!(query["matchString"], "wrap_app", "{data}");
                assert_eq!(query["contextLines"], 0, "{data}");
                crate::contracts::validate_query("ghGetFileContent", {
                    let mut query = query.clone();
                    query["mainGoal"] = json!("g");
                    query
                })
                .expect("readHits is a valid ghGetFileContent query");
            }
        }

        /// A fully shown hit list needs no extra read.
        #[tokio::test]
        async fn fully_shown_hit_files_get_no_hit_read() {
            let server = MockServer::start().await;
            mount_code_search(&server).await;
            mount_ref(&server, "HEAD").await;
            mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
            mount_content(&server, "src/routing.py", "wrap_app()\n").await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
            assert!(out.data["next"].get("readHits").is_none(), "{}", out.data);
        }

        /// Owner-wide searches spend no contents quota: rows keep their
        /// owner/repo, and fragment offsets stay out of default output.
        #[tokio::test]
        async fn owner_wide_hits_keep_fragments_without_offsets() {
            let server = MockServer::start().await;
            mount_code_search(&server).await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/commits/HEAD"))
                .respond_with(ResponseTemplate::new(200).set_body_string(TREE_SHA))
                .expect(0)
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","keywords":["wrap_app"]}),
            )
            .await
            .expect("search");
            let row = &out.data["files"][0];
            assert_eq!(row["owner"], "a", "{}", out.data);
            assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
            // Offsets are verbose (core field class): the default response
            // drops them, debug keeps them.
            assert!(
                crate::tools::id::ToolId::GhSearchCode
                    .verbose_paths()
                    .contains(&"results[].data.files[].matches[].matchIndices"),
                "{}",
                out.data
            );
            assert!(out.data.get("commitSha").is_none(), "{}", out.data);
            // debug keeps the offsets.
            let debug = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","keywords":["wrap_app"],"debug":true}),
            )
            .await
            .expect("search");
            assert!(
                debug.data["files"][0]["matches"][0]["matchIndices"].is_array(),
                "{}",
                debug.data
            );
        }

        /// `branch` verifies the top files at that ref: lines come from the
        /// ref, a path absent there is flagged, and the page says its
        /// candidates came from the default-branch index.
        #[tokio::test]
        async fn branch_hits_are_verified_at_the_ref_and_labeled() {
            let server = MockServer::start().await;
            mount_code_search(&server).await;
            mount_ref(&server, "dev").await;
            mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/src%2Frouting.py"))
                .respond_with(
                    ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["wrap_app"],"branch":"dev"}),
            )
            .await
            .expect("search");
            let data = &out.data;
            assert_eq!(data["ref"], "dev", "{data}");
            assert_eq!(data["indexRef"], "defaultBranch", "{data}");
            let files = data["files"].as_array().expect("files");
            let handler = files
                .iter()
                .find(|row| row["path"] == "src/handler.py")
                .expect("handler");
            assert_eq!(handler["lines"], json!(["1\tdef wrap_app(app):"]), "{data}");
            let routing = files
                .iter()
                .find(|row| row["path"] == "src/routing.py")
                .expect("routing");
            assert_eq!(routing["atRef"], false, "{data}");
            assert!(
                routing.get("matches").is_none(),
                "default-branch text is not shown as the ref: {data}"
            );
            assert_eq!(
                data["next"]["readTopMatch"]["query"]["branch"], TREE_SHA,
                "the read is pinned to the commit the lines came from: {data}"
            );
        }

        /// A search pinned to a non-default ref warns that its files come
        /// from the default-branch index at the index commit and leads to
        /// the ref's own listing first; a ref that is the default-branch head
        /// adds neither.
        #[tokio::test]
        async fn a_non_default_ref_discloses_the_index_commit_and_leads_to_the_ref() {
            const HEAD_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";
            let server = MockServer::start().await;
            mount_code_search(&server).await;
            mount_ref(&server, "dev").await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/commits/HEAD"))
                .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
                .mount(&server)
                .await;
            mount_content(&server, "src/handler.py", "def wrap_app(app):\n").await;
            mount_content(&server, "src/routing.py", "await wrap_app(app)\n").await;
            let out = run(
                &server,
                json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"],
                    "path":"src","branch":"dev"}),
            )
            .await
            .expect("search");
            let data = &out.data;
            let warning = data["warnings"][0].as_str().unwrap_or_default();
            assert!(
                warning.contains("default-branch index at fedcba9") && warning.contains("not dev"),
                "{data}"
            );
            assert!(warning.contains("hints.viewRepo"), "{data}");
            let lead = &data["next"]["viewRepo"];
            assert_eq!(lead["tool"], "ghStructure", "{data}");
            for (key, value) in [
                ("owner", "a"),
                ("repo", "b"),
                ("path", "src"),
                ("branch", TREE_SHA),
            ] {
                assert_eq!(lead["query"][key], value, "{data}");
            }
            assert_eq!(
                data["next"]
                    .as_object()
                    .and_then(|next| next.keys().next())
                    .map(String::as_str),
                Some("viewRepo"),
                "the ref's listing is the first lead: {data}"
            );

            let default_head = MockServer::start().await;
            mount_code_search(&default_head).await;
            mount_ref(&default_head, "main").await;
            mount_ref(&default_head, "HEAD").await;
            mount_content(&default_head, "src/handler.py", "def wrap_app(app):\n").await;
            mount_content(&default_head, "src/routing.py", "await wrap_app(app)\n").await;
            let out = run(
                &default_head,
                json!({"operation":"code","owner":"a","repo":"b","keywords":["wrap_app"],
                    "branch":"main"}),
            )
            .await
            .expect("search");
            assert!(out.data.get("warnings").is_none(), "{}", out.data);
            assert!(out.data.pointer("/next/viewRepo").is_none(), "{}", out.data);
        }

        /// A file the index lists but the requested ref lacks offers no
        /// read: the fragment came from the default branch, and reading it
        /// there would cross the requested scope.
        #[tokio::test]
        async fn a_file_missing_at_the_requested_ref_offers_no_default_branch_read() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":false,"items":[
                        {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
                    ]
                })))
                .mount(&server)
                .await;
            mount_ref(&server, "dev").await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/app.rs"))
                .respond_with(
                    ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "keywords":["wrap_app"],"branch":"dev"}),
            )
            .await
            .expect("search");
            let data = &out.data;
            assert_eq!(data["files"][0]["atRef"], false, "{data}");
            assert!(
                data.pointer("/next/readTopMatch").is_none(),
                "no read outside the requested ref: {data}"
            );

            // The file exists at the ref but no line holds the keyword: the
            // fragment read stays pinned to the resolved commit.
            let unmatched = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":false,"items":[
                        {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
                    ]
                })))
                .mount(&unmatched)
                .await;
            mount_ref(&unmatched, "dev").await;
            mount_content(&unmatched, "app.rs", "fn renamed() {}\n").await;
            let out = run(
                &unmatched,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "keywords":["wrap_app"],"branch":"dev"}),
            )
            .await
            .expect("search");
            let read = &out.data["next"]["readTopMatch"]["query"];
            assert_eq!(read["branch"], TREE_SHA, "{}", out.data);
            assert_eq!(read["matchString"], "wrap_app", "{}", out.data);
        }

        /// A non-empty page GitHub marks incomplete says so in its data,
        /// without debug, separately from whether another page exists.
        #[tokio::test]
        async fn a_non_empty_incomplete_page_reports_partial_coverage() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":true,"items":[
                        {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                         "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                         "text_matches":[]}
                    ]
                })))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a",
                    "keywords":["wrap_app"],"match":"path"}),
            )
            .await
            .expect("search");
            let data = &out.data;
            assert_eq!(data["isPartial"], true, "{data}");
            assert_eq!(data["incompleteResults"], true, "{data}");
            assert_eq!(
                data["partialReasons"],
                json!(["providerIncompleteResults"]),
                "{data}"
            );
            assert!(data.pointer("/pagination/hasMore").is_none(), "{data}");
            assert_eq!(data["next"]["retry"]["query"]["page"], 1, "{data}");
            assert!(out.diagnostics.partial);
        }

        /// Empty searches name the default-branch index only when a branch
        /// was requested; otherwise the hint names the scope that came up empty.
        #[tokio::test]
        async fn empty_code_search_names_the_default_branch_index() {
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
                        .set_body_json(json!({"default_branch":"main","full_name":"a/b"})),
                )
                .mount(&server)
                .await;
            for (query, cause) in [
                (
                    json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"branch":"dev"}),
                    "default branch",
                ),
                (
                    json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
                    "in a/b",
                ),
                (
                    json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","keywords":["needle"]}),
                    "any a repository",
                ),
                (
                    json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"extension":"rs"}),
                    "extension",
                ),
                (
                    json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"match":"path"}),
                    "path",
                ),
            ] {
                let out = run(&server, query).await.expect("search");
                assert_eq!(out.status, Some("empty"));
                let hint = out.data["hints"][0].as_str().unwrap_or_default();
                assert!(hint.contains(cause), "{cause}: {}", out.data);
                assert!(hint.len() <= 120, "{hint}");
                if cause != "default branch" {
                    assert!(!hint.contains("default branch"), "{}", out.data);
                }
            }
        }

        /// D5: empty-search hints name the actual cause, and a repository
        /// known to exist gets no root viewStructure.
        #[tokio::test]
        async fn empty_code_search_hints_are_cause_specific() {
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
            for (repo, full_name) in [("b", "a/b"), ("old", "c/d")] {
                Mock::given(method("GET"))
                    .and(path(format!("/api/v3/repos/a/{repo}")))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_json(json!({"default_branch":"main","full_name":full_name})),
                    )
                    .mount(&server)
                    .await;
            }
            let hint =
                |out: &ToolData| out.data["hints"][0].as_str().unwrap_or_default().to_owned();
            let renamed = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"old","keywords":["needle"]}),
            )
            .await
            .expect("search");
            assert!(hint(&renamed).contains("renamed"), "{}", renamed.data);
            assert!(
                !hint(&renamed).contains("default branch"),
                "{}",
                renamed.data
            );

            let existing = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"]}),
            )
            .await
            .expect("search");
            assert!(
                existing.data["next"].get("viewStructure").is_none(),
                "{}",
                existing.data
            );
            let scoped = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["needle"],"path":"src/io"}),
            )
            .await
            .expect("search");
            assert_eq!(
                scoped.data["next"]["viewStructure"]["query"]["path"], "src/io",
                "{}",
                scoped.data
            );

            let path_mode = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                       "keywords":["fn spawn_blocking"],"match":"path"}),
            )
            .await
            .expect("search");
            assert!(
                hint(&path_mode).contains("match:\"path\""),
                "{}",
                path_mode.data
            );
            let content = &path_mode.data["next"]["searchContent"]["query"];
            assert_eq!(content["match"], "file", "{}", path_mode.data);
            assert_eq!(content["keywords"], json!(["fn spawn_blocking"]));
        }

        /// A zero-hit recovery for a requested branch inspects that branch,
        /// not the default branch the index covers.
        #[tokio::test]
        async fn empty_scoped_recovery_keeps_the_requested_branch() {
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
            for (repo, archived) in [("b", false), ("old", true)] {
                Mock::given(method("GET"))
                    .and(path(format!("/api/v3/repos/a/{repo}")))
                    .respond_with(ResponseTemplate::new(200).set_body_json(
                        json!({"default_branch":"main","full_name":format!("a/{repo}"),"archived":archived}),
                    ))
                    .mount(&server)
                    .await;
            }
            for repo in ["b", "old"] {
                let out = run(
                    &server,
                    json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":repo,
                           "keywords":["needle"],"path":"src","branch":"nondefault"}),
                )
                .await
                .expect("search");
                let view = &out.data["next"]["viewStructure"]["query"];
                assert_eq!(view["branch"], "nondefault", "{}", out.data);
            }
            let default = run(
                &server,
                json!({"operation":"code","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                       "keywords":["needle"],"path":"src"}),
            )
            .await
            .expect("search");
            assert!(
                default.data["next"]["viewStructure"]["query"]
                    .get("branch")
                    .is_none(),
                "{}",
                default.data
            );
        }

        /// D9: path matches carry no empty snippet list.
        #[tokio::test]
        async fn path_match_rows_omit_empty_matches() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":false,"items":[{
                        "name":"pool.rs","path":"src/pool.rs","sha":"1","html_url":"https://x",
                        "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}
                    }]
                })))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                       "keywords":["pool"],"match":"path"}),
            )
            .await
            .expect("search");
            let row = &out.data["files"][0];
            assert_eq!(row["path"], "src/pool.rs", "{}", out.data);
            assert!(row.get("matches").is_none(), "{}", out.data);
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
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","keywords":["x"],"concise":true}),
            )
            .await
            .expect("search");
            assert_eq!(out.data["repositories"], json!(["o/r"]));
        }

        #[tokio::test]
        async fn owner_listing_updated_sort_tracks_pushes_and_excludes_archived() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/orgs/o/repos"))
                .and(query_param("sort", "pushed"))
                .and(query_param("direction", "desc"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!([repo_item("live", false), repo_item("old", true)])),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o","sort":"updated"}),
            )
            .await
            .expect("owner listing");
            let names = out.data["repositories"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| row["repo"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(names, vec!["o/live".to_owned()], "{}", out.data);
            assert_eq!(out.data["order"], "pushed");
        }

        /// D8: the listing API's own order is creation (oldest first); the
        /// default best-match listing asks for the most recently pushed.
        #[tokio::test]
        async fn owner_listing_default_sort_is_recent_activity_not_creation() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/orgs/o/repos"))
                .and(query_param("sort", "pushed"))
                .and(query_param("direction", "desc"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!([repo_item("live", false)])),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o"}),
            )
            .await
            .expect("owner listing");
            assert_eq!(
                out.data["repositories"][0]["repo"], "o/live",
                "{}",
                out.data
            );
            assert_eq!(out.data["order"], "pushed", "{}", out.data);
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
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o","pageSize":3}),
            )
            .await
            .expect("owner listing");
            assert_eq!(names(&first), vec!["o/a", "o/b", "o/c"], "{}", first.data);
            let pagination = &first.data["pagination"];
            assert_eq!(pagination["hasMore"], true, "{}", first.data);
            // The page cursor lives in next.nextPage only.
            assert!(pagination.get("nextPage").is_none(), "{}", first.data);
            assert!(pagination.get("totalMatches").is_none(), "{}", first.data);
            assert!(pagination.get("totalPages").is_none(), "{}", first.data);
            assert_eq!(first.data["next"]["nextPage"]["query"]["page"], 3);
            assert_ne!(first.status, Some("empty"));

            let last = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test","owner":"o","pageSize":3,"page":3}),
            )
            .await
            .expect("last page");
            assert_eq!(names(&last), vec!["o/d"], "{}", last.data);
            assert_eq!(last.data["pagination"]["hasMore"], false);
            assert!(last.data["next"].get("nextPage").is_none(), "{}", last.data);
        }

        /// ghSearchRepo rows carry decision facts only, topics
        /// favor the query, and pagination does not restate next.nextPage.
        #[tokio::test]
        async fn repository_rows_are_compact_and_paging_is_not_duplicated() {
            let server = MockServer::start().await;
            let topics = (0..20)
                .map(|n| format!("t{n}"))
                .chain(["http-client".into()])
                .collect::<Vec<String>>();
            let long = "x".repeat(300);
            Mock::given(method("GET"))
                .and(path("/api/v3/search/repositories"))
                .and(query_param("page", "1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":25,"incomplete_results":false,"items":[{
                        "full_name":"httpie/cli","name":"cli","html_url":"h","default_branch":"master",
                        "stargazers_count":38602,"forks_count":4012,"language":"Python",
                        "license":{"spdx_id":"BSD-3-Clause"},"description":long,
                        "topics":topics,"pushed_at":"2024-12-17T00:00:00Z",
                        "created_at":"2012-02-25T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"
                    }]
                })))
                .mount(&server)
                .await;
            let query = json!({"operation":"repositories","mainGoal": "test", "reasoning":"test",
                "keywords":["http client"],"pageSize":1});
            let out = run(&server, query.clone()).await.expect("search");
            let row = &out.data["repositories"][0];
            assert_eq!(row["repo"], "httpie/cli", "{row}");
            assert_eq!(row["stars"], 38602);
            assert_eq!(row["pushedAt"], "2024-12-17");
            // Every topic and the whole description stay: nothing reaches a
            // cut remainder, so a row never shortens them.
            let shown = row["topics"].as_array().expect("topics");
            assert_eq!(shown.len(), 21, "{row}");
            assert_eq!(
                shown[0], "http-client",
                "query-matching topics come first: {row}"
            );
            assert_eq!(row["description"], long.as_str(), "{row}");
            for absent in ["owner", "topicCount"] {
                assert!(row.get(absent).is_none(), "{absent}: {row}");
            }
            // Forks and dates are verbose: the verbose stage drops them by
            // default and debug keeps them.
            for verbose in ["forks", "createdAt", "updatedAt"] {
                assert!(
                    crate::tools::id::ToolId::GhSearchRepo
                        .verbose_paths()
                        .contains(&format!("results[].data.repositories[].{verbose}").as_str()),
                    "{verbose}"
                );
            }
            // Page counters are verbose too; the verbose stage drops them.
            let mut pagination = out.data["pagination"].clone();
            for verbose in ["currentPage", "totalPages"] {
                assert!(
                    crate::tools::id::ToolId::GhSearchRepo
                        .verbose_paths()
                        .contains(&format!("results[].data.pagination.{verbose}").as_str()),
                    "{verbose}"
                );
                if let Some(fields) = pagination.as_object_mut() {
                    fields.shift_remove(verbose);
                }
            }
            assert_eq!(
                pagination,
                json!({"totalMatches":25,"hasMore":true}),
                "{}",
                out.data
            );
            assert_eq!(out.data["next"]["nextPage"]["query"]["page"], 2);

            let mut debug = query;
            debug["debug"] = json!(true);
            let out = run(&server, debug).await.expect("debug search");
            let row = &out.data["repositories"][0];
            assert_eq!(row["forks"], 4012, "{row}");
            assert_eq!(row["createdAt"], "2012-02-25");
        }

        /// ghSearchRepo: `qualifiers` alone is a search, not an owner
        /// listing, and reaches GitHub as normalized qualifiers.
        #[tokio::test]
        async fn qualifiers_reach_the_search_query() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/repositories"))
                .and(query_param(
                    "q",
                    "user:o archived:false forks:>50 good-first-issues:>2",
                ))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count":1,"incomplete_results":false,"items":[repo_item("r", false)]
                })))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"repositories","mainGoal": "test", "reasoning":"test",
                    "owner":"o","qualifiers":"forks:>50 good-first-issues:>2"}),
            )
            .await
            .expect("qualified search");
            assert_eq!(out.data["repositories"][0]["repo"], "o/r", "{}", out.data);
            assert!(out.data.get("order").is_none(), "a search, not a listing");
        }

        /// ghStructure: `pattern` finds a file by name at any depth in one
        /// call, keeping the dir/files row shape.
        #[tokio::test]
        async fn tree_pattern_finds_paths_by_name_at_any_depth() {
            let server = MockServer::start().await;
            mount_ref(&server, "main").await;
            let tree = json!([
                {"path":"starlette","type":"tree"},
                {"path":"starlette/_exception_handler.py","type":"blob","size":3},
                {"path":"starlette/routing.py","type":"blob","size":3},
                {"path":"tests","type":"tree"},
                {"path":"tests/test_exception_handler.py","type":"blob","size":3},
                {"path":"docs/exceptions.md","type":"blob","size":3}
            ]);
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"sha":TREE_SHA,"tree":tree,"truncated":false})),
                )
                .mount(&server)
                .await;
            let find = |pattern: &str| {
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                    "branch":"main","pattern":pattern})
            };
            let out = run(&server, find("**/_exception_handler.py"))
                .await
                .expect("glob");
            assert_eq!(
                out.data["structure"],
                json!([{"dir":"starlette","files":["_exception_handler.py"]}]),
                "{}",
                out.data
            );
            assert_eq!(out.data["summary"]["totalFiles"], 1);
            let bare = run(&server, find("Exception_Handler"))
                .await
                .expect("bare word");
            assert_eq!(bare.data["summary"]["totalFiles"], 2, "{}", bare.data);
            let none = run(&server, find("**/nope.py")).await.expect("no match");
            assert_eq!(none.status, Some("empty"));
            assert!(
                none.data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| hint.contains("pattern"))
            );
            let invalid = run(&server, find("src/[")).await.expect_err("bad glob");
            assert_eq!(invalid.kind, ProviderErrorKind::Validation);
        }

        /// ghStructure: a recursive listing fetches its tree once, by the
        /// resolved commit SHA. GitHub's tree response carries the tree
        /// object's SHA, never the commit's, so a tree fetched by ref name
        /// cannot be proven to belong to the resolved commit.
        #[tokio::test]
        async fn deep_tree_is_fetched_once_at_the_resolved_commit() {
            let server = MockServer::start().await;
            mount_ref(&server, "main").await;
            let tree_object = "b".repeat(40);
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/git/trees/main"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    json!({"sha":tree_object,"truncated":false,"tree":[{"path":"by-ref.rs","type":"blob","size":1}]}),
                ))
                .expect(0)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    json!({"sha":tree_object,"truncated":false,"tree":[{"path":"pinned.rs","type":"blob","size":1}]}),
                ))
                .expect(1)
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                    "branch":"main","maxDepth":3}),
            )
            .await
            .expect("tree");
            assert_eq!(
                out.data["structure"][0]["files"],
                json!(["pinned.rs"]),
                "{}",
                out.data
            );
            assert_eq!(out.data["commitSha"], TREE_SHA);
            let requests = server.received_requests().await.unwrap_or_default();
            let paths = requests
                .iter()
                .map(|request| request.url.path().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                paths,
                [
                    "/api/v3/repos/a/b/commits/main".to_owned(),
                    format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
                ],
                "no speculative tree request"
            );
        }

        /// Paths beneath an ignored directory are dropped before paging,
        /// sizing, and materializing, and the recursive tree and the
        /// Contents walk list the same permitted entries.
        #[tokio::test]
        async fn ignored_directories_hide_their_descendants_from_every_consumer() {
            let expected = json!([{"dir":".","files":["app.rs"]}]);
            // Recursive Git Trees listing.
            let server = MockServer::start().await;
            mount_ref(&server, "main").await;
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sha":"c".repeat(40),"truncated":false,"tree":[
                    {"path":"app.rs","type":"blob","size":3},
                    {"path":"vendor","type":"tree"},
                    {"path":"vendor/hidden.rs","type":"blob","size":9},
                    {"path":"vendor/deep","type":"tree"},
                    {"path":"vendor/deep/more.rs","type":"blob","size":9}
                ]})))
                .mount(&server)
                .await;
            mount_content(&server, "app.rs", "app").await;
            Mock::given(method("GET"))
                .and(wiremock::matchers::path_regex("/contents/vendor"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode("hidden")
                })))
                .expect(0)
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "branch":"main","maxDepth":5,"pageSize":1,"include":["sizes"],"materialize":true}),
            )
            .await
            .expect("recursive tree");
            let data = &out.data;
            assert_eq!(data["structure"], expected, "{data}");
            assert!(data.get("pagination").is_none(), "no phantom page: {data}");
            assert_eq!(data["fileSizes"], json!({"app.rs":3}), "{data}");
            assert_eq!(data["omitted"]["entries"], json!({"vendor":1}), "{data}");
            let local = std::path::Path::new(
                data["location"]["localPath"]
                    .as_str()
                    .expect("materialized location"),
            );
            assert!(local.join("app.rs").exists(), "{data}");
            assert!(!local.join("vendor").exists(), "{data}");
            let _ = std::fs::remove_dir_all(local);

            // Contents walk (the recursive tree is unavailable).
            let walked = MockServer::start().await;
            mount_ref(&walked, "main").await;
            Mock::given(method("GET"))
                .and(wiremock::matchers::path_regex("/git/trees/"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&walked)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"app.rs","path":"app.rs","type":"file","size":3},
                    {"name":"vendor","path":"vendor","type":"dir"}
                ])))
                .mount(&walked)
                .await;
            let out = run(
                &walked,
                json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                    "branch":"main","maxDepth":5,"pageSize":1,"include":["sizes"]}),
            )
            .await
            .expect("contents walk");
            assert_eq!(out.data["structure"], expected, "{}", out.data);
            assert_eq!(out.data["fileSizes"], json!({"app.rs":3}), "{}", out.data);
            assert_eq!(
                out.data["omitted"]["entries"],
                json!({"vendor":1}),
                "{}",
                out.data
            );
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
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":1,"pageSize":100}),
            )
            .await
            .expect("page 1");
            assert!(first.data["next"]["nextPage"].is_object());
            assert!(first.data.get("terminalLimit").is_none(), "{}", first.data);
            let last = run(
                &server,
                json!({"operation":"code","mainGoal": "test", "reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}),
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
                .and(query_param("ref", TREE_SHA))
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
            mount_ref(&server, "dev").await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})),
                )
                .mount(&server)
                .await;
            let error = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"dev","path":"missing"}),
            )
            .await
            .expect_err("path missing on an existing branch");
            assert_eq!(error.kind, ProviderErrorKind::NotFound);
        }

        /// D3: an explicit ref that does not exist is an error naming it,
        /// never a silent listing of the default branch.
        #[tokio::test]
        async fn tree_missing_branch_is_an_error_not_a_default_branch_listing() {
            let server = MockServer::start().await;
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
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"x.rs","path":"x.rs","type":"file","size":1}
                ])))
                .mount(&server)
                .await;
            let error = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"gone"}),
            )
            .await
            .expect_err("a missing ref is an error");
            assert_eq!(error.kind, ProviderErrorKind::NotFound);
            assert_eq!(
                error.reason,
                Some(crate::providers::github::ProviderErrorReason::RefNotFound)
            );
            assert!(error.message.contains("\"gone\""), "{}", error.message);
        }

        /// D5: a listing names the commit it read, and its next page reads
        /// that same commit even if the branch moves.
        #[tokio::test]
        async fn tree_pins_its_pages_to_the_resolved_commit() {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})),
                )
                .mount(&server)
                .await;
            mount_ref(&server, "HEAD").await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents"))
                .and(query_param("ref", TREE_SHA))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                    {"name":"a.rs","path":"a.rs","type":"file","size":1},
                    {"name":"b.rs","path":"b.rs","type":"file","size":1}
                ])))
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","pageSize":1}),
            )
            .await
            .expect("tree");
            assert_eq!(out.data["resolvedBranch"], "main", "{}", out.data);
            assert_eq!(out.data["commitSha"], TREE_SHA, "{}", out.data);
            assert_eq!(out.data["next"]["nextPage"]["query"]["branch"], TREE_SHA);
        }

        /// D10: a deep listing pages directory by directory, so its first
        /// page carries files, not only folders.
        #[tokio::test]
        async fn deep_tree_pages_carry_files_with_their_folders() {
            let server = MockServer::start().await;
            mount_ref(&server, "main").await;
            let mut tree = Vec::new();
            // 10 top-level folders × 3 subfolders × 3 files: 40 folders
            // sort before the first file in a folders-first listing.
            for dir in 0..10 {
                tree.push(json!({"path":format!("d{dir}"),"type":"tree"}));
                for sub in 0..3 {
                    tree.push(json!({"path":format!("d{dir}/s{sub}"),"type":"tree"}));
                    for file in 0..3 {
                        tree.push(json!({"path":format!("d{dir}/s{sub}/f{file}.rs"),"type":"blob","size":1}));
                    }
                }
            }
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"sha":TREE_SHA,"tree":tree,"truncated":false})),
                )
                .mount(&server)
                .await;
            let out = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main","maxDepth":5,"pageSize":20}),
            )
            .await
            .expect("tree");
            let files = out.data["summary"]["totalFiles"].as_u64().unwrap_or(0);
            assert!(files >= 5, "page 1 lists files: {}", out.data);
        }

        #[tokio::test]
        async fn tree_on_a_file_path_is_a_clear_error() {
            let server = MockServer::start().await;
            mount_ref(&server, "main").await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"
                })))
                .mount(&server)
                .await;
            let error = run(
                &server,
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main","path":"src/lib.rs"}),
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
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":sha,"materialize":true}),
            )
            .await
            .expect("materialize continues past binary files");
            let local = out.data["location"]["localPath"]
                .as_str()
                .expect("location");
            assert!(std::path::Path::new(local).join("ok.rs").exists());
            // The warning carries the count; `location.skipped` names the file.
            assert_eq!(
                out.data["location"]["skipped"],
                json!(["img.png"]),
                "{}",
                out.data
            );
            let warnings = out.data["warnings"].to_string();
            assert!(
                warnings.contains("Skipped 1 unreadable file(s)"),
                "{}",
                out.data
            );
            let _ = std::fs::remove_dir_all(local);
        }

        #[tokio::test]
        async fn tree_reports_omitted_entries_and_next_page_drops_completed_metadata() {
            let server = MockServer::start().await;
            mount_ref(&server, "main").await;
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
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main",
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
            mount_ref(&server, "main").await;
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
                json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","branch":"main","maxDepth":2}),
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
            r#"{"mainGoal":"test","reasoning":"test","owner":"o","keywords":["x"],"page":10,"pageSize":100}"#,
        )
        .expect("GitHub search test data should be valid");
        let mut value = json!({"pagination":{"hasMore":false}});
        apply_partial(
            &mut value,
            ToolId::GhSearchCode,
            &query,
            true,
            true,
            10,
            false,
            "code",
        );
        assert_eq!(value["terminalLimit"], true);
        assert_eq!(value["providerLimit"], json!({"maxResults":1000}));
        assert_eq!(
            value["partialReasons"],
            json!(["providerResultCap", "providerIncompleteResults"])
        );
        assert_eq!(value["next"]["retry"]["query"]["page"], 10);
    }
}
