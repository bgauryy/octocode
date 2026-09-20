mod support;

use serde_json::{Value, json};
use support::{Workspace, row_data, row_status};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn query() -> Value {
    json!({"state":{"observations":["A cancellation guard precedes the write."]},"questions":{
        "supported":{"type":"noul","instructions":"Does the supplied observation support cancellation before writing?"}
    }})
}

#[test]
fn catalog_has_one_jev_tool_gated_on_a_nonblank_key() {
    let workspace = Workspace::new();
    let without_key = workspace.runtime(&[]);
    assert!(!without_key.is_available("jev"));
    let catalog = without_key.catalog().unwrap();
    let tools = catalog["tools"].as_array().unwrap();
    let jev: Vec<_> = tools
        .iter()
        .filter(|tool| tool["name"].as_str().unwrap_or_default().starts_with("jev"))
        .collect();
    assert_eq!(jev.len(), 1);
    assert_eq!(jev[0]["name"], "jev");
    assert_eq!(jev[0]["available"], false);
    drop(without_key);
    let blank = workspace.runtime(&[("OCTOCODE_JEV_KEY", "   ".to_owned())]);
    assert!(!blank.is_available("jev"));
    drop(blank);
    let with_key = workspace.runtime(&[("OCTOCODE_JEV_KEY", "secret".to_owned())]);
    assert!(with_key.is_available("jev"));
    assert!(!with_key.is_available("jevReasoning"));
    assert!(!with_key.is_available("jevScout"));
}

#[tokio::test]
async fn pure_runtime_preserves_values_and_injects_configured_model() {
    let server = MockServer::start().await;
    let query = query();
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer secret"))
        .and(body_json(
            json!({"model":"jev-test","state":query["state"],"questions":query["questions"]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-test","answers":{"supported":{"type":"noul","noul":0.81}},
            "usage":{"input_tokens":100,"output_tokens":4}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".to_owned()),
        ("OCTOCODE_JEV_MODEL", "jev-test".to_owned()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
        ("REQUEST_TIMEOUT", "30000".to_owned()),
    ]);
    let outcome = runtime
        .execute("test-pure".into(), "jev".into(), query)
        .await
        .unwrap();
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["model"], "jev-test");
    assert_eq!(data["answers"]["supported"]["noul"], 0.81);
    for field in ["gate", "route", "applied", "nextAction", "policyAction"] {
        assert!(
            data.get(field).is_none(),
            "unexpected workflow field: {field}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn legacy_tools_and_workflow_fields_fail_before_transport() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".to_owned()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
    ]);
    for tool in ["jevReasoning", "jevScout"] {
        assert!(
            runtime
                .execute(format!("legacy-{tool}"), tool.into(), query())
                .await
                .is_err()
        );
    }
    for field in ["model", "reasoning", "goal", "debug", "route", "sources"] {
        let mut value = query();
        value[field] = json!("unwanted");
        assert!(
            runtime
                .execute(format!("field-{field}"), "jev".into(), value)
                .await
                .is_err(),
            "accepted {field}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn oversized_input_rejects_before_source_or_jev_network_calls() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
        ("GITHUB_API_URL", server.uri()),
    ]);
    let mut value = query();
    value["state"] = Value::Object(
        (0..5)
            .map(|index| (index.to_string(), json!(vec!["x".repeat(9000); 100])))
            .collect(),
    );
    value["sources"] =
        json!({"remote":{"type":"github","owner":"a","repo":"b","path":"source.rs","ref":"main"}});
    let outcome = runtime
        .execute("oversized".into(), "jev".into(), value)
        .await
        .unwrap();
    assert_eq!(row_status(&outcome), "error");
    assert_eq!(row_data(&outcome)["errorCode"], "invalidJevRequest");
    runtime.close().await;
}

#[tokio::test]
async fn source_receipts_and_caller_rubric_paths_keep_their_identity() {
    let workspace = Workspace::new();
    let source = workspace.write("source.rs", "HIDDEN_SOURCE_BODY\n");
    let canonical = source.canonicalize().unwrap();
    let criteria = json!([{"path":canonical},{"path":"/other/rubric/value"}]);
    let mut value = query();
    value["sources"] = json!({"local":{"type":"local","path":source}});
    value["questions"] = json!({"quality":{"type":"score","instructions":"Assess only supplied evidence","criteria":criteria}});
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_json(json!({"model":"jev-test","state":{"context":value["state"],"sources":{"local":{"source":{"type":"local","path":canonical},"content":"HIDDEN_SOURCE_BODY\n"}}},"questions":value["questions"]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"HIDDEN_SOURCE_BODY","answers":{"quality":{"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":criteria[0],"1":criteria[1]}}},"usage":{"input_tokens":20,"output_tokens":3}})))
        .expect(1).mount(&server).await;
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_MODEL", "jev-test".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
        ("REQUEST_TIMEOUT", "30000".into()),
    ]);
    let outcome = runtime
        .execute("receipt-identity".into(), "jev".into(), value)
        .await
        .unwrap();
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    assert_eq!(
        row_data(&outcome)["sources"]["local"]["source"]["path"],
        json!(canonical)
    );
    assert_eq!(
        row_data(&outcome)["answers"]["quality"]["legend"]["0"],
        criteria[0]
    );
    assert_eq!(row_data(&outcome)["model"], "jev-test");
    assert!(
        !outcome
            .structured_content
            .to_string()
            .contains("HIDDEN_SOURCE_BODY")
    );
    runtime.close().await;
}
