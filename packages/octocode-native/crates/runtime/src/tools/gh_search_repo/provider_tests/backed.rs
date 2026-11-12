//! ghSearchRepo against the mock server: picked rows, owner listings, leads.
use crate::tools::gh_shared::test_support::backed::run_repo as run;
use crate::tools::gh_shared::test_support::backed::*;
use octocode_github::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod listing;

#[tokio::test]
async fn picked_rows_keep_the_repository_id() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        json!({
            "total_count":1,"incomplete_results":false,"items":[repo_item("r", false)]
        }),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"search","keywords":["x"],"fields":["stars"]}),
    )
    .await
    .expect("search");
    assert_eq!(
        out.data["repositories"][0]
            .as_object()
            .map(|row| row.keys().cloned().collect::<Vec<_>>()),
        Some(vec![
            "owner".to_owned(),
            "repo".to_owned(),
            "stars".to_owned()
        ]),
        "{}",
        out.data
    );
    // The page's one lead is the first row's tree, whatever fields are picked.
    assert_eq!(
        out.data["related"].as_array().map(Vec::len),
        Some(1),
        "{}",
        out.data
    );
}

/// A page of repositories carries one lead, not one per row: the root
/// tree of its first repository (S-2: the route to ghStructure is
/// lead-driven).
#[tokio::test]
async fn a_repository_page_leads_to_its_first_repositorys_tree_only() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        json!({
            "total_count":2,"incomplete_results":false,
            "items":[repo_item("top", false), repo_item("second", false)]
        }),
    )
    .await;
    let out = run(&server, json!({"operation":"search","keywords":["x"]}))
        .await
        .expect("search");
    let related = out.data["related"].as_array().expect("one lead");
    assert_eq!(related.len(), 1, "{}", out.data);
    let call = related[0].to_string();
    assert!(
        call.contains("ghStructure") && call.contains("top"),
        "{call}"
    );
    assert!(!call.contains("second"), "{call}");
}

/// ghSearchRepo rows carry decision facts only, topics
/// favor the query, and pagination does not restate next.nextPage.
#[tokio::test]
async fn repository_rows_are_compact_and_paging_is_not_duplicated() {
    let server = MockServer::builder().start().await;
    let topics = (0..20)
        .map(|n| format!("t{n}"))
        .chain(["http-client".into()])
        .collect::<Vec<String>>();
    let long = "x".repeat(300);
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":75,"incomplete_results":false,"items":[{
                "full_name":"httpie/cli","name":"cli","html_url":"h","default_branch":"master",
                "stargazers_count":38602,"forks_count":4012,"language":"Python",
                "license":{"spdx_id":"BSD-3-Clause"},"description":long,
                "topics":topics,"pushed_at":"2024-12-17T00:00:00Z",
                "created_at":"2012-02-25T00:00:00Z","updated_at":"2026-10-01T00:00:00Z"
            }]
        })))
        .mount(&server)
        .await;
    let query = json!({"operation":"search","keywords":["http client"]});
    let out = run(&server, query.clone()).await.expect("search");
    let row = &out.data["repositories"][0];
    // Topics are opt-in through `fields`: a page of rows leaves them out.
    assert!(row.get("topics").is_none(), "{row}");
    let default_row = row.clone();
    let with = run(
        &server,
        json!({"operation":"search","keywords":["http client"],"fields":["topics"]}),
    )
    .await
    .expect("search");
    let picked = &with.data["repositories"][0];
    assert_eq!(default_row["repo"], "cli", "{default_row}");
    assert_eq!(default_row["owner"], "httpie", "{default_row}");
    assert_eq!(default_row["stars"], 38602);
    // Dates, language, license and forks are a `fields` pick too.
    assert!(default_row.get("pushedAt").is_none(), "{default_row}");
    let row = &default_row;
    // Every topic and the whole description stay: nothing reaches a
    // cut remainder, so a row never shortens them.
    let shown = picked["topics"].as_array().expect("topics");
    assert_eq!(shown.len(), 21, "{picked}");
    assert_eq!(
        shown[0], "http-client",
        "query-matching topics come first: {picked}"
    );
    assert_eq!(row["description"], long.as_str(), "{row}");
    assert!(row.get("topicCount").is_none(), "{row}");
    // Forks and the dates are a `fields` pick: a default row leaves them out.
    for field in ["forks", "updatedAt", "createdAt"] {
        assert!(row.get(field).is_none(), "{row}");
    }

    assert_eq!(
        out.data["next"]["nextPage"]["input"]["queries"][0]["page"],
        2
    );

    let facts = run(
        &server,
        json!({"operation":"search","keywords":["http client"],"fields":["forks","createdAt"]}),
    )
    .await
    .expect("search");
    let row = &facts.data["repositories"][0];
    assert_eq!(row["forks"], 4012, "{row}");
    assert_eq!(row["createdAt"], "2012-02-25");
}

/// ghSearchRepo: `qualifiers` alone is a search, not an owner
/// listing, and reaches GitHub as normalized qualifiers.
#[tokio::test]
async fn qualifiers_reach_the_search_query() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param(
            "q",
            "user:o fork:true forks:>=51 good-first-issues:>=3 archived:false",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[repo_item("r", false)]
        })))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json!({"operation":"search","owner":"o","filters":{"forks":{"min":51},"goodFirstIssueCount":{"min":3}}}),
    )
    .await
    .expect("qualified search");
    assert_eq!(out.data["repositories"][0]["repo"], "r", "{}", out.data);
    assert!(out.data.get("order").is_none(), "a search, not a listing");
}

/// The top repository leads to its tree (not a code search of the
/// discovery words), and an empty filtered search leads to the same
/// keywords without filters.
#[tokio::test]
async fn repositories_lead_nowhere_and_an_empty_search_broadens() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(wiremock::matchers::query_param_contains(
            "q",
            "language:rust",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        json!({
            "total_count":1,"incomplete_results":false,"items":[repo_item("top", false)]
        }),
    )
    .await;
    let out = run(&server, json!({"operation":"search","keywords":["needle"]}))
        .await
        .expect("search");
    // Repo-discovery words are not code keywords: the one lead is the tree.
    assert!(out.data["next"].get("searchCode").is_none(), "{}", out.data);
    assert!(
        out.data["related"].to_string().contains("ghStructure"),
        "{}",
        out.data
    );
    let empty = run(
        &server,
        json!({"operation":"search","keywords":["needle"],"filters":{"language":"rust"}}),
    )
    .await
    .expect("search");
    assert_eq!(empty.status, Some("empty"));
    let broader =
        crate::tools::test_support::checks::related_call(&empty.data, "ghSearchRepo", |_| true);
    assert_eq!(broader["tool"], "ghSearchRepo", "{}", empty.data);
    assert_eq!(
        broader["input"]["queries"][0],
        json!({"keywords":["needle"],"operation":"search"}),
        "{}",
        empty.data
    );
}

/// An empty search with nothing to drop (one keyword, no filter) still says
/// how terms match, so the next query changes its words; an empty one that
/// broadens carries the broadening instead.
#[tokio::test]
async fn an_empty_keyword_only_search_says_terms_are_anded() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        json!({"total_count":0,"incomplete_results":false,"items":[]}),
    )
    .await;
    let empty = run(&server, json!({"operation":"search","keywords":["zzqq"]}))
        .await
        .expect("search");
    assert_eq!(empty.status, Some("empty"));
    assert!(
        empty.data["hints"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|hint| hint.as_str().is_some_and(|hint| hint.contains("ANDed"))),
        "{}",
        empty.data
    );
}
