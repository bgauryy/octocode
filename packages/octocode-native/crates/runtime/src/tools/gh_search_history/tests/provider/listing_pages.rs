//! REST listing pages: the provider next page and the totals a page states.
use super::*;

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
                "<{}/api/v3/repos/o/r/commits?page=2&per_page=50>; rel=\"next\"",
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
            .and(query_param("per_page", "50"))
            .and(query_param("path", "src"))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let first: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r","path":"src","filters":{"committedAt":{"from":"2026-01-01T00:00:00Z"}}}))
    .expect("query");
    let one = execute(&provider, &first, &default_context(), &Passthrough)
        .await
        .expect("page 1");
    let next = one["next"]["nextPage"]["input"]["queries"][0].clone();
    for key in ["owner", "repo", "path", "filters"] {
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

/// A proxy forwards GitHub's own `Link`, naming `api.github.com`: the page
/// read stays whole and states `hasMore`, its continuation asks the
/// configured origin for page 2, and no link to another host is followed.
#[tokio::test]
async fn a_proxied_listing_with_a_foreign_next_link_pages_by_its_own_origin() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    let server = MockServer::builder().start().await;
    let commit = |sha: &str| json!({"sha": sha, "commit": {"message": sha, "author": {"date": "2026-09-01T00:00:00Z"}}});
    for (page, sha, link) in [
        (
            "1",
            "a1",
            Some("<https://api.github.com/repos/o/r/commits?page=2&per_page=50>; rel=\"next\""),
        ),
        ("2", "b1", None),
    ] {
        let mut response = ResponseTemplate::new(200).set_body_json(json!([commit(sha)]));
        if let Some(link) = link {
            response = response.insert_header("link", link);
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/commits"))
            .and(query_param("page", page))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let first: GhSearchHistoryQuery =
        serde_json::from_value(json!({"operation":"commit","owner":"o","repo":"r","path":"src"}))
            .expect("query");
    let one = execute(&provider, &first, &default_context(), &Passthrough)
        .await
        .expect("the page is read, not refused");
    assert_eq!(one["commits"][0]["commitSha"], "a1", "{one}");
    let next = one["next"]["nextPage"]["input"]["queries"][0].clone();
    assert_eq!(next["page"], 2, "{one}");
    let two = execute(
        &provider,
        &serde_json::from_value(next).expect("next query"),
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("page 2");
    assert_eq!(two["commits"][0]["commitSha"], "b1", "{two}");
}

/// An uncapped search count is the exact `total`; a count past GitHub's
/// 1,000-result window states the reachable `total` with `countScope:
/// "capped"` and `partialReasons:["providerResultCap"]`.
#[tokio::test]
async fn a_capped_total_is_the_reachable_window() {
    use wiremock::MockServer;
    for (total_count, capped) in [(130, false), (5_000, true)] {
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
        let query = serde_json::from_value(
            json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["x"]}),
        )
        .expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("pull request search");
        assert_eq!(
            data["pagination"]["total"],
            if capped { 1000 } else { total_count },
            "{data}"
        );
        assert_eq!(
            data["pagination"].get("countScope").is_some(),
            capped,
            "{data}"
        );
        if capped {
            assert_eq!(data["pagination"]["countScope"], "capped", "{data}");
            assert_eq!(
                data["partialReasons"],
                json!(["providerResultCap"]),
                "{data}"
            );
        }
    }
}

/// A REST list page has no total: `total` counts the rows listed so far
/// (`countScope: loaded`, a floor), with no static warning. A pull request
/// list also offers one count call: the same filters searched (created since
/// the epoch, every pull request), one row, whose `total` is the count.
/// A commit list has none: commit search needs keywords.
#[tokio::test]
async fn a_rest_list_page_with_more_counts_rows_so_far_and_offers_a_count() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    let link = |route: &str| {
        format!(
            "<{}/api/v3/repos/o/r/{route}?per_page=50&page=2>; rel=\"next\"",
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
        json!({"operation":"commit","owner":"o","repo":"r"}),
        json!({"operation":"pullRequest","owner":"o","repo":"r"}),
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
        assert_eq!(data["pagination"]["total"], 1, "{data}");
        assert_eq!(data["pagination"]["countScope"], "loaded", "{data}");
        assert!(data.get("warnings").is_none(), "{row}: {data}");
        if row["operation"] == "pullRequest" {
            let count = &crate::tools::test_support::checks::related_call(
                &data,
                "ghSearchHistory",
                |input| input["queries"][0]["limit"] == 1,
            )["input"]["queries"][0];
            assert_eq!(
                (&count["operation"], &count["owner"], &count["repo"]),
                (&json!("pullRequest"), &json!("o"), &json!("r")),
                "{data}"
            );
            assert!(count.pointer("/filters/createdAt/from").is_some(), "{data}");
        }
    }
}

/// R7: a pull-request listing offers its count call once, on page 1. The
/// call is the same on every page (the listing's filters searched, one row),
/// so later pages that still have more do not repeat it.
#[tokio::test]
async fn a_pull_request_listing_offers_its_count_call_on_page_one_only() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        "<{}/api/v3/repos/o/r/pulls?per_page=50&page=9>; rel=\"next\"",
                        server.uri()
                    )
                    .as_str(),
                )
                .set_body_json(json!([{"number": 4, "title": "Fix", "state": "open",
                    "user": {"login": "dev"}, "created_at": "2026-01-01T00:00:00Z"}])),
        )
        .mount(&server)
        .await;
    let counts = |data: &Value| {
        crate::tools::test_support::checks::find_related_call(data, "ghSearchHistory", |input| {
            input["queries"][0]["limit"] == 1
        })
        .is_some()
    };
    let provider = one_shot_provider(&server);
    for (page, offered) in [(1, true), (2, false), (3, false)] {
        let query = serde_json::from_value(
            json!({"operation":"pullRequest","owner":"o","repo":"r","page":page}),
        )
        .expect("query");
        let data = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("listing");
        assert_eq!(data["pagination"]["hasMore"], true, "page {page}: {data}");
        assert_eq!(counts(&data), offered, "page {page}: {data}");
    }
}

#[tokio::test]
/// A REST list has no search window: page 1,000 (the former schema ceiling)
/// keeps its rows and continues to the page GitHub links next.
async fn a_rest_list_past_the_former_page_ceiling_continues() {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::builder().start().await;
    for (operation, route, row, collection) in [
        (
            "commit",
            "commits",
            json!({"sha":"a".repeat(40),"commit":{"message":"Kept"}}),
            "commits",
        ),
        (
            "pullRequest",
            "pulls",
            json!({"number":7,"title":"Kept","state":"open"}),
            "pullRequests",
        ),
    ] {
        let link = format!(
            "<{}/api/v3/repos/o/r/{route}?per_page=50&page=1001>; rel=\"next\"",
            server.uri()
        );
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/r/{route}")))
            .and(query_param("page", "1000"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("link", link.as_str())
                    .set_body_json(json!([row])),
            )
            .expect(1)
            .mount(&server)
            .await;
        let query = serde_json::from_value(
            json!({"operation":operation,"owner":"o","repo":"r","page":1000}),
        )
        .expect("stored private progress");
        let data = execute(
            &one_shot_provider(&server),
            &query,
            &default_context(),
            &Passthrough,
        )
        .await
        .expect("page 1000");
        assert_eq!(data[collection].as_array().map(Vec::len), Some(1), "{data}");
        assert!(data.get("terminalLimit").is_none(), "{data}");
        assert_eq!(
            data["next"]["nextPage"]["input"]["queries"][0]["page"], 1001,
            "{data}"
        );
    }
}
