//! Materialize reads up to the request budget's concurrency at once, writes
//! in listing order, and stops on a rate limit through the budget's circuit.
use super::*;
use crate::tools::gh_shared::test_support::{default_context, mock_endpoint};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_github::mock_http::{Arrivals, Delayed, mount_json};
use octocode_github::{GitHubBudget, GitHubProvider, GitHubTransport, RetryPolicy};
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FILES: usize = 60;
const LATENCY: Duration = Duration::from_millis(300);

fn provider(server: &MockServer) -> GitHubProvider {
    GitHubProvider::new(
        GitHubTransport::new(
            mock_endpoint(server),
            RetryPolicy {
                max_attempts: 1,
                ..Default::default()
            },
            GitHubBudget::relaxed(),
        )
        .expect("transport"),
    )
}

fn name(index: usize) -> String {
    format!("src/f{index:03}.txt")
}

async fn mount_tree(server: &MockServer) {
    mount_ref(server, "main").await;
    let tree = (0..FILES)
        .map(|index| json!({"path": name(index), "type": "blob", "size": 12}))
        .chain([json!({"path": "src", "type": "tree"})])
        .collect::<Vec<_>>();
    mount_json(
        server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha": "c".repeat(40), "truncated": false, "tree": tree}),
    )
    .await;
}

async fn materialize(provider: &GitHubProvider, home: &std::path::Path) -> Value {
    let query = serde_json::from_value(json!({"operation":"tree","owner":"a","repo":"b",
        "ref":"main","maxDepth":5,"materialize":true}))
    .expect("tree query");
    crate::tools::gh_structure::execute(provider, &query, &default_context(), home, None)
        .await
        .expect("materialize")
        .data
}

/// The most requests that arrived within one `LATENCY` of each other: each
/// is still waiting for its answer, so that many were in flight together.
fn most_in_flight(arrivals: &Arrivals) -> usize {
    let times = arrivals.times();
    times
        .iter()
        .map(|start| {
            times
                .iter()
                .filter(|at| *at >= start && at.duration_since(*start) < LATENCY)
                .count()
        })
        .max()
        .unwrap_or(0)
}

#[tokio::test]
async fn materialize_keeps_the_budgets_concurrency_in_flight_and_writes_in_order() {
    let server = MockServer::builder().start().await;
    mount_tree(&server).await;
    let arrivals = Arrivals::default();
    for index in 0..FILES {
        let body = format!("body {index:03}");
        let delayed = Delayed {
            arrivals: arrivals.clone(),
            ..Delayed::new(
                LATENCY,
                ResponseTemplate::new(200).set_body_json(json!({
                    "type": "file", "encoding": "base64", "content": STANDARD.encode(&body)
                })),
            )
        };
        Mock::given(method("GET"))
            .and(path(format!(
                "/api/v3/repos/a/b/contents/{}",
                name(index).replace('/', "%2F")
            )))
            .respond_with(delayed)
            .mount(&server)
            .await;
    }
    let provider = provider(&server);
    let limit = provider.transport.read_concurrency();
    assert!(limit > 8 && limit < FILES, "{limit}");
    let home = crate::tools::test_support::Fixture::new();

    let data = materialize(&provider, &home.0).await;
    let flight = most_in_flight(&arrivals);
    assert!(flight <= limit, "{flight} in flight, budget allows {limit}");
    assert!(flight > 8, "reads stayed at the old fixed 8: {flight}");
    assert_eq!(arrivals.count(), FILES);

    let local = std::path::PathBuf::from(data["location"]["path"].as_str().expect("location"));
    let mut modified = Vec::new();
    for index in 0..FILES {
        let file = local.join(name(index));
        assert_eq!(
            std::fs::read_to_string(&file).expect("written"),
            format!("body {index:03}")
        );
        modified.push(
            std::fs::metadata(&file)
                .and_then(|meta| meta.modified())
                .expect("mtime"),
        );
    }
    assert!(
        modified.windows(2).all(|pair| pair[0] <= pair[1]),
        "files were written out of listing order"
    );
    assert_eq!(data["location"]["coverage"]["state"], "complete", "{data}");
}

/// A rate limit on the contents reads ends the call through the budget's
/// circuit: the reads in flight fail, and no read starts after them.
#[tokio::test]
async fn a_rate_limit_stops_the_reads_through_the_budgets_circuit() {
    let server = MockServer::builder().start().await;
    mount_tree(&server).await;
    let arrivals = Arrivals::default();
    let limited = Delayed {
        arrivals: arrivals.clone(),
        ..Delayed::new(
            Duration::from_millis(50),
            ResponseTemplate::new(429).insert_header("retry-after", "0"),
        )
    };
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/contents/"))
        .respond_with(limited)
        .mount(&server)
        .await;
    let provider = provider(&server);
    let home = crate::tools::test_support::Fixture::new();
    let query = serde_json::from_value(json!({"operation":"tree","owner":"a","repo":"b",
        "ref":"main","maxDepth":5,"materialize":true}))
    .expect("tree query");
    let outcome = tokio::time::timeout(
        Duration::from_secs(20),
        crate::tools::gh_structure::execute(&provider, &query, &default_context(), &home.0, None),
    )
    .await;
    assert!(outcome.is_ok(), "the rate-limited materialize hung");
    assert!(
        arrivals.count() < FILES,
        "{} reads sent after the limit",
        arrivals.count()
    );
}
