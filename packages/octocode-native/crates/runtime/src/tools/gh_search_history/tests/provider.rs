//! Searches and listings against a mock GitHub API.
use super::*;

#[tokio::test]
async fn cap_is_terminal_only_after_the_last_reachable_page() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({
            "total_count": 1001, "incomplete_results": false,
            "items": [{"number": 3, "title": "Fix", "state": "open", "user": {"login": "dev"},
                "repository_url": "https://api.github.com/repos/acme/widget"}]
        }),
    )
    .await;
    let provider = one_shot_provider(&server);
    for page in [1, 1000] {
        let query = serde_json::from_value(json!({"operation":"pullRequest","mainGoal": "test", "reasoning":"test","keywords":["fix"],"pageSize":1,"page":page})).expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("history search");
        assert_eq!(data["isPartial"], true, "{data}");
        assert_eq!(
            data["partialReasons"],
            json!(["providerResultCap"]),
            "{data}"
        );
        assert_eq!(
            data["terminalLimit"].as_bool().unwrap_or(false),
            page == 1000,
            "{data}"
        );
        assert_eq!(data["next"]["nextPage"].is_object(), page < 1000, "{data}");
        // A capped page states GitHub's full match count.
        assert!(
            data["warnings"].as_array().is_some_and(|warnings| warnings
                .iter()
                .any(|warning| warning.as_str().is_some_and(|w| w.contains("1001")))),
            "{data}"
        );
        // A search beyond one repository names each row's owner and repo
        // (the read's own inputs), and the read targets it.
        assert_eq!(
            (
                &data["pullRequests"][0]["owner"],
                &data["pullRequests"][0]["repo"]
            ),
            (&json!("acme"), &json!("widget")),
            "{data}"
        );
        let read = &data["next"]["readPullRequest"]["query"]["queries"][0];
        assert_eq!(
            (&read["owner"], &read["repo"], &read["number"]),
            (&json!("acme"), &json!("widget"), &json!(3)),
            "{data}"
        );
    }
}

/// Commit rows are an index of headlines: the commit read targets the
/// first sha and takes any other row's sha; the rows already name them,
/// so the read carries no second list.
/// GitHub answers a search scoped to a missing or hidden repository with
/// a 422 "cannot be searched": that is `notFound`, not invalid input.
#[tokio::test]
async fn search_of_a_missing_repository_is_not_found() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    let unsearchable = json!({
        "message": "Validation Failed",
        "errors": [{"message": "The listed users and repositories cannot be searched either because the resources do not exist or you do not have permission to view them.",
            "resource": "Search", "field": "q", "code": "invalid"}]
    });
    for route in ["/api/v3/search/issues", "/api/v3/search/commits"] {
        mount_json(&server, route, 422, unsearchable.clone()).await;
    }
    let provider = one_shot_provider(&server);
    for row in [
        json!({"operation":"pullRequest","owner":"o","repo":"missing","keywords":["fix"]}),
        json!({"operation":"commit","owner":"o","repo":"missing","keywords":["fix"]}),
    ] {
        let query = serde_json::from_value(row.clone()).expect("query");
        let failure = super::super::execute(&provider, query, &default_context(), &Passthrough)
            .await
            .expect_err("missing repository");
        assert_eq!(
            failure.error.kind,
            ProviderErrorKind::NotFound,
            "{row}: {failure:?}"
        );
        // The recovery is the repository's spelling, not a history filter.
        let next = failure.next.expect("a recovery lead");
        assert_eq!(next["findRepository"]["tool"], "ghSearchRepo", "{next}");
    }
}

