use super::*;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

/// An evaluator with a credential of its own, unique in the process
/// ([`crate::judgment::test_identity`]): the gate remembers a refused
/// credential per endpoint and credential, and a mock server's port is
/// reused across tests, so tests sharing one key would inherit each other's
/// memoized 401 or 402.
fn evaluator(server: &MockServer) -> Evaluator {
    evaluator_with_key(server, &crate::judgment::test_identity::unique("test-key"))
}

fn evaluator_with_key(server: &MockServer, key: &str) -> Evaluator {
    Evaluator::new(
        Some(SecretString::from(key.to_owned())),
        server.uri(),
        // A model of its own: what an endpoint's windows taught is kept per
        // endpoint and model for the process.
        crate::judgment::test_identity::unique("jev-test"),
        &octocode_runtime_base::providers::classification::jev::JEV,
        Some(Duration::from_secs(10)),
        1,
        4,
    )
}

fn evaluation_budget(evaluator: &Evaluator) -> RequestBudget {
    evaluator.budget(
        Instant::now() + Duration::from_secs(10),
        octocode_io::cancellation::Cancellation::new(),
    )
}

fn response(request: &Request) -> ResponseTemplate {
    let body: Value = request.body_json().unwrap();
    let answers: serde_json::Map<String, Value> = body["questions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, question)| {
            let answer = match question["type"].as_str().unwrap() {
                "box" => json_fixture(
                    r#"{
                        "type": "box",
                        "box": {"top": 0.1, "bottom": 0.8, "left": 0.2, "right": 0.7},
                        "confidence": null
                    }"#,
                ),
                _ => json_fixture(r#"{"type": "noul", "noul": 0.8}"#),
            };
            (id.clone(), answer)
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(json!({"model":"jev-resolved","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
}

/// A request the default window refuses is retried in the long-context
/// window when `longContext` is omitted; the answer says so, with the input
/// tokens billed and how to get the refusal instead.
#[tokio::test]
async fn a_long_context_retry_is_disclosed_with_its_billed_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            if request
                .headers
                .get("typesafe-danger-zone-long-context")
                .is_none()
            {
                return ResponseTemplate::new(400).set_body_json(json_fixture(
                    r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
                ));
            }
            let body: Value = request.body_json().expect("body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json_fixture(r#"{"type": "noul", "noul": 0.8}"#)))
                .collect::<serde_json::Map<String, Value>>();
            ResponseTemplate::new(200).set_body_json(json!({"model":"jev-resolved",
                "answers":answers,"usage":{"input_tokens":123_456,"output_tokens":2}}))
        })
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let gate = evaluator.gate();
    let state = json!("source");
    let questions = vec![json_fixture(
        r#"{"type": "noul", "instructions": "Evidence?"}"#,
    )];
    let judged = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &state,
                questions: &questions,
                assets: None,
                long_context: LongContext::OnRefusal,
                companions: 0,
                cap: None,
            },
            None,
            &budget,
            &gate,
        )
        .await
        .expect("answered in the long window");
    assert_eq!(judged.provider_calls, 2, "the refusal and the retry");
    assert_eq!(
        judged.long_context_retry,
        Some(super::LongContextRetry {
            unreported_calls: 0,
            refused_calls: 1
        }),
        "the retry is a fact of the row, its usage reported"
    );
    let mut usage = super::stats::ClassificationUsage::default();
    usage.add(&Ok(judged));
    let notice = usage.long_context_notice().expect("the retry is disclosed");
    for part in [
        "1 evaluation refused",
        "long-context window",
        "123456 input tokens",
        "longContext:false",
    ] {
        assert!(notice.contains(part), "{part}: {notice}");
    }
}

