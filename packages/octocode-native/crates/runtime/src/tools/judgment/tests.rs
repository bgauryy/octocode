use super::*;
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

fn evaluator(server: &MockServer) -> Evaluator {
    Evaluator::new(
        Some(SecretString::from("test-key".to_owned())),
        server.uri(),
        "jev-test".into(),
        &crate::providers::classification::jev::JEV,
        Duration::from_secs(10),
        1,
        4,
        None,
        None,
    )
}

fn evaluation_budget(evaluator: &Evaluator) -> RequestBudget {
    evaluator.budget(
        Instant::now() + Duration::from_secs(10),
        tokio_util::sync::CancellationToken::new(),
    )
}

fn response(request: &Request) -> ResponseTemplate {
    let body: Value = request.body_json().unwrap();
    let answers: serde_json::Map<String, Value> = body["questions"].as_object().unwrap().iter().map(|(id, question)| {
        let answer = match question["type"].as_str().unwrap() {
            "box" => json!({"type":"box","box":{"top":0.1,"bottom":0.8,"left":0.2,"right":0.7},"confidence":null}),
            _ => json!({"type":"noul","noul":0.8}),
        };
        (id.clone(), answer)
    }).collect();
    ResponseTemplate::new(200).set_body_json(json!({"model":"jev-resolved","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
}

#[tokio::test]
async fn prepared_media_batches_headers_and_cache_identity_without_exposing_assets() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(response)
        .expect(3)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let gate = evaluator.gate();
    let state = json!({"image":"opaque"});
    let questions = [
        json!({"type":"box","instructions":"Locate rectangle"}),
        json!({"type":"noul","instructions":null}),
    ];
    let assets =
        json!({"opaque":{"type":"image","format":"png","data_base64":"first-private-media"}});
    let changed =
        json!({"opaque":{"type":"image","format":"png","data_base64":"changed-private-media"}});
    let input = EvaluationInput {
        state: &state,
        questions: &questions,
        assets: Some(&assets),
        long_context: false,
    };
    let (first, concurrent) = tokio::join!(
        evaluator.evaluate_prepared(input, None, &budget, &gate),
        evaluator.evaluate_prepared(input, None, &budget, &gate),
    );
    let first = first.unwrap();
    let concurrent = concurrent.unwrap();
    assert_eq!(first.provider_calls + concurrent.provider_calls, 1);
    assert_ne!(first.replayed, concurrent.replayed);
    assert_eq!(first.model, "jev-resolved");
    assert_eq!(first.answers[0]["confidence"], Value::Null);
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("private-media")
    );
    let modified = evaluator
        .evaluate_prepared(
            EvaluationInput {
                assets: Some(&changed),
                ..input
            },
            None,
            &budget,
            &gate,
        )
        .await
        .unwrap();
    assert_eq!(modified.provider_calls, 1);
    let extended = evaluator
        .evaluate_prepared(
            EvaluationInput {
                long_context: true,
                ..input
            },
            None,
            &budget,
            &gate,
        )
        .await
        .unwrap();
    assert_eq!(extended.provider_calls, 1);
    let requests = server.received_requests().await.unwrap();
    for request in &requests {
        assert_eq!(request.headers["typesafe-danger-zone-images"], "true");
        assert_eq!(request.headers["authorization"], "Bearer test-key");
    }
    assert!(
        !requests[0]
            .headers
            .contains_key("typesafe-danger-zone-long-context")
    );
    assert_eq!(
        requests[2].headers["typesafe-danger-zone-long-context"],
        "true"
    );
    assert_eq!(requests[0].body_json::<Value>().unwrap()["assets"], assets);
    assert_eq!(requests[1].body_json::<Value>().unwrap()["assets"], changed);
}

#[tokio::test]
async fn models_use_authenticated_get_retry_and_validate_the_catalog() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let catalog = json!({"models":[{"name":"jev-preview","description":"Preview","release_date":"2026-09-10T18:39:06.057655+00:00"}]});
    let payload = catalog.clone();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(move |_: &Request| {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(500)
            } else {
                ResponseTemplate::new(200).set_body_json(payload.clone())
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let mut evaluator = evaluator(&server);
    evaluator.stats_home = Some(home.path().to_owned());
    assert_eq!(
        evaluator
            .models(&evaluation_budget(&evaluator), &evaluator.gate())
            .await
            .unwrap(),
        catalog
    );
    let recorded: Value =
        serde_json::from_slice(&std::fs::read(home.path().join("stats.json")).unwrap()).unwrap();
    assert_eq!(recorded["stats"]["clasify"]["discovery_calls"], 2);
    assert_eq!(recorded["stats"]["clasify"]["calls"], 0);
    assert_eq!(recorded["stats"]["clasify"]["unknown_usage_calls"], 0);
    for request in server.received_requests().await.unwrap() {
        assert!(request.body.is_empty());
        assert_eq!(request.headers["authorization"], "Bearer test-key");
        assert!(!request.headers.contains_key("typesafe-danger-zone-images"));
    }
}

#[tokio::test]
async fn split_media_requests_keep_only_assets_referenced_by_each_question() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &Request| {
            let body = request.body_json::<Value>().unwrap();
            assert_eq!(body["questions"].as_object().unwrap().len(), 1);
            let question = &body["questions"]["answer"];
            let mut expected = serde_json::Map::new();
            if body["state"]["shared"] == "image-shared" {
                expected.insert(
                    "image-shared".into(),
                    json!({"type":"image","format":"png","data_base64":"shared-bytes"}),
                );
            }
            match question["instructions"]["kind"].as_str().unwrap() {
                "instruction" => {
                    expected.insert(
                        "image-instruction".into(),
                        json!({"type":"image","format":"png","data_base64":"instruction-bytes"}),
                    );
                }
                "criterion" => {
                    expected.insert(
                        "image-criterion".into(),
                        json!({"type":"image","format":"png","data_base64":"criterion-bytes"}),
                    );
                }
                "text" => {}
                other => panic!("Unexpected question: {other}"),
            }
            let expected_assets = (!expected.is_empty()).then_some(Value::Object(expected));
            if body.get("assets") != expected_assets.as_ref() {
                return ResponseTemplate::new(400).set_body_json(
                    json!({"error":"Every asset must be referenced in this request"}),
                );
            }
            assert_eq!(
                request.headers.contains_key("typesafe-danger-zone-images"),
                expected_assets.is_some()
            );
            response(request)
        })
        .expect(6)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let padding = "x".repeat(45 * 1024);
    let questions = [
        json!({"type":"noul","instructions":{"kind":"instruction","target":"Inspect image-instruction here.","padding":padding}}),
        json!({"type":"noul","instructions":{"kind":"criterion"},"criteria":{"true":{"examples":["image-criterion"],"padding":padding}}}),
        json!({"type":"noul","instructions":{"kind":"text","padding":padding}}),
    ];
    for shared in [false, true] {
        let state = if shared {
            json!({"shared":"image-shared"})
        } else {
            json!("No shared image")
        };
        let mut assets = json!({
            "image-instruction":{"type":"image","format":"png","data_base64":"instruction-bytes"},
            "image-criterion":{"type":"image","format":"png","data_base64":"criterion-bytes"}
        });
        if shared {
            assets["image-shared"] =
                json!({"type":"image","format":"png","data_base64":"shared-bytes"});
        }
        let judged = evaluator
            .evaluate_prepared(
                EvaluationInput {
                    state: &state,
                    questions: &questions,
                    assets: Some(&assets),
                    long_context: false,
                },
                None,
                &evaluation_budget(&evaluator),
                &evaluator.gate(),
            )
            .await
            .expect("split requests must not send assets used only by sibling questions");
        assert_eq!(judged.provider_calls, 3);
        assert_eq!(judged.answers.len(), 3);
        assert_eq!((judged.input_tokens, judged.output_tokens), (30, 6));
    }
}

