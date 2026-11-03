//! Independent reads always consult GitHub, including commit-pinned resources.
use super::*;

#[tokio::test]
async fn pinned_content_and_listings_are_read_fresh() {
    let server = MockServer::builder().start().await;
    let sha = "2222222222222222222222222222222222222222";
    mount_file(
        &server,
        "/api/v3/repos/a/b/contents/pinned.rs",
        Duration::ZERO,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "tree")
                .set_body_json(serde_json::json!([
                    {"name":"a.rs","path":"src/a.rs","type":"file","size":1}
                ])),
        )
        .expect(2)
        .mount(&server)
        .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1024);
    for _ in 0..2 {
        let content = provider
            .get_file_bytes(&file_at("pinned.rs", sha), &context)
            .await
            .expect("content");
        assert_eq!(content.bytes, &b"body\n"[..]);
        assert_eq!(content.resolved_ref, sha);
        assert!(content.raw_response_bytes > 0);
        assert_eq!(
            provider
                .repository_contents("a", "b", "src", sha, &context)
                .await
                .expect("listing")
                .entries
                .len(),
            1
        );
    }
    assert_eq!(
        requests_to(&server, "/api/v3/repos/a/b/contents/pinned.rs").await,
        2
    );
    for request in server.received_requests().await.unwrap() {
        assert!(!request.headers.contains_key("if-none-match"));
    }
}