/// A text group the default window refuses, with `longContext` omitted, is
/// retried whole in the long window: the state is billed once more, not once
/// per question, and no per-question request is sent. (An image group splits
/// instead: `media::a_refused_image_group_is_answered_per_question_in_the_default_window`.)
#[tokio::test]
async fn a_refused_text_group_retries_whole_in_the_long_window_without_splitting() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            if request
                .headers
                .get("typesafe-danger-zone-long-context")
                .is_none()
            {
                return ResponseTemplate::new(400).set_body_json(json_fixture(
                    r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
                ));
            }
            let body: Value = request.body_json().expect("body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json_fixture(r#"{"type": "noul", "noul": 0.8}"#)))
                .collect::<serde_json::Map<String, Value>>();
            ResponseTemplate::new(200).set_body_json(json!({"model":"jev-resolved",
                "answers":answers,"usage":{"input_tokens":1_000,"output_tokens":2}}))
        })
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let state = json!("source");
    let questions = vec![
        json_fixture(r#"{"type": "noul", "instructions": "Evidence?"}"#),
        json_fixture(r#"{"type": "noul", "instructions": "Other evidence?"}"#),
    ];
    let judged = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &state,
                questions: &questions,
                assets: None,
                long_context: LongContext::OnRefusal,
                companions: 0,
                cap: None,
            },
            None,
            &budget,
            &evaluator.gate(),
        )
        .await
        .expect("answered in the long window");
    assert_eq!(judged.answers.len(), 2);
    let sent = server.received_requests().await.unwrap();
    assert_eq!(sent.len(), 2, "the refused group and its long-window retry");
    for request in &sent {
        let body: Value = request.body_json().expect("body");
        assert_eq!(
            body["questions"].as_object().expect("questions").len(),
            2,
            "no per-question request"
        );
    }
    assert!(
        sent[1]
            .headers
            .get("typesafe-danger-zone-long-context")
            .is_some_and(|value| *value == "true")
    );
}

mod media;
mod sanitization;
mod shared;
mod usage;

