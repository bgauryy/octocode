//! Tag-ref listings follow every linked page the request's deadline admits.
use super::*;

#[tokio::test]
async fn tag_names_follow_every_linked_page() {
    let server = MockServer::builder().start().await;
    let pages = 25;
    for page in 1..=pages {
        let mut response = ResponseTemplate::new(200)
            .set_body_json(serde_json::json!([{"ref": format!("refs/tags/v{page}")}]));
        if page < pages {
            let next = format!(
                "{}/api/v3/repos/o/r/git/matching-refs/tags?page={}",
                server.uri(),
                page + 1
            );
            response = response.insert_header("link", format!("<{next}>; rel=\"next\""));
        }
        let mock = Mock::given(method("GET")).and(path("/api/v3/repos/o/r/git/matching-refs/tags"));
        let mock = if page == 1 {
            mock.and(wiremock::matchers::query_param_is_missing("page"))
        } else {
            mock.and(query_param("page", page.to_string()))
        };
        mock.respond_with(response).mount(&server).await;
    }
    let endpoint = mock_endpoint(&server);
    let names = anonymous_transport(&endpoint)
        .repository_tag_names(
            "o",
            "r",
            &RequestContext::with_timeout(Duration::from_secs(10), 1 << 20),
        )
        .await
        .expect("every page");
    assert_eq!(
        names,
        (1..=pages)
            .map(|page| format!("v{page}"))
            .collect::<Vec<_>>()
    );
}
