//! Owner-only listings: archived counts, page sizes, the token user's private
//! repositories and forks.
use super::*;
use octocode_test_support::json_fixture;

/// An owner listing knows how many archived repositories it skipped: it
/// says so. A page with repositories is the answer, with no lead; a page the
/// archive filter left empty leads to the same listing including them.
#[tokio::test]
async fn owner_listing_counts_skipped_archived_repositories() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            repo_item("live", false),
            repo_item("old", true),
            repo_item("older", true)
        ])))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(r#"{"owner": "o", "operation": "list"}"#),
    )
    .await;
    assert_eq!(out.data["repositories"].as_array().map(Vec::len), Some(1));
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains(
            r#"filters.archive \"exclude\" left out 2 archived repositories (a filter, not a gap)"#
        ),
        "{}",
        out.data
    );
    assert!(out.data.get("related").is_none(), "{}", out.data);

    let archived_only = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([repo_item("old", true), repo_item("older", true)])),
        )
        .mount(&archived_only)
        .await;
    let empty = run(
        &archived_only,
        json_fixture(r#"{"owner": "o", "operation": "list"}"#),
    )
    .await;
    assert_eq!(empty.status, Some("empty"), "{}", empty.data);
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(
            &empty.data,
            "ghSearchRepo",
            |input| input["queries"][0]["filters"]["archive"] == "include"
        )["input"]["queries"][0],
        json_fixture(
            r#"{
                "owner": "o",
                "operation": "list",
                "filters": {"archive": "include"}
            }"#
        ),
        "{}",
        empty.data
    );
    // The lead lists the same owner, archived rows included: no search
    // (which would drop forks) and nothing skipped.
    let mut fork = repo_item("forked", false);
    fork["fork"] = json!(true);
    let with_fork = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            repo_item("live", false),
            repo_item("old", true),
            fork
        ])))
        .mount(&with_fork)
        .await;
    let lead = octocode_runtime_base::test_support::checks::related_call(
        &empty.data,
        "ghSearchRepo",
        |input| input["queries"][0]["filters"]["archive"] == "include",
    )["input"]["queries"][0]
        .clone();
    let all = run(&with_fork, lead).await;
    let repos = all.data["repositories"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["repo"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        json!(repos),
        json_fixture(r#"["live", "old", "forked"]"#),
        "{}",
        all.data
    );
    assert_eq!(all.data["repositories"][2]["fork"], true, "{}", all.data);
    assert!(all.data.get("warnings").is_none(), "{}", all.data);

    let clean = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("live", false)])))
        .mount(&clean)
        .await;
    let out = run(
        &clean,
        json_fixture(r#"{"owner": "o", "operation": "list"}"#),
    )
    .await;
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
    assert!(
        out.data["next"].get("includeArchived").is_none(),
        "{}",
        out.data
    );
}

