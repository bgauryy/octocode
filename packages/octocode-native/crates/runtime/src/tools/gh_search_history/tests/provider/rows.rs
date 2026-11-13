//! The rows a search or listing returns: typed rows, read targets and next-hop anchors.
use super::*;

mod path_scope;

#[tokio::test]
async fn commit_search_read_targets_the_first_sha_without_a_candidate_list() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    let commit = |sha: &str| {
        json!({"sha": sha, "commit": {"message": format!("Fix {sha}\n\nWhy: the body names the cache"),
            "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}})
    };
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({
            "total_count": 3, "incomplete_results": false,
            "items": [commit("aaa"), commit("bbb"), commit("ccc")]
        }),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(json!({"operation":"commit",
        "owner":"o","repo":"r","keywords":["cache"]}))
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("history search");
    let read =
        crate::tools::test_support::checks::related_call(&data, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "commit"
        });
    assert_eq!(read["input"]["queries"][0]["ref"], "aaa", "{data}");
    assert_eq!(
        read["input"]["queries"][0]["sections"],
        json!(["patches"]),
        "{data}"
    );
    assert!(read.get("candidates").is_none(), "{data}");
    assert_eq!(data["commits"].as_array().map(Vec::len), Some(3), "{data}");
    // Commit search pages use the shared pagination: the first page starts
    // at offset 0.
    assert_eq!(data["pagination"]["offset"], 0, "{data}");
}

/// A plain issue listing pages `is:issue` search results, so every
/// page holds up to pageSize real issues (the REST /issues list
/// interleaves PRs) and page numbers advance by one. An empty or PR-only
/// repository is a clean empty row.
#[tokio::test]
async fn plain_issue_listing_pages_issue_search_without_pr_gaps() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    let issue = |n: u64| json!({"number": n, "title": format!("issue {n}"), "state": "open", "user": {"login": "dev"}});
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .and(query_param("q", "is:issue repo:o/full"))
        .and(query_param("sort", "created"))
        .and(query_param("order", "desc"))
        .respond_with(octocode_github::mock_http::SearchIndex(
            (1..=101).map(issue).collect(),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .and(query_param("q", "is:issue repo:o/prs-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 0, "incomplete_results": false, "items": []
        })))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let run = |query: Value| {
        let provider = &provider;
        async move {
            execute(
                provider,
                &serde_json::from_value(query).expect("query"),
                &default_context(),
                &Passthrough,
            )
            .await
            .expect("issue listing")
        }
    };
    let data = run(json!({"operation":"issue","owner":"o","repo":"full","page":2})).await;
    // Page 2 at the default 50 a page: GitHub's results 51 to 100.
    assert_eq!(
        data["issues"].as_array().map(|rows| rows
            .iter()
            .map(|row| row["number"].clone())
            .collect::<Vec<_>>()),
        Some((51..=100).map(|n| json!(n)).collect()),
        "{data}"
    );
    assert_eq!(data["pagination"]["offset"], 50, "{data}");
    // The cursor rides `next.nextPage` only, never the page block.
    assert_eq!(data.pointer("/pagination/nextPage"), None, "{data}");
    assert_eq!(
        data["next"]["nextPage"]["input"]["queries"][0]["page"], 3,
        "{data}"
    );
    assert_eq!(data["pagination"]["total"], 101, "{data}");
    assert!(data.get("skippedPullRequestPages").is_none(), "{data}");

    let empty = run(json!({"operation":"issue","owner":"o","repo":"prs-only"})).await;
    assert_eq!(empty["issues"], json!([]), "{empty}");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert!(empty.get("pagination").is_none(), "{empty}");
    assert!(empty["next"].get("nextPage").is_none(), "{empty}");
}

