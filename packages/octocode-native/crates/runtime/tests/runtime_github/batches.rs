//! Bulk GitHub queries: concurrency, order, per-query requests and contract-valid picked repositories.

use crate::support::{self, Run as _};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::{Arrivals, Delayed};
use serde_json::json;
use std::time::Duration;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Verifies that independent GitHub API calls run concurrently while output
/// rows retain their input order.
#[tokio::test]
async fn three_github_bulk_queries_are_concurrent_and_preserve_order() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let arrivals = Arrivals::default();

    for name in ["alpha.rs", "beta.rs", "gamma.rs"] {
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", name).await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(Delayed {
                arrivals: arrivals.clone(),
                ..Delayed::new(
                    Duration::from_millis(200),
                    ResponseTemplate::new(200).set_body_json(json!({
                        "type": "file",
                        "encoding": "base64",
                        "content": STANDARD.encode("fn placeholder(){}"),
                        "size": 18,
                        "sha": "a".repeat(40),
                        "path": name
                    })),
                )
            })
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    let queries = ["alpha.rs", "beta.rs", "gamma.rs"].map(|name| {
        json!({
            "owner": "a",
            "repo": "b",
            "path": name,
            "ref": sha
        })
    });
    let outcome = runtime
        .run(
            "github-bulk".into(),
            "ghGetFileContent".into(),
            json!({"queries": queries}),
        )
        .await
        .expect("GitHub bulk result");
    let rows = outcome.structured_content["results"]
        .as_array()
        .expect("GitHub result rows");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row["index"].as_u64())
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2)]
    );
    assert!(
        rows.iter().all(|row| row.get("status").is_none()),
        "{rows:?}"
    );
    {
        let arrivals = arrivals.times();
        assert_eq!(arrivals.len(), 3);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "GitHub API calls were dispatched serially: {arrivals:?}"
        );
    }

    runtime.close().await;
}

/// Verifies that three sequential single-query calls each produce exactly one
/// server-side HTTP request — `.expect(1)` on each mock endpoint is the
/// observable proof.
#[tokio::test]
async fn three_sequential_queries_each_hit_the_server() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";

    // One SHA endpoint shared by all three queries.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;

    // Three distinct content endpoints — each MUST be hit exactly once.
    for name in ["p.rs", "q.rs", "r.rs"] {
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", name).await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn ok(){}"),
                "size": 9,
                "sha": "b".repeat(40),
                "path": name
            })))
            .expect(1) // exactly 1 request per file endpoint
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    for name in ["p.rs", "q.rs", "r.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name,
                   "ref": "main"}),
        )
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_ne!(
            row_status(&outcome),
            "error",
            "{name} failed: {}",
            outcome.structured_content
        );
    }
    // WireMock verifies `.expect(1)` on drop: each content endpoint hit exactly once.

    runtime.close().await;
}

#[tokio::test]
async fn gh_search_picked_repositories_are_contract_valid() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "full_name": "o/r",
                        "name": "r",
                        "html_url": "https://x",
                        "default_branch": "main"
                    }
                ]
            }"#,
        )))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    // The response carries the identity needed for an independent tree read.
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json_fixture(
            r#"{
                "operation": "search",
                "keywords": ["x"],
                "fields": ["stars"]
            }"#,
        ),
    )
    .await
    .expect("picked repositories");
    assert_eq!(
        row_data(&outcome)["repositories"],
        json_fixture(r#"[{"repo": "r", "stars": 0}]"#)
    );
    assert_eq!(row_data(&outcome)["owner"], "o");
    assert!(row_data(&outcome).get("related").is_none());
    let repository = &row_data(&outcome)["repositories"][0];
    let tree =
        json!({"operation":"tree","owner":row_data(&outcome)["owner"],"repo":repository["repo"]});
    assert_eq!(tree["owner"], "o");
    assert_eq!(tree["repo"], "r");
    call(&runtime, "ghStructure", tree)
        .await
        .expect("the identity-based request is contract-valid and runs");
    assert!(row_data(&outcome).get("next").is_none());
    runtime.close().await;
}
