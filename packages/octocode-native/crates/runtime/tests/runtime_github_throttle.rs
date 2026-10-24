//! Throttled and failed GitHub rows: what they tell the caller to do next
//! (`next.retry`, the provider's delay, the SSO authorization URL) and how
//! the batch exits.

use crate::support::{PACED_LIMITS, Run as _, Workspace, call, row_data};
use octocode_native::response::exit::ExitClass;
use octocode_native::runtime::FailureKind;
use serde_json::json;
use wiremock::matchers::{method, path, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SEARCH_REPOS: &str = "/api/v3/search/repositories";

fn repo_hit(name: &str) -> serde_json::Value {
    json!({"total_count":1,"incomplete_results":false,"items":[{
        "full_name": format!("o/{name}"), "name": name, "owner":{"login":"o"},
        "stargazers_count":1,"forks_count":0,"html_url": format!("https://github.com/o/{name}"),
        "default_branch":"main","private":false,"archived":false,"fork":false,
        "topics":[],"description":"d","language":"Rust"}]})
}

#[tokio::test]
async fn a_rate_limited_row_offers_a_retry_and_the_batch_exits_rate_limited() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(SEARCH_REPOS))
        .and(query_param_contains("q", "good"))
        .respond_with(ResponseTemplate::new(200).set_body_json(repo_hit("good")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(SEARCH_REPOS))
        .and(query_param_contains("q", "limited"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("retry-after", "1")
                .set_body_json(json!({"message":"You have exceeded a secondary rate limit."})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = runtime
        .run(
            "t".into(),
            "ghSearchRepo".into(),
            crate::support::envelope(json!({"queries":[
                {"operation":"search","keywords":["good"]},
                {"operation":"search","keywords":["limited"]}]})),
        )
        .await
        .expect("batch");
    let rows = outcome.structured_content["results"]
        .as_array()
        .expect("rows");
    let limited = rows
        .iter()
        .find(|row| row["data"]["errorCode"] == "rateLimited")
        .unwrap_or_else(|| panic!("a rateLimited row: {}", outcome.structured_content));
    assert!(
        rows.iter().any(|row| row["status"] != "error"),
        "the other row answered: {}",
        outcome.structured_content
    );
    let retry = &limited["data"]["next"]["retry"];
    assert_eq!(retry["tool"], "ghSearchRepo", "{limited}");
    assert_eq!(
        limited["data"]["rateLimit"]["retryAfterSeconds"], 1,
        "{limited}"
    );
    assert!(retry["input"]["cursor"].is_string(), "{retry}");
    let again = call(&runtime, "ghSearchRepo", retry["input"].clone())
        .await
        .expect("issued retry");
    assert_eq!(row_data(&again)["errorCode"], "rateLimited");
    assert_eq!(
        outcome.exit_class(),
        ExitClass::Failed(FailureKind::RateLimited)
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_server_outage_row_states_the_provider_delay_and_offers_a_retry() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(SEARCH_REPOS))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after", "30")
                .set_body_json(json!({"message":"Service Unavailable"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json!({"operation":"search","keywords":["x"]}),
    )
    .await
    .expect("row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "server", "{data}");
    assert_eq!(data["retryable"], true, "{data}");
    assert_eq!(data["rateLimit"]["retryAfterSeconds"], 30, "{data}");
    assert_eq!(data["next"]["retry"]["tool"], "ghSearchRepo", "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn a_permanent_failure_offers_no_retry() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(SEARCH_REPOS))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json!({"operation":"search","keywords":["x"]}),
    )
    .await
    .expect("row");
    assert!(row_data(&outcome).pointer("/next/retry").is_none());
    runtime.close().await;
}

#[tokio::test]
async fn a_401_keeps_githubs_message_beside_the_login_hint() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(SEARCH_REPOS))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json!({"operation":"search","keywords":["x"]}),
    )
    .await
    .expect("row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "authentication", "{data}");
    let error = data["error"].as_str().expect("error text");
    assert!(error.contains("authentication required"), "{data}");
    assert!(error.contains("Bad credentials"), "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn a_saml_sso_403_surfaces_the_authorization_url() {
    let server = MockServer::builder().start().await;
    let url = "https://github.com/orgs/acme/sso?authorization_request=abc";
    Mock::given(method("GET"))
        .and(path(SEARCH_REPOS))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-github-sso", format!("required; url={url}").as_str())
                .set_body_json(
                    json!({"message":"Resource protected by organization SAML enforcement."}),
                ),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json!({"operation":"search","keywords":["x"]}),
    )
    .await
    .expect("row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "permission", "{data}");
    assert_eq!(data["ssoUrl"], url, "{data}");
    let hint = data["hints"].to_string();
    assert!(hint.contains("Authorize") && hint.contains("SSO"), "{data}");
    runtime.close().await;
}

/// An empty scoped code search reads the repository once (the rename check
/// and the empty-scope check share it) and offers at most two broader
/// variants, each one more search against the 10-per-minute window.
#[tokio::test]
async fn an_empty_code_search_reads_the_repository_once_and_offers_two_variants() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"o/r","default_branch":"main","archived":false})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"o","repo":"r","keywords":["alpha","beta"],"pathPrefix":"src"}),
    )
    .await
    .expect("empty search");
    let data = row_data(&outcome);
    let repo_reads = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path() == "/api/v3/repos/o/r")
        .count();
    assert_eq!(repo_reads, 1, "GET /repos/o/r once: {data}");
    let hint = crate::support::checks::find_related_call(data, "ghSearchCode", |_| true)
        .unwrap_or(&serde_json::Value::Null);
    let variants = hint["input"]["queries"]
        .as_array()
        .expect("broader requests");
    assert_eq!(variants.len(), 2, "{data}");
}

/// A code search the token only partly covered (SAML enforcement withheld
/// organizations) says so in `warnings`.
#[tokio::test]
async fn a_partly_withheld_code_search_warns_to_authorize_the_token() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-github-sso", "partial-results; organizations=21955855")
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"acme","keywords":["alpha"]}),
    )
    .await
    .expect("search");
    let data = row_data(&outcome);
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("SAML") && warnings.contains("21955855"),
        "{data}"
    );
}
