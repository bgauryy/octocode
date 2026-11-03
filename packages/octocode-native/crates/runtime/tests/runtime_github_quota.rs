//! GitHub quota pacing: a call that waits for a spent limit tells its host
//! and still returns the page, and a continuation the evidence journal
//! answers is not refused for a spent quota.

use crate::support::{PACED_LIMITS, Workspace, call, row_data, row_status};
use octocode_native::execution::RuntimeLimits;
use serde_json::json;
use wiremock::matchers::{method, path, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A GitHub call that waits for a limit to lift tells its host: the call's
/// progress listener hears the wait (what limit, how long, why), and the
/// call still returns the page that was waited for. A wait that runs in a
/// call nobody listens to says nothing.
#[tokio::test]
async fn a_quota_wait_reports_progress_and_still_returns_the_page() {
    const SEARCH_CODE: &str = "/api/v3/search/code";
    let server = MockServer::builder().start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 2;
    Mock::given(method("GET"))
        .and(path(SEARCH_CODE))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "code_search")
                .set_body_json(json!({"message":"API rate limit exceeded"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(SEARCH_CODE))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":0,"incomplete_results":false,"items":[]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"a/b"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        RuntimeLimits {
            max_retries: 3,
            ..PACED_LIMITS
        },
    );
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = std::sync::Arc::clone(&heard);
    let scope = octocode_io::notice::Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let outcome = scope
        .run(call(
            &runtime,
            "ghSearchCode",
            json!({"owner":"a","repo":"b","keywords":["needle"]}),
        ))
        .await
        .expect("the waited-for page");
    assert_eq!(
        row_status(&outcome),
        "empty",
        "{}",
        outcome.structured_content
    );
    let lines = heard.lock().expect("lines").clone();
    assert!(
        lines
            .first()
            .is_some_and(|line| line.contains("code_search") && line.contains("reset")),
        "the wait is reported: {lines:?}"
    );
    runtime.close().await;
}

/// Results each extension's mock code search holds.
const CODE_RESULTS: usize = 12;

/// A mock code search per extension that reports the quota GitHub would:
/// `rs` leaves one search, `ts` spends the last, and the reset is an hour away.
fn quota_spent_code_search(server_reset: u64) -> impl Fn(&str) -> Mock {
    move |extension: &str| {
        struct Quota(&'static str, u64, octocode_github::mock_http::SearchIndex);
        impl wiremock::Respond for Quota {
            fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
                self.2
                    .respond(request)
                    .insert_header("x-ratelimit-limit", "10")
                    .insert_header("x-ratelimit-remaining", self.0)
                    .insert_header("x-ratelimit-reset", self.1.to_string().as_str())
                    .insert_header("x-ratelimit-resource", "code_search")
            }
        }
        let items = (0..CODE_RESULTS)
            .map(|index| {
                let name = format!("f{index}.{extension}");
                json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
                    "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
            })
            .collect();
        let remaining = if extension == "rs" { "1" } else { "0" };
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(query_param_contains("q", format!("extension:{extension}")))
            .respond_with(Quota(
                remaining,
                server_reset,
                octocode_github::mock_http::SearchIndex(items),
            ))
    }
}

/// A continuation whose searches its journal answers needs no quota: with the
/// code-search quota spent and its reset past the request deadline, page 2 of
/// a multi-extension walk (read from the pages page 1 recorded) is served,
/// while a fresh search in the same state is refused before it is sent.
#[tokio::test]
async fn a_continuation_the_journal_answers_is_not_refused_for_a_spent_quota() {
    let server = MockServer::builder().start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 3600;
    let mock = quota_spent_code_search(reset);
    for extension in ["rs", "ts"] {
        mock(extension).mount(&server).await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        PACED_LIMITS,
    );
    let first = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","keywords":["needle"],"matchIn":["path"],
            "extensions":["rs","ts"],"limit":4}),
    )
    .await
    .expect("page 1");
    let data = row_data(&first);
    let next = data["next"]["nextPage"]["input"].clone();
    assert!(next.is_object(), "page 1 offers page 2: {data}");
    let sent = server.received_requests().await.expect("recorded").len();
    let second = call(&runtime, "ghSearchCode", next).await.expect("page 2");
    let data = row_data(&second);
    assert!(data["files"].is_array(), "page 2 is served: {data}");
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        sent,
        "the journal answered page 2: nothing was sent"
    );
    let fresh = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","keywords":["other"],"matchIn":["path"],
            "extensions":["rs","ts"],"limit":4}),
    )
    .await
    .expect("fresh search");
    assert_eq!(row_data(&fresh)["errorCode"], "rateLimited", "{fresh:?}");
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        sent,
        "the fresh search was refused before it was sent"
    );
    runtime.close().await;
}
