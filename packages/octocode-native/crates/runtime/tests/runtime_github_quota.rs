//! GitHub quota pacing: a call that waits for a spent limit tells its host
//! and still returns the page, and a continuation the evidence journal
//! answers is not refused for a spent quota.

use crate::support::{PACED_LIMITS, Run, Workspace, call, row_data, row_status};
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

/// B91: a row discloses the client-side waits it spent, by kind, as
/// `clientWaits`; a row of the same batch that waited for nothing has none.
/// Row 0 reads its repository against a spent core quota (reset in 2 s);
/// row 1 searches an owner's code, a bucket that quota does not touch.
#[tokio::test]
async fn each_row_discloses_its_own_client_waits() {
    let server = MockServer::builder().start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 2;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(json!({"message":"API rate limit exceeded"})),
        )
        .up_to_n_times(1)
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
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":0,"incomplete_results":false,"items":[]})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        RuntimeLimits {
            max_retries: 3,
            ..crate::support::FIXTURE_LIMITS
        },
    );
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json!({"queries":[
            {"owner":"a","repo":"b","keywords":["needle"]},
            {"owner":"c","keywords":["haystack"]}
        ]}),
    )
    .await
    .expect("both rows");
    let rows = &outcome.structured_content["results"];
    // Row 0 sat out the spent quota; a queue behind row 1's search on the
    // shared code-search lane may sit beside it, one entry per kind.
    let waited = rows[0]["data"]["clientWaits"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let quota = waited
        .iter()
        .filter(|wait| wait["kind"] == "quotaPacing")
        .collect::<Vec<_>>();
    assert_eq!(quota.len(), 1, "{}", outcome.structured_content);
    assert!(
        quota[0]["ms"].as_i64().is_some_and(|ms| ms > 0),
        "{}",
        outcome.structured_content
    );
    let kinds = waited
        .iter()
        .map(|wait| wait["kind"].to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(kinds.len(), waited.len(), "summed by kind: {waited:?}");
    // Row 1 may queue behind row 0's search on the code-search lane, a wait
    // of its own; it never carries row 0's wait for the spent core quota.
    let other = rows[1]["data"]["clientWaits"].as_array().cloned();
    assert!(
        other
            .unwrap_or_default()
            .iter()
            .all(|wait| wait["kind"] != "quotaPacing"),
        "row 0's wait stays on row 0: {}",
        outcome.structured_content
    );
    // A row with nothing to wait for (the quota reset, no other row in
    // flight) says nothing.
    let alone = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","repo":"b","keywords":["needle"]}),
    )
    .await
    .expect("an unpaced row");
    assert!(
        row_data(&alone).get("clientWaits").is_none(),
        "the row that waited for nothing: {}",
        alone.structured_content
    );
    runtime.close().await;
}