#[tokio::test]
async fn failed_split_evaluation_counts_all_completed_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            if request.body_json::<Value>().unwrap()["questions"]["answer"]["instructions"]
                == "fail"
            {
                ResponseTemplate::new(401)
            } else {
                response(request)
            }
        })
        .expect(3)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let questions = [
        json!({"type":"noul","instructions":"fail"}),
        json!({"type":"noul","instructions":"a"}),
        json!({"type":"noul","instructions":"b"}),
    ];
    let error = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &json!("x".repeat(80 * 1024)),
                questions: &questions,
                assets: None,
                long_context: false,
            },
            None,
            &evaluation_budget(&evaluator),
            &evaluator.gate(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.provider_calls, 3);
    assert_eq!(
        (error.usage().input_tokens, error.usage().output_tokens),
        (20, 4)
    );
    let mut usage = stats::ClassificationUsage::default();
    usage.add(&Err(error));
    assert_eq!(usage.calls, 3);
    assert_eq!(usage.known_usage_calls, 2);
    assert_eq!(usage.unknown_usage_calls, 1);
    assert_eq!(usage.input_tokens, 20);
    assert_eq!(usage.output_tokens, 4);
}

#[tokio::test]
async fn invalid_box_is_not_cached_and_retries_preserve_media_payload() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    Mock::given(method("POST")).respond_with(move |_: &Request| {
        let number = count.fetch_add(1, Ordering::SeqCst);
        if number == 0 { return ResponseTemplate::new(500); }
        ResponseTemplate::new(200).set_body_json(json!({"model":"vision","usage":{"input_tokens":5,"output_tokens":2},"answers":{"answer":{"type":"box","box":{"top":0.9,"bottom":0.1,"left":0.0,"right":1.0},"confidence":null}}}))
    }).expect(3).mount(&server).await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let gate = evaluator.gate();
    let state = json!("opaque");
    let questions = [json!({"type":"box"})];
    let assets = json!({"opaque":{"type":"image","format":"png","data_base64":"opaque-bytes"}});
    let input = EvaluationInput {
        state: &state,
        questions: &questions,
        assets: Some(&assets),
        long_context: true,
    };
    for calls in [2, 1] {
        let error = evaluator
            .evaluate_prepared(input, None, &budget, &gate)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidClassificationResponse);
        assert_eq!(error.provider_calls, calls);
        assert_eq!(
            (error.usage().input_tokens, error.usage().output_tokens),
            (5, 2)
        );
        assert_eq!(error.usage().known_usage_calls, 1);
    }
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| request.body == requests[0].body)
    );
    assert!(
        requests.iter().all(
            |request| request.headers["typesafe-danger-zone-images"] == "true"
                && request.headers["typesafe-danger-zone-long-context"] == "true"
        )
    );
}