/// GitHub answers a search naming a missing user (`author:ghost`) with a
/// 422 "The listed users cannot be searched…", without "repositories": the
/// repository is fine. That is a missing user, with the same query minus its
/// person qualifiers as the way on; a repository 422 stays a repository.
#[tokio::test]
async fn search_naming_a_missing_user_is_not_a_missing_repository() {
    use octocode_github::ProviderErrorReason;
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        422,
        json!({
            "message": "Validation Failed",
            "errors": [{"message": "The listed users cannot be searched either because the users do not exist or you do not have permission to view the users.",
                "resource": "Search", "field": "q", "code": "invalid"}]
        }),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"pullRequest","owner":"o","repo":"r",
        "qualifiers":"author:ghost -reviewed-by:bot label:bug"
    }))
    .expect("query");
    let failure = crate::tools::gh_search_history::execute(
        &provider,
        query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect_err("missing user");
    assert_eq!(failure.error.kind, ProviderErrorKind::NotFound);
    assert_eq!(
        failure.error.reason,
        Some(ProviderErrorReason::UserNotFound)
    );
    let (message, hint) = history_failure(&failure.error, true);
    assert!(
        message.contains("user") && !message.contains("Repository"),
        "{message}"
    );
    assert!(
        message.contains("cannot be searched"),
        "GitHub's words stay: {message}"
    );
    // The `withoutUsers` lead is the repair; no hint restates the message.
    assert_eq!(hint, None);
    let lead = failure
        .next
        .expect("a lead")
        .pointer("/withoutUsers/query/queries/0")
        .cloned();
    let lead = lead.expect("withoutUsers is a ghSearchHistory query");
    assert_eq!(lead["qualifiers"], "label:bug", "{lead}");
    assert_eq!(
        (lead["owner"].as_str(), lead["repo"].as_str()),
        (Some("o"), Some("r"))
    );

    // Person qualifiers only: the lead drops `qualifiers` altogether.
    let only: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"pullRequest","owner":"o","repo":"r","qualifiers":"author:ghost"
    }))
    .expect("query");
    let failure =
        crate::tools::gh_search_history::execute(&provider, only, &default_context(), &Passthrough)
            .await
            .expect_err("missing user");
    let lead = failure.next.expect("a lead");
    assert!(
        lead["withoutUsers"]["query"]["queries"][0]
            .get("qualifiers")
            .is_none(),
        "{lead}"
    );
}

/// A commit list (no keywords) answers an unknown author with no rows, which
/// reads as a true empty. Only an empty list confirms the login: a missing
/// one is the typed missing-user failure with its `withoutUsers` lead, and a
/// list with rows never pays for the lookup.
#[tokio::test]
async fn an_empty_commit_list_naming_a_missing_author_is_the_missing_user_failure() {
    use octocode_github::ProviderErrorReason;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/ghost"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "dev"})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .and(query_param("author", "ghost"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .and(query_param("author", "dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "sha": "aaa", "commit": {"message": "Fix cache",
            "committer": {"date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}
        }])))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let list = |login: &str| -> GhSearchHistoryQuery {
        serde_json::from_value(json!({
            "operation":"commit","owner":"o","repo":"r","qualifiers": format!("author:{login}")
        }))
        .expect("query")
    };
    let failure = crate::tools::gh_search_history::execute(
        &provider,
        list("ghost"),
        &default_context(),
        &Passthrough,
    )
    .await
    .expect_err("missing user");
    assert_eq!(
        failure.error.reason,
        Some(ProviderErrorReason::UserNotFound),
        "{:?}",
        failure.error
    );
    assert!(
        failure
            .next
            .as_ref()
            .and_then(|next| next.pointer("/withoutUsers/query/queries/0"))
            .is_some_and(|lead| lead.get("qualifiers").is_none()),
        "{:?}",
        failure.next
    );
    assert!(
        crate::tools::gh_search_history::execute(
            &provider,
            list("dev"),
            &default_context(),
            &Passthrough
        )
        .await
        .is_ok(),
        "a list with rows"
    );
}