/// A 403 spending the `resource` quota, resetting `reset` (epoch seconds)
/// when given; without one GitHub states no end to the limit.
fn spent(resource: &str, reset: Option<u64>) -> ResponseTemplate {
    let template = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("x-ratelimit-resource", resource)
        .set_body_json(json!({"message":"API rate limit exceeded"}));
    match reset {
        Some(reset) => template.insert_header("x-ratelimit-reset", reset.to_string().as_str()),
        None => template,
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

/// B91 r2 (B75 condition 2) through the runtime: a spent quota GitHub gives
/// no reset for, with no caller budget, waits and tells the host; the
/// host's cancel of the request (MCP `notifications/cancelled`) ends that
/// wait at once.
#[tokio::test]
async fn a_cancel_ends_a_quota_wait_github_gives_no_reset_for() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(spent("code_search", None))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        RuntimeLimits {
            max_retries: 3,
            ..crate::support::FIXTURE_LIMITS
        },
    );
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = std::sync::Arc::clone(&heard);
    let scope = octocode_io::notice::Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let input = crate::support::envelope(json!({"owner":"a","repo":"b","keywords":["needle"]}));
    let run = scope.run(runtime.run("no-reset".into(), "ghSearchCode".into(), input));
    tokio::pin!(run);
    let waiting = async {
        while heard.lock().expect("lines").is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        // Let the wait run a while before the host cancels it.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    };
    tokio::select! {
        outcome = &mut run => panic!(
            "the call ended before it waited: {:?}",
            outcome.map(|outcome| outcome.structured_content.to_string())
        ),
        () = waiting => {}
    }
    let cancelled = std::time::Instant::now();
    assert!(
        runtime.requests.cancel("no-reset"),
        "the request is in flight"
    );
    let outcome = run.await;
    let ended = std::time::Instant::now();
    assert!(
        ended.duration_since(cancelled) < std::time::Duration::from_secs(1),
        "the cancel ended the wait at once: {:?} ({:?})",
        ended.duration_since(cancelled),
        outcome
            .as_ref()
            .map(|outcome| outcome.structured_content.to_string())
    );
    let lines = heard.lock().expect("lines").clone();
    assert!(
        lines[0].contains("code_search") && lines[0].contains("waiting"),
        "the wait is disclosed: {lines:?}"
    );
    runtime.close().await;
}

/// B91 r2: `clientWaits` belongs to the execution that waited. Page 1 waits
/// out a spent code-search quota and says so; page 2, served after the
/// quota reset, carries no wait of page 1's.
#[tokio::test]
async fn a_continuation_page_carries_none_of_the_first_pages_waits() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(spent("code_search", Some(epoch_secs() + 2)))
        .up_to_n_times(1)
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
    let items = (0..CODE_RESULTS)
        .map(|index| {
            let name = format!("f{index}.rs");
            json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
                "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(octocode_github::mock_http::SearchIndex(items))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        RuntimeLimits {
            max_retries: 3,
            ..crate::support::FIXTURE_LIMITS
        },
    );
    let first = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","repo":"b","keywords":["needle"],"matchIn":["path"],"limit":4}),
    )
    .await
    .expect("page 1");
    let data = row_data(&first);
    assert!(
        data["clientWaits"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|wait| wait["kind"] == "quotaPacing" && wait["ms"].as_i64() > Some(0)),
        "page 1 keeps its own wait: {data}"
    );
    let next = data["next"]["nextPage"]["input"].clone();
    assert!(next.is_object(), "page 1 offers page 2: {data}");
    let second = call(&runtime, "ghSearchCode", next).await.expect("page 2");
    let data = row_data(&second);
    assert!(data["files"].is_array(), "page 2 is served: {data}");
    assert!(
        data.get("clientWaits").is_none(),
        "page 2 carries none of page 1's waits: {data}"
    );
    runtime.close().await;
}

/// B91 r2: rows of a ghGetFileContent batch wait on the batch warm-up, which
/// resolves their ref under a spent core quota: each row discloses the
/// warm-up's wait as its own `quotaPacing`.
#[tokio::test]
async fn file_rows_disclose_the_batch_warm_ups_quota_wait() {
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/a/b/commits/[^/]+$",
        ))
        .respond_with(spent("core", Some(epoch_secs() + 2)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/a/b/commits/[^/]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "^/api/v3/repos/a/b/contents/.+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"hello\n".as_slice(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{"object":{}}}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "author": {"login": "rest-user"},
            "commit": {"committer": {"date": "2025-05-05T00:00:00Z"}}
        }])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        RuntimeLimits {
            max_retries: 3,
            ..crate::support::FIXTURE_LIMITS
        },
    );
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"queries":[
            {"owner":"a","repo":"b","path":"a.txt"},
            {"owner":"a","repo":"b","path":"b.txt"}
        ]}),
    )
    .await
    .expect("batch");
    for index in 0..2 {
        let data = &outcome.structured_content["results"][index]["data"];
        assert!(
            data["clientWaits"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|wait| wait["kind"] == "quotaPacing" && wait["ms"].as_i64() > Some(0)),
            "row {index} discloses the warm-up's wait: {}",
            outcome.structured_content
        );
    }
    runtime.close().await;
}

/// Results each extension's mock code search holds.
const CODE_RESULTS: usize = 12;

