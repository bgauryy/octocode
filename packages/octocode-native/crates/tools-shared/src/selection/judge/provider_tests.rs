//! The evaluator as a selection judge against a real provider endpoint:
//! request shape, per-candidate failures and typed whole-page errors.
use super::*;
use crate::selection::Candidate;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_test_support::json_fixture;
use std::time::{Duration, Instant};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// A retry-free evaluator, so each candidate is one provider request. Its
/// model is its own: the windows an endpoint taught are kept per endpoint and
/// model for the process, and a mock server's port is reused by a later test.
pub(super) fn evaluator(key: &str, base_url: String) -> Evaluator {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let model = format!(
        "jev-test-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    evaluator_of(key, base_url, &model)
}

/// The same, as a model of its own: what an endpoint's windows have taught
/// is kept per endpoint and model, and a mock server's port can be reused.
/// Each evaluator's credential is its own (`key` plus a counter): the gate
/// remembers a refused credential per endpoint and credential digest.
pub(super) fn evaluator_of(key: &str, base_url: String, model: &str) -> Evaluator {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let key = format!(
        "{key}-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    Evaluator::new(
        Some(secrecy::SecretString::from(key)),
        base_url,
        model.into(),
        &octocode_runtime_base::providers::classification::jev::JEV,
        Some(Duration::from_secs(10)),
        0,
        4,
    )
}

pub(super) fn context(timeout: Duration) -> ExecutionContext {
    ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(Instant::now() + timeout),
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
                // The question about the failing candidate (unit 1 of the shared
                // state) comes back without an answer.
                let failing = |id: &str| {
                    body["questions"][id]["instructions"]["target"]
                        .as_str()
                        .is_some_and(|label| label.starts_with("unit 1"))
                };
                let answers = body["questions"]
                    .as_object()
                    .expect("questions")
                    .iter()
                    .filter(|(id, _)| !failing(id))
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
    let predicates = vec!["Does this describe termination?".to_owned()];
    let context = context(Duration::from_secs(10));
    let run = |research_context| {
        let mut usage = ClassificationUsage::default();
        let mut failures = Vec::new();
        let decisions = evaluator
            .select_grouped(
                JudgeCall {
                    failures: &mut failures,
                    candidates: &candidates,
                    admitted: &[0, 1],
                    predicates: &predicates,
                    research_context: Some(research_context),
                    long_context: crate::judgment::LongContext::Always,
                    handle: executor.handle(),
                    context: &context,
                    usage: &mut usage,
                },
                4,
            )
            .unwrap_or_else(|error| {
                panic!("independent outcomes: {error:?}; failures: {failures:?}")
            });
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].candidate, 0);
        assert_eq!(
            failures[0].error_code.to_string(),
            "invalidClassificationResponse"
        );
        assert!(!failures[0].error.is_empty());
        assert!(
            failures[0]
                .hints
                .as_ref()
                .is_some_and(|hints| !hints.is_empty())
        );
        assert_eq!(usage.calls, 1, "both candidates share one request");
        assert_eq!(usage.known_usage_calls, 1);
        assert_eq!(usage.unknown_usage_calls, 0);
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
    assert_eq!(initial.len(), 1);
    for request in initial {
        assert_eq!(
            request
                .headers
                .get("typesafe-danger-zone-long-context")
                .and_then(|v| v.to_str().ok()),
            Some("true")
        );
        let body: Value = request.body_json().expect("request");
        let units = body["state"]["units"].as_str().expect("units");
        assert!(!units.contains("\"sourceLines\""), "{units}");
        assert!(
            units.contains("\"sourceLineRanges\":[[4,5],[0,0],[9,9]]"),
            "{units}"
        );
        // Context travels once, beside the evidence.
        assert_eq!(body["state"]["context"], "Explain retry behavior");
        for question in body["questions"].as_object().expect("questions").values() {
            assert!(question["instructions"].get("context").is_none());
        }
    }
    run("Explain current defaults");
    assert_eq!(
        executor
            .block_on(server.received_requests())
            .expect("requests")
            .len(),
        2,
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
            predicates: &["Does this describe termination?".to_owned()],
            research_context: None,
            long_context: crate::judgment::LongContext::Never,
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
            predicates: &["Does this describe termination?".to_owned()],
            research_context: None,
            long_context: crate::judgment::LongContext::Never,
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
        octocode_runtime_base::providers::classification::CIRCUIT_OPEN,
        "the provider is unavailable: {error:?}"
    );
    assert!(error.message.contains("could not connect"), "{error:?}");
}

/// A provider answering every question with direct evidence.
pub(super) fn direct_answers(request: &Request) -> ResponseTemplate {
    let body: Value = request.body_json().expect("request");
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
}

