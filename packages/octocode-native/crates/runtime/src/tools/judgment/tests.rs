use super::*;
use octocode_contract_types::ErrorCode;
use serde_json::json;
use std::sync::Arc;
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
    let answers: serde_json::Map<String, Value> = body["questions"].as_object().unwrap().iter().map(|(id, question)| {
        let answer = match question["type"].as_str().unwrap() {
            "box" => json!({"type":"box","box":{"top":0.1,"bottom":0.8,"left":0.2,"right":0.7},"confidence":null}),
            _ => json!({"type":"noul","noul":0.8}),
        };
        (id.clone(), answer)
    }).collect();
    ResponseTemplate::new(200).set_body_json(json!({"model":"jev-resolved","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
}

mod media;
mod shared;
mod usage;

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
        assert_eq!(request.headers["authorization"], "Bearer test-key");
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

#[test]
fn references_are_value_occurrences_including_criteria_not_property_names() {
    assert!(referenced(
        &json!({"criteria":{"yes":"a depicted item in opaque_asset_1"}}),
        "opaque_asset_1"
    ));
    assert!(referenced(
        &json!([{"image":"opaque_asset_1"}]),
        "opaque_asset_1"
    ));
    assert!(!referenced(
        &json!({"opaque_asset_1":"not a value reference"}),
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
