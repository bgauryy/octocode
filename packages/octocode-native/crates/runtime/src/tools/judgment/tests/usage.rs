//! Usage accounting across split, failed, concurrent and replayed
//! evaluations.
use super::*;

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
