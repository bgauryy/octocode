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
            if !queries::code_has_narrowing_selector(query) {
                return Err(ProviderError::new(
                    ProviderErrorKind::Validation,
                    "Code search requires non-empty keywords, path, extension, filename, or language; owner/repo alone is not a bounded code search.",
                ));
            }
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
            concise,
            page,
            page_size,
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
                let (mut items, more) = transport
                    .list_owner_repositories(
                        owner.as_deref().unwrap_or_default(),
                        (sort.as_deref() == Some("updated")).then_some("updated"),
                        current,
                        per,
                        context,
                    )
                    .await?;
                // Search excludes archived repositories by default
                // (`archived:false`); the owner listing API cannot, so filter.
                items.retain(|item| !item.archived);
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
            let provider_incomplete = data.incomplete_results;
            let provider_capped = data.total_count > 1000;
            let repositories = if *concise == Some(true) {
                data.items
                    .into_iter()
                    .map(|r| json!(r.full_name))
                    .collect::<Vec<_>>()
            } else {
                data.items.into_iter().map(|r| { let (o,n)=r.full_name.split_once('/').unwrap_or(("",&r.name)); json!({"owner":o,"repo":n,"stars":r.stargazers_count,"forks":r.forks_count,"language":r.language,"license":r.license.and_then(|v|v.spdx_id),"description":r.description,"pushedAt":date(r.pushed_at),"createdAt":date(r.created_at),"updatedAt":date(r.updated_at),"topics":r.topics}) }).collect::<Vec<_>>()
            };
            let repositories_empty = repositories.is_empty();
            let mut value = json!({"operation":"repositories","repositories":repositories,"pagination":{"currentPage":current,"totalPages":pages,"perPage":per,"totalMatches":total,"totalMatchesCapped":provider_capped,"hasMore":more,"nextPage":more.then_some(current+1)}});
            if !more && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut)
            {
                page.remove("nextPage");
            }
            if owner_only
                && let Some(page) = value.get_mut("pagination").and_then(Value::as_object_mut)
            {
                page.remove("totalMatchesCapped");
            }
            add_next(&mut value, query, current, more, "repositories");
            apply_partial(
                &mut value,
                query,
                provider_incomplete,
                provider_capped,
                current,
                more,
                "repositories",
            );
            Ok(repository_output(
                value,
                repositories_empty,
                provider_incomplete,
                provider_capped,
            ))
        }
        GhSearchQuery::Tree { .. } => tree::execute(provider, query, context, home).await,
    }
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
#[allow(clippy::too_many_arguments)]
fn apply_partial(
    value: &mut Value,
    query: &GhSearchQuery,
    incomplete: bool,
    capped: bool,
    page: usize,
    has_more: bool,
    operation: &str,
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
        for raw in [
            r#"{"operation":"code"}"#,
            r#"{"operation":"code","keywords":[]}"#,
            r#"{"operation":"code","keywords":["   "]}"#,
            r#"{"operation":"code","owner":"o","repo":"r"}"#,
        ] {
            let query: GhSearchQuery =
                serde_json::from_str(raw).expect("code search fixture should deserialize");
            assert!(!queries::code_has_narrowing_selector(&query), "{raw}");
        }
        for raw in [
            r#"{"operation":"code","keywords":["needle"]}"#,
            r#"{"operation":"code","path":"src"}"#,
            r#"{"operation":"code","extension":"rs"}"#,
            r#"{"operation":"code","filename":"Cargo.toml"}"#,
            r#"{"operation":"code","language":"rust"}"#,
        ] {
            let query: GhSearchQuery =
                serde_json::from_str(raw).expect("bounded code search fixture should deserialize");
            assert!(queries::code_has_narrowing_selector(&query), "{raw}");
        }
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
    fn complete_repository_zero_is_empty_but_partial_zero_is_not() {
        let complete = repository_output(
            json!({"operation":"repositories","repositories":[]}),
            true,
            false,
            false,
        );
        assert_eq!(complete.status, Some("empty"));
        assert!(complete.data["hints"][0].is_string());

        for (incomplete, capped) in [(true, false), (false, true)] {
            let partial = repository_output(
                json!({"operation":"repositories","repositories":[]}),
                true,
                incomplete,
                capped,
            );
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

        async fn run(server: &MockServer, query: Value) -> Result<ToolData, ProviderError> {
            let query: GhSearchQuery = serde_json::from_value(query).expect("query");
            let home = std::env::temp_dir().join(format!(
                "gh-search-test-{}-{}",
                std::process::id(),
                server.address().port()
            ));
            execute(
                &provider(server),
                &query,
                &RequestContext::with_timeout(Duration::from_secs(5), 1 << 20),
                &Passthrough,
                &home,
            )
            .await
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
                json!({"operation":"code","owner":"a","repo":"b","keywords":["needle"],
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
                json!({"operation":"repositories","keywords":["x"],"concise":true}),
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
                json!({"operation":"repositories","owner":"o","sort":"updated"}),
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
                json!({"operation":"code","keywords":["x"],"page":1,"pageSize":100}),
            )
            .await
            .expect("page 1");
            assert!(first.data["next"]["nextPage"].is_object());
            assert!(first.data.get("terminalLimit").is_none(), "{}", first.data);
            let last = run(
                &server,
                json!({"operation":"code","keywords":["x"],"page":10,"pageSize":100}),
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
                json!({"operation":"tree","owner":"a","repo":"b","branch":"dev","path":"missing"}),
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
                json!({"operation":"tree","owner":"a","repo":"b","branch":"gone"}),
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
                json!({"operation":"tree","owner":"a","repo":"b","branch":"main","path":"src/lib.rs"}),
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
                json!({"operation":"tree","owner":"a","repo":"b","branch":sha,"materialize":true}),
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
    }

    #[test]
    fn incomplete_and_cap_are_losslessly_typed() {
        let query = serde_json::from_str::<GhSearchQuery>(
            r#"{"operation":"code","keywords":["x"],"page":10,"pageSize":100}"#,
        )
        .expect("GitHub search test data should be valid");
        let mut value = json!({"operation":"code","pagination":{"hasMore":false}});
        apply_partial(&mut value, &query, true, true, 10, false, "code");
        assert_eq!(value["terminalLimit"], true);
        assert_eq!(value["providerLimit"]["maxResults"], 1000);
        assert_eq!(
            value["partialReasons"],
            json!(["providerResultCap", "providerIncompleteResults"])
        );
        assert_eq!(value["next"]["retry"]["query"]["page"], 10);
    }
}