/// GitHub's commit search drops an unknown `author:`/`committer:` login
/// without error and answers the unfiltered total. A named login is
/// confirmed once per call (`GET /users/{login}`): a missing one is the same
/// typed missing-user failure and `withoutUsers` lead that issue and PR
/// search return, and the search never runs.
#[tokio::test]
async fn commit_search_naming_a_missing_user_fails_before_the_search() {
    use octocode_github::ProviderErrorReason;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/ghost"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "dev"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/commits"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"total_count": 1498, "incomplete_results": false, "items": []}),
            ),
        )
        .expect(0)
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    // The missing login may be the author or the committer, next to a real one.
    for qualifiers in ["author:ghost", "author:dev committer:ghost"] {
        let query: GhSearchHistoryQuery = serde_json::from_value(json!({
            "operation":"commit","owner":"o","repo":"r","keywords":["fix"],
            "qualifiers": qualifiers
        }))
        .expect("query");
        let failure = crate::tools::gh_search_history::execute(
            &provider,
            query,
            &default_context(),
            &Passthrough,
        )
        .await
        .expect_err("missing user");
        assert_eq!(
            failure.error.kind,
            ProviderErrorKind::NotFound,
            "{qualifiers}"
        );
        assert_eq!(
            failure.error.reason,
            Some(ProviderErrorReason::UserNotFound),
            "{qualifiers}"
        );
        let (message, hint) = history_failure(&failure.error, true);
        assert!(
            message.contains("user")
                && message.contains("ghost")
                && !message.contains("Repository"),
            "{message}"
        );
        assert_eq!(hint, None, "{qualifiers}");
        let lead = failure
            .next
            .expect("a lead")
            .pointer("/withoutUsers/query/queries/0")
            .cloned()
            .expect("withoutUsers is a ghSearchHistory query");
        assert!(lead.get("qualifiers").is_none(), "{lead}");
        assert_eq!(lead["keywords"], json!(["fix"]), "{lead}");
    }
}

/// A login is confirmed once per call however many qualifiers name it, and
/// not at all when no qualifier (or only an email, which has no profile)
/// names a user. A failed confirmation never blocks the search.
#[tokio::test]
async fn commit_search_confirms_each_named_login_once_and_only_then() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/Dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "dev"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/flaky"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"message": "boom"})))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({"total_count": 0, "incomplete_results": false, "items": []}),
    )
    .await;
    let provider = one_shot_provider(&server);
    let run = |qualifiers: Option<&str>| {
        let mut row = json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"]});
        if let Some(qualifiers) = qualifiers {
            row["qualifiers"] = json!(qualifiers);
        }
        let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
        let provider = &provider;
        async move {
            crate::tools::gh_search_history::execute(
                provider,
                query,
                &default_context(),
                &Passthrough,
            )
            .await
            .expect("search runs")
        }
    };
    // One login named as author and committer (case aside): one request.
    run(Some("author:Dev committer:dev")).await;
    // No user qualifier, or an email: no /users request (an unmounted route
    // would 404 as a missing user and fail the search).
    run(None).await;
    run(Some("author:dev@example.com")).await;
    // Any failure but 404 does not block; the search says it was unchecked.
    let data = run(Some("author:flaky")).await;
    let warnings = data["warnings"].to_string();
    assert!(warnings.contains("flaky"), "{data}");
}

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
    let query = serde_json::from_value(
        json!({"operation":"commit","mainGoal":"test","reasoning":"test",
        "owner":"o","repo":"r","keywords":["cache"]}),
    )
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("history search");
    let read = &data["next"]["readCommit"];
    assert_eq!(read["query"]["queries"][0]["ref"], "aaa", "{data}");
    assert_eq!(
        read["query"]["queries"][0]["sections"],
        json!(["patches"]),
        "{data}"
    );
    assert!(read.get("candidates").is_none(), "{data}");
    assert_eq!(data["commits"].as_array().map(Vec::len), Some(3), "{data}");
    // Commit search pages use the shared `currentPage` key, so the
    // default projection drops page one like every other history page.
    assert_eq!(data["pagination"]["currentPage"], 1, "{data}");
    assert!(data["pagination"].get("page").is_none(), "{data}");
}

