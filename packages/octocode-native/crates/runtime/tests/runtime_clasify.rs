#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use serde_json::json;
use std::time::{Duration, Instant};
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

// Match the production default for HTTP-backed provider fixtures. The shared
// test workspace uses 5 seconds to keep unrelated timeout tests fast, which is
// too narrow during a cold/full native build with several mock servers active.
const MOCK_PROVIDER_TIMEOUT_MS: &str = "30000";

#[derive(Clone)]
struct DelayedJevResponse {
    arrivals: std::sync::Arc<std::sync::Mutex<Vec<Instant>>>,
}

impl Respond for DelayedJevResponse {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Instant::now());
        ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(200))
            .set_body_json(json!({
                "model":"resolved",
                "answers":{"answer":{"type":"noul","noul":0.8}},
                "usage":{"input_tokens":2,"output_tokens":1}
            }))
    }
}

fn query() -> serde_json::Value {
    json!({
        "id":"decision",
        "reasoning":"Choose the next inspection.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[
            {"id":"relevant","type":"noul","instructions":"Is it relevant?"},
            {"id":"risk","type":"score","instructions":{"prompt":"Rate risk"},"criteria":["low",{"label":"high"}]}
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
async fn matrix_is_resource_major_without_agent_telemetry_or_duplicate_text() {
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
    assert!(
        outcome.content.is_empty(),
        "clasify must not duplicate structured hints as text"
    );
    let queries = outcome.structured_content["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["queryId"], "decision");
    assert!(queries[0].get("model").is_none());
    assert!(queries[0].get("usage").is_none());
    let resources = queries[0]["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["resourceId"], "observed");
    assert_eq!(
        resources[0]["coverage"], "complete",
        "{}",
        outcome.structured_content
    );
    let pages = resources[0]["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["answers"]["relevant"], json!({"noul":0.9}));
    assert_eq!(
        pages[0]["answers"]["risk"],
        json!({"score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75}}),
        "no echoed legend or type"
    );
    let text = outcome.structured_content.to_string();
    for redundant in [
        "requestedModel",
        "resolvedModel",
        "resultHash",
        "legend",
        "\"type\"",
    ] {
        assert!(!text.contains(redundant), "{redundant} in {text}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn independent_resource_assessments_are_dispatched_concurrently() {
    let server = MockServer::start().await;
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(DelayedJevResponse {
            arrivals: arrivals.clone(),
        })
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"concurrent-resources",
        "reasoning":"Assess independent resources without serial provider latency.",
        "resources":(0..4).map(|index| json!({
            "id":format!("resource-{index}"),
            "context":{"value":{"index":index}}
        })).collect::<Vec<_>>(),
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });

    let outcome = runtime
        .execute("concurrent-resources".into(), "clasify".into(), input)
        .await
        .unwrap();
    {
        let arrivals = arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(arrivals.len(), 4);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "provider requests were dispatched serially: {arrivals:?}"
        );
    }
    let results = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert_eq!(results.len(), 4);
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result["resourceId"], format!("resource-{index}"));
        assert_eq!(result["coverage"], "complete");
    }
    runtime.close().await;
}

#[tokio::test]
async fn classification_max_concurrency_bounds_provider_requests_in_flight() {
    let server = MockServer::start().await;
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(DelayedJevResponse {
            arrivals: arrivals.clone(),
        })
        .expect(8)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Limit 4 → one call may hold at most 3 permits (fairness cap).
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("OCTOCODE_CLASSIFICATION_CONCURRENCY", "4".into()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"bounded-resources",
        "reasoning":"Assess many resources without exceeding provider concurrency.",
        "resources":(0..8).map(|index| json!({
            "id":format!("resource-{index}"),
            "context":{"value":{"index":index}}
        })).collect::<Vec<_>>(),
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("bounded-resources".into(), "clasify".into(), input)
        .await
        .unwrap();
    let arrivals = arrivals
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert_eq!(arrivals.len(), 8);
    // Each response takes 200ms and a permit is reused only after its
    // response, so arrivals closer than 200ms were in flight together.
    let overlapping = arrivals
        .iter()
        .map(|start| {
            arrivals
                .iter()
                .filter(|other| {
                    *other >= start && other.duration_since(*start) < Duration::from_millis(180)
                })
                .count()
        })
        .max()
        .unwrap();
    assert!(
        (2..=3).contains(&overlapping),
        "expected 2..=3 concurrent provider requests, saw {overlapping}: {arrivals:?}"
    );
    let results = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert!(
        results
            .iter()
            .all(|result| result["coverage"] == "complete")
    );
    runtime.close().await;
}

#[tokio::test]
async fn independent_query_matrices_are_dispatched_concurrently() {
    let server = MockServer::start().await;
    let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(DelayedJevResponse {
            arrivals: arrivals.clone(),
        })
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let queries = (0..4)
        .map(|index| {
            json!({
                "id":format!("query-{index}"),
                "reasoning":"Assess an independent matrix without serial provider latency.",
                "resources":[{"id":"resource","context":{"value":{"index":index}}}],
                "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
            })
        })
        .collect::<Vec<_>>();

    let outcome = runtime
        .execute(
            "concurrent-matrices".into(),
            "clasify".into(),
            json!({"queries":queries}),
        )
        .await
        .unwrap();
    {
        let arrivals = arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(arrivals.len(), 4);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "independent matrices were dispatched serially: {arrivals:?}"
        );
    }
    let queries = outcome.structured_content["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 4);
    for (index, query) in queries.iter().enumerate() {
        assert_eq!(query["queryId"], format!("query-{index}"));
        assert_eq!(query["resources"][0]["coverage"], "complete");
    }
    runtime.close().await;
}

#[tokio::test]
async fn oversized_first_page_is_not_classified_or_given_a_looping_continuation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(0)
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
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("bounded".into(), "clasify".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert_eq!(query["resources"][0]["coverage"], "error");
    assert_eq!(
        query["resources"][0]["pages"][0]["error"]["code"],
        "classificationContextTooLarge"
    );
    assert!(query["resources"][0]["pages"][0].get("answers").is_none());
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
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("recover-full-content".into(), "clasify".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    assert!(query.get("next").is_none(), "{query}");
    let cell = &query["resources"][0];
    assert_eq!(cell["coverage"], "complete", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    // 16 KiB pages already exceed half the 24 KiB coalescing budget, so each
    // stays its own scoped judgment; small line pages merge instead.
    assert_eq!(pages.len(), 5, "{cell}");
    for page in pages {
        assert!(page.get("answers").is_some(), "{page}");
        assert!(page.get("limitations").is_none(), "{page}");
    }
    let last = &pages[pages.len() - 1]["scope"];
    assert_eq!(last["endLine"], last["totalLines"], "{cell}");
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
async fn scout_expands_explicit_question_type_and_preserves_source_identity() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved", "answers":{"answer":{"type":"noul","noul":0.82}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("hooks.md", "preClose runs before active requests finish.\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"novelty", "reasoning":"Decide whether this unread section adds evidence.",
        "resources":[{"id":"hooks","context":{"tool":"localFetch","query":{"path":file,"reasoning":"Screen the complete section."}}}],
        "questions":[{"id":"new","questionType":"addsEvidence","target":"Shutdown timing", "knownEvidence":["onClose runs after requests finish"]}]
    });
    let result = runtime
        .execute("preset".into(), "clasify".into(), input)
        .await
        .unwrap();
    let output = &result.structured_content["queries"][0];
    assert!(output.get("templateVersion").is_none());
    let page = &output["resources"][0]["pages"][0];
    assert_eq!(page["answers"]["new"]["noul"], 0.82);
    assert_eq!(page["source"]["path"], file.to_str().unwrap());
    assert_eq!(page["scope"]["startLine"], 1);
    assert!(
        !result
            .structured_content
            .to_string()
            .contains("preClose runs")
    );
    let requests = server.received_requests().await.unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(sent["state"].to_string().contains("preClose runs"));
    assert_eq!(sent["questions"]["answer"]["type"], "noul");
    assert_eq!(
        sent["questions"]["answer"]["instructions"]["knownEvidence"],
        json!(["onClose runs after requests finish"])
    );
    assert_eq!(sent["questions"].as_object().unwrap().len(), 1);
    runtime.close().await;
}

#[tokio::test]
async fn search_rejects_removed_semantic_addon_without_calling_provider() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_CLASSIFICATION_API", "secret".into())]);
    let error = runtime.execute("search".into(), "localSearch".into(), json!({
        "path":workspace.workspace, "searchText":"hooks", "reasoning":"Discover candidates.",
        "semanticRerank":{"questions":[{"id":"q","question":"Relevant?"}]}
    })).await.expect_err("semantic checks require clasify");
    assert_eq!(error.code, "invalidInput");
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
            "path":file,"reasoning":"Read the next exact line.","chunkSize":1,"fullContent":false
        }}}],
        "questions":[{"id":"relevant","questionType":"contribution","target":"line content"}]
    });
    let outcome = runtime
        .execute("paged".into(), "clasify".into(), input)
        .await
        .unwrap();
    let assess = outcome.structured_content["queries"][0]["next"]["clasify"].clone();
    assert!(assess.is_object(), "{}", outcome.structured_content);
    assert_eq!(
        assess["questions"][0]["id"], "relevant",
        "Continuation must preserve question identity"
    );
    let context = &assess["resources"][0]["context"];
    assert_eq!(context.as_object().unwrap().len(), 2);
    assert!(context.get("tool").is_some());
    assert!(context.get("query").is_some());
    octocode_native::contracts::prepare_many_and_validate(
        "clasify",
        assess.clone(),
        octocode_native::contracts::PrepareOptions::default(),
    )
    .expect("next.clasify must execute unchanged");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("nested query-cell-page output");
    assert_eq!(assess["questions"][0]["questionType"], "contribution");
    let mut next = Some(assess);
    let mut last_end = outcome.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["scope"]["endLine"]
        .as_u64()
        .unwrap();
    for _ in 0..4 {
        let Some(query) = next.take() else {
            break;
        };
        let page = runtime
            .execute("continue-preset".into(), "clasify".into(), query)
            .await
            .unwrap();
        let row = &page.structured_content["queries"][0];
        assert!(row.get("templateVersion").is_none());
        for page in row["resources"][0]["pages"].as_array().unwrap() {
            assert_eq!(page["scope"]["startLine"].as_u64().unwrap(), last_end + 1);
            last_end = page["scope"]["endLine"].as_u64().unwrap();
        }
        next = row.pointer("/next/clasify").cloned();
    }
    assert!(next.is_none(), "continuation must terminate");
    assert_eq!(last_end, 200);
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
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
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
    assert_eq!(resumed_query["resources"][0]["coverage"], "complete");
    runtime.close().await;
}