/// An owner listing never shows more rows than `pageSize`: a provider page
/// that kept rows ends the call, and the next page resumes after it, so no
/// kept row is cut or shown twice.
#[tokio::test]
async fn an_owner_listing_page_never_exceeds_its_page_size() {
    let server = MockServer::builder().start().await;
    let link = |page: usize| {
        format!(
            "<{}/api/v3/orgs/o/repos?page={page}&per_page=2>; rel=\"next\"",
            server.uri()
        )
    };
    for (page, rows) in [
        (1, json!([repo_item("old", true), repo_item("a", false)])),
        (2, json!([repo_item("b", false), repo_item("c", false)])),
        (3, json!([repo_item("d", false)])),
    ] {
        let mut response = ResponseTemplate::new(200).set_body_json(rows);
        if page < 3 {
            response = response.insert_header("link", link(page + 1).as_str());
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/orgs/o/repos"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let mut row = json_fixture(r#"{"owner": "o", "operation": "list"}"#);
    let mut listed = Vec::new();
    for _ in 0..3 {
        let out = run(&server, row.clone()).await;
        let rows = out.data["repositories"].as_array().expect("rows");
        assert!(rows.len() <= 50, "{}", out.data);
        listed.extend(rows.iter().map(|row| row["repo"].clone()));
        match out.data.pointer("/next/nextPage/input/queries/0") {
            Some(next) => row = next.clone(),
            None => break,
        }
    }
    assert_eq!(json!(listed), json_fixture(r#"["a", "b", "c", "d"]"#));
}

/// `archive: "only"` reads provider pages inside one call until it has
/// `limit` archived rows or the listing ends: many non-archived pages before
/// the first archived one never end a call with no rows.
#[tokio::test]
async fn an_archive_only_listing_reads_on_until_it_has_rows() {
    const LIVE_PAGES: usize = 12;
    let server = MockServer::builder().start().await;
    let link = |page: usize| {
        format!(
            "<{}/api/v3/orgs/o/repos?page={page}&per_page=2>; rel=\"next\"",
            server.uri()
        )
    };
    for page in 1..=LIVE_PAGES + 1 {
        let rows = if page <= LIVE_PAGES {
            json!([
                repo_item(&format!("a{page}"), false),
                repo_item(&format!("b{page}"), false)
            ])
        } else {
            json!([repo_item("old", true), repo_item("older", true)])
        };
        let mut response = ResponseTemplate::new(200).set_body_json(rows);
        if page <= LIVE_PAGES {
            response = response.insert_header("link", link(page + 1).as_str());
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/orgs/o/repos"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let out = run(
        &server,
        json_fixture(
            r#"{
                "owner": "o",
                "operation": "list",
                "limit": 2,
                "filters": {"archive": "only"}
            }"#,
        ),
    )
    .await;
    let repos = out.data["repositories"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["repo"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        json!(repos),
        json_fixture(r#"["old", "older"]"#),
        "{}",
        out.data
    );
}

/// The token's own login lists every repository it owns, private ones
/// included; another user's listing is public. Each page independently
/// resolves the route from its credential.
#[tokio::test]
async fn an_owner_listing_of_the_token_user_includes_private_repositories() {
    let server = MockServer::builder().start().await;
    for owner in ["me", "other"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/orgs/{owner}/repos")))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
            )
            // One probe per owner for this credential, however many pages or
            // listing calls follow.
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"login": "Me"}"#)))
        .expect(2)
        .mount(&server)
        .await;
    let link = format!(
        "<{}/api/v3/user/repos?page=2&per_page=1>; rel=\"next\"",
        server.uri()
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .and(query_param("visibility", "all"))
        .and(query_param("affiliation", "owner"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", link.as_str())
                .set_body_json(json!([repo_item("old", true)])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("secret", false)])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/other/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("public", false)])))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    // Each call is its own request, but the key remembers the route.
    let listing = |owner: &str| {
        serde_json::from_value::<GhSearchRepoQuery>(json!({"operation":"list","owner": owner}))
            .expect("query")
    };
    let mine = execute(&provider, &listing("me"), &default_context())
        .await
        .expect("own");
    assert_eq!(
        mine.data["repositories"][0]["repo"], "secret",
        "{}",
        mine.data
    );
    let again = execute(&provider, &listing("me"), &default_context())
        .await
        .expect("own again");
    assert_eq!(
        again.data["repositories"][0]["repo"], "secret",
        "{}",
        again.data
    );
    let theirs = execute(&provider, &listing("other"), &default_context())
        .await
        .expect("other");
    assert_eq!(
        theirs.data["repositories"][0]["repo"], "public",
        "{}",
        theirs.data
    );
}

/// A `/user` read that fails transiently is not remembered: the next
/// listing asks again and, once GitHub answers, lists private repositories.
#[tokio::test]
async fn a_failed_viewer_lookup_is_not_remembered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/me/repos"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"login": "me"}"#)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/me/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("public", false)])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("secret", false)])))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchRepoQuery =
        serde_json::from_value(json_fixture(r#"{"owner": "me", "operation": "list"}"#))
            .expect("query");
    let first = execute(&provider, &query, &default_context())
        .await
        .expect("first");
    assert_eq!(
        first.data["repositories"][0]["repo"], "public",
        "{}",
        first.data
    );
    let second = execute(&provider, &query, &default_context())
        .await
        .expect("second");
    assert_eq!(
        second.data["repositories"][0]["repo"], "secret",
        "{}",
        second.data
    );
}

/// An owner listing marks forks (`fork:true`) so they do not read as the
/// owner's own source; other rows carry no `fork` key.
#[tokio::test]
async fn owner_listing_marks_forks_only() {
    let server = MockServer::builder().start().await;
    let mut fork = repo_item("forked", false);
    fork["fork"] = json!(true);
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([repo_item("own", false), fork])),
        )
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(r#"{"owner": "o", "operation": "list"}"#),
    )
    .await;
    let rows = out.data["repositories"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{}", out.data);
    assert!(rows[0].get("fork").is_none(), "{}", out.data);
    assert_eq!(rows[1]["fork"], true, "{}", out.data);
}

/// `archive:"only"` leaves out the non-archived rows, and says exactly
/// that: a filter, never a gap, and no lead to the rows it filtered.
#[tokio::test]
async fn archive_only_listing_names_the_non_archived_rows_it_filtered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            repo_item("live", false),
            repo_item("old", true),
            repo_item("older", true)
        ])))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "owner": "o",
                "operation": "list",
                "filters": {"archive": "only"}
            }"#,
        ),
    )
    .await;
    assert_eq!(out.data["repositories"].as_array().map(Vec::len), Some(2));
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains(
            r#"filters.archive \"only\" left out 1 non-archived repository (a filter, not a gap)"#
        ),
        "{}",
        out.data
    );
    assert!(!warnings.contains("archived repositories"), "{}", out.data);
    assert!(
        out.data["next"].get("includeArchived").is_none(),
        "{}",
        out.data
    );
}

/// A route a key learned earlier that now answers 404 (the owner was renamed
/// or converted) is asked again once, not trusted forever.
#[tokio::test]
async fn a_remembered_owner_route_that_stops_answering_is_probed_again() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("org", false)])))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/orgs/o/repos"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"login": "me"}"#)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/o/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_item("user", false)])))
        .mount(&server)
        .await;
    let provider = one_shot_provider(&server);
    let query: GhSearchRepoQuery =
        serde_json::from_value(json_fixture(r#"{"owner": "o", "operation": "list"}"#))
            .expect("query");
    let first = execute(&provider, &query, &default_context())
        .await
        .expect("first");
    assert_eq!(
        first.data["repositories"][0]["repo"], "org",
        "{}",
        first.data
    );
    let second = execute(&provider, &query, &default_context())
        .await
        .expect("second");
    assert_eq!(
        second.data["repositories"][0]["repo"], "user",
        "{}",
        second.data
    );
}