#[tokio::test]
async fn concurrent_failed_evaluations_share_attempts_without_double_counting_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(30)).set_body_json(json!({
            "model":"vision","usage":{"input_tokens":7,"output_tokens":2},
            "answers":{"answer":{"type":"box","box":{"top":1.0,"bottom":0.0,"left":0.0,"right":1.0},"confidence":null}}
        })))
        .expect(1).mount(&server).await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let gate = evaluator.gate();
    let state = json!("opaque");
    let questions = [json!({"type":"box"})];
    let assets = json!({"opaque":{"type":"image","format":"png","data_base64":"bytes"}});
    let input = EvaluationInput {
        state: &state,
        questions: &questions,
        assets: Some(&assets),
        long_context: false,
    };
    let (first, second) = tokio::join!(
        evaluator.evaluate_prepared(input, None, &budget, &gate),
        evaluator.evaluate_prepared(input, None, &budget, &gate),
    );
    let errors = [first.unwrap_err(), second.unwrap_err()];
    assert_eq!(
        errors.iter().map(|error| error.provider_calls).sum::<u64>(),
        1
    );
    assert_eq!(
        errors
            .iter()
            .map(|error| error.usage().known_usage_calls)
            .sum::<u64>(),
        1
    );
    assert_eq!(
        errors
            .iter()
            .map(|error| error.usage().input_tokens)
            .sum::<u64>(),
        7
    );
    assert_eq!(
        errors
            .iter()
            .map(|error| error.usage().output_tokens)
            .sum::<u64>(),
        2
    );
}

