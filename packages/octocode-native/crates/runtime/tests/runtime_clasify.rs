#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

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

/// Aperiodic ASCII letters: byte pages of it differ, so identical-page
/// deduplication does not merge the pages a paging test counts.
fn filler(len: usize) -> String {
    let mut seed = 0x2545_f491_u32;
    (0..len)
        .map(|_| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            char::from(b'a' + ((seed >> 16) % 26) as u8)
        })
        .collect()
}

/// The full per-page receipt (`debug:true`): these tests inspect page
/// internals (scopes, sources, runner-up matches) the default output compacts.
fn verbose(mut input: serde_json::Value) -> serde_json::Value {
    if let Some(queries) = input
        .get_mut("queries")
        .and_then(serde_json::Value::as_array_mut)
    {
        queries
            .iter_mut()
            .for_each(|query| query["debug"] = json!(true));
    } else if input.is_object() {
        input["debug"] = json!(true);
    }
    input
}

/// A default-output answer: on the resource when it has one plain page,
/// else on its first page.
fn compact_answer(resource: &serde_json::Value, id: &str) -> serde_json::Value {
    resource["answers"]
        .get(id)
        .or_else(|| resource["pages"][0]["answers"].get(id))
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

fn query() -> serde_json::Value {
    json!({
        "id":"decision",
        "reasoning":"Choose the next inspection.",
        "goal":"Decide the next read.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[
            {"id":"relevant","type":"noul","instructions":"Is it relevant?"},
            {"id":"risk","type":"score","instructions":{"prompt":"Rate risk"},"criteria":["low",{"label":"high"}]}
        ]
    })
}

