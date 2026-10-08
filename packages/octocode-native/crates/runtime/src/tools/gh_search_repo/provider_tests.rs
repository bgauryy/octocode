//! Provider-backed ghSearchRepo tests: typed filters, archived disclosure,
//! and row facts.
use super::{GhSearchRepoQuery, execute};
use crate::providers::github::RetryPolicy;
use crate::tools::gh_shared::test_support::{mock_provider, mount_json};
use crate::tools::result::ToolData;
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn run(server: &MockServer, query: Value) -> ToolData {
    let provider = mock_provider(
        server,
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
    );
    let query: GhSearchRepoQuery = serde_json::from_value(query).expect("query");
    let context =
        crate::tools::gh_shared::test_support::fixture_context(Duration::from_secs(5), 1 << 20);
    execute(&provider, &query, &context).await.expect("search")
}

fn repo_item(name: &str, archived: bool) -> Value {
    json!({
        "full_name": format!("o/{name}"), "name": name,
        "html_url": "https://x", "default_branch": "main",
        "archived": archived
    })
}

fn page(items: Vec<Value>) -> Value {
    json!({"total_count": items.len(), "incomplete_results": false, "items": items})
}

/// Typed date filters reach GitHub as `pushed:`/`created:` qualifiers; a
/// relative window resolves to an absolute lower bound.
#[tokio::test]
async fn typed_date_filters_reach_the_search_query() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param(
            "q",
            "x stars:>100 pushed:>2025-01-01 created:2020-01-01..2021-01-01 archived:false",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![repo_item("r", false)])))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json!({"keywords":["x"],"stars":"> 100","pushed":">2025-01-01",
               "created":"2020-01-01 .. 2021-01-01"}),
    )
    .await;
    assert_eq!(out.data["repositories"][0]["repo"], "r", "{}", out.data);

    let relative = MockServer::start().await;
    mount_json(
        &relative,
        "/api/v3/search/repositories",
        200,
        page(vec![repo_item("r", false)]),
    )
    .await;
    run(&relative, json!({"pushed":"30d"})).await;
    let requests = relative.received_requests().await.expect("requests");
    let q = requests[0]
        .url
        .query_pairs()
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.into_owned())
        .expect("q");
    let pushed = q
        .split(' ')
        .find(|term| term.starts_with("pushed:"))
        .expect("pushed");
    assert!(pushed.starts_with("pushed:>="), "{q}");
    assert_eq!(pushed.len(), "pushed:>=2026-01-01".len(), "{q}");
}

/// The default search excludes archived repositories: page 1 leads to them
/// (`includeArchived`, archived:true, page 1) and the replay includes them
/// (no archive qualifier); that search flags archived rows and offers no
/// such lead.
#[tokio::test]
async fn default_archived_exclusion_is_disclosed_with_a_lead() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param("q", "x"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![repo_item("old", true)])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param("q", "x archived:false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 40, "incomplete_results": false,
            "items": [repo_item("live", false)]
        })))
        .mount(&server)
        .await;

    let first = run(&server, json!({"keywords":["x"]})).await;
    assert!(
        first.data["repositories"][0].get("archived").is_none(),
        "{}",
        first.data
    );
    let lead = &first.data["next"]["includeArchived"];
    assert_eq!(lead["tool"], "ghSearchRepo", "{}", first.data);
    assert_eq!(
        lead["query"]["queries"][0],
        json!({"keywords":["x"],"archived":true}),
        "{}",
        first.data
    );
    // Repo-discovery words are not code keywords: no code-search lead.
    assert!(
        first.data["next"].get("searchCode").is_none(),
        "{}",
        first.data
    );

    // Later pages state nothing more; an explicit filter needs no lead.
    let second = run(&server, json!({"keywords":["x"],"page":2})).await;
    assert!(
        second.data["next"].get("includeArchived").is_none(),
        "{}",
        second.data
    );
    assert!(
        second.data["next"].get("searchCode").is_none(),
        "{}",
        second.data
    );
    assert_eq!(
        second.data["next"]["viewRepo"]["tool"], "ghStructure",
        "{}",
        second.data
    );
    let excluded = run(&server, json!({"keywords":["x"],"archived":false})).await;
    assert!(
        excluded.data["next"].get("includeArchived").is_none(),
        "{}",
        excluded.data
    );

    let archived = run(&server, json!({"keywords":["x"],"archived":true})).await;
    assert_eq!(
        archived.data["repositories"][0]["archived"], true,
        "{}",
        archived.data
    );
    assert!(
        archived.data["next"].get("includeArchived").is_none(),
        "{}",
        archived.data
    );
}