/// The happy-path anchors of each operation's row: the id the next hop
/// reads (`number`, or `commitSha` as its `ref`), `state` (absent only when
/// the query fixes it), `author`, and exactly one date.
#[tokio::test]
async fn happy_path_rows_carry_the_next_hop_anchors() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls",
        200,
        json!([
            {"number": 1, "title": "a", "state": "closed", "merged_at": "2026-09-02T00:00:00Z",
                "created_at": "2026-09-01T00:00:00Z", "user": {"login": "dev"}},
            {"number": 2, "title": "b", "state": "open",
                "created_at": "2026-09-03T00:00:00Z", "user": {"login": "dev"}}
        ]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 1, "incomplete_results": false, "items": [
            {"number": 9, "title": "bug", "state": "open", "created_at": "2026-09-04T00:00:00Z",
                "user": {"login": "dev"}}
        ]}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([{"sha": "c0ffee", "author": {"login": "dev"},
            "commit": {"message": "fix (#1)\n\nbody", "author": {"name": "Dev", "date": "2026-09-05T00:00:00Z"}}}]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({"total_count": 1, "incomplete_results": false, "items": [
            {"sha": "beef", "author": {"login": "dev"},
                "commit": {"message": "fix", "author": {"name": "Dev", "date": "2026-09-06T00:00:00Z"}}}
        ]}),
    )
    .await;
    let provider = one_shot_provider(&server);
    let run = |row: Value| {
        let provider = &provider;
        async move {
            let query = serde_json::from_value(row.clone()).expect("query");
            execute(provider, &query, &default_context(), &Passthrough)
                .await
                .unwrap_or_else(|error| panic!("{row}: {error:?}"))
        }
    };
    let assert_repository = |row: &Value| {
        assert_eq!(row["owner"], "o", "{row}");
        assert_eq!(row["repo"], "r", "{row}");
    };

    let prs = run(json!({"operation":"pullRequest","owner":"o","repo":"r"})).await;
    for row in prs["pullRequests"].as_array().expect("pull requests") {
        assert_repository(row);
        assert!(row["number"].is_u64(), "{row}");
        // Who wrote it is a `fields` pick.
        assert!(row.get("author").is_none(), "{row}");
    }
    let authors =
        run(json!({"operation":"pullRequest","owner":"o","repo":"r","fields":["author"]})).await;
    for row in authors["pullRequests"].as_array().expect("pull requests") {
        assert_eq!(row["author"], "dev", "{row}");
    }
    assert_eq!(prs["pullRequests"][0]["state"], "merged");
    assert_eq!(prs["pullRequests"][0]["mergedAt"], "2026-09-02T00:00:00Z");
    assert_eq!(prs["pullRequests"][1]["createdAt"], "2026-09-03T00:00:00Z");
    // A state the query fixes is not restated per row.
    let open =
        run(json!({"operation":"pullRequest","owner":"o","repo":"r","filters":{"state":"open"}}))
            .await;
    assert!(open["pullRequests"][1].get("state").is_none(), "{open}");

    let issues = run(json!({"operation":"issue","owner":"o","repo":"r"})).await;
    assert_repository(&issues["issues"][0]);
    assert!(issues["issues"][0]["number"].is_u64(), "{issues}");

    let listed = run(json!({"operation":"commit","owner":"o","repo":"r"})).await;
    let searched =
        run(json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"]})).await;
    for data in [&listed, &searched] {
        let row = &data["commits"][0];
        assert_repository(row);
        assert!(row["commitSha"].is_string(), "{row}");
        assert!(row["messageHeadline"].is_string(), "{row}");
        // The commit-search window is the default branch (the description
        // says so once): no per-response scope label.
        assert!(data.get("scope").is_none(), "{data}");
    }
    // A complete list states it (`hasMore:false`) as a search does; the
    // response stage hides a finished single page unless `echo` asks.
    for data in [&listed, &searched] {
        assert_eq!(data["pagination"]["hasMore"], false, "{data}");
        assert!(data["next"].get("nextPage").is_none(), "{data}");
    }
}

