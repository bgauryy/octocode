//! ghSearchRepo against the mock server: picked rows, owner listings, leads.
use crate::response::result::ToolData;
use crate::tools::gh_shared::test_support::backed::run_repo as run;
use crate::tools::gh_shared::test_support::backed::*;
use octocode_github::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &out.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        ),
        &json!({"kind":"related","tool":"ghStructure","input":{"queries":[{"owner":"o","repo":"r","operation":"tree"}]}}),
        "{}",
        out.data
    );
}

/// The top repository leads to its tree; an empty page leads nowhere.
#[tokio::test]
async fn top_repository_leads_to_its_tree() {
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
    let none = MockServer::builder().start().await;
    mount_json(
        &none,
        "/api/v3/search/repositories",
        200,
        json!({
            "total_count":0,"incomplete_results":false,"items":[]
        }),
    )
    .await;
    let out = run(&server, json!({"operation":"search","keywords":["x"]}))
        .await
        .expect("search");
    let lead =
        crate::tools::test_support::checks::related_call(&out.data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        });
    assert_eq!(
        lead,
        &json!({"kind":"related","tool":"ghStructure","input":{"queries":[{"owner":"o","repo":"top","operation":"tree"}]}}),
        "{}",
        out.data
    );
    let replay: octocode_contract_types::GhStructureQuery =
        serde_json::from_value(lead["input"]["queries"][0].clone())
            .expect("valid ghStructure query");
    let octocode_contract_types::GhStructureQuery::Tree(replay) = replay else {
        panic!("repository lead must select the tree view");
    };
    assert_eq!(replay.repo.as_str(), "top");
    let empty = run(&none, json!({"operation":"search","keywords":["none"]}))
        .await
        .expect("empty search");
    assert!(
        empty.data.pointer("/next/viewRepo").is_none(),
        "{}",
        empty.data
    );
}

#[tokio::test]
async fn owner_listing_updated_sort_tracks_pushes_and_excludes_archived() {
    let server = MockServer::builder().start().await;
    recently_pushed_org_repos(
        &server,
        json!([repo_item("live", false), repo_item("old", true)]),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"list","owner":"o","sort":{"by":"pushedAt","order":"desc"}}),
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
    assert!(
        out.data.get("order").is_none(),
        "the description states the order"
    );
}

/// The listing API's own order is creation (oldest first); the
/// default best-match listing asks for the most recently pushed.
#[tokio::test]
async fn owner_listing_default_sort_is_recent_activity_not_creation() {
    let server = MockServer::builder().start().await;
    recently_pushed_org_repos(&server, json!([repo_item("live", false)])).await;
    let out = run(&server, json!({"operation":"list","owner":"o"}))
        .await
        .expect("owner listing");
    assert_eq!(out.data["repositories"][0]["repo"], "live", "{}", out.data);
    assert!(out.data.get("order").is_none(), "{}", out.data);
}

#[tokio::test]
async fn owner_listing_reads_past_archived_pages_and_follows_the_link_cursor() {
    // A full provider page of mostly archived repositories plus a Link
    // next must keep paging instead of ending with a fabricated total.
    let server = MockServer::builder().start().await;
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
    let first = run(&server, json!({"operation":"list","owner":"o"}))
        .await
        .expect("owner listing");
    assert_eq!(names(&first), vec!["a", "b", "c", "d"], "{}", first.data);
    assert_eq!(first.data["pagination"]["providerPagesRead"], 3);
    assert_eq!(first.data["pagination"]["hasMore"], false);
    assert!(first.data["next"].get("nextPage").is_none());
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
    assert_eq!(row["repo"], "cli", "{row}");
    assert_eq!(row["owner"], "httpie", "{row}");
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
    assert!(row.get("topicCount").is_none(), "{row}");
    // Forks and the metadata-update date are stats extras: the extras stage
    // drops them by default and `extras:["stats"]` keeps them.
    for field in ["forks", "updatedAt"] {
        assert!(row.get(field).is_some());
    }

    assert_eq!(
        out.data["next"]["nextPage"]["input"]["queries"][0]["page"],
        2
    );

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
        json!({"operation":"search","owner":"o","filters":{"forkCount":{"min":51},"goodFirstIssueCount":{"min":3}}}),
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
async fn repositories_lead_to_the_tree_and_an_empty_search_broadens() {
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
    // Repo-discovery words are not code keywords; the top
    // repository leads to its tree only.
    assert!(out.data["next"].get("searchCode").is_none(), "{}", out.data);
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &out.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["tool"],
        "ghStructure",
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
        json!({"keywords":["needle"],"operation":"search","sort":{"by":"bestMatch","order":"desc"}}),
        "{}",
        empty.data
    );
}