#[tokio::test]
async fn invalid_inner_query_is_rejected_with_the_exact_contract_field() {
    // A localFetch context missing `reasoning` is rejected before capture, and
    // the contract detail names the offending nested field. The provider is
    // never reached.
    let workspace = Workspace::new();
    let file = workspace.write("dummy.txt", "content");
    let runtime = workspace.runtime(&[("OCTOCODE_CLASSIFICATION_API", "secret".into())]);
    let input = json!({
        "id": "bad-inner-query",
        "reasoning": "Test that a missing inner reasoning surfaces its field name.",
        "resources": [{
            "id": "r1",
            "context": {
                "tool": "localFetch",
                "query": { "path": file }
            }
        }],
        "questions": [{
            "id": "q1",
            "type": "noul", "instructions": "Relevant?"
        }]
    });
    let error = runtime
        .execute("bad-inner-query".into(), "clasify".into(), input)
        .await
        .expect_err("missing delegated reasoning must fail contract validation");
    assert_eq!(error.code, "invalidInput");
    let payload = error.payload.expect("structured validation payload");
    let details = payload["details"].as_array().expect("validation details");
    assert!(
        details.iter().any(|detail| detail
            .as_str()
            .is_some_and(|detail| detail.contains("resources.0.context.query.reasoning"))),
        "validation detail must name the nested reasoning field: {details:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn search_resource_fans_out_candidates_from_only_the_requested_page() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..6 {
        workspace.write(&format!("src/file{index}.txt"), "needle marker\n");
    }
    let root = workspace.write("src/file6.txt", "needle marker\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"search-page",
        "reasoning":"Judge one search page.",
        "resources":[{"id":"hits","context":{"tool":"localSearch","query":{
            "path":root,"searchText":"needle","reasoning":"Find hits.",
            "resultView":"paginated","pageSize":2
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("search-page".into(), "clasify".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let cell = &query["resources"][0];
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2, "{cell}");
    assert_eq!(cell["coverage"], "partial");
    let source_paths = pages
        .iter()
        .map(|page| page["source"]["path"].as_str().unwrap().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        source_paths.len(),
        2,
        "each candidate needs its own source: {cell}"
    );
    assert!(
        source_paths
            .iter()
            .all(|path| std::path::Path::new(path).is_absolute()),
        "local candidate paths must be executable absolute paths: {source_paths:?}"
    );
    let resume = &query["next"]["clasify"]["resources"][0]["context"];
    assert_eq!(resume["tool"], "localSearch", "{query}");
    assert_eq!(resume["query"]["page"], 2, "{resume}");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let judged_paths = requests
        .iter()
        .map(|request| {
            let sent: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert!(sent["state"].to_string().contains("needle"), "{sent}");
            assert!(
                sent["state"]["data"].get("next").is_none(),
                "continuations stay out of provider state: {sent}"
            );
            let files = sent["state"]["data"]["files"].as_array().unwrap();
            assert_eq!(
                files.len(),
                1,
                "one provider state per file candidate: {sent}"
            );
            files[0]["path"].as_str().unwrap().to_owned()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(judged_paths.len(), 2, "each file must be judged once");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("single-page search output contract");
    runtime.close().await;
}

#[tokio::test]
async fn file_chunk_scout_hydrates_five_candidates_and_returns_exact_reads() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .expect(5)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..8 {
        workspace.write(
            &format!("src/candidate{index}.txt"),
            format!("header\nneedle marker\nbody-only fact {index}\nfooter\n"),
        );
    }
    let root = workspace
        .workspace
        .join("src")
        .to_string_lossy()
        .into_owned();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"hydrated-search",
        "reasoning":"Judge source around each search hit.",
        "resources":[{"id":"hits","context":{
            "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                "path":root,"searchText":"needle","reasoning":"Find candidates.",
                "resultView":"paginated","pageSize":20
            }
        },"maxChars":20_000}],
        "questions":[{"id":"relevant",
            "type":"noul","instructions":"Does this source contain a body-only fact?"
        }]
    });
    let outcome = runtime
        .execute("hydrated-search".into(), "clasify".into(), input)
        .await
        .expect("hydrated scout");
    let query = &outcome.structured_content["queries"][0];
    let pages = query["resources"][0]["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 5, "{query}");
    for page in pages {
        assert_eq!(page["next"]["read"]["tool"], "localFetch", "{page}");
        assert_eq!(page["next"]["read"]["confidence"], "exact", "{page}");
        assert!(
            page["source"]["path"]
                .as_str()
                .is_some_and(|p| std::path::Path::new(p).is_absolute())
        );
        assert!(
            page["limitations"]
                .as_array()
                .is_some_and(|limits| limits.iter().any(|v| {
                    v.as_str()
                        .is_some_and(|v| v.contains("bounded candidate chunk"))
                })),
            "{page}"
        );
    }
    let resume = &query["next"]["clasify"]["resources"][0]["context"];
    assert_eq!(resume["candidateEvidence"], "fileChunks");
    assert_eq!(resume["query"]["page"], 2);
    assert_eq!(resume["query"]["pageSize"], 5);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    for request in requests {
        let sent: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let state = &sent["state"];
        assert!(
            state["content"]
                .as_str()
                .is_some_and(|body| body.contains("body-only fact")),
            "{sent}"
        );
        assert!(state["content"].as_str().unwrap().chars().count() <= 4_000);
        assert!(
            state.get("matches").is_none(),
            "search snippets must not reach Jev: {sent}"
        );
    }
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("hydrated scout output contract");
    runtime.close().await;
}

#[tokio::test]
async fn expanded_cells_fail_before_any_provider_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for root in ["a", "b"] {
        for index in 0..5 {
            workspace.write(
                &format!("{root}/candidate{index}.txt"),
                "needle\nbody evidence\n",
            );
        }
    }
    let resource = |id: &str, root: &str| {
        json!({
            "id":id,"context":{"tool":"localSearch","candidateEvidence":"search","query":{
                "path":workspace.workspace.join(root),"searchText":"needle","reasoning":"Find candidates.",
                "pageSize":5
            }}
        })
    };
    let questions = (0..3)
        .map(|index| {
            json!({
                "id":format!("q{index}"),"type":"noul","instructions":format!("Check {index}?")
            })
        })
        .collect::<Vec<_>>();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let outcome = runtime
        .execute(
            "expanded-cells".into(),
            "clasify".into(),
            json!({
                "id":"expanded-cells","reasoning":"Exercise the runtime expansion gate.",
                "resources":[resource("a","a"),resource("b","b")],"questions":questions
            }),
        )
        .await
        .expect("structured expansion failure");
    let resources = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert!(
        resources
            .iter()
            .flat_map(|r| r["pages"].as_array().unwrap())
            .all(|page| { page["error"]["code"] == "classificationExpandedCellsExceeded" }),
        "{}",
        outcome.structured_content
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    runtime.close().await;
}

#[tokio::test]
async fn empty_file_is_reported_without_a_provider_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("empty.txt", "");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"empty","reasoning":"Screen an empty artifact.",
        "resources":[{"id":"e","context":{"tool":"localFetch","query":{"path":file,"reasoning":"Read it."}}}],
        "questions":[{"id":"q","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("empty".into(), "clasify".into(), input)
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    assert_eq!(resource["coverage"], "error", "{resource}");
    assert!(
        outcome.all_failed,
        "MCP must expose failed evidence as a tool error"
    );
    assert_eq!(
        resource["pages"][0]["error"]["code"],
        "classificationContextEmpty"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("empty-file output contract");
    runtime.close().await;
}

#[tokio::test]
async fn empty_search_page_is_not_sent_to_the_provider() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("present.txt", "hello world\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ]);
    let input = json!({
        "id":"empty-search","reasoning":"Screen a search page.",
        "resources":[{"id":"e","context":{"tool":"localSearch","query":{
            "path":file,"searchText":"UNLIKELY_OCTOCODE_SENTINEL_673829",
            "reasoning":"Find matching source."
        }}}],
        "questions":[{"id":"q","type":"noul","instructions":"Does this page show a match?"}]
    });
    let outcome = runtime
        .execute("empty-search".into(), "clasify".into(), input)
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    assert_eq!(resource["coverage"], "error", "{resource}");
    assert!(
        outcome.all_failed,
        "MCP must expose failed evidence as a tool error"
    );
    assert_eq!(
        resource["pages"][0]["error"]["code"],
        "classificationContextEmpty"
    );
    runtime.close().await;
}

#[tokio::test]
async fn disjoint_file_match_windows_return_real_ranges_without_a_focus() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.95}},
            "usage":{"input_tokens":5,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let body = (1..=220)
        .map(|line| {
            if line == 10 || line == 160 {
                format!("MARKER {line}\n")
            } else {
                format!("line {line}\n")
            }
        })
        .collect::<String>();
    let file = workspace.write("disjoint.rs", body);
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"disjoint","reasoning":"Find relevant match windows.",
        "resources":[{"id":"f","context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read matching windows.",
            "matchString":"MARKER","contextLines":45,"chunkSize":50000
        }}}],
        "questions":[{"id":"q","type":"noul","instructions":"Does this content show MARKER?"}]
    });
    let outcome = runtime
        .execute("disjoint".into(), "clasify".into(), input)
        .await
        .unwrap();
    let page = &outcome.structured_content["queries"][0]["resources"][0]["pages"][0];
    assert_eq!(
        page["scope"]["lineRanges"].as_array().map(Vec::len),
        Some(2),
        "{page}"
    );
    assert!(page.get("focus").is_none(), "{page}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("disjoint scope output contract");
    runtime.close().await;
}

#[tokio::test]
async fn long_positive_scout_sends_only_authored_questions_and_preserves_probabilities() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.93456789},
                "answer_1":{"type":"choice","choice":"yes","confidence":0.87654321,
                    "probabilities":{"yes":0.9995,"no":0.0005}}
            },
            "usage":{"input_tokens":5,"output_tokens":2}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let body = (1..=200).map(|n| format!("line {n}\n")).collect::<String>();
    let file = workspace.write("long.rs", body);
    let expected_source_path = file.clone();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"pure-scout","reasoning":"Screen the document.",
        "resources":[{"id":"f","context":{"tool":"localFetch","query":{
            "path":file,"reasoning":"Read the source."
        }}}],
        "questions":[
            {"id":"shutdown","type":"noul","instructions":"Could this document contain shutdown guidance?"},
            {"id":"role","type":"choice","instructions":"Classify the document's role.",
                "criteria":{"yes":"Has guidance","no":"No guidance"}}
        ]
    });
    let outcome = runtime
        .execute("pure-scout".into(), "clasify".into(), input)
        .await
        .unwrap();
    let page = &outcome.structured_content["queries"][0]["resources"][0]["pages"][0];
    assert_eq!(page["answers"]["shutdown"]["noul"], 0.93456789);
    assert_eq!(
        page["answers"]["role"],
        json!({
            "choice":"yes","confidence":0.87654321,
            "probabilities":{"yes":0.9995,"no":0.0005}
        })
    );
    assert!(page.get("focus").is_none(), "{page}");
    assert!(
        outcome.structured_content["queries"][0]
            .get("lowSignal")
            .is_none()
    );
    assert_eq!(
        page["source"]["path"],
        expected_source_path.to_string_lossy().as_ref()
    );
    assert!(page["source"].get("evidenceHash").is_none());
    let requests = server.received_requests().await.expect("mock requests");
    assert_eq!(requests.len(), 1, "Clasify must not add a focus call");
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        sent["questions"]
            .as_object()
            .map(|questions| questions.len()),
        Some(2)
    );
    assert_eq!(
        sent["questions"]["answer_0"],
        json!({
            "type":"noul","instructions":"Could this document contain shutdown guidance?"
        })
    );
    assert_eq!(
        sent["questions"]["answer_1"],
        json!({
            "type":"choice","instructions":"Classify the document's role.",
            "criteria":{"yes":"Has guidance","no":"No guidance"}
        })
    );
    assert!(sent["state"].to_string().contains("line 200"));
    assert!(sent.get("windows").is_none());
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("pure scout output contract");
    runtime.close().await;
}

