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

/// Two owner-wide hits in different repositories, indexed at `SHA`.
async fn mount_owner_wide_search(server: &MockServer) {
    let item = |repo: &str| {
        json!({"name":"x.py","path":"src/x.py","sha":"1",
         "html_url": format!("https://github.com/o/{repo}/blob/{SHA}/src/x.py"),
         "repository":{"full_name":format!("o/{repo}"),"html_url":"https://x","url":"https://x"},
         "text_matches":[{"fragment":"class HTTPAdapter(BaseAdapter):\n    pass","matches":[{"text":"HTTPAdapter","indices":[6,17]}]}]})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,"items":[item("a"), item("b")]
        })))
        .mount(server)
        .await;
}

async fn run_owner_wide(server: &MockServer) -> crate::tools::result::ToolData {
    let provider = mock_provider(
        server,
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
    );
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"o","keywords":["HTTPAdapter"]})).expect("query");
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
    execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search")
}

/// GC2: owner-wide rows are read at the commit each hit was indexed at
/// (`html_url`): numbered `lines` replace fragments, and the top read is
/// pinned to that commit instead of a `contextLines` fragment read.
#[tokio::test]
async fn owner_wide_rows_list_numbered_lines_at_the_indexed_commit() {
    let server = MockServer::start().await;
    mount_owner_wide_search(&server).await;
    for repo in ["a", "b"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/{repo}/contents/src%2Fx.py")))
            .and(wiremock::matchers::query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64",
                "content":STANDARD.encode("import x\n\nclass HTTPAdapter(BaseAdapter):\n    pass\n")
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let out = run_owner_wide(&server).await;
    let files = out.data["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{}", out.data);
    for row in files {
        assert_eq!(
            row["lines"],
            json!(["3\tclass HTTPAdapter(BaseAdapter):"]),
            "{}",
            out.data
        );
        assert!(row.get("matches").is_none(), "{}", out.data);
        assert!(row["owner"].is_string(), "{}", out.data);
    }
    // One commit for every row is stated once.
    assert_eq!(out.data["commitSha"], SHA, "{}", out.data);
    let read = &out.data["next"]["readTopMatch"]["query"]["queries"][0];
    assert_eq!(read["ref"], SHA, "{}", out.data);
    assert_eq!(read["owner"], "o", "{}", out.data);
    assert_eq!(read["repo"], "a", "{}", out.data);
    assert!(read.get("contextLines").is_none(), "{}", out.data);
}

/// GC2: a row whose file is gone at its indexed commit keeps its fragment
/// and says its lines were not read.
#[tokio::test]
async fn owner_wide_row_missing_at_its_commit_keeps_fragments() {
    let server = MockServer::start().await;
    mount_owner_wide_search(&server).await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/o/[ab]/contents/",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let out = run_owner_wide(&server).await;
    let row = &out.data["files"][0];
    assert_eq!(row["lineResolved"], false, "{}", out.data);
    assert!(row["matches"][0]["value"].is_string(), "{}", out.data);
    assert!(row.get("lines").is_none(), "{}", out.data);
}
