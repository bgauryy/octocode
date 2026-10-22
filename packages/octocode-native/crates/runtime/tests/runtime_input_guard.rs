use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;

use serde_json::{Value, json};
use support::Workspace;

const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

fn row(extra: Value) -> Value {
    let mut query = json!({"mainGoal":"Find the needle.","reasoning":"Exercise input guards."});
    for (key, value) in extra.as_object().unwrap() {
        query[key] = value.clone();
    }
    query
}

fn assert_no_rewrite(output: &Value) {
    let text = output.to_string();
    assert!(!text.contains("REDACTED"), "rewritten value leaked: {text}");
    assert!(!text.contains(TOKEN), "credential echoed: {text}");
}

#[tokio::test]
async fn credential_shaped_search_text_is_rejected_not_rewritten() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\nghp_ prefix here\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);

    let error = runtime
        .run(
            "secret-single".into(),
            "localSearch".into(),
            json!({"queries":[row(json!({"path":root,"matchString":TOKEN}))]}),
        )
        .await
        .expect_err("a credential-shaped query must not execute");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let payload = error.payload.as_deref().unwrap();
    assert_no_rewrite(payload);
    let message = payload["error"].as_str().unwrap_or_default();
    assert!(message.contains("queries.0.matchString"), "{payload}");
    assert!(message.contains("credential"), "{payload}");

    let mcp = runtime
        .run_mcp(
            "secret-mcp".into(),
            "localSearch".into(),
            json!({"queries":[row(json!({"path":root,"matchString":TOKEN}))]}),
        )
        .await
        .expect("MCP reports the rejection in-band");
    assert_eq!(mcp["isError"], true);
    let text = mcp["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("credential"), "{text}");
    assert_no_rewrite(&mcp);
    runtime.close().await;
}

#[tokio::test]
async fn a_credential_row_is_isolated_and_never_reaches_continuations() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n".repeat(40));
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[
        row(json!({"path":root,"matchString":"needle","pageSize":1})),
        row(json!({"path":root,"matchString":TOKEN})),
    ]});
    let outcome = runtime
        .run("secret-batch".into(), "localSearch".into(), input)
        .await
        .expect("valid rows still run");
    let rows = outcome.structured_content["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{}", outcome.structured_content);
    assert_ne!(rows[0]["status"], "error");
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[1]["index"], 1);
    assert_eq!(rows[1]["status"], "error");
    assert_eq!(rows[1]["data"]["errorCode"], "invalidInput");
    let message = rows[1]["data"]["error"].as_str().unwrap_or_default();
    assert!(message.contains("queries.1.matchString"), "{}", rows[1]);
    assert!(message.contains("credential"), "{}", rows[1]);
    assert!(rows[1]["data"].get("hints").is_none(), "{}", rows[1]);
    assert_no_rewrite(&outcome.structured_content);
    assert!(!outcome.all_failed);
    octocode_native::contracts::validate_output("localSearch", &outcome.structured_content)
        .expect("isolated batch output contract");

    let fetch = runtime
        .run(
            "secret-fetch".into(),
            "localFetch".into(),
            json!({"queries":[
                row(json!({"path":file,"ranges":["1-2"]})),
                row(json!({"path":file,"matchString":TOKEN})),
            ]}),
        )
        .await
        .expect("valid fetch rows still run");
    let rows = fetch.structured_content["results"].as_array().unwrap();
    assert_ne!(rows[0]["status"], "error");
    assert_eq!(rows[1]["status"], "error");
    assert!(
        rows[1]["data"]["error"]
            .as_str()
            .is_some_and(|error| error.contains("queries.1.matchString")),
        "{}",
        rows[1]
    );
    assert_no_rewrite(&fetch.structured_content);
    runtime.close().await;
}

