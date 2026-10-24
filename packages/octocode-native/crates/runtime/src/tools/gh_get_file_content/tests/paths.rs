//! Path resolution, submodules, missing paths and failure hints.
use super::*;

/// ghGetFileContent reads a path by the one repository-path rule
/// ghStructure lists by: `.`, `..` and repeated slashes resolve before
/// the one contents request, and the row names the resolved path; a
/// path above the root, or the root itself, is invalid input with no
/// request sent.
#[tokio::test]
async fn paths_resolve_like_listings_or_are_refused_unsent() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/README.md",
        200,
        json!({"type":"file", "encoding":"base64", "content":STANDARD.encode("# Title\n")}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let read = run_default(
        &provider,
        json!({"owner":"a","repo":"b","path":"src/.//../README.md","ref":sha}),
    )
    .await
    .unwrap_or_else(|failure| panic!("{}", failure.message));
    assert_eq!(
        read.output.data["path"], "README.md",
        "{}",
        read.output.data
    );
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path().contains("/contents/"))
            .count(),
        1,
        "{requests:?}"
    );
    let acquisition_count = requests.len();
    for refused in ["../x.md", "src/../../x.md", "src/..", "./"] {
        let Err(failure) = run_default(
            &provider,
            json!({"owner":"a","repo":"b","path":refused,"ref":sha}),
        )
        .await
        else {
            panic!("{refused} is refused");
        };
        assert_eq!(
            failure.error.kind,
            octocode_github::ProviderErrorKind::Validation,
            "{refused}"
        );
        assert!(
            failure.message.contains(refused),
            "{refused}: {}",
            failure.message
        );
    }
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(
        requests.len(),
        acquisition_count,
        "refusals send nothing: {requests:?}"
    );
}

/// A path whose case differs is answered by one listing walk at the
/// resolved commit: one case-corrected `read` lead, and the one hint
/// names where that lead is. No listing at `HEAD` is spent.
#[tokio::test]
async fn wrong_case_path_leads_once_to_the_case_corrected_read() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2FLib.rs",
        404,
        serde_json::json!({"message":"Not Found"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src",
        200,
        serde_json::json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file"},
            {"name":"main.rs","path":"src/main.rs","type":"file"}
        ]),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let Err(failure) = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"src/Lib.rs"}),
    )
    .await
    else {
        panic!("a missing path is a failure")
    };
    let next = failure.calls.expect("recovery leads");
    assert_eq!(
        next["read"]["input"]["queries"][0]["path"], "src/lib.rs",
        "{next}"
    );
    assert_eq!(
        failure.hints,
        vec!["Only the path's case differs.".to_owned()]
    );
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(
        !requests
            .iter()
            .any(|request| request.url.query().is_some_and(|q| q.contains("ref=HEAD"))),
        "a listing at HEAD was spent: {requests:?}"
    );
}

/// A submodule path leads to the submodule's repository on the
/// configured GitHub host at the commit the parent pins, not to the
/// generic invalid-field tip; a submodule hosted elsewhere gets no
/// GitHub lead.
#[tokio::test]
async fn a_submodule_path_leads_to_its_repository_at_the_pinned_commit() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let pinned = "89abcdef0123456789abcdef0123456789abcdef";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Flib",
        200,
        serde_json::json!({"type":"submodule","submodule_git_url":format!("{}/o2/r2.git", server.uri()),"sha":pinned}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Fext",
        200,
        serde_json::json!({"type":"submodule","submodule_git_url":"https://gitlab.com/o3/r3.git","sha":pinned}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let Err(failure) = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"vendor/lib","ref":sha}),
    )
    .await
    else {
        panic!("a submodule path is a failure");
    };
    let next = failure.calls.expect("submodule lead");
    let lead = &next["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{next}");
    let row = &lead["input"]["queries"][0];
    assert_eq!(row["owner"], "o2", "{next}");
    assert_eq!(row["repo"], "r2", "{next}");
    assert_eq!(row["ref"], pinned, "{next}");
    assert!(failure.hints.is_empty(), "{:?}", failure.hints);
    let Err(elsewhere) = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"vendor/ext","ref":sha}),
    )
    .await
    else {
        panic!("a submodule path is a failure");
    };
    assert!(elsewhere.calls.is_none(), "{:?}", elsewhere.calls);
    assert_eq!(elsewhere.hints.len(), 1, "{:?}", elsewhere.hints);
    assert!(
        !elsewhere.hints[0].contains("rejected field"),
        "{:?}",
        elsewhere.hints
    );
}

