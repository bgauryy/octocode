//! Prepared media: request headers, per-request assets, cache identity
//! and retries that keep the media payload.
use super::*;

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
