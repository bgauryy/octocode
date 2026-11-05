//! A read-only POST through the system client: its retries, body and headers.
use super::{ArtifactHttp, ArtifactHttpRequest, JSON, SystemArtifactHttp};
use crate::providers::artifact::test_http::test_budget;
use wiremock::matchers::{body_json, header, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(server: &MockServer) -> ArtifactHttpRequest {
    ArtifactHttpRequest {
        url: url::Url::parse(&server.uri()).expect("url"),
        accept: JSON,
        authorization: None,
        dns_pin: None,
        validator: None,
    }
}

/// A 429 waits out its `Retry-After` and sends the same body again; the answer
/// after it is the answer.
#[tokio::test]
async fn a_rate_limited_post_waits_and_retries_with_its_body() {
    let server = MockServer::start().await;
    let body = serde_json::json!({"package": {"name": "lodash", "ecosystem": "npm"}});
    Mock::given(method("POST"))
        .and(header("content-type", JSON))
        .and(body_json(&body))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_json(&body))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .expect(1)
        .mount(&server)
        .await;
    let http = SystemArtifactHttp::shared()
        .expect("client")
        .with_retries(2);
    let budget = test_budget();
    let answer = http
        .post(
            request(&server),
            serde_json::to_vec(&body).expect("body"),
            &budget,
        )
        .await
        .expect("answer");
    assert_eq!((answer.status, answer.body), (200, b"{}".to_vec()));
}

/// Retries spent: the last status comes back for the caller to report.
#[tokio::test]
async fn a_post_that_stays_unavailable_returns_its_status() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&server)
        .await;
    let http = SystemArtifactHttp::shared()
        .expect("client")
        .with_retries(1);
    let budget = test_budget();
    let answer = http
        .post(request(&server), b"{}".to_vec(), &budget)
        .await
        .expect("answer");
    assert_eq!(answer.status, 503);
}
