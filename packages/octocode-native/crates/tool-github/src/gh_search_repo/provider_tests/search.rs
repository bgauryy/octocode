//! Repository search: typed filters, archived exclusion, refused operators, capped counts,
//! ignored languages and failure repairs.
use super::*;
use octocode_test_support::json_fixture;

/// Typed date filters reach GitHub as `pushed:`/`created:` qualifiers; a
/// relative window resolves to an absolute lower bound.
#[tokio::test]
async fn typed_date_filters_reach_the_search_query() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param(
            "q",
            "x stars:>=101 created:2020-01-01T00:00:00Z..2021-01-01T00:00:00Z pushed:>=2025-01-01T00:00:00Z archived:false",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![repo_item("r", false)])))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "keywords": ["x"],
                "operation": "search",
                "filters": {
                    "stars": {"min": 101},
                    "pushedAt": {"from": "2025-01-01T00:00:00Z"},
                    "createdAt": {
                        "from": "2020-01-01T00:00:00Z",
                        "to": "2021-01-01T00:00:00Z"
                    }
                }
            }"#,
        ),
    )
    .await;
    assert_eq!(out.data["repositories"][0]["repo"], "r", "{}", out.data);
}

/// The default search excludes archived repositories (`archived:false`) and,
/// knowing no skipped count, offers no `includeArchived` lead (the tool
/// description states the default once); an explicit `archived:true` search
/// drops the qualifier and flags archived rows.
#[tokio::test]
async fn default_archived_search_exclusion_offers_no_lead() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param("q", "x"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![repo_item("old", true)])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param("q", "x archived:false"))
        .respond_with(octocode_test_support::mock_http::SearchIndex(
            (0..=50)
                .map(|n| repo_item(&format!("live{n}"), false))
                .collect(),
        ))
        .mount(&server)
        .await;

    let first = run(
        &server,
        json_fixture(r#"{"keywords": ["x"], "operation": "search"}"#),
    )
    .await;
    assert!(
        first.data["repositories"][0].get("archived").is_none(),
        "{}",
        first.data
    );
    assert!(
        first.data["next"].get("includeArchived").is_none(),
        "only an owner listing that skipped archived repositories leads to them: {}",
        first.data
    );
    // Repo-discovery words are not code keywords: no code-search lead.
    assert!(
        first.data["next"].get("findCode").is_none(),
        "{}",
        first.data
    );

    // Later pages state nothing more; an explicit filter needs no lead.
    let second = run(
        &server,
        json_fixture(r#"{"keywords": ["x"], "page": 2, "operation": "search"}"#),
    )
    .await;
    assert!(
        second.data["next"].get("includeArchived").is_none(),
        "{}",
        second.data
    );
    assert!(
        second.data["next"].get("findCode").is_none(),
        "{}",
        second.data
    );
    assert!(second.data.get("related").is_none(), "{}", second.data);
    let excluded = run(
        &server,
        json_fixture(
            r#"{
                "keywords": ["x"],
                "operation": "search",
                "filters": {"archive": "exclude"}
            }"#,
        ),
    )
    .await;
    assert!(
        excluded.data["next"].get("includeArchived").is_none(),
        "{}",
        excluded.data
    );

    let archived = run(
        &server,
        json_fixture(
            r#"{
                "keywords": ["x"],
                "operation": "search",
                "filters": {"archive": "include"}
            }"#,
        ),
    )
    .await;
    assert_eq!(
        archived.data["repositories"][0]["archived"], true,
        "{}",
        archived.data
    );
    assert!(
        archived.data["next"].get("includeArchived").is_none(),
        "{}",
        archived.data
    );
}

