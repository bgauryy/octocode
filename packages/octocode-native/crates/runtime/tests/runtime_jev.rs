mod support;

use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn hunch_query() -> serde_json::Value {
    json!({
        "route": "hunch_check",
        "willChangeAction": true,
        "directCheck": { "available": false },
        "deliberation": {
            "observations": "The exact runtime branch returns the observed value.",
            "uncertainty": "Whether the lead is worth expanding into alternatives.",
            "strongestCounter": "The adapter may rewrite the runtime value.",
            "falsifier": "A focused test shows the runtime returns another value."
        },
        "state": {
            "goal": "Decide whether to investigate the lead.",
            "hunch": "The runtime branch owns the behavior.",
            "basis": "One source-anchored observation supports the lead."
        }
    })
}

#[test]
fn catalog_exposes_jev_only_for_a_nonblank_resolved_key() {
    let workspace = Workspace::new();
    let without_key = workspace.runtime(&[]);
    assert!(!without_key.is_available("jevReasoning"));
    let catalog = without_key.catalog().expect("catalog");
    assert_eq!(
        catalog["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|tool| tool["name"] == "jevReasoning"))
            .and_then(|tool| tool["available"].as_bool()),
        Some(false)
    );
    drop(without_key);

    let blank = workspace.runtime(&[("OCTOCODE_JEV_KEY", "   ".to_owned())]);
    assert!(!blank.is_available("jevReasoning"));
    drop(blank);

    let with_key = workspace.runtime(&[("OCTOCODE_JEV_KEY", "secret".to_owned())]);
    assert!(with_key.is_available("jevReasoning"));
}

#[tokio::test]
async fn deterministic_gate_skips_the_provider() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "secret".to_owned())]);
    let mut query = hunch_query();
    query["willChangeAction"] = json!(false);
    let outcome = call(&runtime, "jevReasoning", query)
        .await
        .expect("skipped judgment");
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(row_data(&outcome)["gate"], "skipped");
    assert_eq!(row_data(&outcome)["policyAction"], "act_without_jev");
    assert_eq!(row_data(&outcome)["provisional"], true);
    runtime.close().await;
}

#[tokio::test]
async fn bulk_gates_keep_each_reasoning_fork_independent() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "secret".to_owned())]);
    let mut first = hunch_query();
    first["reasoning"] = json!("Skip a judgment that cannot change the next action.");
    first["willChangeAction"] = json!(false);
    let mut second = hunch_query();
    second["reasoning"] = json!("Prefer the available deterministic check.");
    second["directCheck"] = json!({
        "available": true,
        "action": "Run the focused runtime test."
    });

    let outcome = runtime
        .execute(
            "test-bulk".into(),
            "jevReasoning".into(),
            json!({ "queries": [first, second] }),
        )
        .await
        .expect("independent bulk gates");
    let results = outcome.structured_content["results"]
        .as_array()
        .expect("result rows");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["index"], 0);
    assert_eq!(results[0]["data"]["policyAction"], "act_without_jev");
    assert_eq!(results[1]["index"], 1);
    assert_eq!(results[1]["data"]["policyAction"], "run_direct_check");
    assert_eq!(
        results[1]["data"]["nextAction"],
        "Run the focused runtime test."
    );
    runtime.close().await;
}

#[tokio::test]
async fn jev_request_is_typed_and_response_is_provisional() {
    let server = MockServer::start().await;
    let mut query = hunch_query();
    query["context"] = json!({
        "cot": "Observed one runtime value, considered an adapter rewrite, and identified a focused falsifier.",
        "thinking": "The runtime explanation is the current provisional lead.",
        "context": { "task": "Choose the next evidence step." },
        "agentRole": "research host"
    });
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer secret"))
        .and(body_json(json!({
            "model": "jev-1.13.0",
            "state": {
                "goal": "Decide whether to investigate the lead.",
                "hunch": "The runtime branch owns the behavior.",
                "basis": "One source-anchored observation supports the lead.",
                "context": {
                    "cot": "Observed one runtime value, considered an adapter rewrite, and identified a focused falsifier.",
                    "thinking": "The runtime explanation is the current provisional lead.",
                    "context": { "task": "Choose the next evidence step." },
                    "agentRole": "research host"
                }
            },
            "questions": {
                "worth_pursuing": {
                    "type": "noul",
                    "instructions": "Based solely on state.basis, is state.hunch a useful lead worth turning into competing falsifiable hypotheses?",
                    "criteria": {
                        "true": "The hunch is a useful lead to test.",
                        "false": "The supplied basis does not justify pursuing the hunch."
                    }
                }
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-1.13.0",
            "answers": {
                "worth_pursuing": { "type": "noul", "noul": 0.81 }
            },
            "usage": { "input_tokens": 100, "output_tokens": 4 }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_JEV_KEY", "secret".to_owned()),
        ("OCTOCODE_JEV_MODEL", "jev-1.13.0".to_owned()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
        // TLS/client startup can exceed the harness's 5 s minimum timeout under
        // full-suite contention; keep this transport-contract test deterministic.
        ("REQUEST_TIMEOUT", "30000".to_owned()),
    ]);
    let outcome = call(&runtime, "jevReasoning", query)
        .await
        .expect("Jev judgment");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["gate"], "judgment");
    assert_eq!(data["provisional"], true);
    assert_eq!(data["model"], "jev-1.13.0");
    assert_eq!(data["answers"]["worth_pursuing"]["noul"], 0.81);
    assert_eq!(data["applied"]["blocked"], false);
    runtime.close().await;
}
