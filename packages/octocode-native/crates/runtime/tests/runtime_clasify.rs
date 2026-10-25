//! Public clasify end to end: JEV questions over a literal state or a source
//! captured through the authorized readers, answered by a mock provider.

use crate::support::Run as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

mod failures;
mod gating;
mod judgments;
mod readiness;
mod sources;

/// Answers every question by its own type, as the provider does: Noul 0.9,
/// Choice the first label, Score 1.5 over its levels with an echoed legend.
#[derive(Clone)]
struct Typed;

impl Respond for Typed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
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
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"jev-1.13.0",
            "answers":answers,
            "usage":{"input_tokens":40,"output_tokens":7}
        }))
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
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| serde_json::from_slice(&request.body).unwrap())
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
