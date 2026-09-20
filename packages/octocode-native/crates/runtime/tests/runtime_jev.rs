mod support;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::{Workspace, row_data, row_status};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn question() -> Value {
    json!({"type":"noul","instructions":"Assess only the supplied state"})
}
fn query() -> Value {
    json!({"reasoning":"Decide the next evidence read.","context":{"value":{"observations":["A cancellation guard precedes the write."]}},"question":question()})
}
fn hidden(tool: &str, query: Value) -> Value {
    json!({"reasoning":"Decide the next evidence read.","context":{"tool":tool,"query":query},"question":question()})
}
fn response() -> Value {
    json!({"model":"jev-test","answers":{"answer":{"type":"noul","noul":0.81}},"usage":{"input_tokens":100,"output_tokens":4}})
}
fn settings(server: &MockServer) -> Vec<(&'static str, String)> {
    vec![
        ("OCTOCODE_JEV_KEY", "secret".into()),
        ("OCTOCODE_JEV_MODEL", "jev-test".into()),
        ("OCTOCODE_JEV_BASE_URL", server.uri()),
        ("REQUEST_TIMEOUT", "30000".into()),
    ]
}

#[test]
fn catalog_has_one_jev_tool_gated_on_a_nonblank_key() {
    let workspace = Workspace::new();
    let without_key = workspace.runtime(&[]);
    assert!(!without_key.is_available("jev"));
    let catalog = without_key.catalog().unwrap();
    let tools = catalog["tools"].as_array().unwrap();
    let jev: Vec<_> = tools
        .iter()
        .filter(|tool| tool["name"].as_str().unwrap_or_default().starts_with("jev"))
        .collect();
    assert_eq!(jev.len(), 1);
    assert_eq!(jev[0]["name"], "jev");
    assert_eq!(jev[0]["available"], false);
    drop(without_key);
    let blank = workspace.runtime(&[("OCTOCODE_JEV_KEY", "   ".to_owned())]);
    assert!(!blank.is_available("jev"));
    drop(blank);
    let with_key = workspace.runtime(&[("OCTOCODE_JEV_KEY", "secret".to_owned())]);
    assert!(with_key.is_available("jev"));
    assert!(!with_key.is_available("jevReasoning"));
    assert!(!with_key.is_available("jevScout"));
}

