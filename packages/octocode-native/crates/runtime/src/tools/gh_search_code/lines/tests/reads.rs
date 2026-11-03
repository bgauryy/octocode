//! A hit's read is bounded by what the scan accepts, and disclosed when it is not read.
use super::*;
use crate::tools::gh_shared::test_support::{fixture_context, one_shot_provider};
use octocode_github::{ContentRequest, ProviderErrorKind};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn read(path: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some("0123456789abcdef0123456789abcdef01234567".into()),
    }
}

#[test]
fn a_read_never_exceeds_the_scan_limit_nor_raises_a_smaller_budget() {
    let large = fixture_context(Duration::from_secs(5), 16 * MAX_SCAN_BYTES);
    assert_eq!(read_bound(&large).budget.max_body_bytes, MAX_SCAN_BYTES);
    let small = fixture_context(Duration::from_secs(5), 1024);
    assert_eq!(read_bound(&small).budget.max_body_bytes, 1024);
}

/// A blob over the scan limit is refused while it downloads; one within it
/// is read and scanned. The oversize file stays `Unavailable`, which the
/// row states as `lineResolved: false` with its index fragments kept.
#[tokio::test]
async fn an_oversize_hit_is_refused_during_the_read_and_the_rest_resolve() {
    let server = MockServer::builder().start().await;
    for (name, size) in [("big.txt", MAX_SCAN_BYTES + 1), ("small.txt", 0)] {
        let body = if size == 0 {
            "one\nneedle\n".to_owned()
        } else {
            "x".repeat(size)
        };
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/vnd.github.raw; charset=utf-8")
                    .insert_header("etag", "\"0123456789abcdef0123456789abcdef01234567\"")
                    .set_body_bytes(body.into_bytes()),
            )
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let context = fixture_context(Duration::from_secs(5), 16 * MAX_SCAN_BYTES);
    let error = provider
        .get_file_bytes(&read("big.txt"), &read_bound(&context))
        .await
        .expect_err("over the scan limit");
    assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
    let hits = resolve_files(
        &provider,
        "a",
        "b",
        "0123456789abcdef0123456789abcdef01234567",
        &["big.txt".to_owned(), "small.txt".to_owned()],
        &Listing {
            keywords: &["needle".to_owned()],
            goal: "",
            window: Window::default(),
        },
        &context,
        &Passthrough,
    )
    .await
    .expect("resolved");
    assert_eq!(hits[0], FileHits::Unavailable);
    assert!(
        matches!(hits[1], FileHits::Lines { total: 1, .. }),
        "{:?}",
        hits[1]
    );
}
