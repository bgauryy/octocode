//! Request-owned dedupe: identity from the provider request payload, one
//! request for identical evaluations, spend counted once, nothing kept past
//! the request.
use super::*;
use crate::tools::judgment::shared::Evaluations;

fn question() -> Value {
    json!({"type":"noul","instructions":"Evidence?"})
}

fn input<'a>(state: &'a Value, questions: &'a [Value]) -> EvaluationInput<'a> {
    EvaluationInput {
        state,
        questions,
        assets: None,
        long_context: false,
    }
}

async fn server(expected: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| response(request))
        .expect(expected)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn identical_evaluations_share_one_request_and_count_spend_once() {
    let server = server(1).await;
    let evaluator = evaluator(&server);
    let (budget, gate) = (evaluation_budget(&evaluator), evaluator.gate());
    let evaluations = Evaluations::default();
    let (state, questions) = (json!({"text":"x"}), [question()]);
    let asks = futures_util::future::join_all((0..3).map(|_| {
        evaluator.evaluate_once(
            &evaluations,
            input(&state, &questions),
            None,
            &budget,
            &gate,
        )
    }))
    .await;
    let mut usage = stats::ClassificationUsage::default();
    for ask in &asks {
        let judged = ask.as_ref().unwrap();
        assert_eq!(judged.answers, asks[0].as_ref().unwrap().answers);
        // Every asker reports the evaluation's tokens.
        assert_eq!((judged.input_tokens, judged.output_tokens), (10, 2));
        usage.add(ask);
    }
    assert_eq!(usage.calls, 1);
    assert_eq!(usage.known_usage_calls, 1);
    assert_eq!((usage.input_tokens, usage.output_tokens), (10, 2));
}

#[tokio::test]
async fn a_shared_failure_reaches_every_asker_and_counts_its_attempt_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let (budget, gate) = (evaluation_budget(&evaluator), evaluator.gate());
    let evaluations = Evaluations::default();
    let (state, questions) = (json!("x"), [question()]);
    let (first, second) = tokio::join!(
        evaluator.evaluate_once(
            &evaluations,
            input(&state, &questions),
            None,
            &budget,
            &gate
        ),
        evaluator.evaluate_once(
            &evaluations,
            input(&state, &questions),
            None,
            &budget,
            &gate
        ),
    );
    let (first, second) = (first.unwrap_err(), second.unwrap_err());
    assert_eq!(first.code, second.code);
    let mut usage = stats::ClassificationUsage::default();
    usage.add(&Err(first));
    usage.add(&Err(second));
    assert_eq!(usage.calls, 1);
}

#[tokio::test]
async fn a_different_payload_is_a_different_evaluation() {
    let server = server(6).await;
    let evaluator = evaluator(&server);
    let (budget, gate) = (evaluation_budget(&evaluator), evaluator.gate());
    let evaluations = Evaluations::default();
    let state = json!("x");
    let questions = [question()];
    let other_question = [json!({"type":"noul","instructions":"Other?"})];
    let two_questions = [question(), question()];
    let other_state = json!("y");
    let long = EvaluationInput {
        long_context: true,
        ..input(&state, &questions)
    };
    let asks = [
        (input(&state, &questions), None),
        (input(&state, &other_question), None),
        (input(&state, &two_questions), None),
        (input(&other_state, &questions), None),
        (input(&state, &questions), Some("jev-other")),
        (long, None),
    ];
    for (ask, model) in asks {
        evaluator
            .evaluate_once(&evaluations, ask, model, &budget, &gate)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn a_later_request_reaches_the_provider_again() {
    let server = server(2).await;
    let evaluator = evaluator(&server);
    let (budget, gate) = (evaluation_budget(&evaluator), evaluator.gate());
    let (state, questions) = (json!("x"), [question()]);
    for _ in 0..2 {
        let judged = evaluator
            .evaluate_once(
                &Evaluations::default(),
                input(&state, &questions),
                None,
                &budget,
                &gate,
            )
            .await
            .unwrap();
        assert_eq!(judged.provider_calls, 1);
    }
}