#[tokio::test]
async fn response_paging_rejects_before_context_or_inference() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let source = hidden(
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"a.rs","branch":"main","reasoning":"Read"}),
    );
    for (field, value) in [
        ("responseCharLength", json!(2000)),
        ("responseCharOffset", json!(0)),
        ("responseSnapshot", json!("old")),
    ] {
        for render in [true, false] {
            let mut input = json!({"queries":[source],"renderText":render});
            input[field] = value.clone();
            let error = runtime
                .execute(format!("paging-{field}-{render}"), "jev".into(), input)
                .await
                .unwrap_err();
            assert_eq!(error.code, "unsupportedResponsePagination");
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn explicit_value_preserves_state_and_projects_one_answer() {
    let server = MockServer::start().await;
    let value = query();
    Mock::given(method("POST")).and(path("/v1/systemone")).and(header("authorization","Bearer secret"))
        .and(body_json(json!({"model":"jev-test","state":value["context"]["value"],"questions":{"answer":value["question"]}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response())).expect(1).mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let outcome = runtime
        .execute("value".into(), "jev".into(), value)
        .await
        .unwrap();
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    assert_eq!(
        row_data(&outcome),
        &json!({"model":"jev-test","answer":{"type":"noul","noul":0.81},"usage":{"input_tokens":100,"output_tokens":4}})
    );
    runtime.close().await;
}

#[tokio::test]
async fn old_aliases_multiple_questions_and_effectful_context_fail_before_transport() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    for (i, input) in [
        json!({"state":null,"questions":{"q":question()}}),
        json!({"reasoning":"Decide the next evidence read.","context":{"value":null},"questions":{"q":question()}}),
        json!({"reasoning":"Decide the next evidence read.","context":{"value":null},"question":[question(),question()]}),
        hidden("jev", query()),
        hidden("astRewrite", json!({})),
        hidden("ghCloneRepo", json!({})),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            runtime
                .execute(format!("invalid-{i}"), "jev".into(), input)
                .await
                .is_err()
        );
    }
    for field in [
        "model",
        "goal",
        "debug",
        "route",
        "sources",
        "state",
        "questions",
    ] {
        let mut value = query();
        value[field] = json!("unwanted");
        assert!(
            runtime
                .execute(format!("field-{field}"), "jev".into(), value)
                .await
                .is_err()
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn missing_or_blank_reasoning_rejects_before_context_and_provider() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut config = settings(&server);
    config.push(("GITHUB_API_URL", format!("{}/api/v3", server.uri())));
    let runtime = workspace.runtime(&config);
    for (index, reasoning) in [
        None,
        Some(Value::Null),
        Some(json!(7)),
        Some(json!("")),
        Some(json!(" \t\n")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut value = hidden(
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"a.rs","branch":"main","reasoning":"Read"}),
        );
        match reasoning {
            Some(reasoning) => {
                value["reasoning"] = reasoning;
            }
            None => {
                value.as_object_mut().unwrap().remove("reasoning");
            }
        }
        assert!(
            runtime
                .execute(format!("invalid-reasoning-{index}"), "jev".into(), value)
                .await
                .is_err()
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn nested_queries_use_canonical_validation_and_skip_provider_on_failure() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let file = workspace.write("source.rs", "one\n");
    for (i, inner) in [
        json!({"path":file}),
        json!({"path":file,"reasoning":"Read","invented":true}),
        json!({"queries":[{"path":file,"reasoning":"Read"}]}),
        json!({"cursor":"opaque"}),
        json!({"path":file,"reasoning":"Read","responseCharLength":100}),
    ]
    .into_iter()
    .enumerate()
    {
        let out = runtime
            .execute(
                format!("nested-{i}"),
                "jev".into(),
                hidden("localFetch", inner),
            )
            .await
            .unwrap();
        assert_eq!(row_status(&out), "error");
        assert_eq!(row_data(&out)["errorCode"], "invalidJevContext");
    }
    for (i, path) in [
        workspace.workspace.join("missing.rs"),
        workspace.write(".env", "PRIVATE"),
        workspace.write_outside_allowed_roots("outside.rs", "OUTSIDE"),
    ]
    .into_iter()
    .enumerate()
    {
        let out = runtime
            .execute(
                format!("failed-{i}"),
                "jev".into(),
                hidden("localFetch", json!({"path":path,"reasoning":"Read"})),
            )
            .await
            .unwrap();
        assert_eq!(row_status(&out), "error", "{}", out.structured_content);
    }
    runtime.close().await;
    let mut disabled = settings(&server);
    disabled.push(("ENABLE_LOCAL", "false".into()));
    let runtime = workspace.runtime(&disabled);
    let out = runtime
        .execute(
            "disabled".into(),
            "jev".into(),
            hidden("localFetch", json!({"path":file,"reasoning":"Read"})),
        )
        .await
        .unwrap();
    assert_eq!(row_data(&out)["errorCode"], "jevContextUnavailable");
    runtime.close().await;
}

#[tokio::test]
async fn recoverable_hidden_failure_returns_body_free_receipt_without_provider_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let path = workspace.workspace.join("oversized.txt");
    let file = std::fs::File::create(&path).expect("create sparse oversized source");
    file.set_len(10 * 1024 * 1024 + 1)
        .expect("extend sparse oversized source");
    drop(file);
    let runtime = workspace.runtime(&settings(&server));
    let out = runtime
        .execute(
            "recoverable-hidden-failure".into(),
            "jev".into(),
            hidden(
                "localFetch",
                json!({"path":path,"reasoning":"Read bounded source"}),
            ),
        )
        .await
        .unwrap();

    assert_eq!(row_status(&out), "error", "{}", out.structured_content);
    let data = row_data(&out);
    assert_eq!(data["errorCode"], "fileTooLarge");
    assert!(data.get("answer").is_none());
    assert!(data.get("usage").is_none());
    let receipt = &data["context"];
    assert_eq!(receipt["tool"], "localFetch");
    assert_eq!(receipt["coverage"], "partial");
    assert!(receipt.get("next").is_none());
    assert!(
        receipt["limitations"]
            .as_array()
            .is_some_and(|limitations| {
                limitations.iter().any(|value| {
                    value
                        .as_str()
                        .is_some_and(|text| text.contains("Jev evaluation was not run"))
                })
            })
    );
    let serialized = receipt.to_string();
    assert!(!serialized.contains("oversized.txt"));
    assert!(!serialized.contains("File too large"));
    runtime.close().await;
}

#[tokio::test]
async fn oversized_context_rejects_before_reader_or_provider() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut config = settings(&server);
    config.push(("GITHUB_API_URL", server.uri()));
    let runtime = workspace.runtime(&config);
    let value = hidden(
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"source.rs","branch":"main","reasoning":"x".repeat(4*1024*1024)}),
    );
    let error = runtime
        .execute("oversized-inner".into(), "jev".into(), value)
        .await
        .unwrap_err();
    assert_eq!(error.code, "securityValidationFailed");
    let large: serde_json::Map<String, Value> = (0..5)
        .map(|i| (i.to_string(), json!(vec!["x".repeat(9000); 100])))
        .collect();
    let out = runtime
        .execute(
            "oversized-value".into(),
            "jev".into(),
            json!({"reasoning":"Decide the next evidence read.","context":{"value":large},"question":question()}),
        )
        .await
        .unwrap();
    assert_eq!(row_data(&out)["errorCode"], "invalidJevRequest");
    runtime.close().await;
}

#[tokio::test]
async fn ordinary_tool_state_is_sanitized_and_hidden_receipt_preserves_rubric_identity() {
    let workspace = Workspace::new();
    let server = MockServer::start().await;
    let runtime = workspace.runtime(&settings(&server));
    let token = format!("ghp_{}", "Ab3dEf6hIj9lMn2pQr5tUv8xYz1bCd4fGh7j");
    let file = workspace.write(
        "source.rs",
        format!("HIDDEN_SOURCE_BODY\nconst token = \"{token}\";\n"),
    );
    let inner = json!({"path":file,"reasoning":"Read exact bounded evidence"});
    let ordinary = runtime
        .execute("ordinary".into(), "localFetch".into(), inner.clone())
        .await
        .unwrap();
    let state = ordinary.structured_content;
    assert!(!state.to_string().contains(&token));
    assert!(state.to_string().contains("REDACTED"));
    let criteria = json!([{"path":file},{"path":"/other/rubric/value"}]);
    let question =
        json!({"type":"score","instructions":"Assess supplied evidence","criteria":criteria});
    Mock::given(method("POST")).and(body_json(json!({"model":"jev-test","state":state,"questions":{"answer":question}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":"HIDDEN_SOURCE_BODY","answers":{"answer":{"type":"score","score":0.75,"confidence":0.8,"probabilities":{"0":0.25,"1":0.75},"legend":{"0":criteria[0],"1":criteria[1]},"content":"HIDDEN_SOURCE_BODY"}},"usage":{"input_tokens":20,"output_tokens":3}})))
        .expect(1).mount(&server).await;
    let out = runtime
        .execute(
            "hidden".into(),
            "jev".into(),
            json!({"reasoning":"Decide the next evidence read.","context":{"tool":"localFetch","query":inner},"question":question}),
        )
        .await
        .unwrap();
    assert_eq!(row_status(&out), "success", "{}", out.structured_content);
    assert_eq!(row_data(&out)["answer"]["legend"]["0"], criteria[0]);
    assert_eq!(row_data(&out)["context"]["tool"], "localFetch");
    assert_eq!(row_data(&out)["context"]["coverage"], "bounded");
    assert_eq!(
        row_data(&out)["context"]["resultHash"],
        hex::encode(Sha256::digest(state.to_string().as_bytes()))
    );
    assert!(
        !out.structured_content
            .to_string()
            .contains("HIDDEN_SOURCE_BODY")
    );
    assert!(!out.structured_content.to_string().contains(&token));
    runtime.close().await;
}

#[tokio::test]
async fn partial_context_returns_executable_continuation_without_automatic_paging() {
    let workspace = Workspace::new();
    let file = workspace.write("source.rs", "first\nsecond\nthird\n");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    let runtime = workspace.runtime(&settings(&server));
    let out = runtime
        .execute(
            "partial".into(),
            "jev".into(),
            hidden(
                "localFetch",
                json!({"path":file,"reasoning":"Read first page","debug":true,"limit":1}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(row_status(&out), "success", "{}", out.structured_content);
    let receipt = &row_data(&out)["context"];
    assert_eq!(receipt["coverage"], "partial");
    let continuation = &receipt["next"]["continue"];
    assert!(continuation.get("cursor").is_none());
    assert_eq!(continuation["query"]["reasoning"], "Read first page");
    assert_eq!(continuation["query"]["debug"], true);
    let next = runtime
        .execute(
            "next".into(),
            continuation["tool"].as_str().unwrap().into(),
            continuation["query"].clone(),
        )
        .await
        .unwrap();
    assert_eq!(row_data(&next)["content"], "second\n");
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["state"]["results"][0]["data"]["content"], "first\n");
    runtime.close().await;
}

#[tokio::test]
async fn shared_state_batches_reduce_posts_and_attribute_usage_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            let answers: serde_json::Map<String, Value> = request["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|id| (id.clone(), json!({"type":"noul","noul":0.8})))
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({"model":"jev-test","answers":answers,
            "usage":{"input_tokens":100,"output_tokens":3}}))
        })
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    for (name, states, expected_posts) in [
        (
            "same",
            vec![
                json!({"a":1,"b":2}),
                json!({"b":2,"a":1}),
                json!({"a":1,"b":2}),
            ],
            1,
        ),
        (
            "distinct",
            vec![json!("first"), json!("second"), json!("third")],
            3,
        ),
    ] {
        let before = server.received_requests().await.unwrap().len();
        let queries: Vec<_> = states
            .into_iter()
            .enumerate()
            .map(|(index, state)| json!({"reasoning":format!("TRACE_ONLY_RATIONALE_{index}"),"context":{"value":state},"question":question()}))
            .collect();
        let out = runtime
            .execute(name.into(), "jev".into(), json!({"queries":queries}))
            .await
            .unwrap();
        let rows = out.structured_content["results"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            server.received_requests().await.unwrap().len() - before,
            expected_posts
        );
        let tokens: u64 = rows
            .iter()
            .map(|row| row["data"]["usage"]["input_tokens"].as_u64().unwrap())
            .sum();
        assert_eq!(tokens, 100 * expected_posts as u64);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["index"], index);
            assert_eq!(row["data"]["answer"]["noul"], 0.8);
            if name == "same" {
                assert_eq!(
                    row["data"]["usageAttribution"],
                    json!({"ownerIndex":0,"sharedWith":[0,1,2]})
                );
            } else {
                assert!(row["data"].get("usageAttribution").is_none());
            }
        }
    }
    for request in server.received_requests().await.unwrap() {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert!(body.get("reasoning").is_none());
        assert!(!body.to_string().contains("TRACE_ONLY_RATIONALE"));
    }
    runtime.close().await;
}

#[tokio::test]
async fn shared_state_isolates_context_and_answer_failures_in_original_order() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(request["questions"].as_object().unwrap().len(), 2);
            assert!(request["questions"].get("answer_1").is_none());
            ResponseTemplate::new(200).set_body_json(json!({"model":"jev-test",
            "answers":{"answer_0":{"type":"noul","noul":2},"answer_2":{"type":"noul","noul":0.7}},
            "usage":{"input_tokens":19,"output_tokens":4}}))
        })
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let missing = hidden(
        "localFetch",
        json!({"path":workspace.workspace.join("missing.rs"),"reasoning":"Read"}),
    );
    let out = runtime
        .execute(
            "mixed".into(),
            "jev".into(),
            json!({"queries":[query(),missing,query()]}),
        )
        .await
        .unwrap();
    let rows = out.structured_content["results"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "error");
    assert_eq!(rows[0]["data"]["errorCode"], "invalidJevResponse");
    assert_eq!(rows[1]["status"], "error");
    assert_eq!(rows[2]["data"]["answer"]["noul"], 0.7);
    assert_eq!(
        rows[2]["data"]["usage"],
        json!({"input_tokens":19,"output_tokens":4})
    );
    assert_eq!(
        rows[2]["data"]["usageAttribution"],
        json!({"ownerIndex":2,"sharedWith":[0,2]})
    );
    runtime.close().await;
}

