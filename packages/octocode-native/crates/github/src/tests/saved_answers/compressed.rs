//! A gzip answer is decoded before anything reads it, so the budget counts
//! decoded bytes and a saved answer holds, and revalidates, the same bytes as
//! an uncompressed one.
use super::*;
use flate2::{Compression, write::GzEncoder};
use std::io::Write;

fn gzipped(plain: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(plain).unwrap();
    encoder.finish().unwrap()
}

/// `plain` gzipped under `etag`, or 304 when the request presents `etag`.
fn gzip_validated(
    plain: &str,
    etag: &'static str,
) -> impl Fn(&Request) -> ResponseTemplate + use<> {
    let wire = gzipped(plain.as_bytes());
    move |request: &Request| {
        let presented = request
            .headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok());
        if presented == Some(etag) {
            ResponseTemplate::new(304).insert_header("etag", etag)
        } else {
            ResponseTemplate::new(200)
                .insert_header("etag", etag)
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(wire.clone())
        }
    }
}

async fn repeat_is_revalidated(etag: &'static str) {
    let server = MockServer::start().await;
    let plain = serde_json::json!({"rows": vec!["row"; 400]}).to_string();
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(gzip_validated(&plain, etag))
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = || RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    let first = transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(first.body.as_ref(), plain.as_bytes(), "decoded body");
    assert_eq!(memory.len(), 1);
    let again = transport.execute(spec(), &request("t")).await.unwrap();
    assert_eq!(again.body.as_ref(), plain.as_bytes(), "the saved answer");
    let seen = requests(&server).await;
    assert_eq!(seen.len(), 2);
    assert!(
        seen[0].headers.get("accept-encoding").is_some(),
        "gzip is requested"
    );
    assert_eq!(
        seen[1]
            .headers
            .get("if-none-match")
            .and_then(|value| value.to_str().ok()),
        Some(etag),
        "the validator is presented as received"
    );
    let saved = memory.0.lock().unwrap().values().next().cloned().unwrap();
    assert!(
        saved
            .answer
            .headers
            .iter()
            .all(|(name, _)| !matches!(name.as_str(), "content-encoding" | "content-length")),
        "a decoded body keeps no header that describes the wire bytes"
    );
}

#[tokio::test]
async fn a_gzip_answer_with_a_strong_etag_is_revalidated_by_304() {
    repeat_is_revalidated("\"abc\"").await;
}

#[tokio::test]
async fn a_gzip_answer_with_a_weak_etag_is_revalidated_by_304() {
    repeat_is_revalidated("W/\"abc\"").await;
}

#[tokio::test]
async fn a_gzip_answer_over_the_body_budget_is_refused_and_not_kept() {
    let server = MockServer::start().await;
    let wire = gzipped(&vec![b' '; 4 << 20]);
    assert!(wire.len() < 64 << 10, "small on the wire");
    Mock::given(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"big\"")
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(wire),
        )
        .mount(&server)
        .await;
    let (transport, endpoint, memory) = keeping(&server);
    let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
    let refused = transport
        .execute(spec, &with_token("t", Duration::from_secs(5), 1 << 20))
        .await;
    assert!(refused.is_err(), "decoded size counts against the budget");
    assert_eq!(memory.len(), 0);
}

/// Bytes that do not decode as their `Content-Encoding` says are refused, not
/// kept, and named as what they are: the server answered, so asking again gets
/// the same bytes and no network failure happened.
#[tokio::test]
async fn an_answer_that_does_not_decode_is_a_decode_error_and_not_retried() {
    let truncated = {
        let mut wire = gzipped(
            serde_json::json!({"rows": vec!["row"; 400]})
                .to_string()
                .as_bytes(),
        );
        wire.truncate(wire.len() / 2);
        wire
    };
    for wire in [b"{\"plain\":true}".to_vec(), truncated] {
        let server = MockServer::start().await;
        Mock::given(path("/api/v3/repos/a/b"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-encoding", "gzip")
                    .set_body_bytes(wire),
            )
            .mount(&server)
            .await;
        let (transport, endpoint, memory) = keeping(&server);
        let spec = RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).unwrap());
        let error = transport.execute(spec, &request("t")).await.unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::Decode, "{error:?}");
        assert!(!error.retryable, "{error:?}");
        assert!(error.message.contains("Content-Encoding"), "{error:?}");
        assert_eq!(requests(&server).await.len(), 1, "sent once");
        assert_eq!(memory.len(), 0, "not kept");
    }
}
