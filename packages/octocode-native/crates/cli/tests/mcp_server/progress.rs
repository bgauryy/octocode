//! A GitHub call that waits for a spent quota to reset, over real stdio: a
//! request that sent `_meta.progressToken` hears the wait as
//! `notifications/progress` before the result, and the result is still the
//! page that was waited for; a request without a token hears nothing.

use super::server::{REPLY_TIMEOUT, Server};
use crate::support::Workspace;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Every other answer is a spent code-search quota that resets in two
/// seconds; the one after it is the (empty) page.
struct SpentThenPage(AtomicUsize);

impl Respond for SpentThenPage {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) % 2 == 1 {
            return ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 0, "incomplete_results": false, "items": []}));
        }
        let reset = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            + 2;
        ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", reset.to_string().as_str())
            .insert_header("x-ratelimit-resource", "code_search")
            .set_body_json(json!({"message": "API rate limit exceeded"}))
    }
}

impl Server {
    /// One request: the notifications that arrived before its reply, and
    /// the reply.
    fn request_hearing(&mut self, method: &str, params: Value) -> (Vec<Value>, Value) {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut heard = Vec::new();
        loop {
            let message = self.replies.recv_timeout(REPLY_TIMEOUT).expect("a reply");
            if message["id"] == id {
                return (heard, message);
            }
            heard.push(message);
        }
    }
}

fn search(token: Option<&str>) -> Value {
    let mut params = json!({"name": "ghSearchCode", "arguments": {"queries": [{
        "owner": "a", "repo": "b", "keywords": ["needle"]}]}});
    if let Some(token) = token {
        params["_meta"] = json!({"progressToken": token});
    }
    params
}

#[tokio::test(flavor = "multi_thread")]
async fn a_quota_wait_reports_progress_only_to_a_request_with_a_token() {
    let github = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(SpentThenPage(AtomicUsize::new(0)))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"a/b"})),
        )
        .mount(&github)
        .await;
    let workspace = Workspace::new();
    let api = format!("{}/api/v3", github.uri());
    let token = workspace.token.clone();
    let (with_token, without_token) = tokio::task::spawn_blocking(move || {
        let mut server = Server::start_with(
            &workspace,
            &[("GITHUB_API_URL", api.as_str()), ("GITHUB_TOKEN", &token)],
        );
        server.request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "1"}}),
        );
        server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        let with_token = server.request_hearing("tools/call", search(Some("wait-1")));
        let without_token = server.request_hearing("tools/call", search(None));
        assert!(server.finish().success());
        (with_token, without_token)
    })
    .await
    .expect("stdio exchange");

    let (heard, reply) = with_token;
    let progress: Vec<&Value> = heard
        .iter()
        .filter(|message| message["method"] == "notifications/progress")
        .collect();
    assert!(
        !progress.is_empty(),
        "no progress before the result: {heard:?}"
    );
    assert_eq!(progress[0]["params"]["progressToken"], "wait-1");
    let line = progress[0]["params"]["message"].as_str().expect("a line");
    assert!(
        line.contains("code_search") && line.contains("reset"),
        "{line}"
    );
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(
        text.contains("\"status\":\"empty\""),
        "the waited-for page: {text}"
    );

    let (heard, reply) = without_token;
    assert!(heard.is_empty(), "no token, no notifications: {heard:?}");
    let text = reply["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(text.contains("\"status\":\"empty\""), "{text}");
}