#[tokio::test]
async fn batching_headroom_keeps_oversize_groups_as_singletons() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(request["questions"].as_object().unwrap().len(), 1);
            assert!(request["questions"].get("answer").is_some());
            ResponseTemplate::new(200).set_body_json(response())
        })
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let query = json!({"reasoning":"Decide the next evidence read.","context":{"value":{"chunks":vec!["x".repeat(8000);4]}},"question":question()});
    let out = runtime
        .execute(
            "headroom".into(),
            "jev".into(),
            json!({"queries":[query,query]}),
        )
        .await
        .unwrap();
    for row in out.structured_content["results"].as_array().unwrap() {
        assert!(
            row.get("status").is_none() || row["status"] != "error",
            "{row}"
        );
        assert!(row["data"].get("usageAttribution").is_none());
        assert_eq!(row["data"]["usage"]["input_tokens"], 100);
    }
    runtime.close().await;
}

#[tokio::test]
async fn repeated_tool_contexts_capture_fresh_results_before_grouping() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let server = MockServer::start().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = reads.clone();
    let revisions = AtomicUsize::new(1);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(move |_: &Request| {
            let revision = revisions.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({"sha":format!("{revision:040x}")}))
        })
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/source.rs"))
        .respond_with(move |_: &Request| {
            let revision = counter.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64",
                "content":STANDARD.encode(format!("captured revision {revision}\n"))}))
        })
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(request["questions"].as_object().unwrap().len(), 1);
            ResponseTemplate::new(200).set_body_json(response())
        })
        .expect(3)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut config = settings(&server);
    config.push(("GITHUB_API_URL", format!("{}/api/v3", server.uri())));
    let runtime = workspace.runtime(&config);
    let query = hidden(
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"source.rs","branch":"main","reasoning":"Capture current evidence"}),
    );
    let out = runtime
        .execute(
            "fresh".into(),
            "jev".into(),
            json!({"queries":[query,query,query]}),
        )
        .await
        .unwrap();
    assert_eq!(
        reads.load(Ordering::SeqCst),
        3,
        "{}",
        out.structured_content
    );
    let rows = out.structured_content["results"].as_array().unwrap();
    assert!(rows.iter().all(|row| row["data"]["answer"]["noul"] == 0.81));
    assert_ne!(
        rows[0]["data"]["context"]["resultHash"],
        rows[1]["data"]["context"]["resultHash"]
    );
    assert_ne!(
        rows[1]["data"]["context"]["resultHash"],
        rows[2]["data"]["context"]["resultHash"]
    );
    let requests = server.received_requests().await.unwrap();
    let routes: Vec<_> = requests
        .iter()
        .map(|request| (request.method.as_str(), request.url.path()))
        .collect();
    let mut expected = Vec::new();
    for _ in 0..3 {
        expected.extend([
            ("GET", "/api/v3/repos/a/b/commits/main"),
            ("GET", "/api/v3/repos/a/b/contents/source.rs"),
            ("GET", "/api/v3/repos/a/b/commits"),
        ]);
    }
    expected.extend([("POST", "/v1/systemone"); 3]);
    assert_eq!(routes, expected);
    runtime.close().await;
}

