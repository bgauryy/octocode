//! History failures: missing identity or owner, permission reasons and not-found items, commits and scopes.

use super::HISTORY_A_HEAD;
use crate::support::{self};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_missing_identity_is_a_contract_error() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b"}),
    )
    .await
    .expect_err("missing sha");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_preserves_github_permission_reason() {
    let server = MockServer::builder().start().await;
    let leaked_token = format!("ghp_{}", "a".repeat(37));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message": format!("Resource protected by organization SAML SSO authorization; token {leaked_token}")
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("permission error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error");
    assert_eq!(data["httpStatus"], 403);
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|message| message.contains("SAML SSO authorization")),
        "{data}"
    );
    checks::assert_absent(data, &leaked_token);
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_commit_not_found_surfaces_error() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/deadbeef1234567890"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    // Not-found is surfaced as a row-level error (non-panic)
    let result = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b",
               "ref": "deadbeef1234567890"}),
    )
    .await;
    // Either a top-level RuntimeError or a row-level error status
    let is_error = match &result {
        Err(_) => true,
        Ok(outcome) => {
            let status = row_status(outcome);
            status == "error" || status.is_empty()
        }
    };
    assert!(is_error, "expected not-found to surface as error");
    runtime.close().await;
}

#[tokio::test]
async fn history_repository_without_owner_is_rejected_before_provider_requests() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "repo": "b"}),
    )
    .await
    .expect_err("repository-only scope must fail native contract validation");
    assert_eq!(outcome.code, ErrorCode::InvalidInput);
    assert!(
        !outcome
            .validation_issues
            .as_ref()
            .expect("typed issues")
            .is_empty()
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
    runtime.close().await;
}

/// A bogus commit SHA (GitHub 422 "No commit found") is a missing commit:
/// `notFound`, not an invalid query.
#[tokio::test]
async fn bad_commit_sha_is_not_found() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/deadbeef"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: deadbeef"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":"deadbeef"}),
    )
    .await
    .expect("commit row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|e| e == "Commit \"deadbeef\" not found in a/b"),
        "{data}"
    );
    // A missed commit leads to a commit search in the repository,
    // never to a refs listing (a SHA-shaped ref names no branch).
    let lead = &crate::support::checks::find_related_call(data, "ghSearchHistory", |input| {
        input["queries"][0]["operation"] == "commit"
    })
    .unwrap_or(&serde_json::Value::Null);
    assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
    assert_eq!(lead["input"]["queries"][0]["operation"], "commit", "{data}");
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let discovered = call(&runtime, "ghSearchHistory", lead["input"].clone())
        .await
        .expect("commit discovery");
    assert_eq!(
        row_status(&discovered),
        "empty",
        "{}",
        discovered.structured_content
    );
    assert!(
        crate::support::checks::find_related_call(data, "ghStructure", |input| input["queries"][0]
            ["operation"]
            == "refs")
        .unwrap_or(&serde_json::Value::Null)
        .is_null(),
        "{data}"
    );
    runtime.close().await;
}

/// GitHub answers a missing pull request, issue, or comparison ref
/// with a bare 404. The error row must name what was asked for (the number
/// or the compared refs), not the generic "Repository, resource, or path not
/// found"; a comparison also leads to the repository's refs.
#[tokio::test]
async fn gh_history_item_not_found_names_the_item() {
    let server = MockServer::builder().start().await;
    // An existing repository: `main` resolves, `no-such-head` does not (422),
    // and the comparison of an unknown base is GitHub's bare 404.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HISTORY_A_HEAD))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-head"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-head"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (query, named) in [
        (
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":987654}),
            "#987654",
        ),
        (
            json!({"operation":"issue","owner":"a","repo":"b","number":987654}),
            "#987654",
        ),
        (
            json!({"operation":"compare","owner":"a","repo":"b","baseRef":"no-such-base","headRef":"main"}),
            "no-such-base...",
        ),
        (
            json!({"operation":"compare","owner":"a","repo":"b","baseRef":"main","headRef":"no-such-head"}),
            "no-such-head",
        ),
    ] {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{query}: {data}");
        let error = data["error"].as_str().unwrap_or_default();
        assert!(error.contains(named), "{query}: {data}");
        if query["operation"] != "compare" || query["headRef"] == "main" {
            assert!(error.contains("a/b"), "{query}: {data}");
        }
        if query["operation"] == "compare" {
            let lead = &crate::support::checks::find_related_call(data, "ghStructure", |input| {
                input["queries"][0]["operation"] == "refs"
            })
            .unwrap_or(&serde_json::Value::Null);
            assert_eq!(lead["tool"], "ghStructure", "{data}");
            assert_eq!(lead["input"]["queries"][0]["operation"], "refs", "{data}");
        }
    }
    runtime.close().await;
}

/// A commit listing GitHub answers with a bare 404 (missing repository
/// or ref) names the repository and ref it listed.
#[tokio::test]
async fn gh_search_history_commit_not_found_names_the_scope() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (query, named) in [
        (
            json!({"operation":"commit","owner":"a","repo":"b","ref":"no-such-ref"}),
            "\"no-such-ref\"",
        ),
        (json!({"operation":"commit","owner":"a","repo":"b"}), "a/b"),
    ] {
        let outcome = call(&runtime, "ghSearchHistory", query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{query}: {data}");
        let error = data["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(named) && error.contains("a/b"),
            "{query}: {data}"
        );
    }
    runtime.close().await;
}
