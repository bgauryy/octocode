//! securityCheck `repository` through the runtime's real GitHub reader, against
//! a fake GitHub at the HTTP boundary: what each answer GitHub gives reads as.

use crate::support::{Workspace, call, row_data};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn repository(full_name: &str) -> Value {
    json!({
        "full_name": full_name, "name": full_name.split('/').nth(1), "default_branch": "main",
        "html_url": format!("https://github.com/{full_name}"),
        "stargazers_count": 5000, "forks_count": 10, "archived": false, "fork": false,
        "pushed_at": "2999-01-01T00:00:00Z",
    })
}

/// One `securityCheck` repository row against `server`, as the row's data.
async fn read(server: &MockServer, repo: &str) -> Value {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_SECURITY_CHECKS", "true".into()),
    ]);
    let outcome = call(
        &runtime,
        "securityCheck",
        // The whole row: these tests read the repository's facts and sources.
        json!({"operation": "repository", "repo": repo, "view": "details"}),
    )
    .await
    .expect("securityCheck row");
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
}

async fn mount(server: &MockServer, route: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(response)
        .mount(server)
        .await;
}

fn rules(data: &Value) -> Vec<&str> {
    data["cautions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|caution| caution["rule"].as_str())
        .collect()
}

/// An empty advisory list: the repository has published none.
async fn mount_no_advisories(server: &MockServer, route: &str) {
    mount(
        server,
        route,
        ResponseTemplate::new(200).set_body_json(json!([])),
    )
    .await;
}

#[tokio::test]
async fn a_found_repository_reads_as_its_facts() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(200).set_body_json(repository("o/r")),
    )
    .await;
    mount_no_advisories(&server, "/api/v3/repos/o/r/security-advisories").await;
    let data = read(&server, "https://github.com/o/r?tab=readme").await;
    assert_eq!(data["sources"][0]["status"], "found", "{data}");
    assert_eq!(data["repository"]["owner"], "o", "{data}");
    assert!(rules(&data).is_empty(), "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
}

#[tokio::test]
async fn a_404_is_a_missing_repository() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})),
    )
    .await;
    let data = read(&server, "o/r").await;
    assert_eq!(rules(&data), ["repositoryMissing"], "{data}");
    assert_eq!(data["sources"][0]["status"], "noneKnown", "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
}

#[tokio::test]
async fn a_redirect_to_the_repository_id_is_a_moved_repository() {
    let server = MockServer::builder().start().await;
    let moved = format!("{}/api/v3/repositories/7", server.uri());
    mount(
        &server,
        "/api/v3/repos/old/name",
        ResponseTemplate::new(301).insert_header("location", moved.as_str()),
    )
    .await;
    mount(
        &server,
        "/api/v3/repositories/7",
        ResponseTemplate::new(200).set_body_json(repository("new/name")),
    )
    .await;
    // The transport follows a rename for the row, so the list is read at the
    // repository's id.
    mount_no_advisories(&server, "/api/v3/repositories/7/security-advisories").await;
    let data = read(&server, "old/name").await;
    assert_eq!(rules(&data), ["repositoryMoved"], "{data}");
    assert_eq!(data["repository"]["owner"], "new", "{data}");
    assert_eq!(data["sources"][0]["status"], "found", "{data}");
}

fn assert_source_failed(data: &Value) {
    assert_eq!(data["sources"][0]["status"], "failed", "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
    assert!(data.get("repository").is_none(), "{data}");
    assert!(data.get("cautions").is_none(), "{data}");
}

#[tokio::test]
async fn a_server_error_is_a_failed_source_never_clean() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(500).set_body_json(json!({"message": "Server Error"})),
    )
    .await;
    let data = read(&server, "o/r").await;
    assert_source_failed(&data);
}

#[tokio::test]
async fn a_spent_rate_limit_is_a_failed_source_never_clean() {
    let server = MockServer::builder().start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 3600;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", reset.to_string().as_str())
            .set_body_json(json!({"message": "API rate limit exceeded"})),
    )
    .await;
    let data = read(&server, "o/r").await;
    assert_source_failed(&data);
    assert_eq!(data["sources"][0]["errorCode"], "rateLimited", "{data}");
}

