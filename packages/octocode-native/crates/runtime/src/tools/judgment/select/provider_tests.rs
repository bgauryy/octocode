//! The evaluator as a selection judge against a real provider endpoint:
//! request shape, per-candidate failures and typed whole-page errors.
use super::*;
use crate::execution::{ExecutionContext, SelectionLimits};
use crate::tools::selection::Candidate;
use std::time::Duration;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// A retry-free evaluator, so each candidate is one provider request.
fn evaluator(key: &str, base_url: String) -> Evaluator {
    Evaluator::new(
        Some(secrecy::SecretString::from(key.to_owned())),
        base_url,
        "jev-test".into(),
        &crate::providers::classification::jev::JEV,
        Duration::from_secs(10),
        0,
        4,
        None,
        None,
    )
}

fn context(timeout: Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: Instant::now() + timeout,
        walk_threads: None,
        response_window: None,
        github_credential: None,
    }
}

#[test]
fn evaluator_receives_context_and_long_context_and_keeps_success_after_failure() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(async {
        Mock::given(method("POST"))
            .respond_with(|request: &Request| {
                let body: Value = request.body_json().expect("request");
                if body["state"]["text"] == "failed candidate" {
                    return ResponseTemplate::new(401);
                }
                let answers = body["questions"]
                    .as_object()
                    .expect("questions")
                    .iter()
                    .map(|(id, question)| {
                        let legend = question["criteria"]
                            .as_array()
                            .expect("criteria")
                            .iter()
                            .enumerate()
                            .map(|(i, criterion)| (i.to_string(), criterion.clone()))
                            .collect::<serde_json::Map<String, Value>>();
                        (
                            id.clone(),
                            json!({"type":"score","score":2.0,"confidence":1.0,
                    "legend":legend,"probabilities":{"0":0.0,"1":0.0,"2":1.0}}),
                        )
                    })
                    .collect::<serde_json::Map<String, Value>>();
                ResponseTemplate::new(200).set_body_json(json!({
                    "model":"jev-resolved","answers":answers,
                    "usage":{"input_tokens":10,"output_tokens":2}
                }))
            })
            .mount(&server)
            .await;
    });
    let evaluator = evaluator("test-key", server.uri());
    let candidates = ["failed candidate", "selected candidate"]
        .map(|text| Candidate::new(Some(json!({"text":text,"sourceLines":[4,5,0,9]}))));
    let classifiers = vec!["Does this describe termination?".to_owned()];
    let context = context(Duration::from_secs(10));
    let run = |main_goal| {
        let mut usage = ClassificationUsage::default();
        let mut failures = Vec::new();
        let decisions = evaluator
            .select(JudgeCall {
                failures: &mut failures,
                candidates: &candidates,
                admitted: &[0, 1],
                classifiers: &classifiers,
                main_goal: Some(main_goal),
                reasoning: Some("Find the stopping rule"),
                long_context: true,
                limits: SelectionLimits {
                    max_candidates: 2,
                    max_chars: 10_000,
                    timeout: Duration::from_secs(10),
                },
                handle: executor.handle(),
                context: &context,
                usage: &mut usage,
            })
            .expect("independent candidate outcomes");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].candidate, 0);
        assert_eq!(
            failures[0].error_code.to_string(),
            "classificationProviderError"
        );
        assert!(!failures[0].error.is_empty());
        assert!(
            failures[0]
                .hints
                .as_ref()
                .is_some_and(|hints| !hints.is_empty())
        );
        assert_eq!(usage.calls, 2);
        assert_eq!(usage.known_usage_calls, 1);
        assert_eq!(usage.unknown_usage_calls, 1);
        assert_eq!((usage.input_tokens, usage.output_tokens), (10, 2));
        assert_eq!(
            usage.models.into_iter().collect::<Vec<_>>(),
            ["jev-resolved"]
        );
        decisions
    };
    let mut decisions = run("Explain retry behavior");
    decisions.sort_by_key(|(ordinal, _)| *ordinal);
    assert_eq!(
        decisions,
        [(1, Decision::Selected)],
        "a failed candidate stays undecided (unknown) beside a judged one"
    );
    let initial = executor
        .block_on(server.received_requests())
        .expect("requests");
    assert_eq!(initial.len(), 2);
    for request in initial {
        assert_eq!(
            request
                .headers
                .get("typesafe-danger-zone-long-context")
                .and_then(|v| v.to_str().ok()),
            Some("true")
        );
        let body: Value = request.body_json().expect("request");
        assert!(body["state"].get("sourceLines").is_none());
        assert_eq!(
            body["state"]["sourceLineRanges"],
            json!([[4, 5], [0, 0], [9, 9]])
        );
        for question in body["questions"].as_object().expect("questions").values() {
            assert_eq!(
                question["instructions"]["mainGoal"],
                "Explain retry behavior"
            );
            assert_eq!(
                question["instructions"]["reasoning"],
                "Find the stopping rule"
            );
        }
    }
    run("Explain current defaults");
    assert_eq!(
        executor
            .block_on(server.received_requests())
            .expect("requests")
            .len(),
        4,
        "changing research context must not reuse the previous successful judgment"
    );
}

