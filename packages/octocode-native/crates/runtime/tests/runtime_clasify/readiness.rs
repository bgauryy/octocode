use crate::support::{Run as _, Workspace, provider_env};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn clasify_provider_ping_controls_availability_without_beta() {
    for (response, expected) in [
        (
            ResponseTemplate::new(200).set_body_json(json!({"models":[]})),
            None,
        ),
        (
            ResponseTemplate::new(401),
            Some(ErrorCode::ClassificationProviderError),
        ),
        (
            ResponseTemplate::new(503),
            Some(ErrorCode::ClassificationProviderUnavailable),
        ),
        (
            ResponseTemplate::new(200).set_body_json(json!({"wrong":[]})),
            Some(ErrorCode::InvalidClassificationResponse),
        ),
        (
            ResponseTemplate::new(200)
                .set_body_json(json!({"models":[]}))
                .set_delay(Duration::from_secs(5)),
            Some(ErrorCode::Timeout),
        ),
    ] {
        let server = MockServer::start().await;
        let workspace = Workspace::new();
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer fixture-key"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let started = Instant::now();
        let runtime = workspace
            .runtime(&provider_env(
                &server,
                &[
                    ("OCTOCODE_CLASSIFICATION_API", "fixture-key".into()),
                    ("OCTOCODE_BETA", "false".into()),
                ],
            ))
            .check_clasify_provider()
            .await;
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(runtime.is_available("clasify"), expected.is_none());
        assert_eq!(runtime.catalog()["presentation"]["selection"], true);
        if let Some(code) = expected {
            let error = runtime
                .run(
                    "probe".into(),
                    "clasify".into(),
                    json!({"queries":[{"operation":"models"}]}),
                )
                .await
                .expect_err("provider blocked");
            assert_eq!(error.code, code);
            assert!(error.message.contains("ping failed"));
            assert!(!error.message.contains("fixture-key"));
        }
        runtime.close().await;
    }
}

#[tokio::test]
async fn clasify_ping_skips_missing_keys_and_explicit_tool_exclusions() {
    let server = MockServer::start().await;
    let workspace = Workspace::new();
    for env in [
        vec![("OCTOCODE_CLASSIFICATION_API", "".into())],
        vec![("OCTOCODE_CLASSIFICATION_API", "  ".into())],
        vec![("DISABLE_TOOLS", "clasify".into())],
        vec![("TOOLS_TO_RUN", "localFetch".into())],
    ] {
        let runtime = workspace
            .runtime(&provider_env(&server, &env))
            .check_clasify_provider()
            .await;
        assert!(!runtime.is_available("clasify"));
        runtime.close().await;
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}
