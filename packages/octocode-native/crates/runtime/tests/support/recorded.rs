//! Real GitHub answers from `test-utils/provider-answers`, on a mock server: the
//! store `src/providers/artifact/test_http/recorded.rs` and the end-to-end proxy
//! read too.

use serde_json::Value;
use std::path::PathBuf;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// One recording, by its id: `github/repo-get`.
pub fn recording(case: &str) -> Value {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../test-utils/provider-answers")
        .join(format!("{case}.json"));
    let text = std::fs::read_to_string(&file).expect("a recording");
    serde_json::from_str(&text).expect("recording JSON")
}

/// Mount each exchange of `case` where [`super::github_api`] points GitHub tools.
pub async fn mount_github(server: &MockServer, case: &str) -> Value {
    let recording = recording(case);
    for exchange in recording["exchanges"].as_array().expect("exchanges") {
        let request = &exchange["request"];
        let url = url::Url::parse(request["url"].as_str().expect("url")).expect("recorded URL");
        let served = match url.path() {
            "/graphql" => "/api/graphql".to_owned(),
            rest => format!("/api/v3{rest}"),
        };
        let mut mock = Mock::given(method(request["method"].as_str().expect("method")))
            .and(path(served.as_str()));
        for (name, value) in url.query_pairs() {
            mock = mock.and(query_param(name.into_owned(), value.into_owned()));
        }
        if let Some(body) = request.get("body") {
            mock = mock.and(body_json(body));
        }
        let status = exchange["status"].as_u64().expect("status");
        let mut answer = ResponseTemplate::new(u16::try_from(status).expect("an HTTP status"));
        answer = match &exchange["body"] {
            Value::String(text) => answer.set_body_string(text.as_str()),
            json => answer.set_body_json(json),
        };
        mock.respond_with(answer).mount(server).await;
    }
    recording
}
