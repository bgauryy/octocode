//! B75 over real stdio: an MCP call has no fixed request deadline, and
//! `notifications/cancelled` is what ends one in flight. A GitHub code search
//! that meets a spent quota waits for its reset, which makes a call that
//! stays in flight for as long as the test needs.

use super::server::{REPLY_TIMEOUT, Server};
use crate::support::Workspace;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Every answer is a code-search quota spent until this long after it.
struct Spent(Duration);

impl Respond for Spent {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let reset = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            + self.0.as_secs();
        ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", reset.to_string().as_str())
            .insert_header("x-ratelimit-resource", "code_search")
            .set_body_json(json_fixture(r#"{"message": "API rate limit exceeded"}"#))
    }
}

/// A GitHub whose code-search quota is always spent for `reset_in` more.
async fn spent_github(reset_in: Duration) -> MockServer {
    let github = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(Spent(reset_in))
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"default_branch": "main", "full_name": "a/b"}"#,
        )))
        .mount(&github)
        .await;
    github
}

fn search() -> Value {
    json_fixture(
        r#"{
            "name": "ghSearchCode",
            "_meta": {"progressToken": "wait"},
            "arguments": {
                "queries": [{"owner": "a", "repo": "b", "keywords": ["needle"]}]
            }
        }"#,
    )
}

/// Starts the server against `api`, sends the search, and returns the
/// server, the search's id and the first message the server sent after it.
fn start_search(workspace: &Workspace, api: &str) -> (Server, u64, Value) {
    let token = workspace.token.clone();
    let mut server = Server::start_with(
        workspace,
        &[("GITHUB_API_URL", api), ("GITHUB_TOKEN", &token)],
    );
    server.request(
        "initialize",
        json_fixture(
            r#"{
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}
            }"#,
        ),
    );
    server.send(&json_fixture(
        r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#,
    ));
    server.next_id += 1;
    let id = server.next_id;
    server.send(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": search()}));
    let first = server
        .replies
        .recv_timeout(REPLY_TIMEOUT)
        .expect("a message");
    (server, id, first)
}

fn cancel(server: &mut Server, id: u64) {
    server.send(
        &json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                        "params": {"requestId": id, "reason": "test"}}),
    );
}

/// A quota that resets past the old 300 s request budget is waited for: the
/// call reports the wait as progress instead of answering a refusal at once.
#[tokio::test(flavor = "multi_thread")]
async fn a_wait_past_the_old_300_s_budget_is_waited_for() {
    let github = spent_github(Duration::from_secs(400)).await;
    let api = format!("{}/api/v3", github.uri());
    let first = tokio::task::spawn_blocking(move || {
        let workspace = Workspace::new();
        let (mut server, id, first) = start_search(&workspace, &api);
        cancel(&mut server, id);
        assert!(server.finish().success());
        first
    })
    .await
    .expect("stdio exchange");
    assert_eq!(
        first["method"], "notifications/progress",
        "the call waits for the reset instead of answering: {first}"
    );
    let line = first["params"]["message"].as_str().expect("a line");
    assert!(line.contains("reset"), "{line}");
}

/// `notifications/cancelled` stops the call in flight: the search waiting for
/// its quota never sends the request it waited for (a call still running
/// searches again at each reset), and the call has no reply
/// (MCP drops a cancelled request's response). The server keeps serving.
#[tokio::test(flavor = "multi_thread")]
async fn notifications_cancelled_stops_the_call_in_flight() {
    let reset_in = Duration::from_secs(3);
    let github = spent_github(reset_in).await;
    let api = format!("{}/api/v3", github.uri());
    let (first, late) = tokio::task::spawn_blocking(move || {
        let workspace = Workspace::new();
        std::fs::write(workspace.workspace.join("a.txt"), "needle\n").unwrap();
        let (mut server, id, first) = start_search(&workspace, &api);
        cancel(&mut server, id);
        // Past the reset, a call still running would have searched again.
        std::thread::sleep(reset_in * 2);
        let after = server.request("tools/call", super::server::local_search("."));
        assert_eq!(after["result"]["isError"], false, "{after}");
        let late: Vec<Value> = server.replies.try_iter().collect();
        assert!(server.finish().success());
        (first, late)
    })
    .await
    .expect("stdio exchange");
    assert_eq!(first["method"], "notifications/progress", "{first}");
    assert!(late.is_empty(), "the cancelled call answered: {late:?}");
    let searches = github
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path() == "/api/v3/search/code")
        .count();
    assert_eq!(searches, 1, "the cancelled call searched again");
}