/// A commit page whose headlines name no `(#N)` offers the PR search by
/// the first row's SHA (`findPullRequest`); a named PR is read instead.
#[tokio::test]
async fn commit_rows_without_a_named_pull_request_find_it_by_sha() {
    use wiremock::MockServer;
    let server = MockServer::builder().start().await;
    let commit = |sha: &str, message: &str| {
        json!({"sha": sha, "commit": {"message": message,
            "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}})
    };
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({"total_count": 2, "incomplete_results": false,
            "items": [commit("aaa", "Fix cache"), commit("bbb", "Bump deps")]}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([commit("ccc", "Fix cache (#9)")]),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(
        json!({"operation":"commit","owner":"o","repo":"r","keywords":["cache"]}),
    )
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("commit search");
    let find =
        crate::tools::test_support::checks::related_call(&data, "ghSearchHistory", |input| {
            input["queries"][0]["operation"] == "pullRequest"
        });
    assert_eq!(find["tool"], "ghSearchHistory", "{data}");
    assert_eq!(
        find["input"]["queries"][0],
        json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["aaa"]}),
        "{data}"
    );
    let query = serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r"}))
        .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("commit listing");
    assert!(data["next"].get("findPullRequest").is_none(), "{data}");
    assert_eq!(
        crate::tools::test_support::checks::related_call(&data, "ghGetHistoryItem", |input| input
            ["queries"][0]["operation"]
            == "pullRequest")["input"]["queries"][0]["number"],
        9,
        "{data}"
    );
}

/// A merge commit's headline (`Merge pull request #N from …`) names its PR
/// as a squash headline's `(#N)` does: the lead reads that PR, not a SHA
/// keyword search whose best match can be another PR.
#[tokio::test]
async fn a_merge_commit_headline_leads_to_the_pull_request_it_names() {
    use wiremock::MockServer;
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([{"sha": "aaa", "commit": {
            "message": "Merge pull request #6 from someone/patch-1\n\nFix cache",
            "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}}]),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r"}))
        .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("commit listing");
    let read =
        crate::tools::test_support::checks::related_call(&data, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "pullRequest"
        });
    assert_eq!(read["input"]["queries"][0]["number"], 6, "{data}");
    assert!(data["next"].get("findPullRequest").is_none(), "{data}");
}

/// `sort.by` names the row field it orders by: `commentsCount` reaches GitHub
/// as `comments` and `updatedAt` as `updated`, and a page ranked by either
/// states that field on each row, which a default row leaves to `fields`.
#[tokio::test]
async fn a_sorted_history_search_states_the_field_it_is_ranked_by() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };
    let item = |n: u64, comments: u64, pull_request: bool| {
        let mut item = json!({"number": n, "title": format!("item {n}"), "state": "open",
            "user": {"login": "dev"}, "comments": comments,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-02-02T00:00:00Z"});
        if pull_request {
            item["pull_request"] = json!({"merged_at": null});
        }
        item
    };
    for (operation, rows_key) in [("pullRequest", "pullRequests"), ("issue", "issues")] {
        for (by, order, provider_key, stated) in [
            ("commentsCount", "desc", "comments", "commentsCount"),
            ("updatedAt", "desc", "updated", "updatedAt"),
            ("updatedAt", "asc", "updated", "updatedAt"),
        ] {
            let server = MockServer::builder().start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/issues"))
                .and(query_param("sort", provider_key))
                .and(query_param("order", order))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count": 2, "incomplete_results": false,
                    "items": [item(7, 40, operation == "pullRequest"), item(9, 3, operation == "pullRequest")]
                })))
                .expect(1)
                .mount(&server)
                .await;
            let provider = one_shot_provider(&server);
            let query =
                serde_json::from_value(json!({"operation":operation,"owner":"o","repo":"r",
                "keywords":["cache"],"sort":{"by":by,"order":order}}))
                .expect("query");
            let data = execute(&provider, &query, &default_context(), &Passthrough)
                .await
                .expect("history search");
            let rows = data[rows_key].as_array().expect("rows");
            assert_eq!(rows.len(), 2, "{data}");
            for row in rows {
                assert!(row.get(stated).is_some(), "{operation} {by}: {data}");
                // Nothing else a default row leaves to `fields` comes back.
                for detail in ["author", "labels", "baseRef"] {
                    assert!(
                        row.get(detail).is_none(),
                        "{operation} {by} {detail}: {data}"
                    );
                }
            }
            if stated == "commentsCount" {
                assert_eq!(rows[0]["commentsCount"], 40, "{data}");
            } else {
                assert!(rows[0].get("commentsCount").is_none(), "{data}");
                assert_eq!(rows[0]["updatedAt"], "2026-02-02T00:00:00Z", "{data}");
            }
        }
        // The retired spellings are no aliases.
        for by in ["bestMatch", "commentCount"] {
            let row = json!({"operation":operation,"owner":"o","repo":"r","keywords":["cache"],
                "sort":{"by":by,"order":"desc"}});
            assert!(
                serde_json::from_value::<GhSearchHistoryQuery>(row.clone()).is_err(),
                "{row}"
            );
        }
    }
}
