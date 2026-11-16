//! Contents, blobs, independent requests and commit identity.
use super::*;

mod blobs;
mod flight;
mod fresh;
mod refs;
mod repository;

#[tokio::test]
async fn ghes_content_route_auth_and_decode() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/acme/repo/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    Mock::given(method("GET")).and(path("/api/v3/repos/acme/repo/contents/src%2Flib.rs")).and(query_param("ref",sha)).and(header("authorization","Bearer secret"))
        .respond_with(ResponseTemplate::new(200).insert_header("etag","\"v1\"").set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("hello\n")}))).mount(&server).await;
    let result = provider(&server)
        .await
        .get_file_bytes(
            &ContentRequest {
                owner: "acme".into(),
                repo: "repo".into(),
                path: "src/lib.rs".into(),
                reference: Some("main".into()),
            },
            &with_token("secret", Duration::from_secs(2), 1024),
        )
        .await
        .expect("content");
    assert_eq!(result.bytes, &b"hello\n"[..]);
}

fn file_at(path: &str, reference: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some(reference.into()),
    }
}

async fn mount_file(server: &MockServer, path_pattern: &str, delay: Duration) {
    Mock::given(method("GET"))
        .and(path(path_pattern))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_delay(delay)
                .set_body_json(serde_json::json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode("body\n")})),
        )
        .mount(server)
        .await;
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().starts_with(route))
        .count()
}

/// A body its caller stores itself is kept nowhere else,
/// so a continuation that request issues owns no copy of it either. The
/// same read through `get_file_bytes` is recorded as evidence, as before.
#[tokio::test]
async fn a_body_kept_by_its_caller_is_not_continuation_evidence() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_file(&server, "/api/v3/repos/a/b/contents/one.rs", Duration::ZERO).await;
    let provider = provider(&server).await;
    let contents_bodies = |context: &RequestContext| {
        context
            .evidence()
            .answers
            .iter()
            .filter(|answer| answer.request.contains("/contents/"))
            .map(|answer| answer.body.len())
            .sum::<usize>()
    };
    let caller = with_token("caller", Duration::from_secs(2), 1024);
    let read = provider
        .get_file_bytes_kept_by_caller(&file_at("one.rs", sha), &caller)
        .await
        .expect("kept by caller");
    assert_eq!(read.bytes, &b"body\n"[..]);
    assert_eq!(
        contents_bodies(&caller),
        0,
        "{:?}",
        caller.evidence().answers
    );
    let transport = with_token("transport", Duration::from_secs(2), 1024);
    provider
        .get_file_bytes(&file_at("one.rs", sha), &transport)
        .await
        .expect("kept by the transport");
    assert!(
        contents_bodies(&transport) > 0,
        "an ordinary read is evidence"
    );
}
