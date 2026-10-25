use crate::support;

use support::Workspace;

/// `schema <tool>` carries the runtime's availability: enabled, or the
/// variables of exactly the prerequisites still unset.
#[tokio::test]
async fn schema_availability_names_the_unset_prerequisites() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for (beta, key) in [(false, false), (false, true), (true, false), (true, true)] {
        let availability = async |tool: &str| {
            let mut command = workspace.cli();
            command
                .args(["schema", tool])
                .env("OCTOCODE_CLASSIFICATION_API_HOST", server.uri());
            if beta {
                command.env("OCTOCODE_BETA", "true");
            }
            if key {
                command.env("OCTOCODE_CLASSIFICATION_API", "fixture-key");
            }
            let output =
                tokio::task::spawn_blocking(move || command.output().expect("schema command"))
                    .await
                    .unwrap();
            assert!(output.status.success(), "{output:?}");
            let view: serde_json::Value =
                serde_json::from_slice(&output.stdout).expect("schema JSON");
            view["availability"].clone()
        };
        let clasify = availability("clasify").await;
        assert_eq!(clasify["enabled"], key, "{clasify}");
        let unset: Vec<&str> = [(!key).then_some("OCTOCODE_CLASSIFICATION_API")]
            .into_iter()
            .flatten()
            .collect();
        if !unset.is_empty() {
            assert_eq!(clasify["envVar"], unset.join("|"), "{clasify}");
        }
    }
}
