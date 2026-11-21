//! A first search page asks GitHub for what its `limit` needs, and so does a
//! second; a longer walk reads provider pages that double in size up to
//! GitHub's largest, so a walk at a small `limit` costs a request each time it
//! doubles and then one per 100 results, no page asks for more than the walk
//! already read, and every result arrives once, in the provider's order, with
//! the same totals as one large read.
use crate::support::{self, Run as _, Workspace};
use octocode_test_support::json_fixture;
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

/// The `per_page` of each search request the server received on `route`.
async fn page_sizes(server: &MockServer, route: &str) -> Vec<usize> {
    let requests = server.received_requests().await.expect("recorded");
    requests
        .iter()
        .filter(|request| request.url.path() == route)
        .map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "per_page")
                .and_then(|(_, value)| value.parse().ok())
                .expect("per_page")
        })
        .collect()
}

/// A limit-3 repository search walk over 139 results sends its first and second
/// pages as asked, then one request each time the walk doubles, and lists
/// every repository once, in the index's order.
#[tokio::test]
async fn a_small_limit_repository_walk_reads_pages_that_double() {
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
    let start = json_fixture(
        r#"{
            "queries": [{"operation": "search", "keywords": ["x"], "limit": 3}]
        }"#,
    );
    let walked = support::walk(&runtime, "ghSearchRepo", start, |output| {
        data_rows(output, "repositories", "repo")
    })
    .await;
    let expected = (0..RESULTS)
        .map(|n| json!(format!("r{n:03}")).to_string())
        .collect::<Vec<_>>();
    assert_eq!(walked.rows, expected);
    assert_eq!(
        page_sizes(&server, "/api/v3/search/repositories").await,
        [3, 3, 6, 12, 24, 48, 96]
    );
    runtime.close().await;
}

/// A limit-7 pull-request search walk (GitHub's issue search) over 139
/// results doubles until the next size would pass GitHub's largest page, then
/// reads whole provider pages, and lists every pull request once, in the
/// index's order.
#[tokio::test]
async fn a_small_limit_history_walk_doubles_up_to_the_largest_page() {
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
    let start = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "pullRequest",
                    "owner": "a",
                    "repo": "b",
                    "keywords": ["fix"],
                    "limit": 7
                }
            ]
        }"#,
    );
    let walked = support::walk(&runtime, "ghSearchHistory", start, |output| {
        data_rows(output, "pullRequests", "number")
    })
    .await;
    let expected = (1..=RESULTS).map(|n| n.to_string()).collect::<Vec<_>>();
    assert_eq!(walked.rows, expected);
    assert_eq!(
        page_sizes(&server, "/api/v3/search/issues").await,
        [7, 7, 14, 28, 56, PROVIDER_PAGE]
    );
    runtime.close().await;
}

fn code_hit(n: usize) -> Value {
    let name = format!("f{n:03}.rs");
    json!({
        "name": name, "path": format!("src/{name}"), "sha": "1".repeat(40),
        "html_url": format!("https://github.com/o/r/blob/main/src/{name}"),
        "repository": {"full_name": "o/r", "html_url": "https://github.com/o/r",
                       "url": "https://api.github.com/repos/o/r", "default_branch": "main"}
    })
}

/// A limit-7 code search walk over 139 files: pages that double, then the
/// provider page that holds the rest, and every file once in the index's order.
#[tokio::test]
async fn a_small_limit_code_walk_lists_every_file_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(SearchIndex((0..RESULTS).map(code_hit).collect()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json_fixture(
        r#"{
            "queries": [
                {
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"],
                    "limit": 7,
                    "fields": ["path"]
                }
            ]
        }"#,
    );
    let walked = support::walk(&runtime, "ghSearchCode", start, |output| {
        data_rows(output, "files", "path")
    })
    .await;
    let expected = (0..RESULTS)
        .map(|n| json!(format!("src/f{n:03}.rs")).to_string())
        .collect::<Vec<_>>();
    assert_eq!(walked.rows, expected);
    let sizes = page_sizes(&server, "/api/v3/search/code").await;
    assert_eq!(sizes, [7, 7, 14, 28, 56, PROVIDER_PAGE]);
    runtime.close().await;
}