/// A submodule URL names a repository only on the host the configured
/// endpoint serves, in every clone URL form.
#[test]
fn submodule_urls_resolve_on_the_configured_host_only() {
    let public = octocode_github::GitHubEndpoint::new(
        url::Url::parse("https://api.github.com").expect("URL"),
    )
    .expect("endpoint");
    let enterprise = octocode_github::GitHubEndpoint::new(
        url::Url::parse("https://ghe.example.com/api/v3").expect("URL"),
    )
    .expect("endpoint");
    let named = Some(("o".to_owned(), "r".to_owned()));
    for url in [
        "https://github.com/o/r.git",
        "http://github.com/o/r",
        "git://github.com/o/r.git",
        "ssh://git@github.com/o/r.git",
        "git@github.com:o/r.git",
    ] {
        assert_eq!(
            crate::tools::gh_shared::hosted_repository(url, &public),
            named,
            "{url}"
        );
        assert_eq!(
            crate::tools::gh_shared::hosted_repository(url, &enterprise),
            None,
            "{url}"
        );
    }
    assert_eq!(
        crate::tools::gh_shared::hosted_repository("git@ghe.example.com:o/r.git", &enterprise),
        named
    );
    assert_eq!(
        crate::tools::gh_shared::hosted_repository("https://gitlab.com/o/r.git", &public),
        None
    );
}

/// A missing path read at a branch or tag names that ref and the commit
/// it resolved to; the recovery listing pins that commit.
#[tokio::test]
async fn a_missing_path_at_a_ref_names_the_ref_and_its_commit() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Fmissing.rs",
        404,
        serde_json::json!({"message":"Not Found"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src",
        200,
        serde_json::json!([{"name":"lib.rs","path":"src/lib.rs","type":"file","size":3}]),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let Err(failure) = run_default(
        &provider,
        serde_json::json!({"owner":"a","repo":"b","path":"src/missing.rs","ref":"v1"}),
    )
    .await
    else {
        panic!("a missing path is a failure");
    };
    assert_eq!(
        failure.message,
        "Path not found in a/b at v1 (0123456): src/missing.rs"
    );
    let next = failure.calls.expect("recovery");
    assert_eq!(
        next["viewTree"]["input"]["queries"][0]["ref"], sha,
        "{next}"
    );
}

/// A failed read takes the shared GitHub hint for the error, so a SAML
/// organization's read names the authorization URL step like every other
/// GitHub tool, and an unauthenticated one names the sign-in step.
#[test]
fn a_failed_read_takes_the_shared_provider_hint() {
    use crate::tools::gh_shared::provider_hint;
    use octocode_github::{ProviderError, ProviderErrorKind};
    let query: GhGetFileContentQuery = serde_json::from_value(serde_json::json!({
        "owner":"a","repo":"b","path":"src/lib.rs"}))
    .expect("query");
    let mut saml = ProviderError::new(ProviderErrorKind::Permission, "Resource protected by SAML");
    saml.sso_url = Some("https://github.com/orgs/a/sso".into());
    let unauthenticated = ProviderError::new(ProviderErrorKind::Authentication, "Bad credentials");
    for error in [saml, unauthenticated] {
        let expected = provider_hint(&error).expect("shared hint");
        let failure = errors::failure(error, &query, None);
        assert_eq!(failure.hints, [expected]);
    }
}
