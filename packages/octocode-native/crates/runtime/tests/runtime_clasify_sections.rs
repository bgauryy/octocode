//! clasify `operation: sections`: how each kind of named resource is
//! acquired. Every failure is that resource's own row, found before any
//! provider call; a sibling keeps its sections.

use crate::runtime_clasify::{clasify, sent};
use octocode_native::runtime::ToolRuntime;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod concurrency;
mod resources;
mod resume;
mod retention;

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
                .map(|id| (id.clone(), json_fixture(r#"{"type": "noul", "noul": 0.9}"#)))
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

/// A finished plan has no continuation that judges more: whatever `next`
/// holds is a `read`/`readN` of the judged lines, an executable localFetch or
/// ghGetFileContent continuation (a pinned cursor), never a page; or
/// `quiet`, the unbilled replay of the sections it held back.
pub(crate) fn assert_next_is_only_pinned_reads(out: &Value) {
    let Some(next) = out.get("next") else {
        return;
    };
    for (key, call) in next.as_object().expect("next is an object") {
        if key == "quiet" {
            assert_eq!(call["tool"], "clasify", "{call}");
            continue;
        }
        let index = key
            .strip_prefix("read")
            .unwrap_or_else(|| panic!("`next.{key}` is a continuation, not a read: {out:#}"));
        assert!(
            index.bytes().all(|byte| byte.is_ascii_digit()),
            "`next.{key}` is not read/readN: {out:#}"
        );
        assert!(
            matches!(
                call["tool"].as_str(),
                Some("localFetch" | "ghGetFileContent")
            ),
            "`next.{key}` is not a source read: {call}"
        );
        assert_eq!(call.get("kind"), None, "{call}");
        assert!(call["input"]["cursor"].is_string(), "pinned cursor: {call}");
    }
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