#[tokio::test]
async fn independent_provider_groups_overlap_and_preserve_ordered_failures() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    let server = MockServer::start().await;
    let started = Arc::new(AtomicUsize::new(0));
    let observed = started.clone();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            let index = request["state"]["candidate"].as_u64().unwrap();
            assert_eq!(request["questions"].as_object().unwrap().len(), 1);
            observed.fetch_add(1, Ordering::SeqCst);
            // Later rows finish first; one malformed answer must stay isolated.
            let probability = if index == 2 { 2.0 } else { (index + 1) as f64 / 10.0 };
            ResponseTemplate::new(200)
                .set_body_json(json!({"model":"jev-test","answers":{"answer":{"type":"noul","noul":probability}},
                    "usage":{"input_tokens":index + 10,"output_tokens":1}}))
                .set_delay(Duration::from_millis(5000 - index * 200))
        })
        .expect(5)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let queries: Vec<_> = (0..5).map(|index|
        json!({"reasoning":"Assess this candidate.","context":{"value":{"candidate":index}},"question":question()})
    ).collect();
    let execution = runtime.execute(
        "concurrent-groups".into(),
        "jev".into(),
        json!({"queries":queries}),
    );
    tokio::pin!(execution);
    // This tests admission overlap, not a provider latency or throughput claim.
    let all_started = async {
        // Admission includes synchronous contract/security preparation. Start the
        // overlap clock at the first HTTP request, not before that preparation.
        while started.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while started.load(Ordering::SeqCst) != 5 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
    };
    tokio::select! {
        result = &mut execution => panic!("execution finished before all groups started; error: {:?}", result.err()),
        result = all_started => result.expect("all five independent requests must start before the first delayed response"),
    }
    let output = execution.await.unwrap();
    let rows = output.structured_content["results"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["index"], index);
        if index == 2 {
            assert_eq!(row["status"], "error", "{row}");
        } else {
            assert_eq!(row["data"]["answer"]["noul"], (index + 1) as f64 / 10.0);
            assert_eq!(row["data"]["usage"]["input_tokens"], index + 10);
            assert!(row["data"].get("usageAttribution").is_none());
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn cancellation_preserves_usage_from_completed_provider_groups() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    let server = MockServer::start().await;
    let workspace = Workspace::new();
    let mut config = settings(&server);
    config.push(("OCTOCODE_ENABLE_STATS", "true".into()));
    config.push(("OCTOCODE_STORAGE_MODE", "persistent".into()));
    let runtime = Arc::new(workspace.runtime(&config));
    let pending_started = Arc::new(AtomicBool::new(false));
    let observed = pending_started.clone();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let request: Value = serde_json::from_slice(&request.body).unwrap();
            if request["state"] == "pending" {
                assert_eq!(request["questions"].as_object().unwrap().len(), 1);
                observed.store(true, Ordering::SeqCst);
                return ResponseTemplate::new(200)
                    .set_body_json(response())
                    .set_delay(Duration::from_secs(30));
            }
            assert_eq!(request["state"], "complete");
            assert_eq!(request["questions"].as_object().unwrap().len(), 2);
            let answers: serde_json::Map<String, Value> = request["questions"]
                .as_object()
                .unwrap()
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        json!({"type":"noul","noul":if id == "answer_0" {2.0} else {0.8}}),
                    )
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({"model":"jev-test","answers":answers,
                "usage":{"input_tokens":37,"output_tokens":5}}))
        })
        .expect(2)
        .mount(&server)
        .await;
    let queries: Vec<_> = ["complete", "complete", "pending"]
        .into_iter()
        .map(|state| json!({"reasoning":"Decide the next evidence read.","context":{"value":state},"question":question()}))
        .collect();
    let execution = runtime.execute(
        "cancel-after-group".into(),
        "jev".into(),
        json!({"queries":queries}),
    );
    tokio::pin!(execution);
    let completed_usage = async {
        // Exclude synchronous admission setup from the accounting deadline.
        while !pending_started.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let stats = std::fs::read_to_string(workspace.home.join("stats.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok());
                if pending_started.load(Ordering::SeqCst)
                    && stats
                        .as_ref()
                        .is_some_and(|stats| stats["stats"]["jev"]["input_tokens"] == 37)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
    };
    tokio::select! {
        result = &mut execution => panic!("execution finished before cancellation; error: {:?}", result.err()),
        result = completed_usage => result.expect("completed group usage must be recorded while another request is pending"),
    }
    assert!(runtime.requests.cancel("cancel-after-group"));
    let outcome = tokio::time::timeout(Duration::from_secs(3), execution)
        .await
        .expect("cancellation must stop the pending provider request");
    assert_eq!(outcome.unwrap_err().code, "cancelled");
    runtime.close().await;
    let stats: Value =
        serde_json::from_str(&std::fs::read_to_string(workspace.home.join("stats.json")).unwrap())
            .unwrap();
    assert_eq!(
        stats["stats"]["jev"],
        json!({"calls":1,"input_tokens":37,"output_tokens":5})
    );
}

#[tokio::test]
async fn local_search_and_ast_files_reuse_the_same_hidden_dispatch() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("source.rs", "pub fn hidden_function() {}\n");
    let runtime = workspace.runtime(&settings(&server));
    for (tool, query) in [
        (
            "localSearch",
            json!({"path":workspace.workspace,"searchText":"hidden_function","reasoning":"Find definition"}),
        ),
        (
            "astSearch",
            json!({"path":workspace.workspace,"operation":"files","reasoning":"Discover source files"}),
        ),
    ] {
        let out = runtime
            .execute(tool.into(), "jev".into(), hidden(tool, query))
            .await
            .unwrap();
        assert_eq!(row_status(&out), "success", "{}", out.structured_content);
        assert_eq!(row_data(&out)["context"]["tool"], tool);
        assert!(
            !out.structured_content
                .to_string()
                .contains("hidden_function")
        );
    }
    let requests = server.received_requests().await.unwrap();
    assert!(String::from_utf8_lossy(&requests[0].body).contains("hidden_function"));
    runtime.close().await;
}