/// A mock code search per extension that reports the quota GitHub would: a first
/// page at a small `limit` leaves plenty, then on GitHub's largest page `rs`
/// leaves one search, `ts` spends the last, and the reset is an hour away.
fn quota_spent_code_search(server_reset: u64) -> impl Fn(&str) -> Mock {
    move |extension: &str| {
        struct Quota(&'static str, u64, octocode_github::mock_http::SearchIndex);
        impl wiremock::Respond for Quota {
            fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
                let whole = request
                    .url
                    .query_pairs()
                    .any(|(key, value)| key == "per_page" && value == "100");
                self.2
                    .respond(request)
                    .insert_header("x-ratelimit-limit", "10")
                    .insert_header("x-ratelimit-remaining", if whole { self.0 } else { "9" })
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
/// code-search quota spent and its reset past the request deadline, page 3 of
/// a multi-extension walk (read from the pages page 2 recorded) is served,
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
    // A refusal needs a caller budget the reset falls past: without one
    // (B75) a quota wait keeps waiting until the reset or a cancel.
    let runtime = workspace
        .runtime_with(
            &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
            PACED_LIMITS,
        )
        .with_request_budget(std::time::Duration::from_secs(300))
        .expect("caller budget");
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
    let second = call(&runtime, "ghSearchCode", next).await.expect("page 2");
    let data = row_data(&second);
    let next = data["next"]["nextPage"]["input"].clone();
    assert!(next.is_object(), "page 2 offers page 3: {data}");
    let sent = server.received_requests().await.expect("recorded").len();
    let third = call(&runtime, "ghSearchCode", next).await.expect("page 3");
    let data = row_data(&third);
    assert!(data["files"].is_array(), "page 3 is served: {data}");
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        sent,
        "the journal answered page 3: nothing was sent"
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

/// B91 r3: a clone waits out a secondary-limit cooldown at its git admission
/// and its own row says so as `secondaryLimit`. The cooldown begins while the
/// clone's commit lookup is in flight (another call meets the abuse limit),
/// so the clone's API reads were admitted before it and none follows it: the
/// only wait that can reach the clone row is the git admission's, which only
/// the row's limiter (`git_limiter().for_row(request)`) records there.
#[tokio::test]
async fn a_clone_discloses_its_git_admission_cooldown_on_its_own_row() {
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const COMMIT: &str = "/api/v3/repos/a/b/commits/main";
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(COMMIT))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(SHA)
                .set_delay(std::time::Duration::from_millis(1500)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "21")
                .insert_header("retry-after", "3")
                .set_body_json(json!({"message":
                    "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        RuntimeLimits {
            max_retries: 3,
            ..crate::support::FIXTURE_LIMITS
        },
    );
    let api_reads = |requests: &[wiremock::Request]| {
        requests
            .iter()
            .filter(|request| request.url.path().starts_with("/api/v3/repos/"))
            .count()
    };
    let clone = call(
        &runtime,
        "ghCloneRepo",
        json!({"owner":"a","repo":"b","ref":"main"}),
    );
    let trigger = async {
        // Once the clone's commit lookup is in flight, another call meets
        // the abuse limit: the key cools down for 3 s.
        loop {
            let requests = server.received_requests().await.unwrap_or_default();
            if requests.iter().any(|request| request.url.path() == COMMIT) {
                break api_reads(&requests);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    let searched = async {
        let before = trigger.await;
        // Its own request id: the clone runs beside it.
        let search = runtime
            .run(
                "abuse-limited".into(),
                "ghSearchRepo".into(),
                crate::support::envelope(json!({"keywords":["needle"]})),
            )
            .await
            .expect("the search row");
        (before, search)
    };
    let (clone, (before, search)) = tokio::join!(clone, searched);
    let clone = clone.expect("the clone row");
    let requests = server.received_requests().await.unwrap_or_default();
    assert_eq!(
        api_reads(&requests),
        before,
        "no API read of the clone followed the cooldown: its wait is git's"
    );
    let waits = row_data(&clone)["clientWaits"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        waits
            .iter()
            .any(|wait| wait["kind"] == "secondaryLimit" && wait["ms"].as_i64() > Some(0)),
        "the clone row discloses its git admission wait: {}",
        clone.structured_content
    );
    // Each row carries its own share of the one cooldown, never the other's:
    // the search waited all of it before its retry, the clone only what was
    // left once its commit lookup answered.
    let secondary = |data: &serde_json::Value| {
        data["clientWaits"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|wait| wait["kind"] == "secondaryLimit")
            .filter_map(|wait| wait["ms"].as_i64())
            .sum::<i64>()
    };
    let (cloned, searched) = (secondary(row_data(&clone)), secondary(row_data(&search)));
    assert!(
        searched >= 2500 && cloned < searched,
        "clone {cloned} ms, search {searched} ms: {} / {}",
        clone.structured_content,
        search.structured_content
    );
    runtime.close().await;
}
