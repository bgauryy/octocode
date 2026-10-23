//! The typed errors, credential header and body bounds of GitHub responses.
use super::response::{authorize, read_bounded, response_error};
use super::*;
use crate::ProviderErrorReason;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;

/// A loopback API (a test server, a local proxy to GHES) is reached
/// directly: no system proxy lookup, which is the first client's
/// startup cost, and no proxy that could see the token.
#[test]
fn a_loopback_api_connects_directly() {
    use octocode_io::http::Proxy;
    for (base, proxy) in [
        ("http://127.0.0.1:8080/api/v3", Proxy::Direct),
        ("http://localhost:1/api/v3", Proxy::Direct),
        ("https://api.github.com", Proxy::System),
        ("https://ghe.example.com/api/v3", Proxy::System),
    ] {
        let endpoint = GitHubEndpoint::new(url::Url::parse(base).expect("url")).expect("endpoint");
        assert_eq!(client_policy(&endpoint).proxy, proxy, "{base}");
    }
}

/// Review open item: the token header is marked sensitive so `Debug`
/// output of a request or header map never prints it.
#[test]
fn authorization_header_is_sensitive() {
    let request = authorize(
        reqwest::Client::new().get("https://api.github.com/"),
        "ghp_secret",
    )
    .build()
    .expect("request");
    let value = &request.headers()[AUTHORIZATION];
    assert_eq!(value, "Bearer ghp_secret");
    assert!(value.is_sensitive());
    assert!(!format!("{:?}", request.headers()).contains("ghp_secret"));
}

/// Every validation cause GitHub returns is kept (never-trim).
#[test]
fn validation_errors_keep_every_detail() {
    let body = Bytes::from_static(
        br#"{"message":"Validation Failed","errors":[{"message":"a"},{"message":"b"},{"message":"c"},{"message":"d"},{"message":"e"}]}"#,
    );
    let error = response_error(StatusCode::UNPROCESSABLE_ENTITY, &HeaderMap::new(), body);
    assert_eq!(&*error.message, "Validation Failed: a; b; c; d; e");
}

#[test]
fn validation_errors_keep_github_detail() {
    let body = Bytes::from_static(
        br#"{"message":"Validation Failed","errors":[{"message":"\"abc\" is not a numeric value"}]}"#,
    );
    let error = response_error(StatusCode::UNPROCESSABLE_ENTITY, &HeaderMap::new(), body);
    assert_eq!(error.kind, ProviderErrorKind::Validation);
    assert_eq!(
        &*error.message,
        "Validation Failed: \"abc\" is not a numeric value"
    );
    let bare = response_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        &HeaderMap::new(),
        Bytes::from_static(br#"{"message":"Validation Failed"}"#),
    );
    assert_eq!(&*bare.message, "Validation Failed");
}

/// GitHub's "No commit found" (422 on commits/compare, 404 on contents)
/// is a missing ref for every tool, typed once here.
#[test]
fn no_commit_found_is_a_missing_ref() {
    for (status, body) in [
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            &br#"{"message":"No commit found for SHA: nope"}"#[..],
        ),
        (
            StatusCode::NOT_FOUND,
            &br#"{"message":"No commit found for the ref nope"}"#[..],
        ),
    ] {
        let error = response_error(status, &HeaderMap::new(), Bytes::from_static(body));
        assert_eq!(error.kind, ProviderErrorKind::NotFound);
        assert_eq!(error.reason, Some(ProviderErrorReason::RefNotFound));
        assert_eq!(error.status, Some(status.as_u16()));
    }
    let missing = response_error(
        StatusCode::NOT_FOUND,
        &HeaderMap::new(),
        Bytes::from_static(br#"{"message":"Not Found"}"#),
    );
    assert_eq!(missing.reason, None);
}

#[tokio::test]
async fn a_body_that_stalls_after_the_headers_hits_the_request_deadline() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Promise 1000 bytes, send 5, then go silent.
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\nhello")
                .await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
    let response = reqwest::get(format!("http://{addr}/"))
        .await
        .expect("headers arrive");
    let context = RequestContext::with_timeout(Duration::from_millis(300), 1 << 20);
    let started = Instant::now();
    let error = read_bounded(response, &context)
        .await
        .expect_err("stalled body times out");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "bounded by the deadline"
    );
}
