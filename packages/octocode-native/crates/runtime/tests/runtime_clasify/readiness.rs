use crate::support::{Run as _, Workspace, provider_env};
use octocode_contract_types::ErrorCode;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn clasify_provider_errors_only_on_requested_models_invocation() {
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
    ] {
        let server = MockServer::start().await;
        let workspace = Workspace::new();
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer fixture-key"))
            .respond_with(response)
            .expect(1..)
            .mount(&server)
            .await;
        let runtime = workspace.runtime(&provider_env(
            &server,
            &[
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key".into()),
                ("OCTOCODE_BETA", "false".into()),
            ],
        ));
        assert!(runtime.is_available("clasify"));
        let _catalog = runtime.catalog();
        assert!(server.received_requests().await.unwrap().is_empty());
        let result = runtime
            .run(
                "models".into(),
                "clasify".into(),
                json!({"queries":[{"operation":"models"}]}),
            )
            .await;
        let outcome = result.expect("provider failures are isolated result rows");
        if let Some(code) = expected {
            assert_eq!(
                outcome.structured_content["results"][0]["data"]["errorCode"],
                json!(code)
            );
            assert!(
                !outcome
                    .structured_content
                    .to_string()
                    .contains("fixture-key")
            );
        } else {
            assert!(!outcome.all_failed, "{:?}", outcome.structured_content);
        }
        assert!(
            runtime.is_available("clasify"),
            "provider response must not change discovery"
        );
        runtime.close().await;
    }
}

#[tokio::test]
async fn runtime_and_catalog_construction_never_contact_provider() {
    let server = MockServer::start().await;
    let workspace = Workspace::new();
    for env in [
        vec![("OCTOCODE_CLASSIFICATION_API", "".into())],
        vec![("OCTOCODE_CLASSIFICATION_API", "  ".into())],
        vec![("DISABLE_TOOLS", "clasify".into())],
        vec![("TOOLS_TO_RUN", "localFetch".into())],
        vec![("OCTOCODE_CLASSIFICATION_API", "fixture-key".into())],
    ] {
        for surface in [
            octocode_native::config::RuntimeSurface::Cli,
            octocode_native::config::RuntimeSurface::Mcp,
        ] {
            let mut input = workspace.config(&provider_env(&server, &env));
            input.runtime_surface = surface;
            let runtime = octocode_native::runtime::ToolRuntime::with_limits(
                input,
                crate::support::FIXTURE_LIMITS,
            )
            .expect("runtime");
            let _catalog = runtime.catalog();
            runtime.close().await;
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}
