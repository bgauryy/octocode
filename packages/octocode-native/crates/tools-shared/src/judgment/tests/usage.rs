//! Usage accounting across split, failed, concurrent and replayed
//! evaluations.
use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn failed_split_evaluation_counts_all_completed_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let body = request.body_json::<Value>().unwrap();
            if body["questions"].as_object().unwrap().len() > 1 {
                super::media::window_exceeded()
            } else if body["questions"]["answer"]["instructions"] == "fail" {
                ResponseTemplate::new(401)
            } else {
                response(request)
            }
        })
        .expect(4)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let questions = [
        json_fixture(r#"{"type": "noul", "instructions": "fail"}"#),
        json_fixture(r#"{"type": "noul", "instructions": "a"}"#),
        json_fixture(r#"{"type": "noul", "instructions": "b"}"#),
    ];
    let error = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &json!("x".repeat(80 * 1024)),
                questions: &questions,
                assets: None,
                long_context: crate::judgment::LongContext::Never,
                companions: 0,
                cap: None,
            },
            None,
            &evaluation_budget(&evaluator),
            &evaluator.gate(),
        )
        .await
        .unwrap_err();
    // The refused group plus one request per question.
    assert_eq!(error.provider_calls, 4);
    assert_eq!(
        (error.usage().input_tokens, error.usage().output_tokens),
        (20, 4)
    );
    let mut usage = stats::ClassificationUsage::default();
    usage.add(&Err(error));
    assert_eq!(usage.calls, 4);
    assert_eq!(usage.known_usage_calls, 2);
    assert_eq!(usage.unknown_usage_calls, 2);
    assert_eq!(usage.input_tokens, 20);
    assert_eq!(usage.output_tokens, 4);
}

#[tokio::test]
async fn concurrent_failed_evaluations_count_each_independent_attempt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(30))
                .set_body_json(json_fixture(
                    r#"{
                        "model": "vision",
                        "usage": {"input_tokens": 7, "output_tokens": 2},
                        "answers": {
                            "answer": {
                                "type": "box",
                                "box": {"top": 1.0, "bottom": 0.0, "left": 0.0, "right": 1.0},
                                "confidence": null
                            }
                        }
                    }"#,
                )),
        )
        .expect(2)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let gate = evaluator.gate();
    let state = json!("opaque");
    let questions = [json_fixture(r#"{"type": "box"}"#)];
    let assets = json_fixture(
        r#"{
            "opaque": {"type": "image", "format": "png", "data_base64": "bytes"}
        }"#,
    );
    let input = EvaluationInput {
        state: &state,
        questions: &questions,
        assets: Some(&assets),
        long_context: crate::judgment::LongContext::Never,
        companions: 0,
        cap: None,
    };
    let (first, second) = tokio::join!(
        evaluator.evaluate_prepared(input, None, &budget, &gate),
        evaluator.evaluate_prepared(input, None, &budget, &gate),
    );
    let errors = [first.unwrap_err(), second.unwrap_err()];
    let mut usage = stats::ClassificationUsage::default();
    for error in errors {
        assert_eq!(error.provider_calls, 1);
        assert_eq!(error.usage().known_usage_calls, 1);
        usage.add(&Err(error));
    }
    assert_eq!(usage.calls, 2);
    assert_eq!(usage.known_usage_calls, 2);
    assert_eq!(usage.input_tokens, 14);
    assert_eq!(usage.output_tokens, 4);
}

#[test]
fn per_call_usage_counts_attempts_and_ignores_results_without_requests() {
    let mut usage = stats::ClassificationUsage::default();
    let mut judged = Judged {
        answers: vec![],
        model: "resolved".into(),
        input_tokens: 10,
        output_tokens: 2,
        provider_calls: 3,
        known_usage_calls: 1,
        long_context_retry: None,
    };
    usage.add(&Ok(judged.clone()));
    judged.provider_calls = 0;
    judged.known_usage_calls = 0;
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
        json_fixture(
            r#"{
                "calls": 4,
                "knownUsageCalls": 1,
                "unknownUsageCalls": 3,
                "inputTokens": 10,
                "outputTokens": 2,
                "models": ["resolved"]
            }"#
        )
    );
}

/// The long-context notice counts only the retried evaluations and their
/// tokens, whatever the default window billed beside them; a retry whose
/// provider reported no usage leaves its tokens unknown, never zero.
#[test]
fn the_long_context_notice_counts_only_retries_and_never_reports_unknown_usage_as_zero() {
    let judged = |input_tokens: u64, known_usage_calls: u64, long_context_retry: bool| Judged {
        answers: vec![],
        model: "resolved".into(),
        input_tokens,
        output_tokens: 1,
        provider_calls: 1,
        known_usage_calls,
        long_context_retry: long_context_retry.then_some(LongContextRetry {
            unreported_calls: 1 - known_usage_calls,
            refused_calls: 1,
        }),
    };
    let notice = |evaluations: &[Judged]| {
        let mut usage = stats::ClassificationUsage::default();
        for judged in evaluations {
            usage.add(&Ok(judged.clone()));
        }
        usage.long_context_notice()
    };
    assert_eq!(
        notice(&[judged(700, 1, false)]),
        None,
        "no retry, no notice"
    );
    let mixed = notice(&[
        judged(700, 1, false),
        judged(40, 1, true),
        judged(2, 1, true),
    ])
    .expect("retries are disclosed");
    assert!(
        mixed.starts_with("2 evaluations refused") && mixed.contains(": 42 input tokens billed."),
        "only the retried evaluations and their tokens: {mixed}"
    );
    let unreported = notice(&[judged(0, 0, true)]).expect("a retry is disclosed");
    assert!(
        unreported.contains("input tokens billed unknown") && !unreported.contains("0 input"),
        "{unreported}"
    );
    let partly = notice(&[judged(40, 1, true), judged(0, 0, true)]).expect("retries");
    assert!(
        partly.contains("at least 40 input tokens billed; 1 of them reported no usage"),
        "{partly}"
    );
}

/// A replayed page still says its judgments were retried in the long-context
/// window, but never that the replaying call billed the tokens: the original
/// read did.
#[test]
fn a_replayed_long_context_notice_attributes_the_billing_to_the_original_read() {
    for (known_usage_calls, billed) in [(1, "42 input tokens billed"), (0, "billed unknown")] {
        let mut usage = stats::ClassificationUsage::default();
        usage.add(&Ok(Judged {
            answers: vec![],
            model: "resolved".into(),
            input_tokens: 42,
            output_tokens: 1,
            provider_calls: 1,
            known_usage_calls,
            long_context_retry: Some(LongContextRetry {
                unreported_calls: 1 - known_usage_calls,
                refused_calls: 1,
            }),
        }));
        let original = usage.long_context_notice().expect("original notice");
        assert!(original.contains(billed) && !original.contains("original read"));
    }
}
