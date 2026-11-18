//! Request-owned dedupe: identity from the provider request payload, one
//! request for identical evaluations, spend counted once, nothing kept past
//! the request.
use super::*;
use crate::judgment::shared::Evaluations;
use octocode_test_support::json_fixture;

fn question() -> Value {
    json_fixture(r#"{"type": "noul", "instructions": "Evidence?"}"#)
}

fn input<'a>(state: &'a Value, questions: &'a [Value]) -> EvaluationInput<'a> {
    EvaluationInput {
        state,
        questions,
        assets: None,
        long_context: crate::judgment::LongContext::Never,
        companions: 0,
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
    let (state, questions) = (json_fixture(r#"{"text": "x"}"#), [question()]);
    let asks = futures_util::future::join_all((0..3).map(|_| {
        evaluator.evaluate_once(
            &evaluations,
            input(&state, &questions),
            None,
            &budget,
            &gate,
        )
    }))
    .await
    .into_iter()
    .map(|(ask, _)| ask)
    .collect::<Vec<_>>();
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
    let (first, second) = (first.0.unwrap_err(), second.0.unwrap_err());
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
    let other_question = [json_fixture(
        r#"{"type": "noul", "instructions": "Other?"}"#,
    )];
    let two_questions = [question(), question()];
    let other_state = json!("y");
    let long = EvaluationInput {
        long_context: crate::judgment::LongContext::Always,
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
            .0
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
            .0
            .unwrap();
        assert_eq!(judged.provider_calls, 1);
    }
}

/// The gate remembers a refusal per endpoint and credential digest: a 401
/// for key A refuses key A within the memo, never key B of the same account
/// holder who fixed the key, nor a quota (402) of another account.
#[tokio::test]
async fn a_rejected_key_is_refused_without_refusing_another_key_of_the_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::header("authorization", "Bearer key-a"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| response(request))
        .mount(&server)
        .await;
    let ask = |evaluator: Evaluator| {
        let (state, questions) = (json!("x"), [question()]);
        async move {
            let (budget, gate) = (evaluation_budget(&evaluator), evaluator.gate());
            evaluator
                .evaluate_prepared(input(&state, &questions), None, &budget, &gate)
                .await
        }
    };
    let (a, b) = (
        evaluator_with_key(&server, "key-a"),
        evaluator_with_key(&server, "key-b"),
    );
    assert_ne!(a.partition().identity(), b.partition().identity());
    let first = ask(a).await.unwrap_err();
    assert_eq!(first.provider_calls, 1);
    // Key A stays refused within the window, without a request.
    let again = ask(evaluator_with_key(&server, "key-a")).await.unwrap_err();
    assert_eq!(again.provider_calls, 0);
    // Key B is another credential: it reaches the provider and is answered.
    ask(b).await.expect("another key is not refused");
}