/// The `lines` of every state the provider received.
fn sent_lines(executor: &tokio::runtime::Runtime, server: &MockServer) -> Vec<String> {
    executor
        .block_on(server.received_requests())
        .expect("requests")
        .iter()
        .map(|request| {
            let body: Value = request.body_json().expect("request");
            body["state"]["lines"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

/// The hits of one file share a state that holds the union of their windows,
/// each hit's own line included: a target whose line is missing from the
/// state is judged from its label alone. Two hits with identical text (the
/// reporter's `sign-in.ts:88`/`:89`) see the same evidence.
#[test]
fn a_shared_state_holds_every_hits_own_line() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(
        Mock::given(method("POST"))
            .respond_with(direct_answers)
            .mount(&server),
    );
    let evaluator = evaluator("test-key", server.uri());
    let same = "fromBrowser.catch(() => undefined);";
    let hit = |line: u64, window: &[(u64, &str)]| {
        Candidate::new(Some(json!({
            "path":"oauth/sign-in.ts",
            "target":{"line":line,"text":window[0].1},
            "lines":window.iter().map(|(at, text)| json!([at, text])).collect::<Vec<_>>(),
        })))
    };
    let candidates = [
        hit(82, &[(82, "store.catch(() => {});"), (83, "return token;")]),
        hit(88, &[(88, same), (89, same)]),
        hit(89, &[(89, same), (90, "done();")]),
    ];
    let context = context(Duration::from_secs(10));
    evaluator
        .select(JudgeCall {
            failures: &mut Vec::new(),
            candidates: &candidates,
            admitted: &[0, 1, 2],
            predicates: &["Does this discard the error?".to_owned()],
            research_context: None,
            long_context: crate::judgment::LongContext::Never,
            handle: executor.handle(),
            context: &context,
            usage: &mut ClassificationUsage::default(),
        })
        .expect("judged");
    let states = sent_lines(&executor, &server);
    assert_eq!(states.len(), 1, "one file, one shared state");
    let lines = states[0].lines().collect::<Vec<_>>();
    assert_eq!(
        lines,
        [
            "82\tstore.catch(() => {});",
            "83\treturn token;",
            &format!("88\t{same}"),
            &format!("89\t{same}"),
            "90\tdone();",
        ],
        "every target line and every window line, once each"
    );
}

/// A target's label names the declaration and the key path its line sits
/// in, so a judge reading one bare line still sees which section holds it.
#[test]
fn a_target_label_names_its_declaration_and_key_path() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(
        Mock::given(method("POST"))
            .respond_with(direct_answers)
            .mount(&server),
    );
    let evaluator = evaluator("test-key", server.uri());
    let candidates = [Candidate::new(Some(json_fixture(
        r#"{
            "path": "package.json",
            "target": {
                "line": 15,
                "text": "    \"x\": \"^1.0.0\"",
                "enclosing": {"kind": "object", "symbolName": "devDependencies"},
                "key": "devDependencies.x"
            },
            "lines": [[15, "    \"x\": \"^1.0.0\""]]
        }"#,
    )))];
    let context = context(Duration::from_secs(10));
    evaluator
        .select(JudgeCall {
            failures: &mut Vec::new(),
            candidates: &candidates,
            admitted: &[0],
            predicates: &["Is this a runtime dependency?".to_owned()],
            research_context: None,
            long_context: crate::judgment::LongContext::Never,
            handle: executor.handle(),
            context: &context,
            usage: &mut ClassificationUsage::default(),
        })
        .expect("judged");
    let requests = executor
        .block_on(server.received_requests())
        .expect("requests");
    let body: Value = requests[0].body_json().expect("body");
    let labels = body["questions"]
        .as_object()
        .expect("questions")
        .values()
        .map(|question| question["instructions"]["target"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [json!(
            "line 15: \"x\": \"^1.0.0\" (in object devDependencies) (key devDependencies.x)"
        )]
    );
    assert_eq!(body["state"]["lines"], "15\t    \"x\": \"^1.0.0\"");
}

/// Candidates with identical states in one call are one evaluation: one
/// provider request, one decision for each, spend counted once.
#[test]
fn identical_candidate_states_in_one_call_share_one_request() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(
        Mock::given(method("POST"))
            .respond_with(|request: &Request| {
                let body: Value = request.body_json().expect("request");
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
            .mount(&server),
    );
    let evaluator = evaluator("test-key", server.uri());
    let candidates =
        ["same", "other", "same"].map(|text| Candidate::new(Some(json!({"text":text}))));
    let context = context(Duration::from_secs(10));
    let mut usage = ClassificationUsage::default();
    let mut decisions = evaluator
        .select_grouped(
            JudgeCall {
                failures: &mut Vec::new(),
                candidates: &candidates,
                admitted: &[0, 1, 2],
                predicates: &["Does this describe termination?".to_owned()],
                research_context: None,
                long_context: crate::judgment::LongContext::Never,
                handle: executor.handle(),
                context: &context,
                usage: &mut usage,
            },
            4,
        )
        .expect("judged");
    decisions.sort_by_key(|(ordinal, _)| *ordinal);
    assert_eq!(
        decisions,
        [
            (0, Decision::Selected),
            (1, Decision::Selected),
            (2, Decision::Selected)
        ]
    );
    let requests = executor
        .block_on(server.received_requests())
        .expect("requests");
    assert_eq!(requests.len(), 1, "one request for the distinct states");
    let body: Value = requests[0].body_json().expect("request");
    assert_eq!(
        body["questions"].as_object().expect("questions").len(),
        2,
        "one question per distinct state"
    );
    assert_eq!(usage.calls, 1);
    assert_eq!((usage.input_tokens, usage.output_tokens), (10, 2));
}
