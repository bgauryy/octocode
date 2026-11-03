//! Search continuations read GitHub's largest search pages: a walk at a
//! small `limit` costs one search request per 100 results, and every
//! result arrives once, in the provider's order.
use crate::support::{self, Workspace};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Results the mock search index holds.
const RESULTS: usize = 139;
/// The largest page GitHub search serves.
const PROVIDER_PAGE: usize = 100;

/// A search index answering `page`/`per_page` the way GitHub does.
struct SearchIndex(Vec<Value>);

impl Respond for SearchIndex {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };
        let (page, per) = (param("page").unwrap_or(1), param("per_page").unwrap_or(30));
        assert!(
            per <= PROVIDER_PAGE,
            "per_page {per} is past GitHub's largest"
        );
        let items = self
            .0
            .iter()
            .skip((page - 1) * per)
            .take(per)
            .cloned()
            .collect::<Vec<_>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count": self.0.len(), "incomplete_results": false, "items": items
        }))
    }
}

/// Search requests the server received on `route`.
async fn searches(server: &MockServer, route: &str) -> usize {
    let requests = server.received_requests().await.expect("recorded");
    requests
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

fn data_rows(output: &Value, field: &str, id: &str) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["data"][field].as_array().cloned().unwrap_or_default())
        .map(|row| row[id].to_string())
        .collect()
}

/// A limit-3 repository search walk over 139 results sends
/// `ceil(139 / 100)` search requests and lists every repository once, in
/// the index's order.
#[tokio::test]
async fn a_small_limit_repository_walk_reads_each_provider_page_once() {
    let server = MockServer::builder().start().await;
    let index = (0..RESULTS)
        .map(|n| {
            let name = format!("r{n:03}");
            json!({
                "name": name, "full_name": format!("o/{name}"),
                "html_url": format!("https://github.com/o/{name}"), "default_branch": "main",
                "stargazers_count": 1, "pushed_at": "2024-01-01T00:00:00Z"
            })
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(SearchIndex(index))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json!({"queries":[{"operation":"search","keywords":["x"],"limit":3}]});
    let walked = support::walk(&runtime, "ghSearchRepo", start, |output| {
        data_rows(output, "repositories", "repo")
    })
    .await;
    let expected = (0..RESULTS)
        .map(|n| json!(format!("r{n:03}")).to_string())
        .collect::<Vec<_>>();
    assert_eq!(walked.rows, expected);
    assert_eq!(
        searches(&server, "/api/v3/search/repositories").await,
        RESULTS.div_ceil(PROVIDER_PAGE)
    );
    runtime.close().await;
}

/// A limit-7 pull-request search walk (GitHub's issue search) over 139
/// results sends `ceil(139 / 100)` search requests and lists every pull
/// request once, in the index's order.
#[tokio::test]
async fn a_small_limit_history_walk_reads_each_provider_page_once() {
    let server = MockServer::builder().start().await;
    let index = (1..=RESULTS)
        .map(|n| {
            json!({
                "number": n, "title": format!("PR {n}"), "state": "open",
                "user": {"login": "bob"},
                "html_url": format!("https://github.com/a/b/pull/{n}"),
                "pull_request": {"url": format!("https://api.github.com/repos/a/b/pulls/{n}")},
                "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"
            })
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(SearchIndex(index))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json!({"queries":[{
        "operation":"pullRequest","owner":"a","repo":"b","keywords":["fix"],"limit":7
    }]});
    let walked = support::walk(&runtime, "ghSearchHistory", start, |output| {
        data_rows(output, "pullRequests", "number")
    })
    .await;
    let expected = (1..=RESULTS).map(|n| n.to_string()).collect::<Vec<_>>();
    assert_eq!(walked.rows, expected);
    assert_eq!(
        searches(&server, "/api/v3/search/issues").await,
        RESULTS.div_ceil(PROVIDER_PAGE)
    );
    runtime.close().await;
}
