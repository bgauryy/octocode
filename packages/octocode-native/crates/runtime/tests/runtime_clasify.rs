//! Public clasify end to end: JEV questions over a literal state or a source
//! captured through the authorized readers, answered by a mock provider.

use crate::support::Run as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

mod across_rows;
mod failures;
mod gating;
pub(crate) mod github_withheld;
mod hits;
mod judgments;
mod long_context;
mod oversized;
mod pages;
mod readiness;
mod resources;
mod sanitization;
mod sources;
mod stubbed_rows;
mod units;
mod views;

/// Answers every question by its own type, as the provider does: Noul 0.9,
/// Choice the first label, Score 1.5 over its levels with an echoed legend.
#[derive(Clone)]
struct Typed;

impl Respond for Typed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(Self::reply(request))
    }
}

impl Typed {
    /// The provider's answer body for `request`.
    fn reply(request: &Request) -> Value {
        let body: Value = serde_json::from_slice(&request.body).expect("provider body");
        let answers = body["questions"]
            .as_object()
            .expect("questions")
            .iter()
            .map(|(id, question)| {
                let answer = match question["type"].as_str() {
                    Some("choice") => {
                        let labels = question["criteria"]
                            .as_object()
                            .expect("labels")
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>();
                        let probabilities = labels
                            .iter()
                            .enumerate()
                            .map(|(index, label)| {
                                let share = if index == 0 { 1.0 } else { 0.0 };
                                (label.clone(), json!(share))
                            })
                            .collect::<serde_json::Map<_, _>>();
                        json!({"type":"choice","choice":labels[0],"confidence":1,"probabilities":probabilities})
                    }
                    Some("score") => {
                        let levels = question["criteria"].as_array().expect("levels");
                        let legend = levels
                            .iter()
                            .enumerate()
                            .map(|(index, level)| (index.to_string(), level.clone()))
                            .collect::<serde_json::Map<_, _>>();
                        let mut probabilities = serde_json::Map::new();
                        for index in 0..levels.len() {
                            let share = match index {
                                1 | 2 => 0.5,
                                _ => 0.0,
                            };
                            probabilities.insert(index.to_string(), json!(share));
                        }
                        json!({"type":"score","score":1.5,"confidence":0.5,"probabilities":probabilities,"legend":legend})
                    }
                    _ => json!({"type":"noul","noul":0.9}),
                };
                (id.clone(), answer)
            })
            .collect::<serde_json::Map<_, _>>();
        json!({
            "model":"jev-1.13.0",
            "answers":answers,
            "usage":{"input_tokens":40,"output_tokens":7}
        })
    }
}

async fn provider(expected: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Typed)
        .expect(expected)
        .mount(&server)
        .await;
    server
}

/// Provider request bodies, in arrival order.
pub(crate) async fn sent(server: &MockServer) -> Vec<Value> {
    let requests = server
        .received_requests()
        .await
        .expect("recorded provider requests");
    let envelopes = requests
        .iter()
        .map(|request| {
            json!({
                "method": request.method.as_str(),
                "url": request.url.as_str(),
                "bodyBytes": request.body.len(),
                "userAgent": request.headers.get("user-agent").and_then(|value| value.to_str().ok()),
                "contentType": request.headers.get("content-type").and_then(|value| value.to_str().ok()),
                "contentLength": request.headers.get("content-length").and_then(|value| value.to_str().ok()),
                "origin": request.headers.get("origin").and_then(|value| value.to_str().ok()),
                "referer": request.headers.get("referer").and_then(|value| value.to_str().ok()),
            })
        })
        .collect::<Vec<_>>();
    requests
        .iter()
        .enumerate()
        .map(|(index, request)| {
            assert!(
                request.method == "POST" && request.url.path() == "/v1/systemone",
                "unexpected provider request #{index}; recorded envelopes: {envelopes:#?}"
            );
            serde_json::from_slice(&request.body).unwrap_or_else(|error| {
                panic!(
                    "invalid provider JSON in request #{index}: {error}; recorded envelopes: {envelopes:#?}"
                )
            })
        })
        .collect()
}

fn noul(instructions: &str) -> Value {
    json!({"type":"noul","instructions":instructions})
}

fn content_row() -> Value {
    json!({
        "operation":"evaluate",
        "content":{"document":{"text":"Update the configuration, then restart the service."}},
        "questions":{"restart":noul("Does `document.text` require a restart?")}
    })
}

/// Run a classification batch and validate its structured result.
pub(crate) async fn clasify(
    runtime: &octocode_native::runtime::ToolRuntime,
    rows: Vec<Value>,
) -> Value {
    runtime
        .run(
            "clasify-test".into(),
            "clasify".into(),
            json!({"queries":rows}),
        )
        .await
        .expect("clasify result")
        .structured_content
        .into_value()
}

/// `security.credentialFiles: allow` opens credential files to local reads, not
/// to a provider: the row a clasify source captures goes through the egress
/// policy, so nothing is sent.
#[tokio::test]
async fn a_clasify_source_never_reads_an_allowed_credential_file() {
    let workspace = crate::support::Workspace::new();
    workspace.write(".env", "PLAIN=plain_value_kept\n");
    let server = provider(0).await;
    let runtime = crate::support::clasify_runtime(
        &workspace,
        &server,
        &[("OCTOCODE_CREDENTIAL_FILES", "allow".to_owned())],
    );
    let result = clasify(
        &runtime,
        vec![json!({
            "operation":"evaluate",
            "source":{"tool":"localFetch","query":{"path":".env"}},
            "questions":{"a":noul("Is this set?")}
        })],
    )
    .await;
    assert_eq!(
        result["results"][0]["data"]["errorCode"], "pathPolicyDenied",
        "{result}"
    );
    assert!(sent(&server).await.is_empty());
}
