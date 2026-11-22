//! The search result window: retries of an incomplete page, the 1,000-result cap and the leads past it.
use super::*;
use octocode_test_support::json_fixture;

/// GitHub's incomplete-results flag on the last page is no terminal limit:
/// the same page again may complete, so the page offers `next.retry`.
#[tokio::test]
async fn an_incomplete_last_page_offers_a_retry_not_a_terminal_limit() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": true,
                "items": [
                    {
                        "number": 3,
                        "title": "Fix",
                        "state": "open",
                        "user": {"login": "dev"},
                        "repository_url": "https://api.github.com/repos/acme/widget"
                    }
                ]
            }"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(json_fixture(
        r#"{"operation": "pullRequest", "keywords": ["fix"]}"#,
    ))
    .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("history search");
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json_fixture(r#"["providerIncompleteResults"]"#),
        "{data}"
    );
    assert!(data.get("terminalLimit").is_none(), "{data}");
    let retry = &data["next"]["retry"];
    assert_eq!(retry["tool"], "ghSearchHistory", "{data}");
    assert!(
        retry["input"]["queries"][0].get("page").is_none(),
        "page 1 is the default: {data}"
    );
    assert_eq!(
        retry["input"]["queries"][0]["keywords"],
        json_fixture(r#"["fix"]"#),
        "{data}"
    );
}

#[tokio::test]
async fn cap_is_terminal_only_after_the_last_reachable_page() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    octocode_test_support::mock_http::mount_search_index(
        &server,
        "/api/v3/search/issues",
        (1..=1001)
            .map(|number| {
                json!({"number": number, "title": "Fix", "state": "open",
                "user": {"login": "dev"},
                "repository_url": "https://api.github.com/repos/acme/widget"})
            })
            .collect(),
    )
    .await;
    let provider = one_shot_provider(&server);
    for page in [1, 20] {
        let query = serde_json::from_value(
            json!({"operation":"pullRequest", "keywords":["fix"],"page":page}),
        )
        .expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("history search");
        assert_eq!(data["isPartial"], true, "{data}");
        assert_eq!(
            data["partialReasons"],
            json_fixture(r#"["providerResultCap"]"#),
            "{data}"
        );
        assert_eq!(
            data["terminalLimit"].as_bool().unwrap_or(false),
            page == 20,
            "{data}"
        );
        assert_eq!(data["next"]["nextPage"].is_object(), page < 20, "{data}");
        // Every page states GitHub's full match count: a page can be read on
        // its own (the shared provider-limit rule of every GitHub search).
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
    }
}

