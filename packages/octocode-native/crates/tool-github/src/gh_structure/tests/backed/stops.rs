//! Terminal request-stop handling during tree traversal.
use super::*;
use octocode_github::ProviderErrorKind;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A stopped request cannot turn an unread tree into an unavailable-tree
/// fallback. No socket is needed: admission refuses both stopped budgets.
#[tokio::test]
async fn stopped_tree_reads_preserve_timeout_and_cancellation() {
    use crate::gh_structure::traverse::{Listing, recursive_tree};
    use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider_at};

    let endpoint = octocode_github::GitHubEndpoint::new(
        url::Url::parse("http://127.0.0.1:9/api/v3").expect("endpoint URL"),
    )
    .expect("endpoint");
    let provider = one_shot_provider_at(endpoint);
    let listing = Listing {
        owner: "a",
        repo: "b",
        branch: TREE_SHA,
        root: "",
        max_depth: 2,
    };
    let cancelled = default_context();
    cancelled.budget.cancellation.cancel();
    let expired = octocode_github::RequestContext::with_timeout(std::time::Duration::ZERO, 1024);
    for (context, expected) in [
        (&cancelled, ProviderErrorKind::Cancelled),
        (&expired, ProviderErrorKind::Timeout),
    ] {
        let result = recursive_tree(&provider, &listing, context).await;
        assert!(
            matches!(result, Err(ref error) if error.kind == expected),
            "a stopped tree read must propagate {expected:?}"
        );
    }
}

/// Cancellation below a successfully read root cannot become a successful
/// partial listing.
#[tokio::test]
async fn cancelled_child_directory_does_not_return_partial_success() {
    use crate::gh_structure::traverse::{Listing, Pace, walk_contents};
    use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
    let server = MockServer::builder().start().await;
    let provider = one_shot_provider(&server);
    let listing = Listing {
        owner: "a",
        repo: "b",
        branch: TREE_SHA,
        root: "",
        max_depth: 2,
    };
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json_fixture(r#"[{"name": "src", "path": "src", "type": "dir"}]"#),
    )
    .await;
    let context = default_context();
    let cancellation = context.budget.cancellation.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(move |_: &wiremock::Request| {
            cancellation.cancel();
            ResponseTemplate::new(200).set_body_json(json!([]))
        })
        .mount(&server)
        .await;
    let pace = Pace {
        resume_from: 0,
        stop_at: None,
    };
    let result = walk_contents(&provider, &listing, false, pace, &context).await;
    assert!(
        matches!(result, Err(ref error) if error.kind == ProviderErrorKind::Cancelled),
        "cancellation in a child directory must not become a partial success"
    );
}
