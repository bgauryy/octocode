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
    json!({"context":{"value":{"observations":["A cancellation guard precedes the write."]}},"question":question()})
}
fn hidden(tool: &str, query: Value) -> Value {
    json!({"context":{"tool":tool,"query":query},"question":question()})
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
        json!({"context":{"value":null},"questions":{"q":question()}}),
        json!({"context":{"value":null},"question":[question(),question()]}),
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
        "reasoning",
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
            json!({"context":{"value":large},"question":question()}),
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
            json!({"context":{"tool":"localFetch","query":inner},"question":question}),
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
                json!({"path":file,"reasoning":"Read first page","limit":1}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(row_status(&out), "success", "{}", out.structured_content);
    let receipt = &row_data(&out)["context"];
    assert_eq!(receipt["coverage"], "partial");
    let continuation = &receipt["next"]["continue"];
    assert!(continuation.get("cursor").is_none());
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
async fn batch_rows_repeat_context_and_execute_independently_in_order() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(|request:&Request| {
        let request:Value=serde_json::from_slice(&request.body).unwrap();
        let probability=if request["state"]=="first" {0.2}else{0.8};
        ResponseTemplate::new(200).set_body_json(json!({"model":"jev-test","answers":{"answer":{"type":"noul","noul":probability}},"usage":{"input_tokens":1,"output_tokens":1}}))
    }).expect(3).mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&settings(&server));
    let rows: Vec<Value> = ["first", "second", "second"]
        .into_iter()
        .map(|value| json!({"context":{"value":value},"question":question()}))
        .collect();
    let out = runtime
        .execute("batch".into(), "jev".into(), json!({"queries":rows}))
        .await
        .unwrap();
    let rows = out.structured_content["results"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for (i, probability) in [0.2, 0.8, 0.8].into_iter().enumerate() {
        assert_eq!(rows[i]["index"], i);
        assert_eq!(rows[i]["data"]["answer"]["noul"], probability);
    }
    let requests = server.received_requests().await.unwrap();
    let states: Vec<Value> = requests
        .iter()
        .map(|r| serde_json::from_slice::<Value>(&r.body).unwrap()["state"].clone())
        .collect();
    assert_eq!(
        states,
        vec![json!("first"), json!("second"), json!("second")]
    );
    runtime.close().await;
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
            ResponseTemplate::new(200).insert_header("etag","\"v1\"").set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode("REMOTE_HIDDEN_BODY\n")}))
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
    runtime.close().await;
}
