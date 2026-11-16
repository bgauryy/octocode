//! The missing-path recovery walk against a mocked Contents API.
use super::*;
use crate::tools::gh_shared::test_support::{default_context, mock_provider};
use octocode_github::RetryPolicy;
use octocode_github::mock_http::mount_json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn server() -> MockServer {
    let server = MockServer::builder().start().await;
    for reference in ["main", "HEAD"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
            .mount(&server)
            .await;
    }
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main","full_name":"a/b"}),
    )
    .await;
    server
}

async fn located(server: &MockServer, requested: &str) -> Option<PathRecovery> {
    let provider = mock_provider(server, RetryPolicy::default());
    let at = RepoPath {
        path: requested,
        ..RepoPath::root("a", "b")
    };
    locate_path(&provider, &at, &default_context()).await
}

/// The Contents API answers a file path with one object, which is no
/// directory: a missing path below a file climbs to the directory that holds
/// the file, never to the file itself.
#[tokio::test]
async fn a_missing_path_below_a_file_climbs_past_the_file() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/a.json",
        200,
        json!({"name":"a.json","path":"a.json","type":"file","size":3}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v3/repos/a/b/contents/?$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"a.json","path":"a.json","type":"file","size":3}
        ])))
        .mount(&server)
        .await;
    let found = located(&server, "a.json/x.json").await.expect("located");
    assert_eq!(found.directory, "", "the parent is a file");
    assert!(found.file.is_none());
}

/// A listing the Contents API cut at its limit may omit the wanted entry:
/// the recovery says so.
#[tokio::test]
async fn a_full_parent_listing_is_disclosed_as_possibly_cut() {
    let server = server().await;
    let entries = (0..crate::tools::gh_shared::CONTENTS_LIMIT)
        .map(|n| json!({"name":format!("f{n}"),"path":format!("docs/f{n}"),"type":"file","size":1}))
        .collect::<Vec<_>>();
    mount_json(&server, "/api/v3/repos/a/b/contents/docs", 200, entries).await;
    let found = located(&server, "docs/readme.md").await.expect("located");
    assert_eq!(found.directory, "docs");
    assert!(found.truncated, "a 1,000-entry listing may be cut");

    let short = server_with_short_listing().await;
    let found = located(&short, "docs/readme.md").await.expect("located");
    assert!(!found.truncated);
}

async fn server_with_short_listing() -> MockServer {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/docs",
        200,
        json!([{"name":"guide.md","path":"docs/guide.md","type":"file","size":1}]),
    )
    .await;
    server
}
