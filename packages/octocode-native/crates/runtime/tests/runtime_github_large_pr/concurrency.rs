//! Provider batch concurrency and the one shared budget across patch rows.

use super::{mount_file_batches, mount_pr, numbered, pr, rest_file, run};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
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
        json!({"sections":["patches"],"include":["src/late.rs"],
               "extras": []}),
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
/// pagination; each row keeps its own exact `continuePatch`.
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
               "mainGoal": "g", "reasoning": "r",
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
        content.get("responsePagination").is_none(),
        "two rows overflowed the page: {}",
        content
            .get("responsePagination")
            .cloned()
            .unwrap_or_default()
    );
    for index in 0..2 {
        let data = &content["results"][index]["data"];
        let file = &data["pullRequests"][0]["files"][0];
        let taken = file["patchPagination"]["length"].as_u64().unwrap_or(0);
        // Half the default window each (one budget), not a whole window each.
        assert!((4_000..=20_000).contains(&taken), "row {index}: {taken}");
        assert!(
            data["next"]["continuePatch"].is_object(),
            "row {index}: {data}"
        );
    }
    runtime.close().await;
}

/// A later comment page of a large PR does not re-scan every provider batch
/// one round trip at a time: with the PR's comment count in hand, the
/// batches load at once, and the page holds exactly its comments.
#[tokio::test]
async fn later_review_comment_pages_load_provider_batches_concurrently() {
    let server = MockServer::builder().start().await;
    let mut raw = pr(1);
    raw["review_comments"] = json!(600);
    // The clock starts at the first provider request: building the HTTP
    // client (system proxy lookup) is a one-time cost, not a batch scan.
    let first_request = std::sync::Arc::new(std::sync::OnceLock::<Instant>::new());
    let pr_template = ResponseTemplate::new(200).set_body_json(raw);
    let arrival = first_request.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(move |_: &wiremock::Request| {
            arrival.get_or_init(Instant::now);
            pr_template.clone()
        })
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
        let mut response = ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(500))
            .set_body_json(Value::Array(comments));
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
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/comments"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let data = run(
        &server,
        json!({"sections":["reviewComments"], "page": 15, "pageSize": 30, "extras": []}),
    )
    .await;
    let elapsed = first_request.get().expect("the PR was read").elapsed();
    let comments = data["pullRequests"][0]["reviewComments"]
        .as_array()
        .expect("review comments");
    assert_eq!(comments.len(), 30, "{data}");
    assert_eq!(comments[0]["body"], "note 420", "{data}");
    assert!(
        // A sequential re-scan of the five batches before the page takes 2.5 s.
        elapsed < Duration::from_millis(1_800),
        "sequential re-scan: {elapsed:?}"
    );
}