/// An owner listing knows how many archived repositories it skipped: it
/// says so and leads to them.
#[tokio::test]
async fn owner_listing_counts_skipped_archived_repositories() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            repo_item("live", false),
            repo_item("old", true),
            repo_item("older", true)
        ])))
        .mount(&server)
        .await;
    let out = run(&server, json!({"owner":"o"})).await;
    assert_eq!(out.data["repositories"].as_array().map(Vec::len), Some(1));
    let warnings = out.data["warnings"].to_string();
    assert!(warnings.contains("2 archived repositories"), "{}", out.data);
    assert_eq!(
        out.data["next"]["includeArchived"]["query"]["queries"][0],
        json!({"owner":"o","archived":true}),
        "{}",
        out.data
    );

    let clean = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("live", false)])))
        .mount(&clean)
        .await;
    let out = run(&clean, json!({"owner":"o"})).await;
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
    assert!(
        out.data["next"].get("includeArchived").is_none(),
        "{}",
        out.data
    );
}

/// Rows carry the facts that decide a repository choice: creation date
/// beside the last push; forks and the metadata-update date stay verbose.
#[tokio::test]
async fn rows_carry_creation_date_and_keep_counts_verbose() {
    let server = MockServer::start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        page(vec![json!({
            "full_name":"o/r","name":"r","html_url":"h","default_branch":"main",
            "stargazers_count":5,"forks_count":0,"open_issues_count":3,
            "pushed_at":"2026-01-02T00:00:00Z","created_at":"2019-05-06T00:00:00Z",
            "updated_at":"2026-01-03T00:00:00Z","homepage":""
        })]),
    )
    .await;
    let out = run(&server, json!({"keywords":["x"]})).await;
    let row = &out.data["repositories"][0];
    assert_eq!(row["createdAt"], "2019-05-06", "{row}");
    assert_eq!(row["pushedAt"], "2026-01-02", "{row}");
    for absent in ["homepage", "openIssuesCount"] {
        assert!(row.get(absent).is_none(), "{absent}: {row}");
    }
    let verbose = crate::tools::id::ToolId::GhSearchRepo.verbose_paths();
    assert!(!verbose.contains(&"results[].data.repositories[].createdAt"));
    for field in ["forks", "updatedAt"] {
        assert!(
            verbose.contains(&format!("results[].data.repositories[].{field}").as_str()),
            "{field}"
        );
    }
}

/// An owner listing marks forks (`fork:true`) so they do not read as the
/// owner's own source; other rows carry no `fork` key.
#[tokio::test]
async fn owner_listing_marks_forks_only() {
    let server = MockServer::start().await;
    let mut fork = repo_item("forked", false);
    fork["fork"] = json!(true);
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([repo_item("own", false), fork])),
        )
        .mount(&server)
        .await;
    let out = run(&server, json!({"owner":"o"})).await;
    let rows = out.data["repositories"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{}", out.data);
    assert!(rows[0].get("fork").is_none(), "{}", out.data);
    assert_eq!(rows[1]["fork"], true, "{}", out.data);
}

/// QA2: GitHub matched more repositories than search reaches (1,000): the
/// first page states the provider's full count, not only the reachable
/// `totalItems`, so an org count is never silently 1,000.
#[tokio::test]
async fn capped_search_states_the_full_match_count() {
    let server = MockServer::start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        json!({"total_count": 4156, "incomplete_results": false, "items": [repo_item("r", false)]}),
    )
    .await;
    let out = run(&server, json!({"owner":"o","archived":false,"pageSize":1})).await;
    assert_eq!(
        out.data["partialReasons"],
        json!(["providerResultCap"]),
        "{}",
        out.data
    );
    let warnings = out.data["warnings"].to_string();
    assert!(warnings.contains("4156"), "{}", out.data);
    // Later pages do not repeat it.
    let second = run(
        &server,
        json!({"owner":"o","archived":false,"pageSize":1,"page":2}),
    )
    .await;
    assert!(
        !second.data["warnings"].to_string().contains("4156"),
        "{}",
        second.data
    );
}
