//! Provider batch concurrency and the one shared budget across patch rows.

use super::{mount_file_batches, mount_pr, numbered, pr, rest_file, run};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use std::time::Duration;
use support::Workspace;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn selected_patch_scan_reads_file_batches_concurrently() {
    // A selected late file must not cost one sequential round trip per
    // provider batch before it: the six batches are read in parallel. Each
    // reply is held `delay`, so a sequential scan sends no request within
    // `delay` of the one before; overlapping requests prove the parallel
    // reads whatever the machine's load (a wall-clock bound does not).
    let delay = Duration::from_millis(300);
    let server = MockServer::builder().start().await;
    mount_pr(&server, 600).await;
    let mut batches = (1..=6).map(|b| numbered(b, 100)).collect::<Vec<_>>();
    batches[5][99] = rest_file("src/late.rs", Some("@@ -1 +1 @@\n-a\n+late"), 1, 1);
    let arrivals = mount_file_batches(&server, batches, delay).await;
    let data = run(
        &server,
        json!({"sections":["patches"],"include":["src/late.rs"]}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["path"], "src/late.rs", "{data}");
    assert_eq!(file["patch"], "@@ -1 +1 @@\n1\t-a\n1\t+late");
    let times = arrivals.sorted();
    assert_eq!(times.len(), 6, "every batch is read once");
    let overlapping = times
        .windows(2)
        .filter(|pair| pair[1].duration_since(pair[0]) < delay)
        .count();
    assert!(overlapping > 0, "sequential scan: {times:?}");
}

/// Rows of one call share one patch budget, so two large patch
/// reads fit one response page instead of spilling into response
/// pagination; each row keeps its own exact patch continuation.
#[tokio::test]
async fn patch_rows_in_one_call_share_one_budget() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let big = |tag: &str| {
        format!(
            "@@ -1,9000 +1,9000 @@\n{}",
            format!("+{tag} line\n").repeat(3_000)
        )
    };
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("pyproject.toml", Some(&big("a")), 3_000, 0),
            rest_file("src/_runtime.py", Some(&big("b")), 3_000, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let row = |file: &str| {
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 9,
               
               "sections": ["patches"], "include": [file]})
    };
    let outcome = runtime
        .run(
            "test-1".into(),
            "ghGetHistoryItem".into(),
            json!({"queries": [row("pyproject.toml"), row("src/_runtime.py")]}),
        )
        .await
        .expect("two patch rows");
    let content = &outcome.structured_content;
    assert!(
        content["responsePagination"].get("expiresAt").is_none(),
        "two rows overflowed the page: {}",
        content["responsePagination"]
    );
    // An unpaged response leaves each row its own section walk.
    assert!(content.get("responsePagination").is_none(), "{content}");
    for index in 0..2 {
        let data = &content["results"][index]["data"];
        let file = &data["pullRequests"][0]["files"][0];
        let taken = file["patchPagination"]["length"].as_u64().unwrap_or(0);
        // Half the default window each (one budget), not a whole window each.
        assert!((4_000..=20_000).contains(&taken), "row {index}: {taken}");
        assert_eq!(
            data["pullRequests"][0]["sectionPagination"]["patches"]["hasMore"], true,
            "row {index}: {data}"
        );
        assert!(
            data["pullRequests"][0]["next"]["patches"]["input"]["cursor"].is_string(),
            "row {index}: {data}"
        );
    }
    runtime.close().await;
}

/// `matchString` selects patch text only, so review comments stay complete
/// even when no body holds it, and each continuation jumps straight to its
/// own provider batch. Walking every page reads each provider batch exactly
/// once (no call rescans an earlier batch to find its place) and recovers
/// every body exactly once.
#[tokio::test]
async fn match_string_review_comment_pages_read_each_provider_batch_once() {
    let server = MockServer::builder().start().await;
    let mut raw = pr(1);
    raw["review_comments"] = json!(600);
    let comment_arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(raw))
        .mount(&server)
        .await;
    for page in 1..=6usize {
        let comments = (0..100)
            .map(|i| {
                let n = (page - 1) * 100 + i;
                json!({"id": n, "user": {"login": "reviewer"}, "body": format!("note {n}"),
                       "created_at": "2024-01-01T00:00:00Z", "path": "a.rs", "line": 1})
            })
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200).set_body_json(Value::Array(comments));
        if page < 6 {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/9/comments?per_page=100&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                )
                .as_str(),
            );
        }
        let arrivals = comment_arrivals.clone();
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/comments"))
            .and(query_param("page", page.to_string()))
            .respond_with(move |_: &wiremock::Request| {
                arrivals.lock().expect("comment arrivals").push(page);
                response.clone()
            })
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut input = json!({"queries":[{"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "sections":["reviewComments"],"matchString":["in no body"]}]});
    let mut bodies = Vec::new();
    let mut seen_inputs = std::collections::BTreeSet::new();
    let mut call_arrivals = Vec::new();
    loop {
        assert!(
            seen_inputs.insert(input.to_string()),
            "repeated page: {input}"
        );
        let arrival_start = comment_arrivals.lock().expect("comment arrivals").len();
        let result = support::run(&runtime, "ghGetHistoryItem", input).await;
        call_arrivals
            .push(comment_arrivals.lock().expect("comment arrivals")[arrival_start..].to_vec());
        let data = result.data();
        bodies.extend(
            data["pullRequests"][0]["reviewComments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| row["body"].as_str().map(str::to_owned)),
        );
        let next = result
            .structured_content
            .pointer("/responsePagination/next")
            .or_else(|| data.pointer("/pullRequests/0/next/reviewComments"));
        let Some(next) = next else { break };
        assert!(next["input"]["cursor"].is_string());
        assert_eq!(next["tool"], "ghGetHistoryItem");
        input = next["input"].clone();
    }
    let mut sorted = bodies.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), bodies.len(), "a body was listed twice");
    assert_eq!(
        bodies
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        (0..600).map(|index| format!("note {index}")).collect(),
        "every review body is recovered: matchString does not select discussion"
    );
    let mut read = call_arrivals.concat();
    read.sort_unstable();
    assert_eq!(
        read,
        (1..=6).collect::<Vec<_>>(),
        "each provider batch is read exactly once across the walk: {call_arrivals:?}"
    );
    runtime.close().await;
}
