//! Provider-backed ghStructure tests against a mock GitHub server.
use super::*;
use crate::tools::gh_shared::test_support::{default_context, mock_provider};
use octocode_github::RetryPolicy;
use octocode_github::mock_http::mount_json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
mod date_cursors;
mod listing;
mod refs;
mod renamed;

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
    mount_json(&server, "/api/v3/repos/a/b/branches", 200, json!([])).await;
    server
}

fn query(value: Value) -> GhStructureQuery {
    serde_json::from_value(value).expect("ghStructure query")
}

async fn list(server: &MockServer, value: Value) -> Result<ToolData, ProviderError> {
    let home = tempfile::tempdir().expect("home");
    let provider = mock_provider(server, RetryPolicy::default());
    let context = default_context();
    execute(&provider, &query(value), &context, home.path()).await
}
