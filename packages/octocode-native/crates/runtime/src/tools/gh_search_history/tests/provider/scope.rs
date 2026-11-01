//! The repository and path a search or listing is scoped to: missing, renamed or empty, and refused keywords.
use super::*;

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
        let failure = crate::tools::gh_search_history::execute(
            &provider,
            query,
            &default_context(),
            &Passthrough,
        )
        .await
        .expect_err("missing repository");
        assert_eq!(
            failure.error.kind,
            ProviderErrorKind::NotFound,
            "{row}: {failure:?}"
        );
        // The recovery is the repository's spelling, not a history filter:
        // the shared leads, labelled by the shared hint.
        assert_eq!(
            failure.hints,
            [crate::tools::gh_shared::repository_search_hint(
                Some("o"),
                "missing"
            )],
            "{row}"
        );
        let next = failure.calls.expect("a recovery lead");
        assert_eq!(
            next,
            crate::tools::gh_shared::repository_recovery(Some("o"), "missing"),
            "{row}"
        );
    }
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
    let read =
        &crate::tools::test_support::checks::related_call(&data, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "pullRequest"
        })["input"]["queries"][0];
    assert_eq!(
        (&read["owner"], &read["repo"]),
        (&json!("new"), &json!("r")),
        "{data}"
    );
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
    let next = failure.calls.expect("refs lead");
    assert_eq!(next["viewRefs"]["tool"], "ghStructure", "{next}");
    assert_eq!(
        next["viewRefs"]["input"]["queries"][0]["operation"], "refs",
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
    let next = failure.calls.expect("a repository search");
    assert_eq!(
        next["findRepository"]["input"]["queries"][0]["keywords"],
        json!(["r"]),
        "{next}"
    );
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
    let query: GhSearchHistoryQuery =
        serde_json::from_value(json!({"operation":"commit","owner":"old","repo":"r"}))
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
    let next = &data["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(
        (&next["owner"], &next["repo"]),
        (&json!("new"), &json!("r")),
        "{data}"
    );
    let read =
        &crate::tools::test_support::checks::related_call(&data, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "commit"
        })["input"]["queries"][0];
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

/// An empty commit listing scoped to a path broadens by dropping each
/// narrowing filter, the path among them, and leads to the listing that
/// checks the path: its parent directory at the listed ref. An empty
/// pull-request search narrowed by `state` drops it too.
#[tokio::test]
async fn an_empty_path_scoped_listing_broadens_past_the_path_and_checks_it() {
    let server = wiremock::MockServer::builder().start().await;
    octocode_github::mock_http::mount_json(&server, "/api/v3/repos/o/r/commits", 200, json!([]))
        .await;
    octocode_github::mock_http::mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 0, "incomplete_results": false, "items": []}),
    )
    .await;
    let provider = one_shot_provider(&server);
    let run = |row: Value| {
        let provider = &provider;
        async move {
            let query = serde_json::from_value(row).expect("query");
            execute(provider, &query, &default_context(), &Passthrough)
                .await
                .expect("listing")
        }
    };
    let data = run(json!({"operation":"commit","owner":"o","repo":"r","ref":"v2","pathPrefix":"crates/nonexistent","filters":{"committedAt":{"from":"2024-01-01T00:00:00Z"}}}))
    .await;
    assert_eq!(data["status"], "empty", "{data}");
    let rows = crate::tools::test_support::checks::related_call(&data, "ghSearchHistory", |_| true)
        ["input"]["queries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        rows.iter()
            .any(|row| row.get("filters").is_none() && row["pathPrefix"] == "crates/nonexistent"),
        "{data}"
    );
    assert!(
        rows.iter()
            .any(|row| row.get("pathPrefix").is_none() && row["filters"]["committedAt"].is_object()),
        "{data}"
    );
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["input"]["queries"][0],
        json!({"operation":"tree","owner":"o","repo":"r","path":"crates","ref":"v2"}),
        "{data}"
    );
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["tool"],
        "ghStructure"
    );

    let data = run(json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["fix"],"filters":{"isMerged":true}}))
    .await;
    let rows = &crate::tools::test_support::checks::related_call(&data, "ghSearchHistory", |_| {
        true
    })["input"]["queries"];
    assert!(
        rows.as_array()
            .is_some_and(|rows| rows.iter().any(|row| row.get("state").is_none())),
        "{data}"
    );
    assert!(data["next"].get("viewTree").is_none(), "{data}");
}