#[tokio::test]
async fn github_read_context_uses_ordinary_security_and_shared_cache() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/source.rs"))
        .respond_with(|request:&Request| if request.headers.get("if-none-match").is_some(){ResponseTemplate::new(304)}else{
            ResponseTemplate::new(200).insert_header("etag","\"v1\"").set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode("REMOTE_HIDDEN_BODY developer@example.com\n")}))
        }).expect(2).mount(&server).await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut config = settings(&server);
    config.push(("GITHUB_API_URL", format!("{}/api/v3", server.uri())));
    let runtime = workspace.runtime(&config);
    let inner = json!({"owner":"a","repo":"b","path":"source.rs","branch":"a".repeat(40),"reasoning":"Read remote snapshot"});
    for i in 0..2 {
        let out = runtime
            .execute(
                format!("remote-{i}"),
                "jev".into(),
                hidden("ghGetFileContent", inner.clone()),
            )
            .await
            .unwrap();
        assert_eq!(row_status(&out), "success", "{}", out.structured_content);
        assert!(
            !out.structured_content
                .to_string()
                .contains("REMOTE_HIDDEN_BODY")
        );
    }
    let requests = server.received_requests().await.unwrap();
    let content: Vec<_> = requests
        .iter()
        .filter(|r| r.url.path().contains("/contents/"))
        .collect();
    assert!(content[1].headers.get("if-none-match").is_some());
    let posts: Vec<_> = requests
        .iter()
        .filter(|r| r.method.as_str() == "POST")
        .collect();
    assert!(String::from_utf8_lossy(&posts[0].body).contains("REMOTE_HIDDEN_BODY"));
    assert!(String::from_utf8_lossy(&posts[0].body).contains("developer@example.com"));
    runtime.close().await;
}

