use crate::support;

use support::Workspace;

/// CLI schemas expose every tool without resolving provider prerequisites.
#[tokio::test]
async fn schema_availability_is_independent_of_invocation_prerequisites() {
    let server = wiremock::MockServer::start().await;
    let workspace = Workspace::new();
    for (beta, key) in [(false, false), (false, true), (true, false), (true, true)] {
        let availability = async |tool: &str| {
            let mut command = workspace.cli();
            command
                .args(["schema", tool])
                .env("OCTOCODE_CLASSIFICATION_API_HOST", server.uri())
                .env("OCTOCODE_ENABLE_LOCAL", "false")
                .env("OCTOCODE_ENABLE_CLONE", "false")
                .env("OCTOCODE_STORAGE_MODE", "memory")
                .env("TOOLS_TO_RUN", "localFetch")
                .env("DISABLE_TOOLS", "localFetch,clasify")
                .env("OCTOCODE_BETA", if beta { "true" } else { "false" })
                .env(
                    "OCTOCODE_CLASSIFICATION_API",
                    if key { "fixture-key" } else { "" },
                );
            let output =
                tokio::task::spawn_blocking(move || command.output().expect("schema command"))
                    .await
                    .unwrap();
            assert!(output.status.success(), "{output:?}");
            let view: serde_json::Value =
                serde_json::from_slice(&output.stdout).expect("schema JSON");
            view["availability"].clone()
        };
        for tool in [
            "clasify",
            "localFetch",
            "graphBuild",
            "graphQuery",
            "ghCloneRepo",
        ] {
            let available = availability(tool).await;
            assert_eq!(available["enabled"], true, "{tool}: {available}");
            assert!(available.get("envVar").is_none(), "{tool}: {available}");
            assert!(
                available.get("unavailableReason").is_none(),
                "{tool}: {available}"
            );
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "schema must not probe a configured provider"
    );
}
