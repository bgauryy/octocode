use super::*;
use crate::providers::classification::gate;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
mod failures;
mod retries;

fn test_budget(timeout: Duration) -> RequestBudget {
    budget(
        Instant::now() + timeout,
        octocode_io::cancellation::Cancellation::new(),
    )
}

/// A fresh gate per test so shared cooldowns never leak across tests.
fn fresh_gate(limit: usize) -> GateLease {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    gate::lease(
        &format!("test://transport-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
        limit,
    )
}

const OK_BODY: &str = r#"{"model":"m","answers":{},"usage":{}}"#;

fn body(request: &Value) -> bytes::Bytes {
    serde_json::to_vec(request).unwrap().into()
}

async fn send(
    server: &str,
    timeout: Duration,
    retries: u32,
    lease: &GateLease,
) -> Result<Value, ClassificationError> {
    post(
        body(&json!({})),
        reqwest::header::HeaderMap::new(),
        &SecretString::from("test-key".to_owned()),
        endpoint(server, "v1/systemone").unwrap(),
        &test_budget(timeout),
        retries,
        lease,
    )
    .await
    .map(|(body, _)| body)
}

/// One successful provider answer, expected exactly once.
async fn mount_ok_once(server: &MockServer) {
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(OK_BODY))
        .expect(1)
        .mount(server)
        .await;
}

/// B75: a classification request may run without a deadline, so only a
/// provider that stops sending ends it ([`octocode_io::http::READ_STALL`]),
/// never a total, whichever proxy route it takes.
#[test]
fn a_provider_request_is_bounded_only_by_its_read_stall() {
    for bypass_proxy in [false, true] {
        let policy = provider_policy(bypass_proxy);
        assert_eq!(policy.read_timeout, Some(octocode_io::http::READ_STALL));
        assert_eq!(policy.timeout, None);
    }
}