/// Keywords are ANDed literals: a bare `OR`/`NOT` would be searched as a
/// word and silently empty the search, so it is refused before any request
/// and the message names the operator; a lowercase word is a plain term.
#[tokio::test]
async fn a_boolean_operator_in_keywords_is_refused() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        page(vec![repo_item("r", false)]),
    )
    .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    for (keywords, operator) in [
        (json_fixture(r#"["parser OR lexer"]"#), "OR"),
        (json_fixture(r#"["toml", "NOT"]"#), "NOT"),
    ] {
        let query: GhSearchRepoQuery =
            serde_json::from_value(json!({"operation":"search","keywords": keywords}))
                .expect("query");
        let error = execute(&provider, &query, &context)
            .await
            .expect_err("refused");
        assert_eq!(
            error.kind,
            octocode_github::ProviderErrorKind::Validation,
            "{error:?}"
        );
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
    let plain = run(
        &server,
        json_fixture(r#"{"keywords": ["error or warning"], "operation": "search"}"#),
    )
    .await;
    assert_eq!(plain.data["repositories"][0]["repo"], "r", "{}", plain.data);
}

/// GitHub matched more repositories than search reaches (1,000): the
/// first page states the provider's full count, not only the reachable
/// `total`, so an org count is never silently 1,000.
#[tokio::test]
async fn capped_search_states_the_full_match_count() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        json!({"total_count": 4156, "incomplete_results": false, "items": [repo_item("r", false)]}),
    )
    .await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "owner": "o",
                "sort": {"by": "stars", "order": "desc"},
                "operation": "search"
            }"#,
        ),
    )
    .await;
    // The cap is stated once, as the warning that carries the full count;
    // `isPartial` and `countScope: "capped"` stay as facts.
    assert!(out.data.get("partialReasons").is_none(), "{}", out.data);
    assert_eq!(out.data["isPartial"], json!(true), "{}", out.data);
    assert_eq!(
        out.data["pagination"]["countScope"],
        json!("capped"),
        "{}",
        out.data
    );
    let warnings = out.data["warnings"].to_string();
    assert!(warnings.contains("4156"), "{}", out.data);
    // A page can be read on its own: a later page states the count too.
    let second = run(
        &server,
        json_fixture(
            r#"{
                "owner": "o",
                "sort": {"by": "stars", "order": "desc"},
                "page": 2,
                "operation": "search"
            }"#,
        ),
    )
    .await;
    assert!(
        second.data["warnings"].to_string().contains("4156"),
        "{}",
        second.data
    );
}

/// GitHub silently drops a `language:` it does not know and returns
/// unfiltered rows. Neither mixed language metadata nor equal totals prove
/// this happened; disclose ambiguity. An alias that narrows the total does
/// not need a warning.
#[tokio::test]
async fn an_ignored_language_filter_is_named() {
    let item = |name: &str, language: &str| {
        let mut item = repo_item(name, false);
        item["language"] = json!(language);
        item
    };
    let ignored = MockServer::builder().start().await;
    mount_json(
        &ignored,
        "/api/v3/search/repositories",
        200,
        page(vec![item("a", "Python"), item("b", "Rust")]),
    )
    .await;
    let out = run(
        &ignored,
        json_fixture(
            r#"{
                "keywords": ["x"],
                "operation": "search",
                "filters": {"language": "zz-nolang"}
            }"#,
        ),
    )
    .await;
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains("language") && warnings.contains("zz-nolang"),
        "{}",
        out.data
    );
    assert!(warnings.contains("unverified"), "{}", out.data);
    assert!(
        !warnings.contains("GitHub ignored language"),
        "{}",
        out.data
    );
    let requests = ignored
        .received_requests()
        .await
        .expect("recorded requests");
    let queries = requests
        .iter()
        .filter_map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.into_owned())
        })
        .collect::<Vec<_>>();
    assert_eq!(queries.len(), 2, "search and advisory probe: {queries:?}");
    assert!(queries[0].contains("language:zz-nolang"), "{queries:?}");
    assert!(!queries[1].contains("language:"), "{queries:?}");
    assert!(queries[1].contains('x'), "{queries:?}");

    // A one-language page with equal totals still discloses the filter
    // ambiguity, without asserting that an alias was ignored.
    let agreeing = MockServer::builder().start().await;
    mount_json(
        &agreeing,
        "/api/v3/search/repositories",
        200,
        page(vec![item("a", "TypeScript"), item("b", "TypeScript")]),
    )
    .await;
    let out = run(
        &agreeing,
        json_fixture(
            r#"{
                "keywords": ["x"],
                "operation": "search",
                "filters": {"language": "zz-nolang"}
            }"#,
        ),
    )
    .await;
    assert!(
        out.data["warnings"].to_string().contains("zz-nolang"),
        "{}",
        out.data
    );

    let alias = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(wiremock::matchers::query_param_contains("q", "language:"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(page(vec![item("a", "JavaScript"), item("b", "JavaScript")])),
        )
        .with_priority(1)
        .mount(&alias)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "total_count": 99,
                "incomplete_results": false,
                "items": []
            }"#,
        )))
        .mount(&alias)
        .await;
    let out = run(
        &alias,
        json_fixture(
            r#"{
                "keywords": ["x"],
                "operation": "search",
                "filters": {"language": "js"}
            }"#,
        ),
    )
    .await;
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
}