/// Every walk size tiles the same rows with the same totals as one read at the
/// largest size: no row is lost or repeated by asking for less on page one, and
/// a first page never asks for more than its `limit`.
#[tokio::test]
async fn walks_of_every_limit_equal_the_unbudgeted_answer() {
    let repositories = (0..RESULTS)
        .map(|n| {
            let name = format!("r{n:03}");
            json!({"name": name, "full_name": format!("o/{name}"),
                   "html_url": format!("https://github.com/o/{name}"), "default_branch": "main",
                   "stargazers_count": 1, "pushed_at": "2024-01-01T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    let pulls = (1..=RESULTS)
        .map(|n| {
            json!({"number": n, "title": format!("PR {n}"), "state": "open", "user": {"login": "bob"},
                   "html_url": format!("https://github.com/a/b/pull/{n}"),
                   "pull_request": {"url": format!("https://api.github.com/repos/a/b/pulls/{n}")},
                   "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    let cases = [
        (
            "ghSearchRepo",
            "/api/v3/search/repositories",
            repositories,
            json_fixture(r#"{"operation": "search", "keywords": ["x"]}"#),
            "repositories",
            "repo",
        ),
        (
            "ghSearchHistory",
            "/api/v3/search/issues",
            pulls,
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "a",
                    "repo": "b",
                    "keywords": ["fix"]
                }"#,
            ),
            "pullRequests",
            "number",
        ),
        (
            "ghSearchCode",
            "/api/v3/search/code",
            (0..RESULTS).map(code_hit).collect::<Vec<_>>(),
            json_fixture(
                r#"{
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"],
                    "fields": ["path"]
                }"#,
            ),
            "files",
            "path",
        ),
    ];
    for (tool, route, index, row, field, id) in cases {
        let mut unbudgeted = None;
        for limit in [100, 50, 7, 1] {
            let server = MockServer::builder().start().await;
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(SearchIndex(index.clone()))
                .mount(&server)
                .await;
            let workspace = Workspace::new();
            let runtime = workspace.runtime(&support::github_api(&server));
            let mut query = row.clone();
            query["limit"] = json!(limit);
            let start = json!({"queries":[query.clone()]});
            let first = runtime
                .run("first".into(), tool.into(), start.clone())
                .await
                .expect("first page")
                .structured_content
                .into_value();
            let total = first["results"][0]["data"]["pagination"]["total"].clone();
            // GitHub's code count over several provider pages is an
            // estimate a walk need not deliver: ghSearchCode states no total.
            if tool == "ghSearchCode" {
                assert!(total.is_null(), "{tool} at {limit}: {first}");
            } else {
                assert_eq!(total, json!(RESULTS), "{tool} at {limit}: {first}");
            }
            let walked =
                support::walk(&runtime, tool, start, |output| data_rows(output, field, id)).await;
            assert_eq!(walked.rows.len(), RESULTS, "{tool} at {limit}");
            match &unbudgeted {
                None => unbudgeted = Some(walked.rows),
                Some(rows) => assert_eq!(&walked.rows, rows, "{tool} at {limit}"),
            }
            assert_eq!(
                page_sizes(&server, route).await[0],
                limit,
                "{tool} first page at {limit}"
            );
            runtime.close().await;
        }
    }
}

/// A first page whose rows a held 100-row answer already has reads them from it:
/// a smaller repeat of a walked search costs nothing.
#[tokio::test]
async fn a_first_page_reads_a_whole_page_already_held() {
    let server = MockServer::builder().start().await;
    let index = (0..RESULTS)
        .map(|n| {
            let name = format!("r{n:03}");
            json!({"name": name, "full_name": format!("o/{name}"),
                   "html_url": format!("https://github.com/o/{name}"), "default_branch": "main",
                   "stargazers_count": 1, "pushed_at": "2024-01-01T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(SearchIndex(index))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let whole = json_fixture(
        r#"{
            "queries": [{"operation": "search", "keywords": ["x"], "limit": 100}]
        }"#,
    );
    runtime
        .run("whole".into(), "ghSearchRepo".into(), whole)
        .await
        .expect("whole page");
    let small = json_fixture(
        r#"{
            "queries": [{"operation": "search", "keywords": ["x"], "limit": 5}]
        }"#,
    );
    let output = runtime
        .run("small".into(), "ghSearchRepo".into(), small)
        .await
        .expect("small page")
        .structured_content
        .into_value();
    assert_eq!(
        data_rows(&output, "repositories", "repo").len(),
        5,
        "{output}"
    );
    assert_eq!(
        searches(&server, "/api/v3/search/repositories").await,
        1,
        "the held answer was not read"
    );
    runtime.close().await;
}

/// A code search walked at a tiny `responseChars`: the first page states the
/// complete `coverage` receipt, no later page of the answer repeats it, every
/// file arrives exactly once. A path-only search emits no `related` leads, so
/// a page keeping its own leads is covered by the unit walk in
/// `response/page_receipts/tests.rs`.
#[tokio::test]
async fn a_tiny_window_walk_states_no_coverage_state_on_its_hit_pages() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(SearchIndex((0..RESULTS).map(code_hit).collect()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let mut query = json_fixture(
        r#"{
            "queries": [
                {
                    "owner": "o",
                    "repo": "r",
                    "keywords": ["x"],
                    "limit": 7,
                    "fields": ["path"]
                }
            ],
            "responseChars": 400
        }"#,
    );
    let mut pages = Vec::new();
    loop {
        let outcome = runtime
            .run("walk".into(), "ghSearchCode".into(), query.clone())
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"));
        let output = outcome.structured_content.into_value();
        let next = output["next"]["input"]
            .as_object()
            .or_else(|| output["next"]["input"].as_object())
            .cloned();
        pages.push(output);
        match next {
            Some(input) => query = Value::Object(input),
            None => break,
        }
    }
    assert!(pages.len() > 2, "the window splits the answer");
    let paths = pages
        .iter()
        .flat_map(|page| data_rows(page, "files", "path"))
        .collect::<Vec<_>>();
    let expected = (0..RESULTS)
        .map(|n| json!(format!("src/f{n:03}.rs")).to_string())
        .collect::<Vec<_>>();
    assert_eq!(paths, expected, "every file exactly once");
    let state = |page: &Value| page["results"][0]["data"]["coverage"]["state"].clone();
    assert!(
        state(&pages[0]).is_null(),
        "a page of hits states no coverage state: {}",
        pages[0]
    );
    // Every reply after the first is a later page of the answer (a code
    // search over several provider pages states no total, so its page block
    // carries nothing to tell a later page by).
    for page in pages.iter().skip(1) {
        let data = &page["results"][0]["data"];
        if data["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty())
        {
            assert!(
                data["coverage"].get("state").is_none(),
                "later page: {page}"
            );
        }
    }
    runtime.close().await;
}

/// A code search walk carries every file it listed in its private progress,
/// 16 hex digits a file, so past 625 files that state is longer than any
/// caller's string may be, well before GitHub's 1,000 reachable results. The
/// walk is the runtime's own state, not the caller's text: it is followed to
/// its end, every file once.
#[tokio::test]
async fn a_code_walk_past_625_files_is_followed_to_its_end() {
    const FILES: usize = 749;
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(SearchIndex((0..FILES).map(code_hit).collect()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json_fixture(
        r#"{"queries": [{"owner": "o", "repo": "r", "keywords": ["x"], "limit": 100, "fields": ["path"]}]}"#,
    );
    let walked = support::walk(&runtime, "ghSearchCode", start, |output| {
        data_rows(output, "files", "path")
    })
    .await;
    let expected = (0..FILES)
        .map(|n| json!(format!("src/f{n:03}.rs")).to_string())
        .collect::<Vec<_>>();
    assert_eq!(walked.rows, expected);
    runtime.close().await;
}