#[tokio::test]
async fn clasify_requires_goal_and_reasoning_on_the_provider_state() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("trace.txt", "Evidence is present.\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut input = query();
    input["resources"] =
        json!([{"id":"source","context":{"tool":"localFetch","query":{"path":file}}}]);
    input["questions"] =
        json!([{"id":"relevant","type":"noul","instructions":"Is evidence present?"}]);
    input["reasoning"] = json!("The next read depends on whether the file states the fact.");
    input["goal"] = json!("Files that state whether the evidence is present.");
    let outcome = runtime
        .execute(
            "semantic-reasoning".into(),
            "clasify".into(),
            verbose(input.clone()),
        )
        .await
        .expect("required briefs reach the provider");
    assert_eq!(
        outcome.structured_content["queries"][0]["resources"][0]["pages"][0]["answers"]["relevant"]
            ["noul"],
        0.8,
        "{}",
        outcome.structured_content
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body = String::from_utf8(requests[0].body.clone()).unwrap();
    assert!(
        body.contains("The next read depends on whether the file states the fact."),
        "reasoning must reach the provider state: {body}"
    );
    assert!(
        body.contains("Files that state whether the evidence is present."),
        "goal must reach the provider state: {body}"
    );
    for (field, invalid) in [
        ("reasoning", serde_json::Value::Null),
        ("reasoning", json!("")),
        ("reasoning", json!("   ")),
        ("goal", serde_json::Value::Null),
        ("goal", json!("")),
        ("goal", json!("   ")),
    ] {
        let mut rejected = input.clone();
        rejected[field] = invalid;
        let error = runtime
            .execute("semantic-reasoning".into(), "clasify".into(), rejected)
            .await
            .expect_err("blank or missing briefs are rejected");
        assert_eq!(error.code, "invalidInput");
    }
    for field in ["reasoning", "goal"] {
        let mut missing = input.clone();
        missing.as_object_mut().unwrap().remove(field);
        let error = runtime
            .execute("semantic-reasoning".into(), "clasify".into(), missing)
            .await
            .expect_err("missing briefs are rejected");
        assert_eq!(error.code, "invalidInput");
    }
    runtime.close().await;
}

#[tokio::test]
async fn provider_byte_limit_forwards_reasoning_and_isolates_oversized_questions() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.8}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(26)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut ordinary = query();
    ordinary["questions"] =
        json!([{"id":"relevant","type":"noul","instructions":"Is evidence present?"}]);
    let mut trace = ordinary.clone();
    trace["id"] = json!("trace");
    trace["reasoning"] = json!("Bound each captured resource before the next read.");
    // The matrix exceeds 4 MiB, but each independently captured resource and
    // provider request is within its own budget.
    trace["resources"] = json!((0..25)
        .map(|index| json!({"id":format!("source-{index}"),"context":{"value":format!("{index}{}", "界".repeat(60_000))}}))
        .collect::<Vec<_>>());
    let mut oversized = ordinary.clone();
    oversized["id"] = json!("oversized");
    oversized["questions"][0]["instructions"] = json!(
        (0..500)
            .map(|index| (format!("section-{index}"), json!("evidence ".repeat(1000))))
            .collect::<serde_json::Map<_, _>>()
    );
    let outcome = runtime
        .execute(
            "provider-byte-limit".into(),
            "clasify".into(),
            verbose(json!({"queries":[trace, oversized, ordinary]})),
        )
        .await
        .expect("one oversized provider request must not fail the matrix batch");
    let queries = &outcome.structured_content["queries"];
    for index in [0, 2] {
        assert_eq!(
            queries[index]["resources"][0]["pages"][0]["answers"]["relevant"]["noul"], 0.8,
            "{}",
            outcome.structured_content
        );
    }
    let oversized_resource = &queries[1]["resources"][0];
    assert_eq!(oversized_resource["coverage"], "error");
    let error = &oversized_resource["pages"][0]["answers"]["relevant"]["error"];
    assert_eq!(error["code"], "invalidClassificationRequest");
    assert!(error["message"].as_str().unwrap().contains("4 MiB"));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 26);
    for request in requests {
        let body = String::from_utf8(request.body).unwrap();
        assert!(
            body.contains("Bound each captured resource before the next read.")
                || body.contains("Choose the next inspection."),
            "reasoning must reach the provider state"
        );
        assert!(body.len() < 200_000);
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
    // Default output: a supplied value is one plain page, so its answers sit
    // on the resource; complete coverage is implied.
    assert_eq!(
        queries[0]["resources"],
        json!([{"resourceId":"observed","answers":{
            "relevant":0.9,
            "risk":{"score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75}}
        }}]),
        "{}",
        outcome.structured_content
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
        "reasoning":"Assess independent resources without serial provider latency.","goal":"Decide the next read.",
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
    // Default output: complete coverage is implied and a single page's
    // answers sit on the resource.
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result["resourceId"], format!("resource-{index}"));
        assert!(result.get("coverage").is_none(), "{result}");
        assert!(result["answers"]["relevant"].is_number(), "{result}");
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
        "reasoning":"Assess many resources without exceeding provider concurrency.","goal":"Decide the next read.",
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
            .all(|result| result.get("coverage").is_none()
                && result["answers"]["relevant"].is_number()),
        "{results:?}"
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
                "reasoning":"Assess an independent matrix without serial provider latency.","goal":"Decide the next read.",
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
        let resource = &query["resources"][0];
        assert!(resource.get("coverage").is_none(), "{resource}");
        assert!(resource["answers"]["relevant"].is_number(), "{resource}");
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
        "reasoning":"Bound the supplied resource.","goal":"Decide the next read.",
        "resources":[{"id":"large","maxChars":5,"context":{"value":{"text":"far too large"}}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("bounded".into(), "clasify".into(), verbose(input))
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
        format!("{}{}", filler(78_377 - marker.len()), marker),
    );
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"recover-full-content",
        "reasoning":"Recover the exact pages of an oversized whole-file request.","goal":"Decide the next read.",
        "resources":[{"id":"file","maxChars":80_000,"context":{"tool":"localFetch","query":{
            "path":file,"goal": "test", "reasoning":"Read the complete file.","fullContent":true
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute(
            "recover-full-content".into(),
            "clasify".into(),
            verbose(input),
        )
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
        "id":"novelty", "reasoning":"Decide whether this unread section adds evidence.","goal":"Decide the next read.",
        "resources":[{"id":"hooks","context":{"tool":"localFetch","query":{"path":file,"goal": "test", "reasoning":"Screen the complete section."}}}],
        "questions":[{"id":"new","questionType":"addsEvidence","target":"Shutdown timing", "knownEvidence":["onClose runs after requests finish"]}]
    });
    let result = runtime
        .execute("preset".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let output = &result.structured_content["queries"][0];
    assert!(output.get("templateVersion").is_none());
    let page = &output["resources"][0]["pages"][0];
    assert_eq!(page["answers"]["new"]["noul"], 0.82);
    assert_eq!(
        page["source"]["path"],
        file.file_name().unwrap().to_str().unwrap()
    );
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
        "path":workspace.workspace, "searchText":"hooks", "goal": "test", "reasoning":"Discover candidates.",
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
        "reasoning":"Assess bounded pages.","goal":"Decide the next read.",
        "resources":[{"id":"file","maxChars":5000,"context":{"tool":"localFetch","query":{
            "path":file,"goal": "test", "reasoning":"Read the next exact line.","chunkSize":1,"fullContent":false
        }}}],
        "questions":[{"id":"relevant","questionType":"contribution","target":"line content"}]
    });
    let outcome = runtime
        .execute("paged".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let assess = outcome.structured_content["queries"][0]["next"]["clasify"].clone();
    assert!(assess.is_object(), "{}", outcome.structured_content);
    assert_eq!(assess["debug"], true, "a debug walk continues as one");
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
    let file = workspace.write("over-budget.txt", filler(80_001));
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"over-budget",
        "reasoning":"Assess no more than the resource payload budget.","goal":"Decide the next read.",
        "resources":[{"id":"file","maxChars":80_000,"context":{"tool":"localFetch","query":{
            "path":file,"goal": "test", "reasoning":"Read the complete file.","fullContent":true
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let first = runtime
        .execute("over-budget-first".into(), "clasify".into(), verbose(input))
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
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_CLASSIFICATION_API", "secret".into())]);
    let input = json!({
        "id": "bad-inner-query",
        "reasoning": "Test that an invalid inner path surfaces its field name.","goal":"Decide the next read.",
        "resources": [{
            "id": "r1",
            "context": {
                "tool": "localFetch",
                "query": { "path": 42 }
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
        .expect_err("invalid delegated path must fail contract validation");
    assert_eq!(error.code, "invalidInput");
    let payload = error.payload.expect("structured validation payload");
    let details = payload["details"].as_array().expect("validation details");
    assert!(
        details.iter().any(|detail| detail
            .as_str()
            .is_some_and(|detail| detail.contains("resources.0.context.query.path"))),
        "validation detail must name the nested path field: {details:?}"
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
        "reasoning":"Judge one search page.","goal":"Decide the next read.",
        "resources":[{"id":"hits","context":{"tool":"localSearch","query":{
            "path":root,"searchText":"needle","goal": "test", "reasoning":"Find hits.",
            "resultView":"paginated","pageSize":2
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("search-page".into(), "clasify".into(), verbose(input))
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
    // Workspace-relative (TOOL_DATA_CONTRACT "Paths"); localFetch resolves
    // them against the workspace root, so they stay executable.
    assert!(
        source_paths.iter().all(|path| {
            !std::path::Path::new(path).is_absolute() && workspace.workspace.join(path).is_file()
        }),
        "local candidate paths must be workspace-relative files: {source_paths:?}"
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

/// clasify's own `next.clasify` is a generated `ClasifyInput` that passes the
/// same preparation and validation as caller input, and replays as-is.
#[tokio::test]
async fn emitted_next_clasify_is_schema_valid_input_and_replays() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    for index in 0..5 {
        workspace.write(&format!("src/file{index}.txt"), "needle marker\n");
    }
    let root = workspace.write("src/file5.txt", "needle marker\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"walk",
        "reasoning":"Judge the hits page by page.","goal":"Decide the next read.",
        "resources":[{"id":"hits","context":{"tool":"localSearch","query":{
            "path":root,"searchText":"needle","resultView":"paginated","pageSize":2
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let first = runtime
        .execute("walk-1".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    octocode_native::contracts::validate_output("clasify", &first.structured_content)
        .expect("first page output contract");
    let next = first.structured_content["queries"][0]["next"]["clasify"].clone();
    assert_eq!(next["goal"], "Decide the next read.", "{next}");
    assert_eq!(next["questions"][0]["id"], "relevant", "{next}");
    assert_eq!(
        next["resources"][0]["context"]["query"]["page"], 2,
        "{next}"
    );
    serde_json::from_value::<octocode_native::contracts::tool_types::ClasifyInput>(next.clone())
        .unwrap_or_else(|error| panic!("generated ClasifyInput: {error}: {next}"));
    let prepared = octocode_native::contracts::prepare_many_and_validate(
        "clasify",
        next.clone(),
        octocode_native::contracts::PrepareOptions::default(),
    )
    .unwrap_or_else(|error| {
        panic!(
            "clasify input contract: {:?}: {next}",
            error
                .issues
                .first()
                .map(|issue| (&issue.path, &issue.message))
        )
    });
    assert_eq!(prepared.len(), 1);
    let first_paths = first.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|page| page["source"]["path"].clone())
        .collect::<Vec<_>>();
    let replay = runtime
        .execute("walk-2".into(), "clasify".into(), next)
        .await
        .expect("replayed next.clasify");
    octocode_native::contracts::validate_output("clasify", &replay.structured_content)
        .expect("replayed page output contract");
    let cell = &replay.structured_content["queries"][0]["resources"][0];
    assert_ne!(cell["coverage"], "error", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    assert!(!pages.is_empty(), "{cell}");
    assert!(
        pages
            .iter()
            .all(|page| !first_paths.contains(&page["source"]["path"])),
        "the replay reads the next page: {cell}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn file_chunk_scout_judges_every_hit_cluster_of_a_clipped_file() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":3,"output_tokens":1}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Eleven hits near the top clip the file at ten rows; the deciding hit
    // sits far below, listed only in the file's moreLines.
    let mut body = String::from("header\n");
    for _ in 0..11 {
        body.push_str("needle marker\n");
    }
    for _ in 0..390 {
        body.push_str("filler line\n");
    }
    body.push_str("needle decides the answer\n");
    workspace.write("src/only.txt", body);
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
        "id":"clusters",
        "reasoning":"Judge every hit cluster.","goal":"Find the deciding line.",
        "resources":[{"id":"hits","context":{
            "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                "path":root,"searchText":"needle"
            }
        }}],
        "questions":[{"id":"decides","type":"noul","instructions":"Does this source decide the answer?"}]
    });
    let outcome = runtime
        .execute("clusters".into(), "clasify".into(), verbose(input))
        .await
        .expect("cluster scout");
    let pages = outcome.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .clone();
    let starts = pages
        .iter()
        .map(|page| page["scope"]["startLine"].as_u64())
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2, "{}", outcome.structured_content);
    assert!(
        pages.iter().all(|page| page.get("error").is_none()),
        "{pages:?}"
    );
    let sent = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["state"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        sent.iter()
            .any(|content| content.contains("needle decides the answer")),
        "{sent:?}"
    );
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("cluster output contract");
    runtime.close().await;
}

/// Two hit clusters of one file within a window radius are judged as one
/// contiguous page: one provider call instead of two. A span too large for
/// one bounded page falls back to one page per cluster.
#[tokio::test]
async fn file_chunk_scout_judges_near_clusters_of_one_file_in_one_call() {
    for (filler, calls) in [("filler line", 1u64), (&*"long filler ".repeat(9), 2)] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model":"resolved",
                "answers":{"answer":{"type":"noul","noul":0.7}},
                "usage":{"input_tokens":3,"output_tokens":1}
            })))
            .expect(calls)
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let mut body = String::from("header\n");
        for _ in 0..11 {
            body.push_str("needle marker\n");
        }
        for _ in 0..150 {
            body.push_str(filler);
            body.push('\n');
        }
        body.push_str("needle decides the answer\n");
        workspace.write("src/only.txt", body);
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
            "id":"near",
            "reasoning":"Judge near hit clusters.","goal":"Find the deciding line.",
            "resources":[{"id":"hits","context":{
                "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                    "path":root,"searchText":"needle"
                }
            }}],
            "questions":[{"id":"decides","type":"noul","instructions":"Does this source decide the answer?"}]
        });
        let outcome = runtime
            .execute("near".into(), "clasify".into(), verbose(input))
            .await
            .expect("near scout");
        let pages = outcome.structured_content["queries"][0]["resources"][0]["pages"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(pages.len() as u64, calls, "{}", outcome.structured_content);
        assert!(
            pages.iter().all(|page| page.get("error").is_none()),
            "{pages:?}"
        );
        let sent = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|request| {
                serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["state"]["content"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert!(
            sent.iter()
                .any(|content| content.contains("needle decides the answer")),
            "{sent:?}"
        );
        assert!(
            sent.iter().any(|content| content.contains("needle marker")),
            "{sent:?}"
        );
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("near cluster output contract");
        runtime.close().await;
    }
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
        "reasoning":"Judge source around each search hit.","goal":"Decide the next read.",
        "resources":[{"id":"hits","context":{
            "tool":"localSearch","candidateEvidence":"fileChunks","query":{
                "path":root,"searchText":"needle","goal": "test", "reasoning":"Find candidates.",
                "resultView":"paginated","pageSize":20
            }
        },"maxChars":20_000}],
        "questions":[{"id":"relevant",
            "type":"noul","instructions":"Does this source contain a body-only fact?"
        }]
    });
    let outcome = runtime
        .execute("hydrated-search".into(), "clasify".into(), verbose(input))
        .await
        .expect("hydrated scout");
    let query = &outcome.structured_content["queries"][0];
    let pages = query["resources"][0]["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 5, "{query}");
    for page in pages {
        assert_eq!(page["next"]["read"]["tool"], "localFetch", "{page}");
        assert_eq!(page["next"]["read"]["confidence"], "exact", "{page}");
        // Workspace-relative, like every local tool's rows.
        assert!(
            page["source"]["path"]
                .as_str()
                .is_some_and(|p| p.starts_with("src/candidate")),
            "{page}"
        );
        assert!(page.get("limitations").is_none(), "stated once: {page}");
    }
    let read = &pages[0]["next"]["read"];
    let fetched = runtime
        .execute(
            "hydrated-read".into(),
            "localFetch".into(),
            json!({"queries":[read["query"]]}),
        )
        .await
        .expect("read replays");
    assert_eq!(
        fetched.structured_content["results"][0]["data"]["path"], pages[0]["source"]["path"],
        "{}",
        fetched.structured_content
    );
    // Every page shares the bounded-chunk limit, so the resource states it once.
    assert!(
        query["resources"][0]["limitations"]
            .as_array()
            .is_some_and(|limits| limits.iter().any(|v| {
                v.as_str()
                    .is_some_and(|v| v.contains("bounded candidate chunk"))
            })),
        "{query}"
    );
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
        // Six matches per root with pageSize 5: each search has a next page.
        for index in 0..6 {
            workspace.write(
                &format!("{root}/candidate{index}.txt"),
                "needle\nbody evidence\n",
            );
        }
    }
    let resource = |id: &str, root: &str| {
        json!({
            "id":id,"context":{"tool":"localSearch","candidateEvidence":"search","query":{
                "path":workspace.workspace.join(root),"searchText":"needle","goal": "test", "reasoning":"Find candidates.",
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
            verbose(json!({
                "id":"expanded-cells","reasoning":"Exercise the runtime expansion gate.","goal":"Decide the next read.",
                "resources":[resource("a","a"),resource("b","b")],"questions":questions
            })),
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
    // No page was judged, so a continuation past them would skip them for good.
    assert!(
        outcome.structured_content["queries"][0]
            .get("next")
            .is_none(),
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
        "id":"empty","reasoning":"Screen an empty artifact.","goal":"Decide the next read.",
        "resources":[{"id":"e","context":{"tool":"localFetch","query":{"path":file,"goal": "test", "reasoning":"Read it."}}}],
        "questions":[{"id":"q","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("empty".into(), "clasify".into(), verbose(input))
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
        "id":"empty-search","reasoning":"Screen a search page.","goal":"Decide the next read.",
        "resources":[{"id":"e","context":{"tool":"localSearch","query":{
            "path":file,"searchText":"UNLIKELY_OCTOCODE_SENTINEL_673829",
            "goal": "test", "reasoning":"Find matching source."
        }}}],
        "questions":[{"id":"q","type":"noul","instructions":"Does this page show a match?"}]
    });
    let outcome = runtime
        .execute("empty-search".into(), "clasify".into(), verbose(input))
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
        "id":"disjoint","reasoning":"Find relevant match windows.","goal":"Decide the next read.",
        "resources":[{"id":"f","context":{"tool":"localFetch","query":{
            "path":file,"goal": "test", "reasoning":"Read matching windows.",
            "matchString":"MARKER","contextLines":45,"chunkSize":50000
        }}}],
        "questions":[{"id":"q","type":"noul","instructions":"Does this content show MARKER?"}]
    });
    let outcome = runtime
        .execute("disjoint".into(), "clasify".into(), verbose(input))
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
        "id":"pure-scout","reasoning":"Screen the document.","goal":"Decide the next read.",
        "resources":[{"id":"f","context":{"tool":"localFetch","query":{
            "path":file,"goal": "test", "reasoning":"Read the source."
        }}}],
        "questions":[
            {"id":"shutdown","type":"noul","instructions":"Could this document contain shutdown guidance?"},
            {"id":"role","type":"choice","instructions":"Classify the document's role.",
                "criteria":{"yes":"Has guidance","no":"No guidance"}}
        ]
    });
    let outcome = runtime
        .execute("pure-scout".into(), "clasify".into(), verbose(input))
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
    // Local sources display workspace-relative (TOOL_DATA_CONTRACT "Paths").
    assert_eq!(
        page["source"]["path"],
        expected_source_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .as_ref()
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
            "instructions":{
                "question":"Could this document contain shutdown guidance?",
                "goal":"Decide the next read."
            },
            "type":"noul"
        })
    );
    assert_eq!(
        sent["questions"]["answer_1"],
        json!({
            "instructions":{
                "question":"Classify the document's role.",
                "goal":"Decide the next read."
            },
            "type":"choice",
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
        "id":"matrix-1","reasoning":"Cover every file and match page.","goal":"Decide the next read.",
        "resources":[{"id":"files","context":{"tool":"localSearch","query":{
            "goal": "test", "reasoning":"Page snippets.","path":root,"searchText":"marker",
            "pageSize":1,"maxMatchesPerFile":1,"sort":"path"
        }}}],
        "questions":[{"id":"needle","type":"noul","instructions":"Does the evidence contain secret needle?"}]
    });
    let mut coverages = Vec::new();
    for call in 0..10 {
        let outcome = runtime
            .execute(
                format!("page-{call}"),
                "clasify".into(),
                verbose(input.clone()),
            )
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

#[tokio::test]
async fn clasify_locate_preflight_returns_a_typed_error_without_capture_or_provider_calls() {
    let server = MockServer::start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ]);
    let input = json!({
        "reasoning":"Reject locate on a search resource.",
        "goal":"Files that state the validation rules.",
        "resources":[
            {"id":"unread","context":{"tool":"localFetch","query":{"path":"/does-not-exist"}}},
            {"id":"search","context":{"tool":"ghSearchCode","query":{"owner":"nonexistent"}}}
        ],
        "questions":[
            {"questionType":"locate","target":"Validation rules"},
            {"type":"noul","instructions":"Does this validate requests?"}
        ]
    });
    let outcome = runtime
        .execute("preflight".into(), "clasify".into(), verbose(input))
        .await
        .expect("typed rejection rather than WorkerFailed");
    let resources = outcome.structured_content["queries"][0]["resources"]
        .as_array()
        .unwrap();
    assert_eq!(resources.len(), 2);
    for resource in resources {
        assert_eq!(resource["coverage"], "error");
        let page = &resource["pages"][0];
        assert_eq!(page["error"]["code"], "classificationLocateUnsupported");
        assert!(
            page["error"]["message"]
                .as_str()
                .unwrap()
                .contains("search")
        );
        assert!(page.get("answers").is_none());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_repeated_matrix_replays_its_judgment_without_a_second_provider_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("replay.txt", "The retry floor is 100 ms.\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut input = query();
    input["resources"] =
        json!([{"id":"source","context":{"tool":"localFetch","query":{"path":file}}}]);
    input["questions"] =
        json!([{"id":"floor","type":"noul","instructions":"Is the retry floor stated?"}]);
    let first = runtime
        .execute("replay-1".into(), "clasify".into(), input.clone())
        .await
        .expect("first judgment");
    let second = runtime
        .execute("replay-2".into(), "clasify".into(), input)
        .await
        .expect("replayed judgment");
    let answer = |outcome: &octocode_native::runtime::ToolOutcome| {
        compact_answer(
            &outcome.structured_content["queries"][0]["resources"][0],
            "floor",
        )
    };
    assert_eq!(answer(&first), json!(0.7), "{}", first.structured_content);
    assert_eq!(answer(&second), answer(&first));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

fn noul_response(noul: f64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "model":"resolved",
        "answers":{"answer":{"type":"noul","noul":noul}},
        "usage":{"input_tokens":2,"output_tokens":1}
    }))
}

#[tokio::test]
async fn identical_pages_in_one_call_share_a_single_provider_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.6).set_delay(Duration::from_millis(200)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let evidence = json!({"value":"The dedupe sentinel 4411 is stated here."});
    let matrix = |id: &str| {
        json!({
            "id":id,"reasoning":"Screen duplicated evidence.","goal":"Decide the next read.",
            "resources":[{"id":"a","context":evidence},{"id":"b","context":evidence}],
            "questions":[{"id":"q","type":"noul","instructions":"Is sentinel 4411 stated?"}]
        })
    };
    let outcome = runtime
        .execute(
            "dedupe".into(),
            "clasify".into(),
            json!({"queries":[matrix("m1"), matrix("m2")]}),
        )
        .await
        .unwrap();
    for query in outcome.structured_content["queries"].as_array().unwrap() {
        for resource in query["resources"].as_array().unwrap() {
            assert_eq!(
                compact_answer(resource, "q"),
                json!(0.6),
                "{}",
                outcome.structured_content
            );
        }
    }
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "four identical cells must not race four provider requests"
    );
    runtime.close().await;
}

#[tokio::test]
async fn judgment_cache_ignores_correlation_ids_but_not_question_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.4))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = |question_id: &str, instructions: &str| {
        json!({
            "id":"ids","reasoning":"Cache key probe.","goal":"Decide the next read.",
            "resources":[{"id":"v","context":{"value":"Cache id sentinel 7719."}}],
            "questions":[{"id":question_id,"type":"noul","instructions":instructions}]
        })
    };
    for (label, question_id, instructions) in [
        ("first", "q", "Is sentinel 7719 stated?"),
        ("renamed", "renamed", "Is sentinel 7719 stated?"),
        ("changed", "q", "Is sentinel 7720 stated?"),
    ] {
        let outcome = runtime
            .execute(
                label.into(),
                "clasify".into(),
                input(question_id, instructions),
            )
            .await
            .unwrap();
        assert_eq!(
            compact_answer(
                &outcome.structured_content["queries"][0]["resources"][0],
                question_id
            ),
            json!(0.4),
            "{label}: {}",
            outcome.structured_content
        );
    }
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "a renamed id replays; changed question text asks again"
    );
    runtime.close().await;
}

#[tokio::test]
async fn prefilter_window_is_centered_on_the_hit_not_aligned_to_a_bucket() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.9))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // The hit sits on line 601, the first line of the second 600-line bucket;
    // its enclosing function header is on line 596.
    let source = (1..=1300)
        .map(|line| match line {
            596 => "function retryDelay(attempt) { // HEADER_SENTINEL\n".to_owned(),
            601 => "  const RETRY_FLOOR_MS = 100;\n".to_owned(),
            _ => format!("const filler_{line} = {line};\n"),
        })
        .collect::<String>();
    let file = workspace.write("huge.js", source);
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let outcome = runtime
        .execute(
            "prefilter".into(),
            "clasify".into(),
            verbose(json!({
                "id":"pf","reasoning":"Judge only the hit window.","goal":"Find the retry floor.",
                "resources":[{"id":"f","prefilter":["RETRY_FLOOR_MS"],
                    "context":{"tool":"localFetch","query":{"path":file}}}],
                "questions":[{"id":"q","type":"noul","instructions":"Is the retry floor defined?"}]
            })),
        )
        .await
        .unwrap();
    let pages = outcome.structured_content["queries"][0]["resources"][0]["pages"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(pages.len(), 1, "{}", outcome.structured_content);
    let scope = &pages[0]["scope"];
    let (start, end) = (
        scope["startLine"].as_u64().unwrap(),
        scope["endLine"].as_u64().unwrap(),
    );
    assert!(
        start < 596 && end > 601 && end - start < 600,
        "window {start}-{end} must surround the hit"
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(String::from_utf8_lossy(&requests[0].body).contains("HEADER_SENTINEL"));
    runtime.close().await;
}

#[tokio::test]
async fn prefilter_hits_beyond_three_windows_resume_through_next_clasify() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(noul_response(0.9))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Four hits, each needing its own 600-line window.
    let hits = [100_u64, 1100, 2100, 3100];
    let source = (1..=4000_u64)
        .map(|line| {
            if hits.contains(&line) {
                format!("const NEEDLE_{line} = {line}; // NEEDLE_MARK\n")
            } else {
                format!("const filler_{line} = {line};\n")
            }
        })
        .collect::<String>();
    let file = workspace.write("walk.js", source);
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let judged = |content: &serde_json::Value| {
        content["queries"][0]["resources"][0]["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|page| {
                (
                    page["scope"]["startLine"].as_u64().unwrap(),
                    page["scope"]["endLine"].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    let covers = |pages: &[(u64, u64)], line: u64| {
        pages
            .iter()
            .any(|(start, end)| (*start..=*end).contains(&line))
    };
    let input = |max_chars: Option<u64>| {
        let mut resource = json!({"id":"f","prefilter":["NEEDLE_MARK"],
            "context":{"tool":"localFetch","query":{"path":file}}});
        if let Some(max_chars) = max_chars {
            resource["maxChars"] = json!(max_chars);
        }
        json!({
            "id":"walk","reasoning":"Judge every hit window.","goal":"Find the needle.",
            "resources":[resource],
            "questions":[{"id":"q","type":"noul","instructions":"Is a needle defined?"}]
        })
    };

    let first = runtime
        .execute("walk-1".into(), "clasify".into(), verbose(input(None)))
        .await
        .unwrap();
    let pages = judged(&first.structured_content);
    assert_eq!(pages.len(), 3, "{}", first.structured_content);
    for line in &hits[..3] {
        assert!(covers(&pages, *line), "{line} in {pages:?}");
    }
    assert!(!covers(&pages, 3100));
    let query = &first.structured_content["queries"][0];
    assert!(
        query.to_string().contains("Prefilter"),
        "dropped hits must be reported: {query}"
    );
    let resume = query["next"]["clasify"].clone();
    assert!(
        resume.is_object(),
        "dropped hits need a continuation: {query}"
    );
    let resumed = runtime
        .execute("walk-2".into(), "clasify".into(), resume)
        .await
        .expect("next.clasify must execute unchanged");
    let pages = judged(&resumed.structured_content);
    assert!(covers(&pages, 3100), "{}", resumed.structured_content);
    assert!(pages.iter().all(|(start, _)| *start > 2100), "{pages:?}");
    assert!(
        resumed.structured_content["queries"][0]
            .get("next")
            .is_none()
    );

    // One maxChars budget spans every window: the second window waits for
    // the next call instead of each window getting its own full budget.
    let tight = runtime
        .execute(
            "walk-tight".into(),
            "clasify".into(),
            verbose(input(Some(20_000))),
        )
        .await
        .unwrap();
    let pages = judged(&tight.structured_content);
    assert_eq!(pages.len(), 1, "{}", tight.structured_content);
    assert!(covers(&pages, 100));
    let resume = &tight.structured_content["queries"][0]["next"]["clasify"]["resources"][0];
    assert_eq!(resume["prefilter"], json!(["NEEDLE_MARK"]), "{resume}");
    let from = resume["context"]["query"]["startLine"].as_u64().unwrap();
    assert!(from > 100 && from <= 1100, "resume at {from}");

    // Local absolute paths never reach the external provider.
    for request in server.received_requests().await.unwrap() {
        let body = String::from_utf8_lossy(&request.body);
        for root in [
            workspace.workspace.clone(),
            std::fs::canonicalize(&workspace.workspace).unwrap(),
        ] {
            let root = root.to_string_lossy().replace('\\', "/");
            assert!(
                !body.contains(&root),
                "absolute path sent to provider: {body}"
            );
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn partial_provider_answers_are_not_cached_and_a_failed_read_is_isolated() {
    let server = MockServer::start().await;
    // First response drops answer_1 (partial); later responses are complete.
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer_0":{"type":"noul","noul":0.3}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer_0":{"type":"noul","noul":0.3},"answer_1":{"type":"noul","noul":0.9}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let missing = workspace.workspace.join("missing-8812.txt");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "id":"partial","reasoning":"Partial answers stay unresolved.","goal":"Decide the next read.",
        "resources":[
            {"id":"gone","context":{"tool":"localFetch","query":{"path":missing}}},
            {"id":"v","context":{"value":"Partial cache sentinel 5521."}}
        ],
        "questions":[
            {"id":"a","type":"noul","instructions":"Is sentinel 5521 stated?"},
            {"id":"b","type":"noul","instructions":"Is sentinel 5522 stated?"}
        ]
    });
    let first = runtime
        .execute("partial-1".into(), "clasify".into(), verbose(input.clone()))
        .await
        .unwrap();
    let resources = &first.structured_content["queries"][0]["resources"];
    assert_eq!(resources[0]["coverage"], "error", "{resources}");
    assert!(resources[0]["pages"][0]["error"]["code"].is_string());
    assert_eq!(resources[1]["coverage"], "partial", "{resources}");
    assert_eq!(
        resources[1]["pages"][0]["answers"]["a"],
        json!({"noul":0.3})
    );
    assert!(resources[1]["pages"][0]["answers"]["b"]["error"].is_object());
    let second = runtime
        .execute("partial-2".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let page = &second.structured_content["queries"][0]["resources"][1]["pages"][0];
    assert_eq!(page["answers"]["b"], json!({"noul":0.9}), "{page}");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "a partial answer set must be asked again, not replayed"
    );
    runtime.close().await;
}

/// orangu 09-28: clasify with a ghSearchCode resource failed with
/// `classificationContextContractViolation` on every resource. A code-search
/// page must be judged like any other search resource.
#[tokio::test]
async fn gh_search_code_resource_is_judged_without_a_context_contract_violation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.6}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let items = (0..3)
        .map(|n| {
            json!({
                "name": format!("f{n}.rs"), "path": format!("src/f{n}.rs"),
                "sha": format!("{n:040}"), "html_url": "https://github.com/o/r",
                "repository": {"full_name":"o/r","html_url":"https://github.com/o/r","url":"https://api.github.com/repos/o/r"},
                "text_matches": [{"fragment": format!("fn semaphore_acquire_{n}() {{}}"), "matches": [{"indices":[3,12]}]}]
            })
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 3, "incomplete_results": false, "items": items
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        // Own GitHub limiter key: the process-wide budget is keyed by host and
        // token, so concurrent GitHub fixtures must not share throttling state.
        ("OCTOCODE_TOKEN", "clasify-gh-search-code-fixture".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "reasoning":"Judge the code-search hits.","goal":"Where the semaphore is acquired.",
        "resources":[{"id":"hits","context":{"tool":"ghSearchCode","query":{
            "owner":"o","repo":"r","keywords":["semaphore"]
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Does this acquire the semaphore?"}]
    });
    let outcome = runtime
        .execute("gh-code".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let rendered = outcome.structured_content.to_string();
    assert!(!rendered.contains("ContextContractViolation"), "{rendered}");
    let cell = &outcome.structured_content["queries"][0]["resources"][0];
    assert_ne!(cell["coverage"], "error", "{cell}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("clasify output contract");
    runtime.close().await;
}

/// Locate provider stub: the choice question puts 0.9 on the first passage ID
/// it was offered; the existence question answers 0.9.
#[derive(Clone)]
struct LocateFirstPassage;

impl Respond for LocateFirstPassage {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        fn passage_ids(value: &serde_json::Value, ids: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (key, value) in map {
                        if key.len() == 4 && key.starts_with('P') && !ids.contains(key) {
                            ids.push(key.clone());
                        }
                        passage_ids(value, ids);
                    }
                }
                serde_json::Value::Array(items) => {
                    items.iter().for_each(|item| passage_ids(item, ids));
                }
                _ => {}
            }
        }
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let mut ids = Vec::new();
        passage_ids(&body, &mut ids);
        ids.sort();
        let rest = if ids.len() > 1 {
            0.1 / (ids.len() - 1) as f64
        } else {
            0.0
        };
        let probabilities = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), json!(if index == 0 { 0.9 } else { rest })))
            .collect::<serde_json::Map<_, _>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"choice","choice":ids.first(),"confidence":0.9,
                    "probabilities":probabilities},
                "answer_1":{"type":"noul","noul":0.9}
            },
            "usage":{"input_tokens":5,"output_tokens":2}
        }))
    }
}

/// A lone strong locate window on a GitHub file is public in `best` with an
/// exact, schema-valid read that names owner/repo/path/branch.
#[tokio::test]
async fn a_lone_strong_locate_window_in_best_carries_an_exact_github_read() {
    use base64::Engine as _;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateFirstPassage)
        .mount(&server)
        .await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let source = (1..=40)
        .map(|n| format!("fn step_{n}() -> u32 {{\n    {n}\n}}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents/src%2Fsteps.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content": base64::engine::general_purpose::STANDARD.encode(&source),
            "size": source.len(), "sha": "f".repeat(40), "path":"src/steps.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        // Own GitHub limiter key: the process-wide budget is keyed by host and
        // token, so concurrent GitHub fixtures must not share throttling state.
        ("OCTOCODE_TOKEN", "clasify-locate-read-fixture".into()),
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "reasoning":"Locate the first step.","goal":"Where step one is defined.",
        "resources":[{"id":"gh","context":{"tool":"ghGetFileContent","query":{
            "owner":"o","repo":"r","path":"src/steps.rs","branch":"main","fullContent":true
        }}}],
        "questions":[{"id":"t","questionType":"locate","target":"The function that returns one."}]
    });
    let outcome = runtime
        .execute("locate-gh".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output)
        .expect("best row read output contract");
    let rows = output["queries"][0]["best"]["t"]
        .as_array()
        .unwrap_or_else(|| panic!("best rows: {output}"));
    assert!(!rows.is_empty(), "{output}");
    for row in rows {
        let read = &row["next"]["read"];
        assert_eq!(read["tool"], "ghGetFileContent", "{row}");
        let query = &read["query"];
        assert_eq!(query["owner"], "o", "{row}");
        assert_eq!(query["repo"], "r", "{row}");
        assert_eq!(query["path"], "src/steps.rs", "{row}");
        assert_eq!(query["branch"], "main", "{row}");
        assert_eq!(query["startLine"], row["startLine"], "{row}");
        assert_eq!(query["endLine"], row["endLine"], "{row}");
        assert!(query.get("fullContent").is_none(), "{row}");
    }
    assert_eq!(rows[0]["startLine"], 1, "{output}");
    // A finished walk has no carry; nothing private leaks.
    assert!(output["queries"][0].get("next").is_none(), "{output}");
    assert!(!output.to_string().contains("fileRead"), "{output}");
    runtime.close().await;
}

/// A locate target naming an identifier over a local file keeps its string
/// hint and gains `next.localSearch`, a schema-valid literal search that runs.
#[tokio::test]
async fn identifier_locate_target_emits_an_executable_local_search() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(LocateFirstPassage)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let source = (1..=40)
        .map(|n| format!("fn step_{n}() -> u32 {{\n    {n}\n}}\n"))
        .collect::<String>();
    let file = workspace.write("src/steps.rs", source);
    let file = file.to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let input = json!({
        "reasoning":"Locate a step.","goal":"Where step_17 is defined.",
        "resources":[{"id":"steps","context":{"tool":"localFetch","query":{
            "path":file,"fullContent":true
        }}}],
        "questions":[{"id":"t","questionType":"locate","target":"Where is step_17 defined?"}]
    });
    let outcome = runtime
        .execute("locate-literal".into(), "clasify".into(), verbose(input))
        .await
        .expect("clasify");
    let output = &outcome.structured_content;
    octocode_native::contracts::validate_output("clasify", output)
        .expect("literal search continuation output contract");
    let query = &output["queries"][0];
    assert!(
        query["hints"][0]
            .as_str()
            .unwrap_or_default()
            .contains("step_17"),
        "{query}"
    );
    let search = &query["next"]["localSearch"];
    assert_eq!(search["tool"], "localSearch", "{query}");
    assert_eq!(search["query"]["path"], file.as_str(), "{search}");
    assert_eq!(search["query"]["searchText"], "step_17", "{search}");
    let found = runtime
        .execute(
            "literal-search".into(),
            "localSearch".into(),
            search["query"].clone(),
        )
        .await
        .expect("next.localSearch executes");
    let rendered = found.structured_content.to_string();
    assert!(rendered.contains("fn step_17()"), "{rendered}");
    assert!(
        !rendered.contains("fn step_1()"),
        "literal match only: {rendered}"
    );
    runtime.close().await;
}

/// A directory outline pages by declaration rows, so one file's declarations
/// can straddle a symbols page. Scout judges each file once, whole: the page
/// holding a file's first row judges all of its rows, and the next page skips
/// the rows it already judged.
#[tokio::test]
async fn symbols_scout_judges_each_file_once_across_outline_pages() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{"answer":{"type":"noul","noul":0.4}},
            "usage":{"input_tokens":2,"output_tokens":1}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write(
        "src/a.rs",
        "fn a1() {}\nfn a2() {}\nfn a3() {}\nfn a4() {}\nfn a5() {}\n",
    );
    workspace.write("src/b.rs", "fn b1() {}\nfn b2() {}\n");
    let root = workspace.write("src/c.rs", "fn c1() {}\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let mut input = json!({
        "id":"outline",
        "reasoning":"Pick a file.","goal":"Which file declares the handler.",
        "resources":[{"id":"ast","context":{"tool":"astSearch","query":{
            "path":root,"operation":"symbols","pageSize":2
        }}}],
        "questions":[{"id":"rel","type":"noul","instructions":"Relevant?"}]
    });
    let mut judged: Vec<(String, usize)> = Vec::new();
    for call in 0..6 {
        let outcome = runtime
            .execute(
                format!("outline-{call}"),
                "clasify".into(),
                verbose(input.clone()),
            )
            .await
            .expect("clasify");
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let path = page["source"]["path"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let lines = page["next"]["read"]["query"]["endLine"]
                .as_u64()
                .unwrap_or(0) as usize;
            judged.push((path, lines));
        }
        match query["next"].get("clasify") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    let files = judged
        .iter()
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(files, ["src/a.rs", "src/b.rs", "src/c.rs"], "{judged:?}");
    runtime.close().await;
}

/// The unified input (flat resources, `type`+`ask` questions) and the nested
/// form send byte-identical provider requests; the default output is compact
/// and `debug:true` returns the full receipt plus provider usage.
#[tokio::test]
async fn unified_and_nested_matrices_reach_the_provider_identically() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"noul","noul":0.8},
                "answer_1":{"type":"choice","choice":"runtime","confidence":1.0,
                    "probabilities":{"runtime":1.0,"test":0.0}}
            },
            "usage":{"input_tokens":7,"output_tokens":2}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("trace.txt", "Evidence is present.\n");
    let runtime = workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ]);
    let brief = json!({"id":"m","goal":"Decide whether the trace states the fact.","reasoning":"The next read depends on it."});
    let mut nested = brief.clone();
    nested["resources"] = json!([{"id":"src","context":{"tool":"localFetch","query":{"path":file,"fullContent":true}}}]);
    nested["questions"] = json!([
        {"id":"present","questionType":"contribution","target":"the fact"},
        {"id":"kind","type":"choice","instructions":"Kind?","criteria":{"runtime":null,"test":null}}
    ]);
    let mut unified = brief.clone();
    unified["resources"] = json!([{"id":"src","tool":"localFetch","query":{"path":file}}]);
    unified["questions"] = json!([
        {"id":"present","type":"relevant","ask":"the fact"},
        {"id":"kind","type":"choice","ask":"Kind?","labels":{"runtime":null,"test":null}}
    ]);
    let old = runtime
        .execute("nested".into(), "clasify".into(), nested)
        .await
        .expect("nested form stays valid");
    let new = runtime
        .execute("unified".into(), "clasify".into(), unified.clone())
        .await
        .expect("unified form is valid");
    // The judgment cache keys on the provider state and question text, so the
    // unified matrix replays the nested one's judgment: one provider request.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(old.structured_content, new.structured_content);
    for outcome in [&old, &new] {
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("compact output contract");
        assert_eq!(
            outcome.structured_content["queries"][0]["resources"][0],
            // The host never saw the delegated read: a positive verdict
            // carries the read of exactly the judged lines.
            json!({"resourceId":"src","path":"trace.txt","totalLines":1,
                "pages":[{"lines":[1,1],"answers":{"present":0.8,"kind":"runtime"},
                    "next":{"read":{"tool":"localFetch","confidence":"exact","query":{
                        "path":"trace.txt","startLine":1,"endLine":1,
                        "goal":"Decide whether the trace states the fact.",
                        "reasoning":"The next read depends on it."}}}}]}),
            "{}",
            outcome.structured_content
        );
    }
    unified["debug"] = json!(true);
    unified["questions"][0]["ask"] = json!("the fact, judged again");
    let debug = runtime
        .execute("debug".into(), "clasify".into(), unified)
        .await
        .expect("debug is accepted");
    let query = &debug.structured_content["queries"][0];
    assert_eq!(query["usage"]["calls"], 1, "{query}");
    assert_eq!(query["usage"]["inputTokens"], 7);
    assert_eq!(query["usage"]["outputTokens"], 2);
    assert!(query["usage"]["ms"].is_u64());
    let page = &query["resources"][0]["pages"][0];
    assert_eq!(
        page["usage"],
        json!({"calls":1,"inputTokens":7,"outputTokens":2})
    );
    assert_eq!(page["answers"]["present"], json!({"noul":0.8}));
    assert_eq!(page["answers"]["kind"]["probabilities"]["runtime"], 1.0);
    runtime.close().await;
}

