//! A mocked provider that fails every request whose state names `FAIL` with
//! an HTTP 500 (a transient failure the runtime retries) until it is healed,
//! and answers every other question yes (`noul` 0.9): what a row held to its
//! ceiling does when its shared transient retries run out, and what its
//! retry asks again.
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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

/// The marker of a state the provider fails until healed.
pub const FAIL: &str = "FAIL";

/// The provider and its executor.
pub struct Transient {
    pub executor: tokio::runtime::Runtime,
    pub server: MockServer,
    healed: Arc<AtomicBool>,
}

impl Transient {
    pub fn start() -> Self {
        let executor = tokio::runtime::Runtime::new().expect("runtime");
        let server = executor.block_on(MockServer::start());
        let healed = Arc::new(AtomicBool::new(false));
        let answers = Arc::clone(&healed);
        executor.block_on(
            Mock::given(method("POST"))
                .respond_with(move |request: &Request| respond(request, &answers))
                .mount(&server),
        );
        Self {
            executor,
            server,
            healed,
        }
    }

    /// From now on, every request is answered.
    pub fn heal(&self) {
        self.healed.store(true, Ordering::SeqCst);
    }

    /// The bodies of every request received, in arrival order.
    pub fn bodies(&self) -> Vec<Value> {
        self.executor
            .block_on(self.server.received_requests())
            .expect("requests")
            .iter()
            .map(|request| request.body_json().expect("body"))
            .collect()
    }

    /// An evaluator against this provider that retries a transient failure
    /// `retries` times and sends one request at a time (so which request
    /// takes a row's shared retries is fixed), with a credential and a model
    /// unique in the process ([`crate::judgment::test_identity`]).
    pub fn evaluator(&self, retries: u32) -> Evaluator {
        Evaluator::new(
            Some(secrecy::SecretString::from(
                crate::judgment::test_identity::unique("transient"),
            )),
            self.server.uri(),
            crate::judgment::test_identity::unique("jev-test"),
            &octocode_runtime_base::providers::classification::jev::JEV,
            Some(Duration::from_secs(20)),
            retries,
            1,
        )
    }
}

fn respond(request: &Request, healed: &AtomicBool) -> ResponseTemplate {
    let body: Value = request.body_json().expect("request");
    if !healed.load(Ordering::SeqCst) && body["state"].to_string().contains(FAIL) {
        return ResponseTemplate::new(500);
    }
    let answers = body["questions"]
        .as_object()
        .expect("questions")
        .keys()
        .map(|key| (key.clone(), json!({"type":"noul","noul":0.9})))
        .collect::<serde_json::Map<_, _>>();
    ResponseTemplate::new(200).set_body_json(json!({
        "model":"jev-resolved","answers":answers,
        "usage":{"input_tokens":10,"output_tokens":1}
    }))
}