/// A failure's one repair is a lead, not prose: a page past the search
/// window leads to the last reachable page; an owner+repo that does not
/// resolve leads to a search for the repository's name.
#[tokio::test]
async fn search_failures_lead_to_their_repair() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/missing",
        404,
        json_fixture(r#"{"message": "Not Found"}"#),
    )
    .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    let fail = |query: Value| {
        let query: GhSearchRepoQuery = serde_json::from_value(query).expect("query");
        let provider = &provider;
        let context = &context;
        async move {
            super::super::run(provider, &query, Ok(context))
                .await
                .expect_err("failure")
        }
    };
    let window = fail(json_fixture(
        r#"{"keywords": ["x"], "page": 51, "operation": "search"}"#,
    ))
    .await;
    assert!(window.hints.is_empty(), "{:?}", window.hints);
    let next = window.calls.expect("repair");
    assert_eq!(next["repair"]["tool"], "ghSearchRepo", "{next}");
    let repair = &next["repair"]["input"]["queries"][0];
    assert_eq!(repair["page"], 20, "{next}");
    assert_eq!(repair["keywords"], json_fixture(r#"["x"]"#), "{next}");
    let missing = fail(json_fixture(
        r#"{"owner": "o", "repo": "missing", "operation": "get"}"#,
    ))
    .await;
    // The shared hint labels the name search's rows as candidates under any
    // owner, beside the owner's own listing.
    assert_eq!(
        missing.hints,
        [octocode_tools_shared::gh_shared::REPOSITORY_SEARCH_HINT.to_owned()]
    );
    let next = missing.calls.expect("find");
    assert_eq!(
        next["findRepository"]["input"]["queries"][0],
        json!({"keywords":["missing"],"operation":"search",
            "limit":octocode_tools_shared::gh_shared::RECOVERY_LIMIT}),
        "{next}"
    );
    assert_eq!(
        next["findRepository2"]["input"]["queries"][0],
        json!({"operation":"list","owner":"o","limit":octocode_tools_shared::gh_shared::RECOVERY_LIMIT}),
        "{next}"
    );
}

/// A repository search the token only partly covered (SAML enforcement
/// withheld organizations) says so in `warnings`, as code search does: the
/// page is not a complete answer.
#[tokio::test]
async fn a_partly_withheld_repository_search_warns_to_authorize_the_token() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-github-sso", "partial-results; organizations=21955855")
                .set_body_json(page(vec![repo_item("r", false)])),
        )
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(r#"{"keywords": ["x"], "operation": "search"}"#),
    )
    .await;
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains("SAML") && warnings.contains("21955855"),
        "{}",
        out.data
    );
}

