//! Picked row fields and the date bounds every operation sends.
use super::*;

#[tokio::test]
async fn fields_project_listed_commits_end_to_end() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([{
            "sha": "aaa", "commit": {"message": "Fix cache",
            "committer": {"date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}
        }]),
    )
    .await;
    let query = serde_json::from_value(
        json!({"operation":"commit","owner":"o","repo":"r","fields":["commitSha"]}),
    )
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("commit listing");
    // The pick keeps the commit's id (commitSha) beside the picked keys.
    assert_eq!(data["commits"][0], json!({"commitSha": "aaa"}), "{data}");
}

/// A picked key no row carries (here a typo) still answers, with one
/// per-call warning instead of rows that silently lack the key.
#[tokio::test]
async fn an_absent_picked_key_is_named_once() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([{
            "sha": "aaa", "commit": {"message": "Fix cache",
            "committer": {"date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}
        }]),
    )
    .await;
    let query = serde_json::from_value(
        json!({"operation":"commit","owner":"o","repo":"r","fields":["messageHedline"]}),
    )
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("commit listing");
    assert_eq!(data["commits"][0], json!({"commitSha": "aaa"}), "{data}");
    assert_eq!(
        data["warnings"],
        json!(["No row has messageHedline."]),
        "{data}"
    );
}

/// `fields:["state","createdAt"]` on merged rows of a `state:"merged"`
/// search returns both keys: the pick names what the query implies, so it
/// is not dropped as implied (and no "No row has" warning fires).
#[tokio::test]
async fn picked_keys_survive_the_implied_drop() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 1, "incomplete_results": false, "items": [{
            "number": 7, "title": "t", "state": "closed",
            "pull_request": {"merged_at": "2026-09-20T10:00:00Z"},
            "user": {"login": "dev"}, "created_at": "2026-09-19T10:00:00Z",
            "repository_url": "https://api.github.com/repos/o/r"
        }]}),
    )
    .await;
    let query = serde_json::from_value(json!({
        "operation":"pullRequest","owner":"o","repo":"r","keywords":["x"],
        "state":"merged","fields":["state","createdAt"]
    }))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("pull request search");
    assert_eq!(
        data["pullRequests"][0],
        json!({"number": 7, "state": "merged", "createdAt": "2026-09-19T10:00:00Z"}),
        "{data}"
    );
    assert!(data.get("warnings").is_none(), "{data}");
}

/// `since`/`until` reach the provider request on every operation and
/// route: PR and issue searches as a `created:` window (a PR window routes
/// a REST-listable query to search), commit search as `committer-date:`,
/// and the REST commit listing (with `path`) as `since`/`until` params,
/// a date-only `until` covering its whole day.
#[tokio::test]
async fn since_until_reach_the_request_of_every_operation() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param, query_param_contains},
    };

    let server = MockServer::builder().start().await;
    let empty = json!({"total_count": 0, "incomplete_results": false, "items": []});
    for (route, scope) in [
        ("/api/v3/search/issues", "is:pr repo:o/r"),
        ("/api/v3/search/issues", "is:issue repo:o/r"),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param_contains("q", scope))
            .and(query_param_contains("q", "created:2026-10-01..2026-10-03"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty.clone()))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/search/commits"))
        .and(query_param_contains(
            "q",
            "committer-date:2026-10-01..2026-10-03",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .and(query_param("since", "2026-10-01"))
        .and(query_param("until", "2026-10-03T23:59:59Z"))
        .and(query_param("path", "src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    for row in [
        json!({"operation":"pullRequest","owner":"o","repo":"r"}),
        json!({"operation":"issue","owner":"o","repo":"r"}),
        json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"]}),
        json!({"operation":"commit","owner":"o","repo":"r","path":"src"}),
    ] {
        let mut row = row;
        row["since"] = json!("2026-10-01");
        row["until"] = json!("2026-10-03");
        let query = serde_json::from_value(row.clone()).expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .unwrap_or_else(|error| panic!("{row}: the window missed the request: {error:?}"));
        assert_eq!(data["status"], "empty", "{row}: {data}");
        // An empty window states no static note; the broadened search
        // without the window is the per-call repair.
        assert!(data.get("warnings").is_none(), "{row}: {data}");
        let broader = &data["next"]["broadenSearch"]["query"]["queries"];
        assert!(
            broader
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row.get("since").is_none())),
            "{row}: {data}"
        );
    }
}
