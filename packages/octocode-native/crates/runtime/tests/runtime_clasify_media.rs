//! Original image bytes stay inside the runtime/provider boundary.

use crate::support::{Workspace, capture_allowed, clasify_runtime};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

mod bounds;
mod github_and_models;
mod local_images;

fn png() -> Vec<u8> {
    STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAARgAAAEYCAAAAACi5bZQAAABXElEQVR42u3QAQ0AAAwCIPuX1gJP8EEEUk5RIEaMGDFixIgRI0aMGMSIESNGjBgxYsSIEYMYMWLEiBEjRowYMWIQI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIEYMYMWLEiBEjRowYMWIQI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRI0YMYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsSIQYwYMWLEiBEjRowYMYgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsQgRowYMWLEiBEjRowYxIgRI0aMGDFixIgRIwYxYsSIESNGjBgxYsQgRowYMWLEiBEjRowYxIgRI0aMGDFixIgRgxgxYsSIESNGjBgxYsQoECNGjBgxYsSIESNGDGLEiBEjRowYMWK+G5X/H6AJTpTJAAAAAElFTkSuQmCC").unwrap()
}

#[derive(Clone)]
struct Answer;
impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let answers: serde_json::Map<String, Value> = body["questions"].as_object().unwrap().iter()
            .map(|(key, question)| (key.clone(), if question["type"] == "box" {
                json!({"type":"box","box":{"top":0.1,"bottom":0.9,"left":0.2,"right":0.8},"confidence":null})
            } else { json!({"type":"noul","noul":0.9}) })).collect();
        ResponseTemplate::new(200).set_body_json(json!({"model":"jev-preview","answers":answers,
            "usage":{"input_tokens":100,"output_tokens":10}}))
    }
}

async fn provider() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(Answer)
        .mount(&server)
        .await;
    server
}

/// A clasify runtime that may also read what URL capture saved.
fn runtime(workspace: &Workspace, server: &MockServer, extra: &[(&str, String)]) -> ToolRuntime {
    let mut env = vec![capture_allowed(workspace)];
    env.extend(extra.iter().cloned());
    clasify_runtime(workspace, server, &env)
}

fn row(path: &str) -> Value {
    json!({"state":"image_a","assets":{"image_a":{"type":"image","source":{"tool":"localFetch","query":{"path":path}}}},
        "questions":{"visible":{"type":"noul","instructions":"Does image_a contain visible content?"},
        "bounds":{"type":"box","instructions":"Locate the object in image_a."}}})
}

async fn requests(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap()
}
