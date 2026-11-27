//! Real GitHub answers from `test-utils/provider-answers`, on a mock server: the
//! store `octocode-tool-packages`'s `src/artifact/test_http/recorded.rs`
//! and the end-to-end proxy read too.

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
    mount(server, case, github_path).await
}

/// Where the GitHub double serves a recorded GitHub URL.
pub fn github_path(url: &url::Url) -> String {
    match url.path() {
        "/graphql" => "/api/graphql".to_owned(),
        rest => format!("/api/v3{rest}"),
    }
}

/// Mount each exchange of `case` at the path `served` gives its recorded URL.
pub async fn mount(server: &MockServer, case: &str, served: impl Fn(&url::Url) -> String) -> Value {
    let recording = recording(case);
    for exchange in recording["exchanges"].as_array().expect("exchanges") {
        let request = &exchange["request"];
        let url = url::Url::parse(request["url"].as_str().expect("url")).expect("recorded URL");
        let served = served(&url);
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
        // The media type decides how a reader decodes a text body (raw file content).
        let media_type = exchange["headers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|header| header[0] == "content-type")
            .and_then(|header| header[1].as_str());
        answer = match (&exchange["body"], media_type) {
            (Value::String(text), Some(media_type)) => {
                answer.set_body_raw(text.as_bytes().to_vec(), media_type)
            }
            (Value::String(text), None) => answer.set_body_string(text.as_str()),
            (json, _) => answer.set_body_json(json),
        };
        mock.respond_with(answer).mount(server).await;
    }
    // A file read first asks its folder's git tree for a symlink on its path.
    // GitHub's tree of the commit lists the recorded file; a recorded tree
    // exchange answers first.
    if recording["tool"] == "ghGetFileContent" {
        let field = |name: &str| {
            recording["query"][name]
                .as_str()
                .expect("recorded query field")
        };
        octocode_test_support::mock_http::list_in_trees(
            server,
            field("owner"),
            field("repo"),
            field("path"),
        )
        .await;
    }
    recording
}
