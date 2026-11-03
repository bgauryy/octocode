//! An owner listing's continuations: the route to a user's repositories is
//! found once, by the first page, and its cursors carry it.
use crate::support::{self, Run as _, Workspace};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Provider pages of user `u`'s listing, one repository each.
const PAGES: usize = 12;

/// Organization probes and viewer reads sent so far.
async fn route_reads(server: &MockServer) -> usize {
    let requests = server.received_requests().await.expect("recorded");
    requests
        .iter()
        .filter(|request| {
            request.url.path().starts_with("/api/v3/orgs/") || request.url.path() == "/api/v3/user"
        })
        .count()
}

/// Only the first page probes the organization route and reads the
/// viewer; every continuation lists the user's repositories directly, and
/// every repository arrives exactly once.
#[tokio::test]
async fn a_user_listing_resolves_its_route_once_for_every_page() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/u/repos"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"me"})))
        .mount(&server)
        .await;
    for page in 1..=PAGES {
        let name = format!("r{page:02}");
        let mut response = ResponseTemplate::new(200).set_body_json(json!([{
            "name": name, "full_name": format!("u/{name}"), "owner": {"login":"u"},
            "html_url": format!("https://github.com/u/{name}"), "archived": false, "fork": false,
            "stargazers_count": 1, "pushed_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z", "default_branch": "main"
        }]));
        if page < PAGES {
            let next = format!(
                "<{}/api/v3/users/u/repos?page={}>; rel=\"next\"",
                server.uri(),
                page + 1
            );
            response = response.insert_header("link", next.as_str());
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/users/u/repos"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json!({"queries":[{"owner":"u","operation":"list","limit":2}]});
    let mut walker = support::checks::Walker::new("ghSearchRepo", start);
    let mut continued = 0;
    while let Some(query) = walker.next_query() {
        let first = query.get("cursor").is_none();
        let before = route_reads(&server).await;
        let output = runtime
            .run("walk".into(), "ghSearchRepo".into(), query.clone())
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"))
            .structured_content
            .into_value();
        let sent = route_reads(&server).await - before;
        if first {
            assert_eq!(
                sent, 2,
                "one organization probe and one viewer read: {output}"
            );
        } else {
            assert_eq!(sent, 0, "{query}: {output}");
            continued += 1;
        }
        let rows = output["results"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|row| {
                row["data"]["repositories"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .map(|repository| repository["repo"].as_str().unwrap_or_default().to_owned())
            .collect();
        walker.accept(support::checks::Page { output, rows });
    }
    let walked = walker.finish();
    assert!(continued > 0, "{:?}", walked.queries);
    assert_eq!(walked.rows.len(), PAGES, "{:?}", walked.rows);
    runtime.close().await;
}