fn provider_runtime(
    workspace: &Workspace,
    server: &MockServer,
) -> octocode_native::runtime::ToolRuntime {
    workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ])
}

/// Provider request states, in arrival order.
async fn sent_states(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["state"].clone()
        })
        .collect()
}

/// Three files of `matches` hit lines each, every line `width` characters and
/// tagged with its file's marker.
fn write_hit_files(workspace: &Workspace, matches: usize, width: usize) -> String {
    let mut root = None;
    for (index, marker) in ["ALPHA", "BRAVO", "CHARLIE"].iter().enumerate() {
        let body = (0..matches)
            .map(|line| {
                let text = format!("needle {marker} {line} ");
                format!("{text}{}\n", filler(width - text.len()))
            })
            .collect::<String>();
        let path = workspace.write(&format!("hits/file{index}.txt"), body);
        root = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned());
    }
    root.unwrap()
}

#[tokio::test]
async fn search_candidates_above_max_chars_never_reach_the_provider() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.7))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let root = write_hit_files(&workspace, 1, 120);
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"capped","reasoning":"Bound candidate evidence.","goal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":1,"context":{"tool":"localSearch","query":{
            "path":root,"searchText":"needle","sort":"path","pageSize":3,"reasoning":"Find hits."
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("capped".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let cell = &query["resources"][0];
    assert_eq!(cell["coverage"], "error", "{cell}");
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 3, "every candidate stays visible: {cell}");
    for page in pages {
        assert_eq!(
            page["error"]["code"], "classificationContextTooLarge",
            "{page}"
        );
        assert_eq!(
            page["next"]["read"]["tool"], "localFetch",
            "an unjudged candidate keeps its read: {page}"
        );
    }
    assert!(query.get("next").is_none(), "{query}");
    assert_eq!(query["usage"]["calls"], 0, "{query}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("capped search output contract");
    runtime.close().await;
}

#[tokio::test]
async fn search_candidates_past_the_remaining_budget_resume_without_skips() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.4))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // One candidate carries 3 × 190 characters of snippets plus its row
    // fields; two never fit 1,100.
    let root = write_hit_files(&workspace, 3, 190);
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"budget","reasoning":"Bound candidate evidence.","goal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":1100,"context":{"tool":"localSearch","query":{
            "path":root,"searchText":"needle","sort":"path","pageSize":3,"reasoning":"Find hits."
        }}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 5, "the walk must terminate");
        let outcome = runtime
            .execute(
                format!("budget-{calls}"),
                "clasify".into(),
                verbose(input.clone()),
            )
            .await
            .unwrap();
        octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
            .expect("budgeted search output contract");
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"].as_array().unwrap() {
            assert!(page.get("error").is_none(), "{page}");
        }
        match query["next"].get("clasify") {
            Some(next) => {
                octocode_native::contracts::prepare_many_and_validate(
                    "clasify",
                    next.clone(),
                    octocode_native::contracts::PrepareOptions::default(),
                )
                .expect("next.clasify replays unchanged");
                input = next.clone();
            }
            None => break,
        }
    }
    let states = sent_states(&server).await;
    for marker in ["ALPHA", "BRAVO", "CHARLIE"] {
        assert_eq!(
            states
                .iter()
                .filter(|state| state.to_string().contains(marker))
                .count(),
            1,
            "{marker} must be judged exactly once: {states:#?}"
        );
    }
    assert_eq!(states.len(), 3);
    assert!(calls >= 2, "1,100 characters cannot hold two candidates");
    runtime.close().await;
}