/// A page past GitHub's 1,000-result window leads to the last reachable
/// page (`repair`), so the row never falls back to a generic hint.
#[tokio::test]
async fn a_page_past_the_search_window_leads_to_the_last_reachable_page() {
    use octocode_github::ProviderErrorReason;
    let server = wiremock::MockServer::builder().start().await;
    let query: GhSearchHistoryQuery = serde_json::from_value(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "page": 21
        }"#,
    ))
    .expect("query");
    let failure = crate::gh_search_history::execute(
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
    let next = *failure.calls.expect("a repair lead");
    let repair = &next["repair"];
    assert_eq!(repair["tool"], "ghSearchHistory", "{next}");
    let row = &repair["input"]["queries"][0];
    assert_eq!(row["page"], 20, "{next}");
    assert_eq!(row["keywords"], json_fixture(r#"["fix"]"#), "{next}");
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
        json_fixture(
            r#"{
                "total_count": 4000,
                "incomplete_results": false,
                "items": [
                    {
                        "number": 9,
                        "title": "a",
                        "state": "open",
                        "user": {"login": "dev"},
                        "created_at": "2026-03-02T10:00:00Z",
                        "updated_at": "2026-04-02T10:00:00Z"
                    },
                    {
                        "number": 8,
                        "title": "b",
                        "state": "open",
                        "user": {"login": "dev"},
                        "created_at": "2026-03-01T09:30:00+02:00",
                        "updated_at": "2026-04-01T08:00:00Z"
                    }
                ]
            }"#,
        ),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json_fixture(
            r#"{
                "total_count": 4000,
                "incomplete_results": false,
                "items": [
                    {
                        "sha": "c1",
                        "commit": {
                            "message": "fix a",
                            "committer": {"date": "2026-02-02T00:00:00Z"}
                        }
                    },
                    {
                        "sha": "c2",
                        "commit": {
                            "message": "fix b",
                            "committer": {"date": "2026-02-01T12:00:00Z"}
                        }
                    }
                ]
            }"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    let lead = |row: Value| {
        let provider = &provider;
        async move {
            let operation = row["operation"].clone();
            let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
            let data = execute(provider, &query, &default_context(), &Passthrough)
                .await
                .expect("capped search");
            assert_eq!(
                data["partialReasons"],
                json_fixture(r#"["providerResultCap"]"#),
                "{data}"
            );
            let lead = octocode_runtime_base::test_support::checks::related_call(
                &data,
                "ghSearchHistory",
                |input| input["queries"][0]["operation"] == operation,
            );
            assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
            let rows = lead["input"]["queries"].as_array().expect("rows").clone();
            assert_eq!(rows.len(), 1, "{lead}");
            octocode_runtime_base::contracts::validate(
                "ghSearchHistory",
                json!({"queries": rows.clone()}),
            )
            .expect("the lead is a valid call");
            rows[0].clone()
        }
    };
    // Newest first: the window continues before the page's last row.
    let row = lead(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "sort": {"by": "createdAt", "order": "desc"},
            "page": 3,
            "filters": {"createdAt": {"from": "2025-01-01T00:00:00Z"}}
        }"#,
    ))
    .await;
    assert_eq!(
        row["filters"]["createdAt"]["to"], "2026-03-01T07:30:00Z",
        "{row}"
    );
    assert_eq!(
        row["filters"]["createdAt"]["from"], "2025-01-01T00:00:00Z",
        "{row}"
    );
    assert!(row.get("page").is_none(), "{row}");
    let row = lead(json_fixture(
        r#"{
            "operation": "commit",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"]
        }"#,
    ))
    .await;
    assert_eq!(
        row["filters"]["committedAt"]["to"], "2026-02-01T12:00:00Z",
        "{row}"
    );
    // Oldest first: the window continues after the page's last row.
    let row = lead(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "sort": {"by": "createdAt", "order": "asc"},
            "filters": {"createdAt": {"to": "2027-01-01T23:59:59Z"}}
        }"#,
    ))
    .await;
    assert_eq!(
        row["filters"]["createdAt"]["from"], "2026-03-01T07:30:00Z",
        "{row}"
    );
    assert_eq!(
        row["filters"]["createdAt"]["to"], "2027-01-01T23:59:59Z",
        "{row}"
    );
    assert_eq!(
        row["sort"],
        json_fixture(r#"{"by": "createdAt", "order": "asc"}"#),
        "{row}"
    );
    // Update order: an `updated:` bound beside the query's own qualifiers.
    let row = lead(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "sort": {"by": "updatedAt", "order": "desc"},
            "filters": {"labels": ["bug"]}
        }"#,
    ))
    .await;
    assert_eq!(
        row["filters"]["updatedAt"]["to"], "2026-04-01T08:00:00Z",
        "{row}"
    );
    assert_eq!(
        row["sort"],
        json_fixture(r#"{"by": "updatedAt", "order": "desc"}"#),
        "{row}"
    );
    let row = lead(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"],
            "sort": {"by": "updatedAt", "order": "asc"}
        }"#,
    ))
    .await;
    assert_eq!(
        row["filters"]["updatedAt"]["from"], "2026-04-01T08:00:00Z",
        "{row}"
    );
    // Best match, or an update order its own `updated:` qualifier bounds:
    // no date to resume from, so re-sort newest first.
    for query in [json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["fix"]
        }"#,
    )] {
        let row = lead(query).await;
        assert_eq!(
            row["sort"],
            json_fixture(r#"{"by": "createdAt", "order": "desc"}"#),
            "{row}"
        );
        assert!(row.get("until").is_none(), "{row}");
    }
}

