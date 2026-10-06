//! Provider-backed ghSearchCode tests: line resolution latency.
use super::{GhSearchCodeQuery, execute};
use crate::providers::github::{RequestContext, RetryPolicy};
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::mock_provider;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Answers after `delay`, recording when each request arrived.
#[derive(Clone)]
struct Slow {
    arrivals: Arc<Mutex<Vec<Instant>>>,
    delay: Duration,
    body: ResponseTemplate,
}

impl Respond for Slow {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Instant::now());
        self.body.clone().set_delay(self.delay)
    }
}

/// The head commit hit lines are pinned to resolves while the index search
/// runs: one round trip, not two in series, before the file reads.
#[tokio::test]
async fn the_head_commit_resolves_while_the_search_runs() {
    let server = MockServer::start().await;
    let delay = Duration::from_millis(300);
    let search = Slow {
        arrivals: Arc::default(),
        delay,
        body: ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"fn wrap_app() {}","matches":[{"text":"wrap_app","indices":[3,11]}]}]}
            ]
        })),
    };
    let commit = Slow {
        arrivals: Arc::default(),
        delay,
        body: ResponseTemplate::new(200).set_body_string(SHA),
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(search.clone())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(commit.clone())
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/app.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn wrap_app() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = mock_provider(
        &server,
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
    );
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["wrap_app"]}))
            .expect("query");
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    assert_eq!(out.data["commitSha"], SHA, "{}", out.data);
    assert_eq!(
        out.data["files"][0]["lines"],
        json!(["1\tfn wrap_app() {}"])
    );
    let first = |slow: &Slow| {
        *slow
            .arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first()
            .expect("requested")
    };
    let (searched, resolved) = (first(&search), first(&commit));
    let gap = if resolved > searched {
        resolved - searched
    } else {
        searched - resolved
    };
    assert!(
        gap < delay / 2,
        "the commit lookup waited {gap:?} for the search"
    );
}

/// A path-only or concise search reads no lines, so it resolves no commit.
#[tokio::test]
async fn searches_without_line_reads_resolve_no_commit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })))
        .mount(&server)
        .await;
    let provider = mock_provider(
        &server,
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
    );
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
    for row in [
        json!({"owner":"a","repo":"b","keywords":["app"],"match":"path"}),
        json!({"owner":"a","repo":"b","keywords":["app"],"concise":true}),
    ] {
        let query: GhSearchCodeQuery = serde_json::from_value(row).expect("query");
        execute(&provider, &query, &context, &Passthrough)
            .await
            .expect("search");
    }
    let requests = server.received_requests().await.expect("requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() == "/api/v3/search/code"),
        "{:?}",
        requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
}