/// A commit listing names its row kind like commit search: `type` is
/// `commits`, whether `path` names a file, a directory, or nothing.
#[tokio::test]
async fn commit_listing_rows_are_typed_commits() {
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
    let provider = one_shot_provider(&server);
    for scope in [json!("src/lib.rs"), json!("src/"), Value::Null] {
        let mut query = json!({"operation":"commit","owner":"o","repo":"r"});
        if !scope.is_null() {
            query["path"] = scope.clone();
        }
        let query = serde_json::from_value(query).expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("commit listing");
        assert!(data.get("type").is_none(), "{scope}: {data}");
        assert_eq!(data["commits"][0]["commitSha"], "aaa", "{data}");
        // The listed path is the caller's own input: not restated.
        assert!(data.get("path").is_none(), "{scope}: {data}");
        // The commit read scopes its patches with the published `include`.
        let read = &data["next"]["readCommit"]["query"]["queries"][0];
        assert!(read.get("path").is_none(), "{read}");
        if scope.is_null() {
            assert!(read.get("include").is_none(), "{read}");
        } else {
            assert_eq!(read["include"], json!([scope]), "{read}");
        }
    }
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
        .and(query_param("page", "2"))
        .and(query_param("per_page", "5"))
        .and(query_param("sort", "created"))
        .and(query_param("order", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 11, "incomplete_results": false,
            "items": [issue(6), issue(7), issue(8), issue(9), issue(10)]
        })))
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
    let data = run(json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"full","pageSize":5,"page":2})).await;
    assert_eq!(data["issues"].as_array().map(Vec::len), Some(5), "{data}");
    assert_eq!(data["pagination"]["currentPage"], 2, "{data}");
    // The cursor rides `next.nextPage` only, never the page block.
    assert_eq!(data.pointer("/pagination/nextPage"), None, "{data}");
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["page"], 3,
        "{data}"
    );
    assert_eq!(data["pagination"]["totalItems"], 11, "{data}");
    assert!(data.get("skippedPullRequestPages").is_none(), "{data}");

    let empty = run(json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"prs-only","pageSize":5})).await;
    assert_eq!(empty["issues"], json!([]), "{empty}");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert!(empty.get("pagination").is_none(), "{empty}");
    assert!(empty["next"].get("nextPage").is_none(), "{empty}");
}

/// GitHub search does not follow renames, but a search that finds rows
/// needs no rename lookup: the `/repos` request runs only when the search
/// comes back empty, and then the search re-runs against the new name.
#[tokio::test]
async fn rename_lookup_runs_only_when_the_search_finds_nothing() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };
    let row = json!({"number": 7, "title": "Fix", "state": "open", "user": {"login": "dev"}});
    let found = MockServer::builder().start().await;
    mount_json(
        &found,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 1, "incomplete_results": false, "items": [row.clone()]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch": "main", "full_name": "o/r"})),
        )
        .expect(0)
        .mount(&found)
        .await;
    let query = serde_json::from_value(
        json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["fix"]}),
    )
    .expect("query");
    let data = execute(
        &one_shot_provider(&found),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("search");
    assert_eq!(data["pullRequests"][0]["number"], 7, "{data}");

    let renamed = MockServer::builder().start().await;
    for (scope, items) in [("old/r", json!([])), ("new/r", json!([row]))] {
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .and(query_param("q", format!("fix is:pr repo:{scope}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count": items.as_array().map_or(0, Vec::len),
                "incomplete_results": false, "items": items
            })))
            .mount(&renamed)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/old/r"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch": "main", "full_name": "new/r"})),
        )
        .expect(1)
        .mount(&renamed)
        .await;
    let query = serde_json::from_value(
        json!({"operation":"pullRequest","owner":"old","repo":"r","keywords":["fix"]}),
    )
    .expect("query");
    let data = execute(
        &one_shot_provider(&renamed),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("renamed search");
    assert_eq!(data["pullRequests"][0]["number"], 7, "{data}");
    // A static notice: the response says it once, not on every row.
    assert_eq!(
        data[crate::response::paging::NOTICES][0],
        crate::tools::gh_shared::renamed_warning("old", "r", &("new".into(), "r".into())),
        "one shared rename wording: {data}"
    );
    let read = &data["next"]["readPullRequest"]["query"]["queries"][0];
    assert_eq!(
        (&read["owner"], &read["repo"]),
        (&json!("new"), &json!("r")),
        "{data}"
    );
}

