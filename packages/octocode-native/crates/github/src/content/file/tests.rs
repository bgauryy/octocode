use super::*;
use crate::{GitHubBudget, GitHubEndpoint, GitHubTransport, RetryPolicy};
use octocode_io::budget::RequestBudget;
use std::time::Duration;

fn json_page(body: serde_json::Value) -> ResponsePage {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    ResponsePage {
        status: 200,
        headers,
        body: serde_json::to_vec(&body).expect("JSON").into(),
        next: None,
        redirected: false,
    }
}

#[test]
fn a_json_blob_response_yields_its_decoded_file_bytes() {
    let bytes = b"\x89PNG\r\n\x1a\n\0binary";
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut page = json_page(serde_json::json!({
        "encoding": "base64", "content": STANDARD.encode(bytes), "size": bytes.len()
    }));
    for media_type in [
        "application/json",
        "application/vnd.github+json; charset=utf-8",
        "Application/JSON",
    ] {
        page.headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(media_type).expect("media type"),
        );
        assert_eq!(blob_bytes(&page).expect("blob"), &bytes[..], "{media_type}");
    }
}

#[test]
fn raw_blob_bytes_are_never_interpreted_as_a_json_envelope() {
    let mut page = json_page(serde_json::json!({
        "encoding": "base64", "content": "c291cmNl"
    }));
    page.headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.github.raw+json; charset=utf-8"),
    );
    assert_eq!(blob_bytes(&page).expect("raw JSON source"), page.body);
    page.headers.remove(CONTENT_TYPE);
    assert_eq!(blob_bytes(&page).expect("untyped raw source"), page.body);
}

#[test]
fn malformed_json_blob_content_is_refused() {
    for body in [
        serde_json::json!({"encoding": "base64"}),
        serde_json::json!({"encoding": "base64", "content": "%%%"}),
        serde_json::json!({"encoding": "none", "content": ""}),
    ] {
        let page = json_page(body);
        assert!(matches!(
            blob_bytes(&page),
            Err(error) if error.kind == ProviderErrorKind::Decode
        ));
    }
}

#[tokio::test]
async fn unavailable_content_without_a_blob_is_not_an_empty_file() {
    let endpoint = GitHubEndpoint::new(url::Url::parse("https://api.github.com/").expect("URL"))
        .expect("endpoint");
    let provider = GitHubProvider::new(
        GitHubTransport::new(endpoint, RetryPolicy::default(), GitHubBudget::relaxed())
            .expect("transport"),
    );
    let context = RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(1), 1024),
        None,
    );
    let request = ContentRequest {
        owner: "acme".into(),
        repo: "repo".into(),
        path: "source.rs".into(),
        reference: None,
    };
    for content in [serde_json::Value::Null, serde_json::json!("")] {
        let page = json_page(serde_json::json!({
            "type": "file", "encoding": "base64", "content": content, "size": 12
        }));
        let result = provider
            .page_bytes(&request, &page, &context, BodyOwner::Transport)
            .await;
        assert!(
            matches!(result, Err(error) if error.kind == ProviderErrorKind::Decode),
            "an unavailable nonempty file must not become empty content"
        );
    }
}