#[test]
fn a_provider_failing_every_candidate_is_a_typed_error_not_unknown() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server),
    );
    let evaluator = evaluator("bad-key", server.uri());
    let candidates = ["first", "second"].map(|text| Candidate::new(Some(json!({"text":text}))));
    let context = context(Duration::from_secs(10));
    let failure = evaluator
        .select(JudgeCall {
            failures: &mut Vec::new(),
            candidates: &candidates,
            admitted: &[0, 1],
            classifiers: &["Does this describe termination?".to_owned()],
            main_goal: None,
            reasoning: None,
            long_context: false,
            limits: SelectionLimits {
                max_candidates: 2,
                max_chars: 10_000,
                timeout: Duration::from_secs(10),
            },
            handle: executor.handle(),
            context: &context,
            usage: &mut ClassificationUsage::default(),
        })
        .expect_err("no candidate was judged");
    let JudgeFailure::Provider(error) = failure else {
        panic!("a provider failure, not an unavailable judge: {failure:?}");
    };
    assert_eq!(error.code, ErrorCode::ClassificationProviderError);
    assert!(error.message.contains("401"), "{}", error.message);
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint.contains("OCTOCODE_CLASSIFICATION_API")),
        "credential hint: {:?}",
        error.hints
    );
}

/// A host that refuses connections fails every candidate; the circuit
/// it opens within the same call is the consequence, so the row names
/// the cause (no connection), not "failed repeatedly".
#[test]
fn a_refused_connection_is_named_not_the_circuit_it_opens() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port();
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let evaluator = evaluator("key", format!("http://127.0.0.1:{port}"));
    let candidates: Vec<_> = (0..12)
        .map(|n| Candidate::new(Some(json!({"text":format!("candidate {n}")}))))
        .collect();
    let admitted: Vec<usize> = (0..candidates.len()).collect();
    let context = context(Duration::from_secs(20));
    let failure = evaluator
        .select(JudgeCall {
            failures: &mut Vec::new(),
            candidates: &candidates,
            admitted: &admitted,
            classifiers: &["Does this describe termination?".to_owned()],
            main_goal: None,
            reasoning: None,
            long_context: false,
            limits: SelectionLimits {
                max_candidates: candidates.len(),
                max_chars: 100_000,
                timeout: Duration::from_secs(20),
            },
            handle: executor.handle(),
            context: &context,
            usage: &mut ClassificationUsage::default(),
        })
        .expect_err("nothing was judged");
    let JudgeFailure::Provider(error) = failure else {
        panic!("a provider failure: {failure:?}");
    };
    assert_eq!(
        error.code,
        crate::providers::classification::CIRCUIT_OPEN,
        "the provider is unavailable: {error:?}"
    );
    assert!(error.message.contains("could not connect"), "{error:?}");
}
