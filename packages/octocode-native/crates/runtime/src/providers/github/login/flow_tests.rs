use super::*;
use octocode_github::{CredentialSource, ProviderError};
use serde_json::json;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};
mod device_login;
mod refresh;

/// One device-flow second in tests: each `authorization_pending` poll waits
/// this long instead of a real second.
const TEST_SECOND: Duration = Duration::from_millis(10);

fn endpoints(server: &MockServer) -> LoginEndpoints {
    LoginEndpoints {
        web_origin: server.uri(),
        api_origin: server.uri(),
        host: "fixture.example".into(),
    }
}

async fn device(server: &MockServer, expires: u64) {
    Mock::given(method("POST")).and(path("/login/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code":"fixture-device", "user_code":"FIXTURE", "verification_uri":"https://example.test/login",
            "interval":1, "expires_in":expires
        }))).mount(server).await;
}

async fn user(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Bearer synthetic-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"fixture-user"})))
        .expect(1)
        .mount(server)
        .await;
}

fn token() -> serde_json::Value {
    json!({"access_token":"synthetic-access", "refresh_token":"synthetic-refresh",
        "token_type":"bearer", "expires_in":3600, "refresh_token_expires_in":7200, "scope":"repo, read:org"})
}

/// The token poll succeeds on its one expected request.
async fn grant_token_once(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token()))
        .expect(1)
        .mount(server)
        .await;
}
