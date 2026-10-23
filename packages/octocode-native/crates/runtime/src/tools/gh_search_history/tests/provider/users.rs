//! Searches and listings naming a login that does not resolve, and the one confirmation of each login.
use super::*;

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
