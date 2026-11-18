use super::*;
use octocode_test_support::json_fixture;

/// The provider boundary must protect callers that do not originate at the
/// public clasify input gate, including local/GitHub selection callers.
#[tokio::test]
async fn sensitive_boundary_provider_egress_scrubs_state_and_questions() {
    let secret = "ghp_SyntheticCredentialMaterial987654321abcdef";
    for count in [1, 2] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let evaluator = evaluator(&server);
        let state = json!({"password":secret,"folder":"src/secrets/private"});
        let questions = vec![json!({"type":"noul","instructions":{"password":secret}}); count];
        evaluator
            .evaluate_prepared(
                EvaluationInput {
                    state: &state,
                    questions: &questions,
                    assets: None,
                    long_context: LongContext::Never,
                    companions: 0,
                },
                None,
                &evaluation_budget(&evaluator),
                &evaluator.gate(),
            )
            .await
            .unwrap();
        let requests = server.received_requests().await.unwrap();
        let body: Value = requests[0].body_json().unwrap();
        assert!(!body.to_string().contains(secret), "{body}");
        assert_eq!(body["state"]["folder"], "src/secrets/private");
        assert!(
            requests[0].headers["authorization"]
                .to_str()
                .unwrap()
                .starts_with("Bearer test-key")
        );
    }
}

#[tokio::test]
async fn sensitive_boundary_provider_ingress_scrubs_before_returning_to_tools() {
    let token = format!("ghp_{}", "a".repeat(40));
    for count in [1, 2] {
        let server = MockServer::start().await;
        let model = token.clone();
        Mock::given(method("POST"))
            .respond_with(move |request: &Request| {
                let body: Value = request.body_json().unwrap();
                let answers: serde_json::Map<String, Value> = body["questions"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(|key| {
                        (
                            key.clone(),
                            json_fixture(r#"{"type": "noul", "noul": 0.9}"#),
                        )
                    })
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({"model":model,"answers":answers,
                "usage":{"input_tokens":5,"output_tokens":2}}))
            })
            .mount(&server)
            .await;
        let evaluator = evaluator(&server);
        let state = json!("ordinary evidence");
        let questions =
            vec![json_fixture(r#"{"type": "noul", "instructions": "Evidence?"}"#); count];
        let result = evaluator
            .evaluate_prepared(
                EvaluationInput {
                    state: &state,
                    questions: &questions,
                    assets: None,
                    long_context: LongContext::Never,
                    companions: 0,
                },
                None,
                &evaluation_budget(&evaluator),
                &evaluator.gate(),
            )
            .await
            .unwrap();
        assert!(!serde_json::to_string(&result).unwrap().contains(&token));
        assert_eq!(result.input_tokens, 5);
        assert_eq!(result.answers.len(), count);
    }
}

#[tokio::test]
async fn sensitive_boundary_model_catalog_is_scrubbed_before_returning_to_tools() {
    let server = MockServer::start().await;
    let token = format!("ghp_{}", "b".repeat(40));
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models":[{"name":"jev-preview","description":token,"release_date":"2026-01-01"}]
        })))
        .mount(&server)
        .await;
    let evaluator = evaluator(&server);
    let result = evaluator
        .models(&evaluation_budget(&evaluator), &evaluator.gate())
        .await
        .unwrap();
    assert!(!result.to_string().contains(&token), "{result}");
    assert_eq!(result["models"][0]["name"], "jev-preview");
}

#[tokio::test]
async fn sensitive_boundary_credential_in_configured_model_is_refused_before_egress() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(response)
        .expect(0)
        .mount(&server)
        .await;
    let mut evaluator = evaluator(&server);
    evaluator.model = format!("ghp_{}", "a".repeat(40));
    let state = json!("ordinary evidence");
    let questions = [json_fixture(
        r#"{"type": "noul", "instructions": "Evidence?"}"#,
    )];
    let error = evaluator
        .evaluate_prepared(
            EvaluationInput {
                state: &state,
                questions: &questions,
                assets: None,
                long_context: LongContext::Never,
                companions: 0,
            },
            None,
            &evaluation_budget(&evaluator),
            &evaluator.gate(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidClassificationConfiguration);
    assert!(!error.message.contains(&evaluator.model));
}