#[tokio::test]
async fn snippet_continuations_visit_every_file_and_match_page_before_completing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.1}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "fixture/a.txt",
        "marker: alpha\nmarker: secret needle\nmarker: omega\n",
    );
    let root = workspace.write(
        "fixture/b.txt",
        "marker: beta\nmarker: gamma\nmarker: delta\n",
    );
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut input = json!({
        "id":"matrix-1","reasoning":"Cover every file and match page.",
        "resources":[{"id":"files","context":{"tool":"localSearch","query":{
            "reasoning":"Page snippets.","path":root,"searchText":"marker",
            "pageSize":1,"maxMatchesPerFile":1,"sort":"path"
        }}}],
        "questions":[{"id":"needle","type":"noul","instructions":"Does the evidence contain secret needle?"}]
    });
    let mut coverages = Vec::new();
    for call in 0..10 {
        let outcome = runtime
            .execute(format!("page-{call}"), "clasify".into(), input.clone())
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        coverages.push(query["resources"][0]["coverage"].clone());
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    let sent = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            body["state"].to_string()
        })
        .collect::<Vec<_>>();
    let visited = |needle: &str| sent.iter().filter(|state| state.contains(needle)).count();
    for line in ["alpha", "secret needle", "omega", "beta", "gamma", "delta"] {
        assert_eq!(
            visited(line),
            1,
            "{line} must be judged exactly once: {sent:#?}"
        );
    }
    assert_eq!(coverages.len(), 6, "{coverages:?}");
    assert!(
        coverages[..5].iter().all(|coverage| coverage == "partial"),
        "every page with an outstanding branch is partial: {coverages:?}"
    );
    assert_eq!(coverages[5], "complete", "{coverages:?}");
    runtime.close().await;
}
