//! A ghGetFileContent batch asks GitHub for its files' last commits once:
//! one aliased GraphQL request for the batch, not one request per file.

use crate::support::{Workspace, call, github_api};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const FILES: [&str; 3] = ["a.txt", "b.txt", "c.txt"];

/// GraphQL variables of one request: `p<i>` is the i-th path of the batch.
fn graphql_paths(request: &Request) -> Vec<(usize, String)> {
    let body: Value = serde_json::from_slice(&request.body).expect("graphql body");
    let mut paths = body["variables"]
        .as_object()
        .expect("variables")
        .iter()
        .filter_map(|(name, value)| {
            let index = name.strip_prefix('p')?.parse().ok()?;
            Some((index, value.as_str()?.to_owned()))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// The batch server: refs resolve to [`SHA`], files read as raw text, and
/// GraphQL answers every alias `p<i>` with a date named after its path,
/// except the paths `broken` lists, which answer an error at their alias.
async fn server(broken: &'static [&'static str]) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits/[^/]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"hello\n".as_slice(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(move |request: &Request| {
            let mut object = serde_json::Map::new();
            let mut errors = Vec::new();
            for (index, file) in graphql_paths(request) {
                let alias = format!("p{index}");
                if broken.contains(&file.as_str()) {
                    object.insert(alias.clone(), Value::Null);
                    errors.push(json!({"message":"boom","path":["repository","object",alias]}));
                } else {
                    object.insert(
                        alias,
                        json!({"nodes":[{"committedDate":format!("2026-01-01T00:00:00Z/{file}"),
                            "author":{"name":"Ada","user":{"login":format!("gql-{file}")}}}]}),
                    );
                }
            }
            let mut body = json!({"data":{"repository":{"object":object}}});
            if !errors.is_empty() {
                body["errors"] = Value::Array(errors);
            }
            ResponseTemplate::new(200).set_body_json(body)
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("per_page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "author": {"login": "rest-user"},
            "commit": {"committer": {"date": "2025-05-05T00:00:00Z"}}
        }])))
        .mount(&server)
        .await;
    server
}

async fn read_batch(server: &MockServer) -> Value {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(server));
    let queries = FILES
        .iter()
        .map(|file| json!({"owner":"a","repo":"b","path":file}))
        .collect::<Vec<_>>();
    let outcome = call(&runtime, "ghGetFileContent", json!({"queries":queries}))
        .await
        .expect("batch");
    runtime.close().await;
    outcome.structured_content.into_value()
}

async fn count(server: &MockServer, http_method: &str, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() == http_method && request.url.path() == route)
        .count()
}

#[tokio::test]
async fn three_rows_send_one_graphql_request_and_no_per_path_commit_lists() {
    let server = server(&[]).await;
    let out = read_batch(&server).await;
    assert_eq!(count(&server, "POST", "/api/graphql").await, 1, "{out}");
    assert_eq!(count(&server, "GET", "/api/v3/repos/a/b/commits").await, 0);
    // Each row names its own file's last commit, from the one answer.
    for (index, file) in FILES.iter().enumerate() {
        let data = &out["results"][index]["data"];
        assert_eq!(
            data["lastModified"],
            format!("2026-01-01T00:00:00Z/{file}"),
            "{out}"
        );
        assert_eq!(data["lastModifiedBy"], format!("gql-{file}"), "{out}");
    }
    // The three rows resolve their one ref once.
    assert_eq!(
        count(&server, "GET", "/api/v3/repos/a/b/commits/HEAD").await,
        1
    );
}

#[tokio::test]
async fn an_error_at_one_alias_falls_back_to_rest_for_that_path_only() {
    let server = server(&["b.txt"]).await;
    let out = read_batch(&server).await;
    assert_eq!(count(&server, "POST", "/api/graphql").await, 1, "{out}");
    let lists = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.url.path() == "/api/v3/repos/a/b/commits")
        .collect::<Vec<_>>();
    assert_eq!(lists.len(), 1, "only the failed path asks REST");
    assert!(lists[0].url.query().unwrap().contains("path=b.txt"));
    let data = |index: usize| &out["results"][index]["data"];
    assert_eq!(data(0)["lastModifiedBy"], "gql-a.txt", "{out}");
    assert_eq!(data(1)["lastModifiedBy"], "rest-user", "{out}");
    assert_eq!(data(1)["lastModified"], "2025-05-05T00:00:00Z", "{out}");
    assert_eq!(data(2)["lastModifiedBy"], "gql-c.txt", "{out}");
}

/// The calls keep the spelling of the row's repository, and a withheld path
/// asks GitHub nothing.
#[tokio::test]
async fn requests_keep_the_rows_spelling_and_withheld_paths_send_nothing() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/Ab/Cd/commits/[^/]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/Ab/Cd/contents/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"hello\n".as_slice(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data":{"repository":{"object":{
            "p0":{"nodes":[{"committedDate":"2026-01-01T00:00:00Z"}]},
            "p1":{"nodes":[{"committedDate":"2026-01-01T00:00:00Z"}]}}}}}),
        ))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let queries = ["a.txt", ".env", "b.txt"]
        .iter()
        .map(|file| json!({"owner":"Ab","repo":"Cd","path":file}))
        .collect::<Vec<_>>();
    let outcome = call(&runtime, "ghGetFileContent", json!({"queries":queries}))
        .await
        .expect("batch");
    runtime.close().await;
    let out = outcome.structured_content.into_value();
    assert_eq!(
        out["results"][0]["data"]["lastModified"], "2026-01-01T00:00:00Z",
        "{out}"
    );
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| !request.url.path().contains(".env")),
        "{requests:?}"
    );
    let bodies = requests
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| serde_json::from_slice::<Value>(&request.body).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["variables"]["p0"], "a.txt");
    assert_eq!(bodies[0]["variables"]["p1"], "b.txt");
    assert!(bodies[0]["variables"].get("p2").is_none());
}

/// Missing paths cost the batch nothing more: the warm-up is one GraphQL
/// request for the whole batch, whether the files exist or not (before it,
/// each row sent its own). It is not held until the contents answers: the
/// reads and the warm-up overlap, which is where the batch's time is saved.
#[tokio::test]
async fn missing_paths_add_no_graphql_request_to_the_batch() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits/[^/]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/a.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"hello\n".as_slice(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents/[bc]\\.txt$"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{
            "object":{"p0":{"nodes":[{"committedDate":"2026-01-01T00:00:00Z"}]},
                      "p1":{"nodes":[]},"p2":{"nodes":[]}}}}})),
        )
        .mount(&server)
        .await;
    let out = read_batch(&server).await;
    assert_eq!(count(&server, "POST", "/api/graphql").await, 1, "{out}");
    assert_eq!(count(&server, "GET", "/api/v3/repos/a/b/commits").await, 0);
    assert_eq!(
        out["results"][0]["data"]["lastModified"],
        "2026-01-01T00:00:00Z"
    );
}