#[tokio::test]
async fn github_hidden_context_matches_ordinary_email_redaction_policy() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/contact.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file",
            "encoding":"base64",
            "content":STANDARD.encode("contact developer@example.com\n")
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response()))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut config = settings(&server);
    config.push(("GITHUB_API_URL", format!("{}/api/v3", server.uri())));
    config.push(("OCTOCODE_REDACT_EMAILS", "true".into()));
    let runtime = workspace.runtime(&config);
    let inner = json!({
        "owner":"a",
        "repo":"b",
        "path":"contact.txt",
        "branch":"a".repeat(40),
        "reasoning":"Read contact"
    });
    let ordinary = runtime
        .execute(
            "ordinary-email-redaction".into(),
            "ghGetFileContent".into(),
            inner.clone(),
        )
        .await
        .unwrap();
    assert!(
        !ordinary
            .structured_content
            .to_string()
            .contains("developer@example.com")
    );
    assert!(
        ordinary
            .structured_content
            .to_string()
            .contains("[REDACTED-EMAIL]")
    );
    let hidden = runtime
        .execute(
            "hidden-email-redaction".into(),
            "jev".into(),
            hidden("ghGetFileContent", inner),
        )
        .await
        .unwrap();
    assert_eq!(
        row_status(&hidden),
        "success",
        "{}",
        hidden.structured_content
    );
    let requests = server.received_requests().await.unwrap();
    let post = requests
        .iter()
        .find(|request| request.method.as_str() == "POST")
        .expect("provider request");
    let payload: Value = serde_json::from_slice(&post.body).expect("provider payload");
    let state = payload["state"].to_string();
    assert!(!state.contains("developer@example.com"));
    assert!(state.contains("[REDACTED-EMAIL]"));
    runtime.close().await;
}
