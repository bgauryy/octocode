//! Version-range support per ecosystem.
use super::query::versioned;
use super::test_http::RouteHttp;
use super::test_http::test_budget;
use super::*;

/// An ecosystem that takes exact versions refuses a range before any
/// registry read, saying so (a range is never a version to look up);
/// its exact versions, however spelled, pass the gate.
#[tokio::test]
async fn an_exact_only_ecosystem_refuses_a_version_range() {
    let http = RouteHttp::new();
    let budget = test_budget();
    let context = ArtifactProviderContext {
        http: &http,
        budget: &budget,
        npm_registry: None,
        allow_private_registry: false,
        cache: None,
        tags: None,
    };
    for (ecosystem, name, range) in [
        (ArtifactType::Maven, "com.google.guava:guava", "[30.0,32.0)"),
        (ArtifactType::Maven, "com.google.guava:guava", "31.x"),
        (ArtifactType::Nuget, "Newtonsoft.Json", "[13.0,)"),
        (ArtifactType::Go, "github.com/a/b", ">=v1.2.0"),
    ] {
        let row = serde_json::json!({"operation":"resolve","ecosystem":ecosystem,"packageName":name,"version":range});
        if range != "31.x" {
            assert!(
                crate::contracts::validate_query("artifactSearch", row).is_err(),
                "{range}"
            );
            continue;
        }
        let error = execute_artifact(&versioned(ecosystem, name, range), &context)
            .await
            .expect_err("a range");
        assert_eq!(
            error.code,
            ErrorCode::CapabilityUnavailable,
            "{range}: {error:?}"
        );
        assert!(
            error.message.contains(&format!(
                "{} takes exact versions only",
                artifact_type::as_str(ecosystem)
            )),
            "{range}: {}",
            error.message
        );
    }
    assert!(
        http.seen().is_empty(),
        "rejected versions must not dispatch"
    );
    for exact in [
        "31.1-jre",
        "2.0.0.Final",
        "1.0-SNAPSHOT",
        "v0.0.0-20210101-abcdef",
    ] {
        assert!(!is_range(exact), "{exact}");
    }
}
