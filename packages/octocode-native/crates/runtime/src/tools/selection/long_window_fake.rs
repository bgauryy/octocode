//! A classification runtime over a mocked provider that refuses every
//! request in the default window and answers it, with direct evidence, in
//! the long-context window: what each reader's row says of a retry.
use crate::tools::judgment::Evaluator;
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// The input tokens each long-window answer bills.
pub(crate) const BILLED: u64 = 4_321;

/// The provider, its executor and the selection runtime judging through it.
pub(crate) struct LongWindow {
    pub(crate) executor: tokio::runtime::Runtime,
    pub(crate) server: MockServer,
    pub(crate) selection: crate::tools::selection::Runtime,
}

impl LongWindow {
    pub(crate) fn start() -> Self {
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let server = executor.block_on(MockServer::start());
        executor.block_on(
            Mock::given(method("POST"))
                .respond_with(refuse_then_answer)
                .mount(&server),
        );
        let selection = crate::tools::test_support::selection_fake::runtime(
            evaluator(&server),
            executor.handle().clone(),
            Duration::from_secs(20),
        );
        Self {
            executor,
            server,
            selection,
        }
    }

    /// Requests the provider received in the long-context window.
    pub(crate) fn long_requests(&self) -> usize {
        self.executor
            .block_on(self.server.received_requests())
            .expect("requests")
            .iter()
            .filter(|request| request.headers.contains_key(LONG))
            .count()
    }
}

/// A retry-free evaluator against `server`.
pub(crate) fn evaluator(server: &MockServer) -> Evaluator {
    Evaluator::new(
        Some(secrecy::SecretString::from("test-key".to_owned())),
        server.uri(),
        "jev-test".into(),
        &crate::providers::classification::jev::JEV,
        Some(Duration::from_secs(10)),
        0,
        4,
    )
}

const LONG: &str = "typesafe-danger-zone-long-context";

/// Refuses as too large without the long-context header; with it, answers
/// every question at its top level: `noul` yes, a score its last level.
pub(crate) fn refuse_then_answer(request: &Request) -> ResponseTemplate {
    if !request.headers.contains_key(LONG) {
        return ResponseTemplate::new(400)
            .set_body_json(json!({"detail":{"error_type":"max_tokens_exceeded"}}));
    }
    let body: Value = request.body_json().expect("request");
    let answers = body["questions"]
        .as_object()
        .expect("questions")
        .iter()
        .map(|(id, question)| {
            let answer = match question["criteria"].as_array() {
                Some(levels) if question["type"] == "score" => {
                    let top = levels.len() - 1;
                    let legend = levels
                        .iter()
                        .enumerate()
                        .map(|(i, level)| (i.to_string(), level.clone()))
                        .collect::<serde_json::Map<String, Value>>();
                    let probabilities = (0..levels.len())
                        .map(|i| (i.to_string(), json!(if i == top { 1.0 } else { 0.0 })))
                        .collect::<serde_json::Map<String, Value>>();
                    json!({"type":"score","score":top as f64,"confidence":1.0,
                        "legend":legend,"probabilities":probabilities})
                }
                _ => json!({"type":"noul","noul":0.9}),
            };
            (id.clone(), answer)
        })
        .collect::<serde_json::Map<String, Value>>();
    ResponseTemplate::new(200).set_body_json(json!({
        "model":"jev-resolved","answers":answers,
        "usage":{"input_tokens":BILLED,"output_tokens":2}
    }))
}
