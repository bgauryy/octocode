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
            data["pullRequests"][0]["contentPagination"]["patches"]["next"]["nextPage"].is_object(),
            "row {index}: {data}"
        );
    }
    runtime.close().await;
}

/// A later filtered review-comment read prefetches the PR's count and scans
/// that provider span concurrently within one call. An unfiltered page needs
/// only its own provider batch, so separate continuation calls need not
/// overlap. Every selected body survives the response and collection pages.
#[tokio::test]
async fn filtered_review_comment_pages_load_provider_batches_concurrently() {
    let server = MockServer::builder().start().await;
    let mut raw = pr(1);
    raw["review_comments"] = json!(600);
    // The clock starts at the first provider request: building the HTTP
    // client (system proxy lookup) is a one-time cost, not a batch scan.
    let first_request = std::sync::Arc::new(std::sync::OnceLock::<Instant>::new());
    let comment_arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let delay = Duration::from_millis(500);
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
            .set_delay(delay)
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
        let arrivals = comment_arrivals.clone();
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/comments"))
            .and(query_param("page", page.to_string()))
            .respond_with(move |_: &wiremock::Request| {
                arrivals
                    .lock()
                    .expect("comment arrivals")
                    .push((page, Instant::now()));
                response.clone()
            })
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut input = json!({"queries":[{"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "sections":["reviewComments"],"matchString":["note"]}]});
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
        if seen_inputs.len() == 1 {
            assert!(first_request.get().expect("PR read").elapsed() < Duration::from_millis(1800));
        }
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
            .or_else(|| {
                data.pointer("/pullRequests/0/contentPagination/reviewComments/next/nextPage")
            });
        let Some(next) = next else { break };
        assert!(next["input"]["cursor"].is_string());
        assert_eq!(next["tool"], "ghGetHistoryItem");
        input = next["input"].clone();
    }
    assert_eq!(bodies.len(), 600);
    assert_eq!(
        bodies
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        600
    );
    assert_eq!(
        bodies
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        (0..600).map(|index| format!("note {index}")).collect(),
        "every requested review body is recovered"
    );
    let pages = comment_arrivals
        .lock()
        .expect("comment arrivals")
        .iter()
        .map(|(page, _)| *page)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(pages, (1..=6).collect(), "every provider page is read");
    let later = call_arrivals
        .iter()
        .map(|arrivals| {
            let mut pages = arrivals
                .iter()
                .filter(|(page, _)| *page > 1)
                .copied()
                .collect::<Vec<_>>();
            pages.sort_by_key(|(_, at)| *at);
            pages
        })
        .collect::<Vec<_>>();
    assert!(
        later.iter().any(|pages| pages
            .windows(2)
            .any(|pair| pair[0].0 != pair[1].0 && pair[1].1.duration_since(pair[0].1) < delay)),
        "provider batches within each filtered call were sequential: {later:?}"
    );
    runtime.close().await;
}
