//! Picked row fields and the date bounds every operation sends.
use super::*;
use octocode_test_support::json_fixture;

#[tokio::test]
async fn fields_project_listed_commits_end_to_end() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json_fixture(
            r#"[
                {
                    "sha": "aaa",
                    "commit": {
                        "message": "Fix cache",
                        "committer": {"date": "2026-01-01T00:00:00Z"}
                    },
                    "author": {"login": "dev"}
                }
            ]"#,
        ),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "fields": ["commitSha"]
        }"#,
    ))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("commit listing");
    // Projection keeps the complete identity needed to read the commit.
    assert_eq!(
        data["commits"][0],
        json_fixture(r#"{"owner": "o", "repo": "r", "commitSha": "aaa"}"#),
        "{data}"
    );
}

/// A default commit row names its author (the login, else the git name,
/// never an email) from the listing that already carries it: who wrote a
/// commit needs no second request.
#[tokio::test]
async fn a_default_commit_row_names_its_author() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json_fixture(
            r#"[
                {
                    "sha": "aaa",
                    "commit": {
                        "message": "Fix cache",
                        "author": {"name": "Dev One", "email": "dev@example.test"},
                        "committer": {"date": "2026-01-01T00:00:00Z"}
                    },
                    "author": {"login": "dev"}
                },
                {
                    "sha": "bbb",
                    "commit": {
                        "message": "Tidy",
                        "author": {"name": "Git Name", "email": "git@example.test"},
                        "committer": {"date": "2026-01-02T00:00:00Z"}
                    }
                }
            ]"#,
        ),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{"operation": "commit", "owner": "o", "repo": "r"}"#,
    ))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("commit listing");
    assert_eq!(data["commits"][0]["author"], "dev", "{data}");
    assert_eq!(data["commits"][1]["author"], "Git Name", "{data}");
    assert!(!data.to_string().contains("example.test"), "{data}");
}

/// A picked key no row carries (here `committer`, omitted when it repeats
/// the author) still answers, with one
/// per-call warning instead of rows that silently lack the key; the
/// warning names the keys the rows do carry.
#[tokio::test]
async fn an_absent_picked_key_is_named_once() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json_fixture(
            r#"[
                {
                    "sha": "aaa",
                    "commit": {
                        "message": "Fix cache",
                        "committer": {"date": "2026-01-01T00:00:00Z"}
                    },
                    "author": {"login": "dev"}
                }
            ]"#,
        ),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "fields": ["committer"]
        }"#,
    ))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("commit listing");
    assert_eq!(
        data["commits"][0],
        json_fixture(r#"{"owner": "o", "repo": "r", "commitSha": "aaa"}"#),
        "{data}"
    );
    assert_eq!(
        data["warnings"],
        json_fixture(
            r#"[
                "No row has committer; these rows have: committedAt, messageHeadline, author."
            ]"#
        ),
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
        json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "number": 7,
                        "title": "t",
                        "state": "closed",
                        "pull_request": {"merged_at": "2026-09-20T10:00:00Z"},
                        "user": {"login": "dev"},
                        "created_at": "2026-09-19T10:00:00Z",
                        "repository_url": "https://api.github.com/repos/o/r"
                    }
                ]
            }"#,
        ),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["x"],
            "fields": ["state", "createdAt"],
            "filters": {"isMerged": true}
        }"#,
    ))
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
        json_fixture(
            r#"{
                "owner": "o",
                "repo": "r",
                "number": 7,
                "state": "merged",
                "createdAt": "2026-09-19T10:00:00Z"
            }"#
        ),
        "{data}"
    );
    assert!(data.get("warnings").is_none(), "{data}");
}

/// A count GitHub states as 0 is a value the row carries: an issue page
/// whose every row has no comments answers `commentsCount: 0` on each, with
/// no "No row has" warning, which is only for a key no row carries.
#[tokio::test]
async fn a_zero_count_is_carried_not_reported_missing() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    let issue = |number: u64| {
        json!({"number": number, "title": "t", "state": "open", "comments": 0,
            "user": {"login": "dev"}, "created_at": "2026-09-19T10:00:00Z",
            "repository_url": "https://api.github.com/repos/o/r"})
    };
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 2, "incomplete_results": false, "items": [issue(1), issue(2)]}),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["x"],
            "filters": {"state": "open"},
            "fields": ["commentsCount"]
        }"#,
    ))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("issue search");
    for row in data["issues"].as_array().expect("issues") {
        assert_eq!(row["commentsCount"], 0, "{data}");
    }
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
    let empty = json_fixture(r#"{"total_count": 0, "incomplete_results": false, "items": []}"#);
    for (route, scope) in [
        ("/api/v3/search/issues", "is:pr repo:o/r"),
        ("/api/v3/search/issues", "is:issue repo:o/r"),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .and(query_param_contains("q", scope))
            .and(query_param_contains(
                "q",
                "created:2026-10-01T00:00:00Z..2026-10-03T23:59:59Z",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty.clone()))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/search/commits"))
        .and(query_param_contains(
            "q",
            "committer-date:2026-10-01T00:00:00Z..2026-10-03T23:59:59Z",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .and(query_param("since", "2026-10-01T00:00:00Z"))
        .and(query_param("until", "2026-10-03T23:59:59Z"))
        .and(query_param("path", "src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    for row in [
        json_fixture(r#"{"operation": "pullRequest", "owner": "o", "repo": "r"}"#),
        json_fixture(r#"{"operation": "issue", "owner": "o", "repo": "r"}"#),
        json_fixture(
            r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "keywords": ["fix"]
            }"#,
        ),
        json_fixture(
            r#"{
                "operation": "commit",
                "owner": "o",
                "repo": "r",
                "path": "src"
            }"#,
        ),
    ] {
        let mut row = row;
        let range = json_fixture(
            r#"{
                "from": "2026-10-01T00:00:00Z",
                "to": "2026-10-03T23:59:59Z"
            }"#,
        );
        row["filters"] = if row["operation"] == "commit" {
            json!({"committedAt":range})
        } else {
            json!({"createdAt":range})
        };
        let query = serde_json::from_value(row.clone()).expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .unwrap_or_else(|error| panic!("{row}: the window missed the request: {error:?}"));
        assert_eq!(data["status"], "empty", "{row}: {data}");
        // An empty window states no static note; the broadened search
        // without the window is the per-call repair.
        assert!(data.get("warnings").is_none(), "{row}: {data}");
        let broader = &octocode_runtime_base::test_support::checks::related_call(
            &data,
            "ghSearchHistory",
            |_| true,
        )["input"]["queries"];
        assert!(
            broader
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row.get("filters").is_none())),
            "{row}: {data}"
        );
    }
}

/// `fields:["mergedBy"]`: GitHub's search returns no merger, so no row can
/// carry it; the warning leads to the pull-request read that does.
#[tokio::test]
async fn a_merged_by_pick_leads_to_the_pull_request_read() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "number": 7,
                        "title": "t",
                        "state": "closed",
                        "pull_request": {"merged_at": "2026-09-20T10:00:00Z"},
                        "user": {"login": "dev"},
                        "created_at": "2026-09-19T10:00:00Z",
                        "repository_url": "https://api.github.com/repos/o/r"
                    }
                ]
            }"#,
        ),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["x"],
            "fields": ["mergedBy"]
        }"#,
    ))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("pull request search");
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("mergedBy is not in search rows"),
        "{data}"
    );
    assert!(warnings.contains("ghGetHistoryItem"), "{data}");
    assert!(data["pullRequests"][0].get("mergedBy").is_none(), "{data}");
}
