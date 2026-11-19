//! A classification runtime over a mocked provider that refuses every
//! request in the default window and answers it, with direct evidence, in
//! the long-context window: what each reader's row says of a retry.
#![cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "fixtures for tests, compiled only for them (`test`, `test-support`): they build processes, files and timings on purpose"
    )
)]
#![expect(
    clippy::expect_used,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
use crate::judgment::Evaluator;
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// The input tokens each long-window answer bills.
pub const BILLED: u64 = 4_321;

/// The provider, its executor and the selection runtime judging through it.
pub struct LongWindow {
    pub executor: tokio::runtime::Runtime,
    pub server: MockServer,
    pub selection: crate::selection::Runtime,
}

impl LongWindow {
    pub fn start() -> Self {
        Self::start_answering(refuse_then_answer)
    }

    /// As [`Self::start`], but the long-window answers carry no `usage`.
    pub fn start_without_usage() -> Self {
        Self::start_answering(refuse_then_answer_without_usage)
    }

    fn start_answering(respond: fn(&Request) -> ResponseTemplate) -> Self {
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let server = executor.block_on(MockServer::start());
        executor.block_on(
            Mock::given(method("POST"))
                .respond_with(respond)
                .mount(&server),
        );
        let selection = crate::test_support::selection_fake::runtime(
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
    pub fn long_requests(&self) -> usize {
        self.executor
            .block_on(self.server.received_requests())
            .expect("requests")
            .iter()
            .filter(|request| request.headers.contains_key(LONG))
            .count()
    }
}

/// A retry-free evaluator against `server`, with a credential and a model
/// unique in the process ([`crate::judgment::test_identity`]): the refusals
/// this fake teaches are kept per endpoint and model, the gate's memos per
/// endpoint and credential, and a mock server's port is reused.
pub fn evaluator(server: &MockServer) -> Evaluator {
    Evaluator::new(
        Some(secrecy::SecretString::from(
            crate::judgment::test_identity::unique("test-key"),
        )),
        server.uri(),
        crate::judgment::test_identity::unique("jev-test"),
        &octocode_runtime_base::providers::classification::jev::JEV,
        Some(Duration::from_secs(10)),
        0,
        4,
    )
}

const LONG: &str = "typesafe-danger-zone-long-context";

/// Refuses as too large without the long-context header; with it, answers
/// every question at its top level: `noul` yes, a score its last level.
pub(crate) fn refuse_then_answer(request: &Request) -> ResponseTemplate {
    answer(request, true)
}

/// [`refuse_then_answer`] whose answers report no token usage.
pub(crate) fn refuse_then_answer_without_usage(request: &Request) -> ResponseTemplate {
    answer(request, false)
}

fn answer(request: &Request, usage: bool) -> ResponseTemplate {
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
    let mut body = json!({"model":"jev-resolved","answers":answers});
    if usage {
        body["usage"] = json!({"input_tokens":BILLED,"output_tokens":2});
    }
    ResponseTemplate::new(200).set_body_json(body)
}