/// A path commit listing leads to the first `(#N)` row's PR
/// scoped to that path, and the envelope neither echoes owner/repo nor
/// restates totalPages.
#[tokio::test]
async fn commit_listing_leads_to_the_squash_pull_request() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    let commit = |sha: &str, message: &str| {
        json!({"sha": sha, "commit": {"message": message,
            "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}})
    };
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([commit("aaa", "Bump deps"), commit("bbb", "Fix cache (#9)")]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 1, "incomplete_results": false,
            "items": [{"number": 3, "title": "Bug", "state": "open", "comments": 4}]}),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(
        json!({"operation":"commit","owner":"o","repo":"r","path":"src/lib.rs"}),
    )
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("commit listing");
    assert!(data["commits"][1].get("prNumber").is_none(), "{data}");
    let read = &data["next"]["readPullRequest"]["query"]["queries"][0];
    assert_eq!(read["number"], 9, "{data}");
    assert_eq!(read["include"], json!(["src/lib.rs"]), "{data}");
    assert!(
        data.get("owner").is_none() && data.get("repo").is_none(),
        "{data}"
    );

    let query = serde_json::from_value(
        json!({"operation":"issue","owner":"o","repo":"r","keywords":["bug"]}),
    )
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("issue search");
    assert!(
        data.get("owner").is_none() && data.get("repo").is_none(),
        "{data}"
    );
    assert_eq!(data["issues"][0]["commentsCount"], 4, "{data}");
    assert!(data["pagination"].get("totalPages").is_none(), "{data}");
}

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
    let keys = |row: &Value| {
        row.as_object()
            .expect("row")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };

    let prs = run(json!({"operation":"pullRequest","owner":"o","repo":"r"})).await;
    assert_eq!(
        keys(&prs["pullRequests"][0]),
        ["number", "title", "state", "mergedAt", "author"],
        "{prs}"
    );
    assert_eq!(
        keys(&prs["pullRequests"][1]),
        ["number", "title", "state", "author", "createdAt"],
        "{prs}"
    );
    // A state the query fixes is not restated per row.
    let open = run(json!({"operation":"pullRequest","owner":"o","repo":"r","state":"open"})).await;
    assert!(open["pullRequests"][1].get("state").is_none(), "{open}");

    let issues = run(json!({"operation":"issue","owner":"o","repo":"r"})).await;
    assert_eq!(
        keys(&issues["issues"][0]),
        ["number", "title", "state", "author", "createdAt"],
        "{issues}"
    );

    let listed = run(json!({"operation":"commit","owner":"o","repo":"r"})).await;
    let searched =
        run(json!({"operation":"commit","owner":"o","repo":"r","keywords":["fix"]})).await;
    for data in [&listed, &searched] {
        assert_eq!(
            keys(&data["commits"][0]),
            ["commitSha", "date", "messageHeadline", "author"],
            "{data}"
        );
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

/// A commit listing pages by the provider's Link header: page 2's request
/// is the provider's page 2 at the same size, so it shares no row with
/// page 1, and the continuation carries the whole query.
#[tokio::test]
async fn commit_listing_page_two_is_the_providers_next_page() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    let server = MockServer::builder().start().await;
    let commit = |sha: &str| json!({"sha": sha, "commit": {"message": sha, "author": {"date": "2026-09-01T00:00:00Z"}}});
    for (page, shas, link) in [
        (
            "1",
            ["a1", "a2"],
            Some(format!(
                "<{}/api/v3/repos/o/r/commits?page=2&per_page=2>; rel=\"next\"",
                server.uri()
            )),
        ),
        ("2", ["b1", "b2"], None),
    ] {
        let mut response = ResponseTemplate::new(200).set_body_json(json!(
            shas.iter().map(|sha| commit(sha)).collect::<Vec<_>>()
        ));
        if let Some(link) = link {
            response = response.insert_header("link", link.as_str());
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/commits"))
            .and(query_param("page", page))
            .and(query_param("per_page", "2"))
            .and(query_param("path", "src"))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let first: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"commit","owner":"o","repo":"r","path":"src","pageSize":2,"since":"2026-01-01"
    }))
    .expect("query");
    let one = execute(&provider, &first, &default_context(), &Passthrough)
        .await
        .expect("page 1");
    let next = one["next"]["nextPage"]["query"]["queries"][0].clone();
    for key in ["owner", "repo", "path", "pageSize", "since"] {
        assert_eq!(
            next[key],
            serde_json::to_value(&first).expect("wire")[key],
            "{key}: {next}"
        );
    }
    assert_eq!(next["page"], 2, "{next}");
    let two = execute(
        &provider,
        &serde_json::from_value(next).expect("next query"),
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("page 2");
    let shas = |data: &Value| {
        data["commits"]
            .as_array()
            .expect("commits")
            .iter()
            .map(|row| row["commitSha"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(shas(&one), ["a1", "a2"]);
    assert_eq!(shas(&two), ["b1", "b2"]);
    assert!(two["next"].get("nextPage").is_none(), "{two}");
}

/// A commit listing's bare 404 at a `ref` leads to the refs listing (the
/// one repair); without a ref the repository did not resolve, and a search
/// for its name is the repair.
#[tokio::test]
async fn a_missing_listing_scope_names_its_repair() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        404,
        json!({"message": "Not Found"}),
    )
    .await;
    let provider = one_shot_provider(&server);
    let at_ref: GhSearchHistoryQuery =
        serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r","ref":"nope"}))
            .expect("query");
    let failure = crate::tools::gh_search_history::execute(
        &provider,
        at_ref,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect_err("missing ref");
    assert!(failure.error.message.contains("\"nope\""), "{failure:?}");
    let next = failure.next.expect("refs lead");
    assert_eq!(next["viewRefs"]["tool"], "ghStructure", "{next}");
    assert_eq!(
        next["viewRefs"]["query"]["queries"][0]["operation"], "refs",
        "{next}"
    );

    let plain: GhSearchHistoryQuery =
        serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r"}))
            .expect("query");
    let failure = crate::tools::gh_search_history::execute(
        &provider,
        plain,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect_err("missing repository");
    assert!(failure.error.message.contains("o/r"), "{failure:?}");
    // The recovery is the repository's spelling, not a history filter.
    let next = failure.next.expect("a repository search");
    assert_eq!(
        next["findRepository"]["query"]["queries"][0]["keywords"],
        json!(["r"]),
        "{next}"
    );
}

/// `totalPages` appears only beside an exact total: an uncapped search
/// count states it; a count past GitHub's 1,000-result window states the
/// reachable `totalItems` and `partialReasons:["providerResultCap"]`, no
/// page count.
#[tokio::test]
async fn total_pages_appear_only_beside_an_exact_total() {
    use wiremock::MockServer;
    for (total_count, total_pages) in [(30, Some(3)), (5_000, None)] {
        let server = MockServer::builder().start().await;
        let items = (1..=10)
            .map(|number| {
                json!({"number": number, "title": "t", "state": "open",
                "pull_request": {"merged_at": null}})
            })
            .collect::<Vec<_>>();
        mount_json(
            &server,
            "/api/v3/search/issues",
            200,
            json!({"total_count": total_count, "incomplete_results": false, "items": items}),
        )
        .await;
        let provider = one_shot_provider(&server);
        let query =
            serde_json::from_value(json!({"operation":"pullRequest","owner":"o","repo":"r",
            "keywords":["x"],"pageSize":10}))
            .expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("pull request search");
        assert_eq!(
            data["pagination"].get("totalPages").and_then(Value::as_u64),
            total_pages,
            "{data}"
        );
        if total_pages.is_none() {
            assert_eq!(data["pagination"]["totalItems"], 1000, "{data}");
            assert_eq!(
                data["partialReasons"],
                json!(["providerResultCap"]),
                "{data}"
            );
        }
    }
}

/// A REST list page states no total and says nothing more about it: the
/// missing `totalItems` beside `hasMore` is the per-call fact.
#[tokio::test]
async fn a_rest_list_page_with_more_carries_no_static_total_warning() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    let link = |route: &str| {
        format!(
            "<{}/api/v3/repos/o/r/{route}?per_page=1&page=2>; rel=\"next\"",
            server.uri()
        )
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", link("commits").as_str())
                .set_body_json(json!([{"sha": "aaa", "commit": {"message": "Fix",
                    "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}}])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", link("pulls").as_str())
                .set_body_json(json!([{"number": 4, "title": "Fix", "state": "open",
                    "user": {"login": "dev"}, "created_at": "2026-01-01T00:00:00Z"}])),
        )
        .mount(&server)
        .await;
    for row in [
        json!({"operation":"commit","owner":"o","repo":"r","pageSize":1}),
        json!({"operation":"pullRequest","owner":"o","repo":"r","pageSize":1}),
    ] {
        let query = serde_json::from_value(row.clone()).expect("query");
        let data = execute(
            &one_shot_provider(&server),
            &query,
            &default_context(),
            &Passthrough,
        )
        .await
        .expect("listing");
        assert_eq!(data["pagination"]["hasMore"], true, "{data}");
        assert!(data["pagination"].get("totalItems").is_none(), "{data}");
        assert!(data.get("warnings").is_none(), "{row}: {data}");
    }
}

/// A page past GitHub's 1,000-result window leads to the last reachable
/// page (`repair`), so the row never falls back to a generic hint.
#[tokio::test]
async fn a_page_past_the_search_window_leads_to_the_last_reachable_page() {
    use octocode_github::ProviderErrorReason;
    let server = wiremock::MockServer::builder().start().await;
    let query: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"pullRequest","owner":"o","repo":"r","keywords":["fix"],
        "page":11,"pageSize":100
    }))
    .expect("query");
    let failure = crate::tools::gh_search_history::execute(
        &one_shot_provider(&server),
        query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect_err("past the window");
    assert_eq!(
        failure.error.reason,
        Some(ProviderErrorReason::SearchWindowExceeded)
    );
    let next = failure.next.expect("a repair lead");
    let repair = &next["repair"];
    assert_eq!(repair["tool"], "ghSearchHistory", "{next}");
    let row = &repair["query"]["queries"][0];
    assert_eq!(row["page"], 10, "{next}");
    assert_eq!(row["keywords"], json!(["fix"]), "{next}");
}

/// A capped search names one narrowing lead per call: a date-ordered
/// result continues past the window from its last row's date (creation
/// newest first: `until`; oldest first: `since`; update order: an
/// `updated:` qualifier); an order with no date to resume from is first
/// re-sorted newest-first, which that keyset continuation then follows.
#[tokio::test]
async fn a_capped_search_names_one_narrowing_lead() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 4000, "incomplete_results": false, "items": [
            {"number": 9, "title": "a", "state": "open", "user": {"login": "dev"},
                "created_at": "2026-03-02T10:00:00Z", "updated_at": "2026-04-02T10:00:00Z"},
            {"number": 8, "title": "b", "state": "open", "user": {"login": "dev"},
                "created_at": "2026-03-01T09:30:00+02:00", "updated_at": "2026-04-01T08:00:00Z"}
        ]}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({"total_count": 4000, "incomplete_results": false, "items": [
            {"sha": "c1", "commit": {"message": "fix a",
                "committer": {"date": "2026-02-02T00:00:00Z"}}},
            {"sha": "c2", "commit": {"message": "fix b",
                "committer": {"date": "2026-02-01T12:00:00Z"}}}
        ]}),
    )
    .await;
    let provider = one_shot_provider(&server);
    let lead = |row: Value| {
        let provider = &provider;
        async move {
            let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
            let data = execute(provider, &query, &default_context(), &Passthrough)
                .await
                .expect("capped search");
            assert_eq!(
                data["partialReasons"],
                json!(["providerResultCap"]),
                "{data}"
            );
            let lead = data["next"]["narrowScope"].clone();
            assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
            let rows = lead["query"]["queries"].as_array().expect("rows").clone();
            assert_eq!(rows.len(), 1, "{lead}");
            crate::contracts::validate("ghSearchHistory", json!({"queries": rows.clone()}))
                .expect("the lead is a valid call");
            rows[0].clone()
        }
    };
    // Newest first: the window continues before the page's last row.
    let row = lead(json!({"operation":"pullRequest","owner":"o","repo":"r",
        "keywords":["fix"],"sort":"created:desc","since":"2025-01-01","pageSize":2,"page":3}))
    .await;
    assert_eq!(row["until"], "2026-03-01T07:30:00Z", "{row}");
    assert_eq!(row["since"], "2025-01-01", "{row}");
    assert!(row.get("page").is_none(), "{row}");
    let row = lead(json!({"operation":"commit","owner":"o","repo":"r",
        "keywords":["fix"],"pageSize":2}))
    .await;
    assert_eq!(row["until"], "2026-02-01T12:00:00Z", "{row}");
    // Oldest first: the window continues after the page's last row.
    let row = lead(json!({"operation":"pullRequest","owner":"o","repo":"r",
        "keywords":["fix"],"sort":"created:asc","until":"2027-01-01","pageSize":2}))
    .await;
    assert_eq!(row["since"], "2026-03-01T07:30:00Z", "{row}");
    assert_eq!(row["until"], "2027-01-01", "{row}");
    assert_eq!(row["sort"], "created:asc", "{row}");
    // Update order: an `updated:` bound beside the query's own qualifiers.
    let row = lead(json!({"operation":"issue","owner":"o","repo":"r",
        "keywords":["fix"],"sort":"updated:desc","qualifiers":"label:bug","pageSize":2}))
    .await;
    assert_eq!(
        row["qualifiers"], "label:bug updated:<=2026-04-01T08:00:00Z",
        "{row}"
    );
    assert_eq!(row["sort"], "updated:desc", "{row}");
    let row = lead(json!({"operation":"issue","owner":"o","repo":"r",
        "keywords":["fix"],"sort":"updated:asc","pageSize":2}))
    .await;
    assert_eq!(row["qualifiers"], "updated:>=2026-04-01T08:00:00Z", "{row}");
    // Best match, or an update order its own `updated:` qualifier bounds:
    // no date to resume from, so re-sort newest first.
    for query in [
        json!({"operation":"issue","owner":"o","repo":"r","keywords":["fix"],"pageSize":2}),
        json!({"operation":"issue","owner":"o","repo":"r","keywords":["fix"],
            "sort":"updated:desc","qualifiers":"updated:>2026-01-01","pageSize":2}),
    ] {
        let row = lead(query).await;
        assert_eq!(row["sort"], "created:desc", "{row}");
        assert!(row.get("until").is_none(), "{row}");
    }
}

/// A commit listing of a renamed repository (GitHub answers the old name
/// with a redirect) warns once and continues under the canonical name, as
/// the pull-request search does.
#[tokio::test]
async fn a_commit_listing_of_a_renamed_repository_names_the_canonical_repository() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/old/r/commits"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!(
                "{}/api/v3/repositories/1/commits?per_page=1&page=1",
                server.uri()
            ),
        ))
        .mount(&server)
        .await;
    let next = format!(
        "{}/api/v3/repositories/1/commits?per_page=1&page=2",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", format!("<{next}>; rel=\"next\"").as_str())
                .set_body_json(json!([{"sha": "aaa", "commit": {"message": "Fix",
                    "committer": {"date": "2026-01-01T00:00:00Z"}}}])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/old/r"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch": "main", "full_name": "new/r"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let query: GhSearchHistoryQuery = serde_json::from_value(json!({
        "operation":"commit","owner":"old","repo":"r","pageSize":1
    }))
    .expect("query");
    let data = execute(
        &one_shot_provider(&server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("renamed listing");
    assert_eq!(data["commits"][0]["commitSha"], "aaa", "{data}");
    assert_eq!(
        data[crate::response::paging::NOTICES],
        json!([crate::tools::gh_shared::renamed_warning(
            "old",
            "r",
            &("new".into(), "r".into())
        )]),
        "{data}"
    );
    let next = &data["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(
        (&next["owner"], &next["repo"]),
        (&json!("new"), &json!("r")),
        "{data}"
    );
    let read = &data["next"]["readCommit"]["query"]["queries"][0];
    assert_eq!(read["owner"], "new", "{data}");
}

/// Keywords are ANDed literals: a bare `OR`/`NOT` would be searched as a
/// word and silently empty the search, so it is refused before any request
/// and the message names the operator.
#[tokio::test]
async fn a_boolean_operator_in_keywords_is_refused() {
    use wiremock::MockServer;
    let server = MockServer::builder().start().await;
    for (keywords, operator) in [
        (json!(["fix OR bug"]), "OR"),
        (json!(["NOT", "flaky"]), "NOT"),
    ] {
        let query = serde_json::from_value(json!({"operation":"issue","owner":"o","repo":"r",
            "keywords": keywords}))
        .expect("query");
        let error = execute(
            &one_shot_provider(&server),
            &query,
            &default_context(),
            &Passthrough,
        )
        .await
        .expect_err("refused");
        assert_eq!(error.kind, ProviderErrorKind::Validation, "{error:?}");
        assert!(error.message.contains(operator), "{error:?}");
    }
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty(),
        "refused before any request"
    );
}
