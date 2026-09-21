#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use serde_json::json;
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Match the production default for HTTP-backed provider fixtures. The shared
// test workspace uses 5 seconds to keep unrelated timeout tests fast, which is
// too narrow during a cold/full native build with several mock servers active.
const MOCK_PROVIDER_TIMEOUT_MS: &str = "30000";

fn query() -> serde_json::Value {
    json!({
        "id":"decision",
        "reasoning":"Choose the next inspection.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[
            {"id":"relevant","question":{"type":"noul","instructions":"Is it relevant?"}},
            {"id":"risk","question":{"type":"score","instructions":{"prompt":"Rate risk"},"criteria":["low",{"label":"high"}]}}
        ]
    })
}

#[tokio::test]
async fn clasify_requires_non_blank_reasoning() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_CLASSIFICATION_API", "secret".into())]);
    for reasoning in [None, Some("   ")] {
        let mut input = query();
        match reasoning {
            Some(reasoning) => input["reasoning"] = json!(reasoning),
            None => {
                input.as_object_mut().unwrap().remove("reasoning");
            }
        }
        let error = runtime
            .execute("semantic-reasoning".into(), "clasify".into(), input)
            .await
            .expect_err("clasify reasoning is required and non-blank");
        assert_eq!(error.code, "invalidInput");
    }
    runtime.close().await;
}

#[tokio::test]
async fn jev_vendor_key_alias_enables_clasify() {
    // The generic OCTOCODE_CLASSIFICATION_API is unset; the jev vendor's native
    // OCTOCODE_JEV_KEY alias alone must satisfy the availability gate.
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "jev-native-secret".into())]);
    assert!(runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn public_identity_is_a_hard_cutover_and_missing_key_is_actionable() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    assert!(!runtime.is_available("clasify"));
    assert!(!runtime.is_available("jev"));
    let catalog = runtime.catalog().unwrap();
    assert!(
        catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| { tool["name"] == "clasify" && tool["available"] == false })
    );
    assert!(
        !catalog["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "jev")
    );

    let error = runtime
        .execute("missing-key".into(), "clasify".into(), query())
        .await
        .unwrap_err();
    assert_eq!(error.code, "missingConfiguration");
    assert!(error.message.contains("OCTOCODE_CLASSIFICATION_API"));
    assert!(
        error
            .message
            .contains("https://docs.typesafe.ai/introduction")
    );
    runtime.close().await;
}

#[tokio::test]
async fn matrix_is_resource_major_and_reports_requested_and_resolved_models() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"provider-resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.9},
                "answer_1":{"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":"low","1":{"label":"high"}}}
            },
            "usage":{"input_tokens":12,"output_tokens":3}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let outcome = runtime
        .execute("matrix".into(), "clasify".into(), query())
        .await
        .unwrap();
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("nested output contract");
    let queries = outcome.structured_content["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["queryId"], "decision");
    let cells = queries[0]["results"].as_array().unwrap();
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0]["resourceId"], "observed");
    assert_eq!(cells[0]["questionId"], "relevant");
    assert_eq!(cells[1]["questionId"], "risk");
    for cell in cells {
        assert_eq!(
            cell["coverage"], "complete",
            "{}",
            outcome.structured_content
        );
        assert_eq!(cell["pages"].as_array().unwrap().len(), 1);
        assert_eq!(cell["pages"][0]["requestedModel"], "jev-latest");
        assert_eq!(cell["pages"][0]["resolvedModel"], "provider-resolved");
    }
    runtime.close().await;
}

#[tokio::test]
async fn oversized_first_page_is_bounded_partial_without_a_looping_continuation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"bounded",
        "reasoning":"Bound the supplied resource.",
        "resources":[{"id":"large","maxChars":5,"context":{"value":{"text":"far too large"}}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("bounded".into(), "clasify".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert_eq!(query["results"][0]["coverage"], "partial");
    assert_eq!(
        query["results"][0]["pages"][0]["context"]["coverage"],
        "partial"
    );
    assert!(query.get("next").is_none());
    runtime.close().await;
}

#[tokio::test]
async fn max_chars_budgets_sanitized_resource_payload_not_serialized_envelope() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(5)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let marker = "FOURTH_PAGE_MARKER";
    let file = workspace.write(
        "large.txt",
        format!("{}{}", "x".repeat(78_377 - marker.len()), marker),
    );
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"recover-full-content",
        "reasoning":"Recover the exact pages of an oversized whole-file request.",
        "resources":[{"id":"file","maxChars":80_000,"context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read the complete file.","fullContent":true
        }}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("recover-full-content".into(), "clasify".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert!(query.get("next").is_none(), "{query}");
    let cell = &query["results"][0];
    assert_eq!(cell["coverage"], "complete", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 5, "{cell}");
    assert_eq!(pages[0]["status"], "success");
    assert_eq!(pages[0]["context"]["coverage"], "partial");
    assert_eq!(pages[4]["status"], "success");
    assert_eq!(pages[4]["context"]["coverage"], "bounded");
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .any(|request| String::from_utf8_lossy(&request.body).contains(marker)),
        "the terminal source marker must reach semantic assessment"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("recovered semantic output contract");
    runtime.close().await;
}

#[tokio::test]
async fn page_budget_continuation_round_trips_through_the_public_contract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "many-lines.txt",
        (0..200)
            .map(|index| format!("line {index}\n"))
            .collect::<String>(),
    );
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"paged",
        "reasoning":"Assess bounded pages.",
        "resources":[{"id":"file","maxChars":5000,"context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read the next exact line.","limit":1,"fullContent":false
        }}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("paged".into(), "clasify".into(), input)
        .await
        .unwrap();
    let assess = outcome.structured_content["queries"][0]["next"]["clasify"].clone();
    assert!(assess.is_object(), "{}", outcome.structured_content);
    let context = &assess["resources"][0]["context"];
    assert_eq!(context.as_object().unwrap().len(), 2);
    assert!(context.get("tool").is_some());
    assert!(context.get("query").is_some());
    octocode_native::contracts::prepare_many_and_validate(
        "clasify",
        assess,
        octocode_native::contracts::PrepareOptions::default(),
    )
    .expect("next.clasify must execute unchanged");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("nested query-cell-page output");
    runtime.close().await;
}

#[tokio::test]
async fn payload_over_max_chars_returns_an_executable_clasify_continuation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(5)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("over-budget.txt", "x".repeat(80_001));
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"over-budget",
        "reasoning":"Assess no more than the resource payload budget.",
        "resources":[{"id":"file","maxChars":80_000,"context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read the complete file.","fullContent":true
        }}}],
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let first = runtime
        .execute("over-budget-first".into(), "clasify".into(), input)
        .await
        .unwrap();
    let assess = first.structured_content["queries"][0]["next"]["clasify"].clone();
    octocode_native::contracts::prepare_many_and_validate(
        "clasify",
        assess.clone(),
        octocode_native::contracts::PrepareOptions::default(),
    )
    .expect("next.clasify must satisfy the public input contract");

    let resumed = runtime
        .execute("over-budget-resume".into(), "clasify".into(), assess)
        .await
        .expect("next.clasify must execute unchanged");
    let resumed_query = &resumed.structured_content["queries"][0];
    assert!(resumed_query.get("next").is_none(), "{resumed_query}");
    assert_eq!(resumed_query["results"][0]["coverage"], "complete");
    runtime.close().await;
}
