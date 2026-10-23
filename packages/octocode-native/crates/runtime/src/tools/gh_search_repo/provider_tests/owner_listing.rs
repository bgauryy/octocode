//! Owner-only listings: archived counts, page sizes, the token user's private
//! repositories and forks.
use super::*;

/// An owner listing knows how many archived repositories it skipped: it
/// says so and leads to them.
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
    let out = run(&server, json!({"owner":"o"})).await;
    assert_eq!(out.data["repositories"].as_array().map(Vec::len), Some(1));
    let warnings = out.data["warnings"].to_string();
    assert!(warnings.contains("2 archived repositories"), "{}", out.data);
    assert_eq!(
        out.data["next"]["includeArchived"]["query"]["queries"][0],
        json!({"owner":"o","includeArchived":true}),
        "{}",
        out.data
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
    let lead = out.data["next"]["includeArchived"]["query"]["queries"][0].clone();
    let all = run(&with_fork, lead).await;
    let repos = all.data["repositories"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["repo"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        json!(repos),
        json!(["live", "old", "forked"]),
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
    let out = run(&clean, json!({"owner":"o"})).await;
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
    let mut row = json!({"owner":"o","pageSize":2});
    let mut listed = Vec::new();
    for _ in 0..3 {
        let out = run(&server, row.clone()).await;
        let rows = out.data["repositories"].as_array().expect("rows");
        assert!(rows.len() <= 2, "{}", out.data);
        listed.extend(rows.iter().map(|row| row["repo"].clone()));
        match out.data.pointer("/next/nextPage/query/queries/0") {
            Some(next) => row = next.clone(),
            None => break,
        }
    }
    assert_eq!(json!(listed), json!(["a", "b", "c", "d"]));
}

/// The token's own login lists every repository it owns, private ones
/// included; another user's listing is public. A user owner is recognized
/// once per transport, not with a failed organization lookup on every page.
#[tokio::test]
async fn an_owner_listing_of_the_token_user_includes_private_repositories() {
    let server = MockServer::builder().start().await;
    for owner in ["me", "other"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/orgs/{owner}/repos")))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"Me"})))
        .expect(1)
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
    let context = default_context();
    let listing = |owner: &str| {
        serde_json::from_value::<GhSearchRepoQuery>(json!({"owner": owner, "pageSize": 1}))
            .expect("query")
    };
    let mine = execute(&provider, &listing("me"), &context)
        .await
        .expect("own");
    assert_eq!(
        mine.data["repositories"][0]["repo"], "secret",
        "{}",
        mine.data
    );
    let again = execute(&provider, &listing("me"), &context)
        .await
        .expect("own again");
    assert_eq!(
        again.data["repositories"][0]["repo"], "secret",
        "{}",
        again.data
    );
    let theirs = execute(&provider, &listing("other"), &context)
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
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"me"})))
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
    let context = default_context();
    let query: GhSearchRepoQuery =
        serde_json::from_value(json!({"owner": "me", "pageSize": 1})).expect("query");
    let first = execute(&provider, &query, &context).await.expect("first");
    assert_eq!(
        first.data["repositories"][0]["repo"], "public",
        "{}",
        first.data
    );
    let second = execute(&provider, &query, &context).await.expect("second");
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
    let out = run(&server, json!({"owner":"o"})).await;
    let rows = out.data["repositories"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{}", out.data);
    assert!(rows[0].get("fork").is_none(), "{}", out.data);
    assert_eq!(rows[1]["fork"], true, "{}", out.data);
}