#[tokio::test]
async fn an_oversized_row_fails_in_band_while_other_rows_run() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[
        row(json!({"path":root,"matchString":"x".repeat(10_001)})),
        row(json!({"path":root,"matchString":"needle"})),
    ]});
    let mcp = runtime
        .run_mcp("oversized".into(), "localSearch".into(), input.clone())
        .await
        .expect("an oversized row is a row error, not a call failure");
    assert_eq!(mcp["isError"], false, "{mcp}");
    let rows = mcp["structuredContent"]["results"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "error");
    assert_eq!(rows[0]["data"]["errorCode"], "invalidInput");
    let message = rows[0]["data"]["error"].as_str().unwrap_or_default();
    assert!(message.contains("matchString"), "{}", rows[0]);
    assert!(message.contains("10000"), "{}", rows[0]);
    assert_ne!(rows[1]["status"], "error");

    let error = runtime
        .run(
            "oversized-single".into(),
            "localSearch".into(),
            json!({"queries":[row(json!({"path":root,"matchString":"x".repeat(10_001)}))]}),
        )
        .await
        .expect_err("no valid row to run");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let mcp = runtime
        .run_mcp(
            "oversized-single-mcp".into(),
            "localSearch".into(),
            json!({"queries":[row(json!({"path":root,"matchString":"x".repeat(10_001)}))]}),
        )
        .await
        .expect("MCP reports the reason in-band");
    let text = mcp["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("matchString") && text.contains("10000"),
        "{text}"
    );
    runtime.close().await;
}

/// A string for an integer or boolean field is the canonical schema's
/// rejection, never a repair: the error names the JSON value to send.
#[tokio::test]
async fn numeric_and_boolean_strings_are_rejected_with_the_value_to_send() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "one\ntwo\nthree\n");
    let runtime = workspace.runtime(&[]);
    for (field, value, hint) in [
        ("contextLines", json!("0"), "send 0"),
        ("fullContent", json!("true"), "send true"),
    ] {
        let mut query = json!({"path":file,"matchString":"two"});
        query[field] = value;
        if field == "fullContent" {
            query.as_object_mut().unwrap().remove("matchString");
        }
        let error = runtime
            .run(
                "strict".into(),
                "localFetch".into(),
                json!({"queries":[row(query)]}),
            )
            .await
            .expect_err("a string is not a number or boolean");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{field}");
        let payload = error.payload.as_deref().unwrap().to_string();
        assert!(payload.contains(hint), "{field}: {payload}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn clasify_rejects_a_credential_in_a_source_read() {
    let server = wiremock::MockServer::builder().start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("trace.txt", "Evidence is present.\n");
    let runtime = workspace.runtime(&support::provider_env(&server, &[]));
    let input = json!({"queries":[{
        "id":"decision",
        "mainGoal":"Decide whether the file states the fact.",
        "reasoning":"Exercise the source-read input guard.",
        "source":{"tool":"localFetch","query":{"path":format!("{TOKEN}.txt"),"ranges":["1-1"]}},
        "questions":{"relevant":{"type":"noul","instructions":"Is evidence present?"}}
    }]});
    let error = runtime
        .run("clasify-secret".into(), "clasify".into(), input)
        .await
        .expect_err("a source read must not run a rewritten query");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let payload = error.payload.as_deref().unwrap();
    let message = payload["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("source.query.path") && message.contains("credential"),
        "{payload}"
    );
    assert!(!payload.to_string().contains(TOKEN), "{payload}");
    runtime.close().await;
}

/// A misspelled tool name is answered with the tool it most likely meant,
/// before the list of every available tool.
#[tokio::test]
async fn an_unknown_tool_name_suggests_the_nearest_tool() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = runtime
        .run(
            "unknown-tool".into(),
            "localSerch".into(),
            json!({"queries":[row(json!({"path":".","matchString":"needle"}))]}),
        )
        .await
        .expect_err("an unknown tool never runs");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(
        error.message.contains("Did you mean localSearch?"),
        "{}",
        error.message
    );
    let far = runtime
        .run("far-tool".into(), "zzzz".into(), json!({}))
        .await
        .expect_err("an unknown tool never runs");
    assert!(!far.message.contains("Did you mean"), "{}", far.message);
    runtime.close().await;
}