#[tokio::test]
async fn models_use_authenticated_get_retry_and_validate_the_catalog() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = attempts.clone();
    let date = "2026-09-10T18:39:06.057655+00:00";
    let payload =
        json!({"models":[{"name":"jev-preview","description":"Preview","release_date":date}]});
    // The listing is spelled as the output contract types it.
    let catalog = json!({"models":[octocode_contract_types::ClasifyModelMetadata {
        default: None,
        description: "Preview".into(),
        name: "jev-preview".into(),
        release_date: date.into(),
    }]});
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
    let evaluator = evaluator(&server);
    assert_eq!(
        evaluator
            .models(&evaluation_budget(&evaluator), &evaluator.gate())
            .await
            .unwrap(),
        catalog
    );
    for request in server.received_requests().await.unwrap() {
        assert!(request.body.is_empty());
        assert!(
            request.headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("Bearer test-key")
        );
        assert!(!request.headers.contains_key("typesafe-danger-zone-images"));
    }
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
                            json_fixture(
                                r#"{"type": "noul", "noul": 0.9, "explanation": "extension"}"#,
                            ),
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
        let questions = vec![
            json_fixture(r#"{"type": "noul", "instructions": "Evidence?"}"#);
            if grouped { 2 } else { 1 }
        ];
        let input = EvaluationInput {
            state: &state,
            questions: &questions,
            assets: None,
            long_context: crate::judgment::LongContext::Never,
            companions: 0,
            cap: None,
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

#[test]
fn references_are_value_occurrences_including_criteria_not_property_names() {
    assert!(referenced(
        &json_fixture(r#"{"criteria": {"yes": "a depicted item in opaque_asset_1"}}"#),
        "opaque_asset_1"
    ));
    assert!(referenced(
        &json_fixture(r#"[{"image": "opaque_asset_1"}]"#),
        "opaque_asset_1"
    ));
    assert!(!referenced(
        &json_fixture(r#"{"opaque_asset_1": "not a value reference"}"#),
        "opaque_asset_1"
    ));
    assert!(!referenced(&json!(null), "opaque_asset_1"));
}

/// A key is cited as a whole token (the asset-name charset `[A-Za-z0-9._-]`),
/// never matched inside a longer word or name.
#[test]
fn references_match_whole_asset_keys_not_substrings() {
    let cited = |text: &str, key: &str| referenced(&json!(text), key);
    for text in ["a", "Is a blank?", "see (a), then b", "a.", "x: a-"] {
        assert!(cited(text, "a"), "{text}");
    }
    for text in [
        "image_a.",
        "Does image_a, image_b differ?",
        "\"image_a\"",
        "image_a",
    ] {
        assert!(cited(text, "image_a"), "{text}");
    }
    for text in [
        "image_ab",
        "my_image_a",
        "image_a-b",
        "x.image_a",
        "image_a.b",
    ] {
        assert!(!cited(text, "image_a"), "{text}");
    }
    for text in ["blank", "table", "a1", "x_a", "a-b"] {
        assert!(!cited(text, "a"), "{text}");
    }
    assert!(!cited("Is it blank, table or data?", "a"));
    assert!(!cited("an a-b pair", "a-b-c"));
}

/// Companions ride only in the grouped request: when both windows refuse the
/// group, the per-question requests carry the asked questions alone, so a
/// companion never adds a request. The complete answer holds only those.
#[tokio::test]
async fn companions_are_never_sent_alone() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let body: Value = request.body_json().expect("body");
            let questions = body["questions"].as_object().expect("questions");
            if questions.len() > 1 {
                return ResponseTemplate::new(400).set_body_json(json_fixture(
                    r#"{"detail": {"error_type": "max_tokens_exceeded"}}"#,
                ));
            }
            response(request)
        })
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let state = json!("source");
    let questions = vec![
        json_fixture(r#"{"type": "noul", "instructions": "Evidence?"}"#),
        json_fixture(
            r#"{
                "type": "choice",
                "instructions": "Where?",
                "criteria": {"p1": null, "unknown": null}
            }"#,
        ),
    ];
    let judged = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &state,
                questions: &questions,
                assets: None,
                long_context: LongContext::OnRefusal,
                companions: 1,
                cap: None,
            },
            None,
            &budget,
            &evaluator.gate(),
        )
        .await
        .expect("the asked question answered alone");
    assert_eq!(
        judged.answers,
        vec![json_fixture(r#"{"type": "noul", "noul": 0.8}"#)]
    );
    let sent = server.received_requests().await.unwrap();
    // The group in each window, then the one asked question; never the companion.
    assert_eq!(sent.len(), 3, "{}", sent.len());
    let last: Value = sent[2].body_json().expect("body");
    assert_eq!(last["questions"]["answer"]["type"], "noul");
    assert_eq!(judged.provider_calls, 3);
}

/// A companion's malformed answer fails nothing: the asked answers stand
/// and the companion's place is `null`.
#[tokio::test]
async fn a_malformed_companion_answer_fails_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "model": "jev-resolved",
                "answers": {
                    "answer_0": {"type": "noul", "noul": 0.7},
                    "answer_1": {
                        "type": "choice",
                        "choice": "nowhere",
                        "confidence": 1,
                        "probabilities": {"nowhere": 1.0}
                    }
                },
                "usage": {"input_tokens": 10, "output_tokens": 2}
            }"#,
        )))
        .expect(1)
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let budget = evaluation_budget(&evaluator);
    let state = json!("source");
    let questions = vec![
        json_fixture(r#"{"type": "noul", "instructions": "Evidence?"}"#),
        json_fixture(
            r#"{
                "type": "choice",
                "instructions": "Where?",
                "criteria": {"p1": null, "unknown": null}
            }"#,
        ),
    ];
    let judged = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &state,
                questions: &questions,
                assets: None,
                long_context: LongContext::Never,
                companions: 1,
                cap: None,
            },
            None,
            &budget,
            &evaluator.gate(),
        )
        .await
        .expect("the asked answer stands");
    assert_eq!(
        judged.answers,
        vec![
            json_fixture(r#"{"type": "noul", "noul": 0.7}"#),
            Value::Null
        ]
    );
    assert_eq!(judged.provider_calls, 1);
}
