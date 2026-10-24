//! Search paging conformance: one table of the page outcomes every GitHub
//! search tool shares.
use crate::security::scan::Passthrough;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use crate::tools::{gh_search_code, gh_search_history, gh_search_repo};
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use wiremock::MockServer;

/// A row of the conformance table: the tool's answer as `data`.
async fn page_data(server: &MockServer, tool: &str, row: Value) -> Value {
    let context = default_context();
    match tool {
        "ghSearchCode" => {
            let query = serde_json::from_value(row).expect("ghSearchCode query");
            gh_search_code::execute(&one_shot_provider(server), &query, &context, &Passthrough)
                .await
                .expect("ghSearchCode page")
                .data
        }
        "ghSearchRepo" => {
            let query = serde_json::from_value(row).expect("ghSearchRepo query");
            gh_search_repo::execute(&one_shot_provider(server), &query, &context)
                .await
                .expect("ghSearchRepo page")
                .data
        }
        "ghSearchHistory" => {
            let query = serde_json::from_value(row).expect("ghSearchHistory query");
            gh_search_history::execute(&one_shot_provider(server), query, &context, &Passthrough)
                .await
                .expect("ghSearchHistory page")
        }
        other => panic!("no conformance row for {other}"),
    }
}

/// A page past the last of a search that has results (or of a REST list
/// past its first page) is out of range, not an empty search: the page
/// says `outOfRange`, lists no row, offers no broadening and no hint, and is
/// not partial (the shared response stage words the page and restarts it).
/// A search keeps its total; no rename lookup runs.
#[tokio::test]
async fn a_page_past_the_end_is_out_of_range_not_an_empty_search() {
    let server = MockServer::builder().start().await;
    let empty_search =
        |total: usize| json!({"total_count": total, "incomplete_results": false, "items": []});
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        empty_search(12),
    )
    .await;
    mount_json(&server, "/api/v3/search/issues", 200, empty_search(12)).await;
    mount_json(&server, "/api/v3/search/code", 200, empty_search(12)).await;
    mount_json(&server, "/api/v3/repos/o/r/commits", 200, json!([])).await;
    let history = |operation: &str, keywords: bool| {
        let mut row = json!({"operation": operation, "owner": "o", "repo": "r",
            "page": 9});
        if keywords {
            row["keywords"] = json!(["fix"]);
        }
        row
    };
    for (tool, row, rows_key, total) in [
        (
            "ghSearchCode",
            json!({"owner":"a","repo":"b","keywords":["needle"],"page":9}),
            "files",
            Some(12),
        ),
        (
            "ghSearchRepo",
            json!({"operation":"search","keywords":["x"],"page":9}),
            "repositories",
            Some(12),
        ),
        (
            "ghSearchHistory",
            history("pullRequest", true),
            "pullRequests",
            Some(12),
        ),
        (
            "ghSearchHistory",
            history("issue", true),
            "issues",
            Some(12),
        ),
        ("ghSearchHistory", history("commit", false), "commits", None),
    ] {
        let data = page_data(&server, tool, row).await;
        let case = format!("{tool} {rows_key}: {data}");
        assert_eq!(
            data.pointer("/pagination/outOfRange"),
            Some(&json!(true)),
            "{case}"
        );
        assert_eq!(data.get(rows_key), Some(&json!([])), "{case}");
        assert_eq!(
            data.pointer("/pagination/totalItems")
                .and_then(Value::as_u64),
            total,
            "{case}"
        );
        for absent in ["/next/broadenSearch", "/hints", "/isPartial", "/warnings"] {
            assert_eq!(data.pointer(absent), None, "{absent}: {case}");
        }
    }
}
