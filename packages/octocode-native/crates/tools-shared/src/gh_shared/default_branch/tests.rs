//! A missing ref's default-branch recovery against a mocked GitHub that
//! names the repository's default branch.
use super::*;
use crate::gh_shared::test_support::{default_context, mock_provider};
use octocode_github::{ProviderErrorKind, ProviderErrorReason, RetryPolicy};
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
use serde_json::json;
use wiremock::MockServer;

fn missing_ref_failure() -> GhFailure {
    let message = "Branch, tag, or SHA not found for a/b: \"nope\"";
    GhFailure::new(
        ProviderError::new(ProviderErrorKind::NotFound, message)
            .with_reason(ProviderErrorReason::RefNotFound),
        message,
    )
}

/// Every missing ref names the default branch. A tree listing is offered
/// again there (`atDefaultBranch`), another listing of the same path; a refs
/// lookup asks about the ref itself, so the same lookup of the default branch
/// would only restate the message and the refs listing.
#[tokio::test]
async fn only_a_read_at_the_ref_is_retried_at_the_default_branch() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main", "full_name": "a/b"}"#),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    for (operation, retried) in [("tree", true), ("refs", false)] {
        let query = json!({"operation": operation, "owner": "a", "repo": "b", "ref": "nope"});
        let failure = with_default_branch(
            &provider,
            missing_ref_failure(),
            ToolId::GhStructure,
            &query,
            &context,
        )
        .await;
        assert!(
            failure
                .message
                .ends_with("; the default branch is \"main\"."),
            "{operation}: {}",
            failure.message
        );
        let retry = failure
            .calls
            .as_deref()
            .and_then(|calls| calls.get("atDefaultBranch"));
        assert_eq!(retry.is_some(), retried, "{operation}: {:?}", failure.calls);
        if let Some(retry) = retry {
            assert_eq!(retry["input"]["queries"][0]["ref"], "main", "{retry}");
        }
    }
}
