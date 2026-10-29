//! clasify `operation: sections`: how each kind of named resource is
//! acquired. Every failure is that resource's own row, found before any
//! provider call; a sibling keeps its sections.

use crate::runtime_clasify::{clasify, sent};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod concurrency;
mod resources;
mod resume;

/// Answers every question Noul 0.9.
async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).expect("provider body");
            let answers = body["questions"]
                .as_object()
                .expect("questions")
                .keys()
                .map(|id| (id.clone(), json!({"type":"noul","noul":0.9})))
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200).set_body_json(json!({
                "model":"jev-1.13.0","answers":answers,
                "usage":{"input_tokens":10,"output_tokens":2}
            }))
        })
        .mount(&server)
        .await;
    server
}

async fn sections(runtime: &ToolRuntime, resources: Value) -> Value {
    let out = clasify(
        runtime,
        vec![json!({
            "operation":"sections","resources":resources,
            "questions":{"q":{"type":"noul","instructions":"Does this section say to restart?"}}
        })],
    )
    .await;
    out["results"][0]["data"].clone()
}

async fn provider_calls(server: &MockServer) -> usize {
    sent(server).await.len()
}

/// What the provider was asked to judge, one request body per call.
async fn judged(server: &MockServer) -> String {
    sent(server)
        .await
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn error_code(resource: &Value) -> &Value {
    &resource["error"]["errorCode"]
}

/// `count` functions, each over half a section page, so grouping keeps one
/// section per function.
pub(crate) fn separate_functions(count: usize) -> String {
    let padding = "x".repeat(4_100);
    (0..count)
        .map(|i| {
            format!(
                "pub fn section_{i}() {{\n    // {padding}\n    println!(\"restart {i}\");\n}}\n"
            )
        })
        .collect()
}
