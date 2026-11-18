//! The listing route an owner answered on is learned once per key: a repeat
//! request in this process, and the first one in the next process on the
//! same rate-limit state directory, list without the organization probe.
use super::*;
use octocode_github::{GitHubBudget, GitHubEndpoint, GitHubProvider, GitHubTransport, RetryPolicy};
use octocode_test_support::json_fixture;
use octocode_tools_shared::gh_shared::test_support::mock_endpoint;
use std::path::Path;

/// `me` is a user, the token's own login: `/orgs/me/repos` is 404 and the
/// listing comes from `/user/repos`.
async fn user_owner_server() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/me/repos"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"login": "me"}"#)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("mine", false)])))
        .mount(&server)
        .await;
    server
}

/// A provider with its own budget, mirroring its limiter facts under
/// `state_dir`, as the runtime builds one per process.
fn provider_with_state(endpoint: GitHubEndpoint, state_dir: &Path) -> GitHubProvider {
    let mut transport = GitHubTransport::new(
        endpoint,
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    transport.set_rate_limit_state_dir(Some(state_dir.to_path_buf()));
    GitHubProvider::new(transport)
}

async fn list_mine(provider: &GitHubProvider) {
    let query: GhSearchRepoQuery =
        serde_json::from_value(json_fixture(r#"{"owner": "me", "operation": "list"}"#))
            .expect("query");
    let out = execute(provider, &query, &default_context())
        .await
        .expect("listing");
    assert_eq!(out.data["repositories"][0]["repo"], "mine", "{}", out.data);
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

/// The second listing of one owner in this process sends no organization
/// probe and no viewer lookup: the key remembers the route.
#[tokio::test]
async fn a_repeat_listing_in_this_process_skips_the_organization_probe() {
    let server = user_owner_server().await;
    let state = tempfile::tempdir().expect("state dir");
    let provider = provider_with_state(mock_endpoint(&server), state.path());
    list_mine(&provider).await;
    assert_eq!(requests_to(&server, "/api/v3/orgs/me/repos").await, 1);
    list_mine(&provider).await;
    assert_eq!(requests_to(&server, "/api/v3/orgs/me/repos").await, 1);
    assert_eq!(requests_to(&server, "/api/v3/user").await, 1);
}

/// A CLI call is a fresh process: its first listing of an owner an earlier
/// process listed on the same key reads the route from the state file and
/// sends no organization probe.
#[tokio::test]
async fn the_next_process_reads_the_owner_route_from_the_state_file() {
    let server = user_owner_server().await;
    let state = tempfile::tempdir().expect("state dir");
    // The earlier process: its own runtime and budget, ended before the next
    // starts (a runtime's shutdown finishes its pending state writes).
    let endpoint = mock_endpoint(&server);
    let dir = state.path().to_path_buf();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(list_mine(&provider_with_state(endpoint, &dir)));
    })
    .join()
    .expect("earlier process");
    assert_eq!(requests_to(&server, "/api/v3/orgs/me/repos").await, 1);
    list_mine(&provider_with_state(mock_endpoint(&server), state.path())).await;
    assert_eq!(
        requests_to(&server, "/api/v3/orgs/me/repos").await,
        1,
        "the next process probed the organization route again"
    );
    assert_eq!(requests_to(&server, "/api/v3/user").await, 1);
}
