//! The search result window: retries of an incomplete page, the 1,000-result cap and the leads past it.
use super::*;

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
        json!({
            "total_count": 1, "incomplete_results": true,
            "items": [{"number": 3, "title": "Fix", "state": "open", "user": {"login": "dev"},
                "repository_url": "https://api.github.com/repos/acme/widget"}]
        }),
    )
    .await;
    let provider = one_shot_provider(&server);
    let query = serde_json::from_value(json!({"operation":"pullRequest","keywords":["fix"]}))
        .expect("query");
    let data = execute(&provider, &query, &default_context(), &Passthrough)
        .await
        .expect("history search");
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerIncompleteResults"]),
        "{data}"
    );
    assert!(data.get("terminalLimit").is_none(), "{data}");
    let retry = &data["next"]["retry"];
    assert_eq!(retry["tool"], "ghSearchHistory", "{data}");
    assert_eq!(retry["query"]["queries"][0]["page"], 1, "{data}");
    assert_eq!(
        retry["query"]["queries"][0]["keywords"],
        json!(["fix"]),
        "{data}"
    );
}

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
        // The first page states GitHub's full match count, once per search
        // (the shared provider-limit rule of every GitHub search).
        assert_eq!(
            data["warnings"].as_array().is_some_and(|warnings| warnings
                .iter()
                .any(|warning| warning.as_str().is_some_and(|w| w.contains("1001")))),
            page == 1,
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
        json!({"total_count": 61, "incomplete_results": false,
            "items": [{"number": 7, "title": "t", "state": "open"}]}),
    )
    .await;
    let query = serde_json::from_value(json!({"operation":"issue","owner":"o","repo":"r",
        "keywords":["mmap"],"pageSize":20,"page":4}))
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
    assert_eq!(pagination["currentPage"], 4, "{data}");
    assert_eq!(pagination["totalPages"], 4, "{data}");
    assert_eq!(pagination["totalItems"], 61, "{data}");
    assert_eq!(pagination["hasMore"], false, "{data}");
}
