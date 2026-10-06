#![allow(clippy::expect_used, clippy::unwrap_used)]

use crate::support;

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
        .execute(
            "secret-single".into(),
            "localSearch".into(),
            json!({"queries":[row(json!({"path":root,"matchString":TOKEN}))]}),
        )
        .await
        .expect_err("a credential-shaped query must not execute");
    assert_eq!(error.code, "invalidInput");
    let payload = error.payload.as_deref().unwrap();
    assert_no_rewrite(payload);
    let details = payload["details"].to_string();
    assert!(details.contains("queries.0.matchString"), "{details}");
    assert!(details.contains("credential"), "{details}");

    let mcp = runtime
        .execute_mcp(
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
        .execute("secret-batch".into(), "localSearch".into(), input)
        .await
        .expect("valid rows still run");
    let rows = outcome.structured_content["results"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{}", outcome.structured_content);
    assert_ne!(rows[0]["status"], "error");
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[1]["index"], 1);
    assert_eq!(rows[1]["status"], "error");
    assert_eq!(rows[1]["data"]["errorCode"], "invalidInput");
    let hints = rows[1]["data"]["hints"].to_string();
    assert!(hints.contains("queries.1.matchString"), "{hints}");
    assert!(hints.contains("credential"), "{hints}");
    assert_no_rewrite(&outcome.structured_content);
    assert!(!outcome.all_failed);
    octocode_native::contracts::validate_output("localSearch", &outcome.structured_content)
        .expect("isolated batch output contract");

    let fetch = runtime
        .execute(
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
        rows[1]["data"]["hints"]
            .to_string()
            .contains("queries.1.matchString")
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
        .execute_mcp("oversized".into(), "localSearch".into(), input.clone())
        .await
        .expect("an oversized row is a row error, not a call failure");
    assert_eq!(mcp["isError"], false, "{mcp}");
    let rows = mcp["structuredContent"]["results"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "error");
    assert_eq!(rows[0]["data"]["errorCode"], "invalidInput");
    let hints = rows[0]["data"]["hints"].to_string();
    assert!(hints.contains("matchString"), "{hints}");
    assert!(hints.contains("10000"), "{hints}");
    assert_ne!(rows[1]["status"], "error");

    let error = runtime
        .execute(
            "oversized-single".into(),
            "localSearch".into(),
            json!({"queries":[row(json!({"path":root,"matchString":"x".repeat(10_001)}))]}),
        )
        .await
        .expect_err("no valid row to run");
    assert_eq!(error.code, "invalidInput");
    let mcp = runtime
        .execute_mcp(
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

#[tokio::test]
async fn numeric_and_boolean_strings_are_coerced_for_typed_fields() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "one\ntwo\nthree\n");
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .execute(
            "coerce".into(),
            "localFetch".into(),
            json!({"queries":[
                row(json!({"path":file,"matchString":"two","contextLines":"0"})),
                row(json!({"path":file,"fullContent":"true"})),
            ]}),
        )
        .await
        .expect("lossless strings are coerced");
    let rows = outcome.structured_content["results"].as_array().unwrap();
    assert!(rows.iter().all(|row| row["status"] != "error"), "{rows:?}");
    let text = outcome.structured_content.to_string();
    assert!(text.contains("two"), "{text}");

    for bad in [json!("2.0"), json!("02"), json!(" 2"), json!("two")] {
        let error = runtime
            .execute(
                "coerce-bad".into(),
                "localFetch".into(),
                json!({"queries":[row(json!({"path":file,"matchString":"two","contextLines":bad}))]}),
            )
            .await
            .expect_err("only exact integer strings coerce");
        assert_eq!(error.code, "invalidInput", "{bad}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn clasify_redacts_evidence_but_rejects_a_credential_in_a_resource_read() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("trace.txt", "Evidence is present.\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ]);
    let input = json!({"queries":[{
        "id":"decision",
        "mainGoal":"Decide whether the file states the fact.",
        "reasoning":"Exercise the resource-read input guard.",
        "resources":[{"id":"source","tool":"localFetch","query":{"path":file,"matchString":TOKEN}}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Is evidence present?"}]
    }]});
    let error = runtime
        .execute("clasify-secret".into(), "clasify".into(), input)
        .await
        .expect_err("a resource read must not run a rewritten query");
    assert_eq!(error.code, "invalidInput");
    let details = error.payload.as_deref().unwrap()["details"].to_string();
    assert!(
        details.contains("resources[].query.matchString") && details.contains("credential"),
        "{details}"
    );
    assert!(!details.contains(TOKEN), "{details}");
    runtime.close().await;
}
