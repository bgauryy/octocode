//! Contents, blobs, single-flight and the response cache for reads.
use super::*;

mod blobs;
mod cache;
mod flight;
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
                force_refresh: false,
            },
            &with_token("secret", Duration::from_secs(2), 1024),
        )
        .await
        .expect("content");
    assert_eq!(result.bytes, &b"hello\n"[..]);
    assert_eq!(result.etag.as_deref(), Some("\"v1\""));
    assert!(!result.from_cache);
}

async fn keyed_provider(server: &MockServer) -> GitHubProvider {
    GitHubProvider::new(provider(server).await.transport, KeyedCache::default())
}

fn file_at(path: &str, reference: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some(reference.into()),
        force_refresh: false,
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

/// Key-aware cache so ref memo and body entries coexist.
#[derive(Default)]
struct KeyedCache(Mutex<std::collections::HashMap<String, CachedContent>>);

impl ConditionalCache for KeyedCache {
    fn get<'a>(
        &'a self,
        _: &'a Partition,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        Box::pin(async move { self.0.lock().expect("cache lock").get(key).cloned() })
    }
    fn put<'a>(
        &'a self,
        _: &'a Partition,
        key: String,
        value: CachedContent,
        _: Retention,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.0.lock().expect("cache lock").insert(key, value);
        })
    }
}
