//! Ref resolution: the default branch, memoized SHAs, missing refs and tag refs.
use super::*;

#[tokio::test]
async fn default_branch_resolves_with_one_sha_request_and_is_memoized_per_batch() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    // No repository-metadata round trip: HEAD resolves the default branch.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/commits/HEAD"))
        .and(header("accept", "application/vnd.github.sha"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .expect(1)
        .mount(&server)
        .await;
    for name in ["a.rs", "b.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/acme/repo/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type":"file","encoding":"base64","content":STANDARD.encode("x\n")
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let base = provider(&server).await;
    let provider = GitHubProvider::new(base.transport, KeyedCache::default());
    let context = with_token("secret", Duration::from_secs(2), 1024);
    for name in ["a.rs", "b.rs", "a.rs"] {
        let result = provider
            .get_file_bytes(
                &ContentRequest {
                    owner: "acme".into(),
                    repo: "repo".into(),
                    path: name.into(),
                    reference: None,
                    force_refresh: false,
                },
                &context,
            )
            .await
            .expect("content");
        assert_eq!(result.resolved_ref, sha);
        assert_eq!(result.bytes, &b"x\n"[..]);
    }
}

#[tokio::test]
async fn a_resolved_full_sha_is_cached_per_credential() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .expect(2)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = |token: &str| {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            Some(ResolvedCredential::new(token, CredentialSource::Storage)),
        )
    };
    // alice twice (one request), bob once (his own partition): two requests.
    for token in ["alice", "alice", "bob"] {
        let resolved = transport
            .commit_sha("o", "r", sha, &context(token))
            .await
            .expect("resolved");
        assert_eq!(resolved, sha);
    }
}

/// A ref GitHub cannot resolve (422 "No commit found") is not-found input
/// with a typed reason, not a malformed request; a 404 stays the repository.
#[tokio::test]
async fn missing_ref_is_not_found_with_a_ref_reason() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits/nope",
        422,
        serde_json::json!({"message": "No commit found for SHA: nope"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/gone/commits/main",
        404,
        serde_json::json!({"message": "Not Found"}),
    )
    .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1 << 20);
    let error = provider
        .transport
        .commit_sha("o", "r", "nope", &context)
        .await
        .expect_err("missing ref");
    assert_eq!(error.kind, ProviderErrorKind::NotFound, "{error:?}");
    assert_eq!(error.reason, Some(ProviderErrorReason::RefNotFound));
    assert_eq!(error.status, Some(422));
    let error = provider
        .transport
        .commit_sha("o", "gone", "main", &context)
        .await
        .expect_err("missing repository");
    assert_eq!(error.reason, Some(ProviderErrorReason::RepositoryNotFound));
}

/// A release tag is a tag ref: a branch of the same name (which
/// `commits/{name}` would resolve) is not one, and a tag name keeps its
/// `/` as path segments.
#[tokio::test]
async fn a_tag_exists_only_as_a_tag_ref() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/ref/tags/pkg/v1.2.3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ref": "refs/tags/pkg/v1.2.3", "object": {"sha": "1".repeat(40), "type": "tag"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/ref/tags/1.2.3"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .mount(&server)
        .await;
    // The branch `1.2.3` resolves as a commit, but is no tag.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/1.2.3"))
        .respond_with(ResponseTemplate::new(200).set_body_string("2".repeat(40)))
        .mount(&server)
        .await;
    let transport = anonymous_transport(&mock_endpoint(&server));
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
    assert!(
        transport
            .tag_exists("a", "b", "pkg/v1.2.3", &context)
            .await
            .expect("tag read")
    );
    assert!(
        !transport
            .tag_exists("a", "b", "1.2.3", &context)
            .await
            .expect("tag read")
    );
}