#[test]
fn selection_preserves_sibling_decisions_and_accounts_for_every_request() {
    use crate::tools::selection::{self, Candidate};
    let executor = tokio::runtime::Runtime::new().unwrap();
    let server = executor.block_on(MockServer::start());
    executor.block_on(async {
        Mock::given(method("POST")).respond_with(|request: &Request| {
            let request: Value = request.body_json().unwrap();
            if request["state"]["text"] == "fail" {
                return ResponseTemplate::new(401);
            }
            let criteria = request["questions"]["answer"]["criteria"].as_array().unwrap();
            let legend: serde_json::Map<String,Value> = criteria.iter().enumerate().map(|(index, value)| (index.to_string(), value.clone())).collect();
            let probabilities: serde_json::Map<String,Value> = criteria.iter().enumerate().map(|(index, _)| (index.to_string(),json!(if index == if request["state"]["text"] == "second" { 2 } else { 0 } { 1.0 } else { 0.0 }))).collect();
            ResponseTemplate::new(200).set_delay(Duration::from_millis(60)).set_body_json(json!({
                "model":"resolved","usage":{"input_tokens":10,"output_tokens":2},
                "answers":{"answer":{"type":"score","score":if request["state"]["text"] == "second" {2.0} else {0.0},"confidence":0.7,"legend":legend,"probabilities":probabilities}}
            }))
        }).expect(3).mount(&server).await;
    });
    let home = tempfile::tempdir().unwrap();
    let mut evaluator = evaluator(&server);
    evaluator.stats_home = Some(home.path().to_owned());
    let mut runtime = crate::tools::test_support::selection_fake::runtime(
        evaluator,
        executor.handle().clone(),
        Duration::from_secs(10),
    );
    runtime.limits.max_candidates = 10;
    runtime.limits.max_chars = 32_000;
    let candidates = ["fail", "second", "third"]
        .into_iter()
        .map(|text| Candidate::new(Some(json!({"text":text}))))
        .collect::<Vec<_>>();
    let outcome = selection::select(
        Some(&runtime),
        &json!({"clasifyContext":{"classifiers":["Does this contain relevant evidence?"]}}),
        "test-source",
        None,
        &candidates,
        None,
        octocode_contract_types::SelectionUnit::Files,
    )
    .map_err(|_| "ranking order failed")
    .unwrap();
    assert_eq!(outcome.selected, vec![1]);
    assert_eq!(outcome.receipt.excluded_items, 1);
    assert_eq!(outcome.receipt.unknown_items, 1);
    assert_eq!(
        outcome.receipt.status,
        octocode_contract_types::SelectionStatus::Partial
    );
    assert!(outcome.receipt.reason.is_some());
    let recorded: Value =
        serde_json::from_slice(&std::fs::read(home.path().join("stats.json")).unwrap()).unwrap();
    assert_eq!(
        recorded["stats"]["clasify"],
        json!({"calls":3,"known_usage_calls":2,"unknown_usage_calls":1,"input_tokens":20,"output_tokens":4})
    );
    assert_eq!(
        executor.block_on(server.received_requests()).unwrap().len(),
        3
    );
}

#[test]
fn per_call_usage_excludes_replayed_tokens_and_retains_unknown_attempts() {
    let mut usage = stats::ClassificationUsage::default();
    let mut judged = Judged {
        answers: vec![],
        model: "resolved".into(),
        input_tokens: 10,
        output_tokens: 2,
        provider_calls: 3,
        known_usage_calls: 1,
        replayed: false,
    };
    usage.add(&Ok(judged.clone()));
    judged.provider_calls = 0;
    judged.known_usage_calls = 0;
    judged.replayed = true;
    usage.add(&Ok(judged));
    let error = ClassificationError {
        provider_calls: 1,
        ..ClassificationError::new(
            ErrorCode::ClassificationProviderError,
            "401",
            "Check credentials.",
        )
    };
    usage.add(&Err(error));
    assert_eq!(
        serde_json::to_value(usage.output()).unwrap(),
        json!({
            "calls":4,"knownUsageCalls":1,"unknownUsageCalls":3,
            "inputTokens":10,"outputTokens":2,"replayedEvaluations":1,"models":["resolved"]
        })
    );
}

#[tokio::test]
async fn noncanonical_provider_answers_are_not_cached() {
    for grouped in [false, true] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(|request: &Request| {
                let body: Value = request.body_json().unwrap();
                let answers: serde_json::Map<String, Value> = body["questions"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(|id| {
                        (
                            id.clone(),
                            json!({"type":"noul","noul":0.9,"explanation":"extension"}),
                        )
                    })
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({
                    "model":"jev-test","answers":answers,
                    "usage":{"input_tokens":10,"output_tokens":2}
                }))
            })
            .expect(2)
            .mount(&server)
            .await;
        let evaluator = evaluator(&server);
        let budget = evaluation_budget(&evaluator);
        let gate = evaluator.gate();
        let state = json!("source");
        let questions =
            vec![json!({"type":"noul","instructions":"Evidence?"}); if grouped { 2 } else { 1 }];
        let input = EvaluationInput {
            state: &state,
            questions: &questions,
            assets: None,
            long_context: false,
        };
        for _ in 0..2 {
            let error = evaluator
                .evaluate_prepared(input, None, &budget, &gate)
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidClassificationResponse);
            assert_eq!(error.provider_calls, 1);
            assert_eq!(error.usage().known_usage_calls, 1);
        }
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}
