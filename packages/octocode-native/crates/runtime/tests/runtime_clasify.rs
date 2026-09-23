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
    assert_eq!(queries[0]["model"], "provider-resolved");
    assert_eq!(
        queries[0]["usage"],
        json!({"input_tokens":12,"output_tokens":3})
    );
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
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
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
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
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
                "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
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
    assert_eq!(query["resources"][0]["coverage"], "partial");
    assert!(
        query["resources"][0]["pages"][0]["limitations"][0]
            .as_str()
            .is_some_and(|text| text.contains("bounded prefix")),
        "{query}"
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
            "question": { "type": "noul", "instructions": "Relevant?" }
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
async fn search_resource_captures_only_the_requested_page() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
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
        "questions":[{"id":"relevant","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("search-page".into(), "clasify".into(), input)
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let cell = &query["resources"][0];
    assert_eq!(cell["pages"].as_array().unwrap().len(), 1, "{cell}");
    assert_eq!(cell["coverage"], "partial");
    let resume = &query["next"]["clasify"]["resources"][0]["context"];
    assert_eq!(resume["tool"], "localSearch", "{query}");
    assert_eq!(resume["query"]["page"], 2, "{resume}");
    let requests = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("needle"), "{body}");
    assert!(
        !body.contains("\"next\":"),
        "continuations stay out of provider state: {body}"
    );
    assert!(
        !body.contains("\"results\":"),
        "the response envelope stays out of provider state"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("single-page search output contract");
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
        "questions":[{"id":"q","question":{"type":"noul","instructions":"Relevant?"}}]
    });
    let outcome = runtime
        .execute("empty".into(), "clasify".into(), input)
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    assert_eq!(resource["coverage"], "error", "{resource}");
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
        "questions":[{"id":"q","question":{"type":"noul","instructions":"Does this page show a match?"}}]
    });
    let outcome = runtime
        .execute("empty-search".into(), "clasify".into(), input)
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    assert_eq!(resource["coverage"], "error", "{resource}");
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
        "questions":[{"id":"q","question":{"type":"noul","instructions":"Does this content show MARKER?"}}]
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
async fn high_scoring_file_pages_are_narrowed_to_a_focus_window() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.9},
                "answer_1":{"type":"choice","choice":"w3","confidence":0.9,
                    "probabilities":{"w1":0.05,"w2":0.05,"w3":0.9,"w4":0.0,"w5":0.0,"insufficient":0.0}}
            },
            "usage":{"input_tokens":5,"output_tokens":2}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let body = (1..=200).map(|n| format!("line {n}\n")).collect::<String>();
    let file = workspace.write("long.rs", body);
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"focus","reasoning":"Find the region.",
        "resources":[{"id":"f","context":{"tool":"localFetch","query":{"path":file,"reasoning":"Read it."}}}],
        "questions":[{"id":"q","question":{"type":"noul","instructions":"Does this content show X?"}}]
    });
    let outcome = runtime
        .execute("focus".into(), "clasify".into(), input)
        .await
        .unwrap();
    let page = &outcome.structured_content["queries"][0]["resources"][0]["pages"][0];
    assert_eq!(
        page["focus"],
        json!({"startLine":81,"endLine":120,"confidence":0.9}),
        "{page}"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("focus output contract");
    runtime.close().await;
}