/// The first page leads from its first repository to that repository's
/// tree, once; a later page leads nowhere new.
#[tokio::test]
async fn the_first_page_leads_to_the_first_repositorys_tree() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        page(vec![repo_item("first", false), repo_item("second", false)]),
    )
    .await;
    let out = run(
        &server,
        json_fixture(r#"{"keywords": ["x"], "operation": "search"}"#),
    )
    .await;
    let leads = out.data["related"].to_string();
    assert!(leads.contains("ghStructure"), "{}", out.data);
    assert!(
        leads.contains("first") && !leads.contains("second"),
        "{}",
        out.data
    );
    let later = run(
        &server,
        json_fixture(r#"{"keywords": ["x"], "page": 2, "operation": "search"}"#),
    )
    .await;
    assert!(
        !later.data["related"].to_string().contains("ghStructure"),
        "{}",
        later.data
    );
}

/// `sort.by` names the row field it orders by and reaches GitHub as its own
/// sort key; a page ranked by a field a default row leaves to `fields`
/// (`forks`, `updatedAt`) states that field on each row, and no other detail.
/// `relevance`, the default, sends no sort.
#[tokio::test]
async fn a_sorted_search_sends_the_provider_key_and_states_the_sorted_field() {
    let item = |name: &str, forks: u64| {
        let mut item = repo_item(name, false);
        item["stargazers_count"] = json!(5);
        item["forks_count"] = json!(forks);
        item["updated_at"] = json!("2026-03-04T05:06:07Z");
        item["pushed_at"] = json!("2026-03-01T00:00:00Z");
        item["language"] = json!("Rust");
        item
    };
    for (by, order, provider_key, stated) in [
        ("forks", "desc", Some("forks"), Some("forks")),
        ("forks", "asc", Some("forks"), Some("forks")),
        ("updatedAt", "desc", Some("updated"), Some("updatedAt")),
        ("stars", "asc", Some("stars"), None),
        (
            "helpWantedIssueCount",
            "desc",
            Some("help-wanted-issues"),
            None,
        ),
        ("relevance", "desc", None, None),
    ] {
        let server = MockServer::builder().start().await;
        mount_json(
            &server,
            "/api/v3/search/repositories",
            200,
            page(vec![item("big", 90), item("small", 2)]),
        )
        .await;
        let out = run(
            &server,
            json!({"operation":"search","keywords":["x"],"sort":{"by":by,"order":order}}),
        )
        .await;
        let requests = server.received_requests().await.expect("requests");
        let sent = requests
            .iter()
            .find(|request| request.url.path() == "/api/v3/search/repositories")
            .expect("search request");
        let param = |name: &str| {
            sent.url
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        assert_eq!(param("sort").as_deref(), provider_key, "{by} {order}");
        assert_eq!(
            param("order").as_deref(),
            provider_key.map(|_| order),
            "{by} {order}"
        );
        let rows = out.data["repositories"].as_array().expect("rows");
        assert_eq!(rows.len(), 2, "{}", out.data);
        for row in rows {
            let keys = row
                .as_object()
                .expect("row")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let mut expected = vec!["owner", "repo", "stars"];
            expected.extend(stated);
            assert_eq!(keys, expected, "{by} {order}: {}", out.data);
        }
        if stated == Some("forks") {
            assert_eq!(rows[0]["forks"], 90, "{}", out.data);
        }
    }
}

/// The retired sort spellings are no aliases: the schema refuses them, and
/// `relevance` has one direction.
#[test]
fn retired_sort_keys_and_an_ascending_relevance_are_refused() {
    for sort in [
        json_fixture(r#"{"by": "bestMatch", "order": "desc"}"#),
        json_fixture(r#"{"by": "forkCount", "order": "desc"}"#),
        json_fixture(r#"{"by": "relevance", "order": "asc"}"#),
    ] {
        let row = json!({"operation":"search","keywords":["x"],"sort":sort});
        assert!(
            serde_json::from_value::<GhSearchRepoQuery>(row.clone()).is_err(),
            "{row}"
        );
    }
}