#[tokio::test]
async fn list_items_above_max_chars_never_reach_the_provider() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.7))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("src/a.rs", "fn alpha_handler() {}\n");
    let root = workspace.write("src/b.rs", "fn bravo_handler() {}\n");
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"outline","reasoning":"Pick a file.","goal":"Which file declares the handler.",
        "resources":[{"id":"ast","maxChars":1,"context":{"tool":"astSearch","query":{
            "path":root,"operation":"symbols"
        }}}],
        "questions":[{"id":"rel","type":"noul","instructions":"Relevant?"}]
    });
    let outcome = runtime
        .execute("outline".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let cell = &outcome.structured_content["queries"][0]["resources"][0];
    let pages = cell["pages"].as_array().unwrap();
    assert_eq!(pages.len(), 2, "{cell}");
    for page in pages {
        assert_eq!(
            page["error"]["code"], "classificationContextTooLarge",
            "{page}"
        );
        assert!(page["next"]["read"].is_object(), "{page}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn hydrated_candidates_share_one_max_chars_budget() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.4))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    // Multi-byte text in three distant hit clusters per file: every window is
    // its own read, so a per-candidate split alone would overrun the budget.
    let body = |marker: &str| {
        (1..=400)
            .map(|line| {
                if [1, 200, 400].contains(&line) {
                    format!("needle {marker} ünïcödé ✓ {}\n", filler(40))
                } else {
                    format!("filler ëëëë {}\n", filler(30))
                }
            })
            .collect::<String>()
    };
    workspace.write("hyd/a.txt", body("ALPHA"));
    let root = workspace.write("hyd/b.txt", body("BRAVO"));
    let root = root.parent().unwrap().to_string_lossy().into_owned();
    let runtime = provider_runtime(&workspace, &server);
    let max_chars = 900;
    let input = json!({
        "id":"hydrated","reasoning":"Bound hydrated evidence.","goal":"Decide the next read.",
        "resources":[{"id":"hits","maxChars":max_chars,"context":{"tool":"localSearch","query":{
            "path":root,"searchText":"needle","sort":"path","reasoning":"Find hits."
        },"candidateEvidence":"fileChunks"}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Relevant?"}]
    });
    runtime
        .execute("hydrated".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    fn content_chars(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Array(items) => items.iter().map(content_chars).sum(),
            serde_json::Value::Object(fields) => fields
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map_or_else(
                    || fields.values().map(content_chars).sum(),
                    |content| content.chars().count(),
                ),
            _ => 0,
        }
    }
    let states = sent_states(&server).await;
    assert!(!states.is_empty());
    let total = states.iter().map(content_chars).sum::<usize>();
    assert!(
        total <= max_chars,
        "hydrated evidence {total} exceeds maxChars {max_chars}: {states:#?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn sufficient_unread_file_evidence_returns_a_bounded_verification_read() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.95))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "policy.rs",
        "// header\nconst WIDE_RESULT_FILES: usize = 8;\nconst OTHER: usize = 3;\n// tail\n",
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"threshold","reasoning":"Answer from the deciding source.","goal":"Find the handoff threshold.",
        "resources":[{"id":"threshold","tool":"localFetch","query":{"path":file,"startLine":2,"endLine":3}}],
        "questions":[{"id":"sufficient","type":"sufficient","ask":"How many files trigger the handoff?"}]
    });
    let outcome = runtime
        .execute("threshold".into(), "clasify".into(), input)
        .await
        .unwrap();
    let resource = &outcome.structured_content["queries"][0]["resources"][0];
    let page = &resource["pages"][0];
    assert_eq!(page["answers"]["sufficient"], 0.95, "{resource}");
    let read = &page["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{resource}");
    assert_eq!(read["query"]["path"], "policy.rs", "{read}");
    assert_eq!(read["query"]["startLine"], 2, "{read}");
    assert_eq!(read["query"]["endLine"], 3, "{read}");
    octocode_native::contracts::validate_output("clasify", &outcome.structured_content)
        .expect("verification read output contract");

    // Supplied evidence is already held: no read is invented for it.
    let held = json!({
        "id":"held","reasoning":"Judge held evidence.","goal":"Find the handoff threshold.",
        "resources":[{"id":"held","value":"const WIDE_RESULT_FILES: usize = 8;"}],
        "questions":[{"id":"sufficient","type":"sufficient","ask":"How many files trigger the handoff?"}]
    });
    let outcome = runtime
        .execute("held".into(), "clasify".into(), held)
        .await
        .unwrap();
    assert!(
        !outcome.structured_content.to_string().contains("\"read\""),
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn confident_negative_file_pages_add_no_read() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.05))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("other.rs", "const OTHER: usize = 3;\n");
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"negative","reasoning":"Answer from the deciding source.","goal":"Find the handoff threshold.",
        "resources":[{"id":"other","tool":"localFetch","query":{"path":file}}],
        "questions":[{"id":"sufficient","type":"sufficient","ask":"How many files trigger the handoff?"}]
    });
    let outcome = runtime
        .execute("negative".into(), "clasify".into(), input)
        .await
        .unwrap();
    assert!(
        !outcome.structured_content.to_string().contains("\"read\""),
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
}

/// Lines 1–8 are short, 9–16 long enough that their 8-line page exceeds the
/// whole budget while half of it fits, then short lines resume.
fn uneven_lines() -> String {
    (1..=24)
        .map(|line| {
            if (9..=16).contains(&line) {
                format!("L{line:02} {}\n", filler(86))
            } else {
                format!("L{line:02} short\n")
            }
        })
        .collect()
}

#[tokio::test]
async fn an_oversized_next_page_shrinks_and_the_replay_advances() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("uneven.txt", uneven_lines());
    let runtime = provider_runtime(&workspace, &server);
    let mut input = json!({
        "id":"walk","reasoning":"Walk the file in bounded pages.","goal":"Decide the next read.",
        "debug":true,
        "resources":[{"id":"doc","maxChars":400,"tool":"localFetch","query":{
            "path":file,"chunkType":"lines","chunkSize":8
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let mut covered = 0u64;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 10, "the walk must terminate");
        let outcome = runtime
            .execute(format!("walk-{calls}"), "clasify".into(), input.clone())
            .await
            .unwrap();
        let query = &outcome.structured_content["queries"][0];
        for page in query["resources"][0]["pages"].as_array().unwrap() {
            assert!(page.get("error").is_none(), "replay must advance: {query}");
            let scope = &page["scope"];
            assert_eq!(
                scope["startLine"].as_u64().unwrap(),
                covered + 1,
                "no interval is skipped or repeated: {query}"
            );
            covered = scope["endLine"].as_u64().unwrap();
        }
        match query["next"].get("clasify") {
            Some(next) => {
                assert_eq!(next["debug"], true, "{next}");
                input = next.clone();
            }
            None => break,
        }
    }
    assert_eq!(covered, 24);
    for state in sent_states(&server).await {
        assert!(
            state.to_string().chars().count() < 2_000,
            "every judged page stayed bounded: {state}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_line_larger_than_the_whole_budget_is_terminal_not_a_repeating_continuation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write(
        "wide.txt",
        format!("short one\n{}\nshort three\n", filler(500)),
    );
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"wide","reasoning":"Walk the file in bounded pages.","goal":"Decide the next read.",
        "resources":[{"id":"doc","maxChars":100,"tool":"localFetch","query":{
            "path":file,"chunkType":"lines","chunkSize":1
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let outcome = runtime
        .execute("wide".into(), "clasify".into(), verbose(input))
        .await
        .unwrap();
    let query = &outcome.structured_content["queries"][0];
    let pages = query["resources"][0]["pages"].as_array().unwrap();
    assert!(pages[0].get("answers").is_some(), "{query}");
    let failed = pages.last().unwrap();
    assert_eq!(
        failed["error"]["code"], "classificationContextTooLarge",
        "{query}"
    );
    assert!(
        query.get("next").is_none(),
        "no continuation may replay the same oversized page: {query}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_changed_source_is_rejected_on_replay_instead_of_mixing_versions() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(noul_response(0.2))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("doc.txt", uneven_lines());
    let runtime = provider_runtime(&workspace, &server);
    let input = json!({
        "id":"versions","reasoning":"Walk the file in bounded pages.","goal":"Decide the next read.",
        "resources":[{"id":"doc","maxChars":200,"tool":"localFetch","query":{
            "path":file,"chunkType":"lines","chunkSize":8
        }}],
        "questions":[{"id":"q","type":"yesno","ask":"Is the marker stated?"}]
    });
    let first = runtime
        .execute("versions-1".into(), "clasify".into(), input)
        .await
        .unwrap();
    let next = first.structured_content["queries"][0]["next"]["clasify"].clone();
    assert!(next.is_object(), "{}", first.structured_content);
    let before = server.received_requests().await.unwrap().len();
    workspace.write("doc.txt", format!("changed\n{}", uneven_lines()));
    let replay = runtime
        .execute("versions-2".into(), "clasify".into(), verbose(next))
        .await
        .unwrap();
    let query = &replay.structured_content["queries"][0];
    let page = &query["resources"][0]["pages"][0];
    assert_eq!(page["error"]["code"], "staleSnapshot", "{query}");
    assert!(page.get("answers").is_none(), "{query}");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        before,
        "no page of the new version is judged"
    );
    runtime.close().await;
}
