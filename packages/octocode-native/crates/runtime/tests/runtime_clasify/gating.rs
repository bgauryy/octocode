//! Whether clasify is offered: MCP key gating and invocation prerequisites, and inputs rejected at validation before any work.

use super::{content_row, noul, provider};
use crate::support::{Run as _, Workspace, clasify_runtime, provider_env};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn retired_jev_vendor_key_does_not_enable_clasify() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "jev-native-secret".into())]);
    assert!(runtime.is_available("clasify"));
    let response = runtime
        .run_mcp(
            "retired-key".into(),
            "clasify".into(),
            json!({"queries":[{"operation":"models"}]}),
        )
        .await
        .expect("MCP response");
    assert_eq!(
        response["structuredContent"]["errorCode"], "toolUnavailable",
        "{response}"
    );
    assert!(
        response.to_string().contains("OCTOCODE_CLASSIFICATION_API"),
        "{response}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn public_clasify_requires_a_nonblank_key_regardless_of_beta() {
    let workspace = Workspace::new();
    for (env, missing) in [
        (
            vec![("OCTOCODE_BETA", "false".into())],
            "OCTOCODE_CLASSIFICATION_API",
        ),
        (
            vec![("OCTOCODE_BETA", "true".into())],
            "OCTOCODE_CLASSIFICATION_API",
        ),
        (
            vec![
                ("OCTOCODE_BETA", "true".into()),
                ("OCTOCODE_CLASSIFICATION_API", "  ".into()),
            ],
            "OCTOCODE_CLASSIFICATION_API",
        ),
    ] {
        let runtime = workspace.runtime(&env);
        assert!(runtime.is_available("clasify"), "CLI discovery: {env:?}");
        let refused = runtime
            .run_mcp(
                "mcp-missing-key".into(),
                "clasify".into(),
                json!({"queries":[{"operation":"models"}]}),
            )
            .await
            .expect("MCP refusal");
        assert_eq!(
            refused["structuredContent"]["errorCode"], "toolUnavailable",
            "{refused}"
        );
        let error = runtime
            .run(
                "missing-gate".into(),
                "clasify".into(),
                json!({"queries":[content_row()]}),
            )
            .await
            .expect_err("an unmet prerequisite blocks public clasify");
        assert_eq!(error.code, ErrorCode::MissingConfiguration);
        assert!(error.message.contains(missing), "{}", error.message);
        runtime.close().await;
    }
}

#[tokio::test]
async fn configuration_resolution_does_not_contact_the_provider() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    assert!(runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn a_disabled_reader_is_rejected_at_validation_before_any_work() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let mut input = workspace.config(&provider_env(
        &server,
        &[("DISABLE_TOOLS", "localFetch".into())],
    ));
    input.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let runtime =
        octocode_native::runtime::ToolRuntime::with_limits(input, crate::support::FIXTURE_LIMITS)
            .expect("MCP runtime");
    let error = runtime
        .run(
            "disabled-reader".into(),
            "clasify".into(),
            json!({"queries":[{"operation":"evaluate","source":{"tool":"localFetch","query":{"path":"a.rs","ranges":[{"startLine":1,"endLine":2}]}},
                "questions":{"q":noul("Anything?")}}]}),
        )
        .await
        .expect_err("rejected at validation");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    let issues = error.validation_issues.expect("field issues");
    assert_eq!(issues[0].rule_id, "clasify.source-reader-available");
    assert_eq!(issues[0].path, ["queries", "0", "source"]);
    assert!(
        issues[0].message.contains("localFetch"),
        "{}",
        issues[0].message
    );
    runtime.close().await;
}

#[tokio::test]
async fn removed_matrix_input_is_rejected_not_converted() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    for legacy in [
        json!({"resources":[{"value":"x"}],"questions":[{"type":"yesno","ask":"Relevant?"}]}),
        json!({"operation":"evaluate","content":"x","questions":{"q":{"type":"locate","instructions":"Where?"}}}),
        json!({"operation":"evaluate","content":"x","questions":{"q":noul("x")},"resources":[{"value":"x"}]}),
    ] {
        let error = runtime
            .run(
                "legacy".into(),
                "clasify".into(),
                json!({"queries":[legacy.clone()]}),
            )
            .await
            .expect_err("legacy shape");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{legacy}");
    }
    runtime.close().await;
}

async fn cli_host_mcp_rejects_disabled_source(setting: (&str, String)) {
    let setting_name = setting.0;
    let server = wiremock::MockServer::start().await;
    let workspace = Workspace::new();
    let source = workspace.write("source.rs", "fn source() {}\n");
    let runtime = workspace.runtime(&provider_env(&server, &[setting]));
    assert!(
        runtime.is_available("localFetch"),
        "CLI keeps reader available"
    );
    assert!(runtime.is_available("clasify"));
    let response = runtime.run_mcp(
        "mcp-disabled-source".into(), "clasify".into(),
        json!({"queries":[{"operation":"evaluate","source":{"tool":"localFetch","query":{"path":source}},
            "questions":{"q":noul("Does this source declare a function?")}}]}),
    ).await.expect("MCP response");
    runtime.close().await;
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "MCP reader restriction must reject before provider work: {response}"
    );
    assert_eq!(
        response["structuredContent"]["errorCode"], "invalidInput",
        "{response}"
    );
    let error = response["structuredContent"]["error"]
        .as_str()
        .expect("public error");
    assert!(error.contains("queries.0.source"), "{response}");
    assert!(error.contains("localFetch"), "{response}");
    assert!(error.contains(setting_name), "{response}");
}

#[tokio::test]
async fn cli_host_mcp_clasify_cannot_bypass_local_disabled_source() {
    cli_host_mcp_rejects_disabled_source(("OCTOCODE_ENABLE_LOCAL", "false".into())).await;
}

#[tokio::test]
async fn cli_host_mcp_clasify_cannot_bypass_disabled_source_tool() {
    cli_host_mcp_rejects_disabled_source(("DISABLE_TOOLS", "localFetch".into())).await;
}