/// The sentence beside a capped search's related call says what the call
/// runs. A date-ordered page continues from its own last row, on every page.
/// A search with no date order is re-run newest first from its first row:
/// that replaces the walk, so it is stated where the walk starts and where
/// it ends, and no page between carries a call that would restart it.
#[tokio::test]
async fn a_capped_search_says_what_its_related_call_does() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(
            r#"{
                "total_count": 4000,
                "incomplete_results": false,
                "items": [
                    {"number": 9, "title": "a", "state": "open", "user": {"login": "dev"},
                     "created_at": "2026-03-02T10:00:00Z", "updated_at": "2026-04-02T10:00:00Z"},
                    {"number": 8, "title": "b", "state": "open", "user": {"login": "dev"},
                     "created_at": "2026-03-01T09:30:00Z", "updated_at": "2026-04-01T08:00:00Z"}
                ]
            }"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    let read = |row: Value| {
        let provider = &provider;
        async move {
            let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
            execute(provider, &query, &default_context(), &Passthrough)
                .await
                .expect("capped search")
        }
    };
    let related = |data: &Value| {
        data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|call| call["tool"] == "ghSearchHistory")
            .cloned()
    };
    let says = |data: &Value, needle: &str| {
        data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|warning| warning.contains(needle))
    };
    let relevance = |page: u64| {
        json!({"operation": "pullRequest", "owner": "o", "repo": "r", "keywords": ["fix"],
               "limit": 100, "page": page})
    };
    // No date order: the first page offers the newest-first search and says
    // it restarts from the first row.
    let first = read(relevance(1)).await;
    let call = related(&first).expect("the first page offers the re-sorted search");
    let row = &call["input"]["queries"][0];
    assert_eq!(
        row["sort"],
        json!({"by": "createdAt", "order": "desc"}),
        "{first}"
    );
    assert!(
        row["filters"].get("createdAt").is_none(),
        "it narrows nothing yet: {first}"
    );
    assert!(
        says(&first, "re-runs the search newest first from its first row"),
        "{first}"
    );
    assert!(!says(&first, "bounded at this page's last row"), "{first}");
    // A page between carries no call that would restart the walk, and still
    // says the search is capped.
    let middle = read(relevance(3)).await;
    assert_eq!(
        middle["partialReasons"],
        json!(["providerResultCap"]),
        "{middle}"
    );
    assert_eq!(related(&middle), None, "{middle}");
    assert!(!says(&middle, "re-runs the search"), "{middle}");
    // The last reachable page states it again: the reader who walked needs it.
    let last = read(relevance(10)).await;
    assert!(related(&last).is_some(), "{last}");
    assert!(says(&last, "re-runs the search newest first"), "{last}");
    // A date order continues from each page's own last row, and says the
    // boundary instant is included.
    let dated = read(
        json!({"operation": "pullRequest", "owner": "o", "repo": "r",
        "keywords": ["fix"], "limit": 100, "page": 3,
        "sort": {"by": "createdAt", "order": "desc"}}),
    )
    .await;
    let call = related(&dated).expect("a date-ordered page continues from its last row");
    assert_eq!(
        call["input"]["queries"][0]["filters"]["createdAt"]["to"], "2026-03-01T09:30:00Z",
        "{dated}"
    );
    assert!(
        says(&dated, "bounded at this page's last row's date"),
        "{dated}"
    );
    assert!(says(&dated, "is included"), "{dated}");
}

/// A keyword-less pullRequest search runs newest created first, as the
/// sort's documented default says ("Omit: relevance with keywords, else
/// newest created"), the same as an issue search. So GitHub is asked with
/// `sort=created&order=desc`, and a capped page's lead narrows the window
/// to its last row's creation date instead of only re-adding that default.
#[tokio::test]
async fn a_capped_keyword_less_pull_request_search_narrows_its_window() {
    use wiremock::MockServer;

    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(
            r#"{
                "total_count": 2567,
                "incomplete_results": false,
                "items": [
                    {
                        "number": 9,
                        "title": "a",
                        "state": "open",
                        "user": {"login": "dev"},
                        "created_at": "2026-03-02T10:00:00Z"
                    },
                    {
                        "number": 8,
                        "title": "b",
                        "state": "open",
                        "user": {"login": "dev"},
                        "created_at": "2026-03-01T09:30:00+02:00"
                    }
                ]
            }"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    let row = json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "filters": {"labels": ["bug"]}
        }"#,
    );
    let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("capped search");
    assert_eq!(
        data["partialReasons"],
        json_fixture(r#"["providerResultCap"]"#),
        "{data}"
    );
    let asked = server.received_requests().await.unwrap_or_default();
    let search = asked
        .iter()
        .find(|request| request.url.path() == "/api/v3/search/issues")
        .expect("a search");
    let pairs: std::collections::HashMap<_, _> = search.url.query_pairs().into_owned().collect();
    assert_eq!(
        (
            pairs.get("sort").map(String::as_str),
            pairs.get("order").map(String::as_str)
        ),
        (Some("created"), Some("desc")),
        "{}",
        search.url
    );
    let lead = octocode_runtime_base::test_support::checks::related_call(
        &data,
        "ghSearchHistory",
        |input| input["queries"][0]["operation"] == "pullRequest",
    );
    let lead = &lead["input"]["queries"][0];
    assert_eq!(
        lead["filters"]["createdAt"]["to"], "2026-03-01T07:30:00Z",
        "the lead narrows the window: {lead}"
    );
    assert_eq!(
        lead["filters"]["labels"],
        json_fixture(r#"["bug"]"#),
        "{lead}"
    );
}

/// The last page of a multi-page issue search keeps the page block every
/// earlier page carried, with `hasMore:false`: only a lone first page
/// states nothing.
#[tokio::test]
async fn the_last_page_of_an_issue_search_keeps_its_pagination() {
    use wiremock::MockServer;
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json_fixture(
            r#"{
                "total_count": 61,
                "incomplete_results": false,
                "items": [{"number": 7, "title": "t", "state": "open"}]
            }"#,
        ),
    )
    .await;
    let query = serde_json::from_value(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "keywords": ["mmap"],
            "page": 2
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
    let pagination = &data["pagination"];
    assert_eq!(pagination["offset"], 50, "{data}");
    assert_eq!(pagination["total"], 61, "{data}");
    assert_eq!(pagination["hasMore"], false, "{data}");
}
