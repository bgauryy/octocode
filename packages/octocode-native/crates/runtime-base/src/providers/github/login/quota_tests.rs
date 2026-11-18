use super::quota;
use octocode_test_support::json_fixture;
use std::time::Duration;
use wiremock::matchers::{header, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn quota_names_each_bucket_the_tools_draw_on() {
    let server = MockServer::builder().start().await;
    Mock::given(path("/api/v3/rate_limit"))
        .and(header("authorization", "Bearer t"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "resources": {
                    "core": {"limit": 5000, "remaining": 4321, "reset": 86400},
                    "search": {"limit": 30, "remaining": 29, "reset": 86400},
                    "code_search": {"limit": 10, "remaining": 9, "reset": 86400},
                    "graphql": {"limit": 5000, "remaining": 5000, "reset": 86400},
                    "dependency_snapshots": {"limit": 100, "remaining": 100, "reset": 86400}
                }
            }"#,
        )))
        .mount(&server)
        .await;
    let api = format!("{}/api/v3", server.uri());
    let buckets = quota(&api, "t", Duration::from_secs(5)).await;
    let names: Vec<_> = buckets.iter().map(|q| q.resource.as_str()).collect();
    assert_eq!(names, ["core", "search", "code_search", "graphql"]);
    assert_eq!((buckets[0].remaining, buckets[0].limit), (4321, 5000));
    assert_eq!(buckets[0].reset_at, "1970-01-02T00:00:00Z");
    assert!(
        quota(&api, "wrong", Duration::from_secs(5))
            .await
            .is_empty()
    );
}