/// Text that names no repository is refused before GitHub is asked.
#[tokio::test]
async fn text_that_is_no_repository_never_reaches_github() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_SECURITY_CHECKS", "true".into()),
    ]);
    let outcome = call(
        &runtime,
        "securityCheck",
        json!({"operation": "repository", "repo": "../user"}),
    )
    .await
    .expect("securityCheck row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "invalidInput", "{data}");
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    runtime.close().await;
}

/// GitHub's advisory pages are linked by an `after` cursor: each is asked once,
/// the second only because the first named it, and an empty last page ends the
/// walk with a finished, empty list.
#[tokio::test]
async fn advisory_pages_are_walked_once_by_their_linked_cursor() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(200).set_body_json(repository("o/r")),
    )
    .await;
    let next = format!(
        "<{}/api/v3/repositories/1/security-advisories?per_page=100&after=cursor-2>; rel=\"next\"",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/security-advisories"))
        .and(wiremock::matchers::query_param_is_missing("after"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", next.as_str())
                .set_body_json(json!([])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/security-advisories"))
        .and(wiremock::matchers::query_param("after", "cursor-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let data = read(&server, "o/r").await;
    assert_eq!(data["advisories"], json!([]), "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
    let asked = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().ends_with("/security-advisories"))
        .map(|request| request.url.query().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert_eq!(
        asked
            .iter()
            .filter(|query| query.contains("after=cursor-2"))
            .count(),
        1,
        "{asked:?}"
    );
}

/// No token and the quota gone: the repository's facts stay, the advisory list
/// is absent, and the row says when GitHub answers again.
#[tokio::test]
async fn a_rate_limited_advisory_page_fails_with_its_reset_time() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(200).set_body_json(repository("o/r")),
    )
    .await;
    mount(
        &server,
        "/api/v3/repos/o/r/security-advisories",
        ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", "32503680000")
            .set_body_json(json!({"message": "API rate limit exceeded"})),
    )
    .await;
    let data = read(&server, "o/r").await;
    let source = &data["sources"][0];
    assert_eq!(source["status"], "partial", "{data}");
    assert_eq!(source["errorCode"], "rateLimited", "{data}");
    assert!(
        source["why"]
            .as_str()
            .is_some_and(|why| why.contains("3000-01-01")),
        "{data}"
    );
    assert!(data.get("advisories").is_none(), "{data}");
    assert_eq!(data["repository"]["owner"], "o", "{data}");
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
}

/// One batch mixes the repository operation, the same repository as a URL, and
/// a commit URL with a short hash: the URL reads as the operation, and the bad
/// row fails alone, before anything is sent.
#[tokio::test]
async fn a_github_url_reads_as_the_repository_operation_in_a_mixed_batch() {
    let server = MockServer::builder().start().await;
    mount(
        &server,
        "/api/v3/repos/o/r",
        ResponseTemplate::new(200).set_body_json(repository("o/r")),
    )
    .await;
    mount_no_advisories(&server, "/api/v3/repos/o/r/security-advisories").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_SECURITY_CHECKS", "true".into()),
    ]);
    let outcome = call(
        &runtime,
        "securityCheck",
        json!({"queries": [
            {"operation": "repository", "repo": "o/r"},
            {"operation": "url", "url": "https://github.com/o/r/tree/main"},
            {"operation": "url", "url": "https://github.com/o/r/commit/4f0b76e"},
        ]}),
    )
    .await
    .expect("securityCheck rows");
    let rows = outcome.structured_content["results"]
        .as_array()
        .expect("rows");
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[0]["data"], rows[1]["data"], "{rows:?}");
    assert_eq!(rows[2]["status"], "error", "{rows:?}");
    assert!(rows[2].to_string().contains("full 40 or 64"), "{rows:?}");
    runtime.close().await;
}
