//! Repository search: typed filters, archived exclusion, refused operators, capped counts,
//! ignored languages and failure repairs.
use super::*;

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
        json!({"keywords":["x"],"operation":"search","filters":{"stars":{"min":101},"pushedAt":{"from":"2025-01-01T00:00:00Z"},"createdAt":{"from":"2020-01-01T00:00:00Z","to":"2021-01-01T00:00:00Z"}}}),
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 40, "incomplete_results": false,
            "items": [repo_item("live", false)]
        })))
        .mount(&server)
        .await;

    let first = run(&server, json!({"keywords":["x"],"operation":"search"})).await;
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
        json!({"keywords":["x"],"page":2,"operation":"search"}),
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
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &second.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["tool"],
        "ghStructure",
        "{}",
        second.data
    );
    let excluded = run(
        &server,
        json!({"keywords":["x"],"operation":"search","filters":{"archive":"exclude"}}),
    )
    .await;
    assert!(
        excluded.data["next"].get("includeArchived").is_none(),
        "{}",
        excluded.data
    );

    let archived = run(
        &server,
        json!({"keywords":["x"],"operation":"search","filters":{"archive":"include"}}),
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
        (json!(["parser OR lexer"]), "OR"),
        (json!(["toml", "NOT"]), "NOT"),
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
        json!({"keywords":["error or warning"],"operation":"search"}),
    )
    .await;
    assert_eq!(plain.data["repositories"][0]["repo"], "r", "{}", plain.data);
}

/// GitHub matched more repositories than search reaches (1,000): the
/// first page states the provider's full count, not only the reachable
/// `totalItems`, so an org count is never silently 1,000.
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
        json!({"owner":"o","sort":{"by":"stars","order":"desc"},"operation":"search"}),
    )
    .await;
    assert_eq!(
        out.data["partialReasons"],
        json!(["providerResultCap"]),
        "{}",
        out.data
    );
    let warnings = out.data["warnings"].to_string();
    assert!(warnings.contains("4156"), "{}", out.data);
    // Later pages do not repeat it.
    let second = run(
        &server,
        json!({"owner":"o","sort":{"by":"stars","order":"desc"},"page":2,"operation":"search"}),
    )
    .await;
    crate::tools::test_support::checks::assert_absent(&second.data["warnings"], "4156");
}

/// GitHub silently drops a `language:` it does not know and returns
/// unfiltered rows. A page where no row is that language says so when the
/// rows mix languages, or when they share one and the same search without
/// the qualifier counts the same; an alias GitHub honors (`js` → JavaScript
/// rows, a smaller total) does not trip it.
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
        json!({"keywords":["x"],"operation":"search","filters":{"language":"zz-nolang"}}),
    )
    .await;
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains("language") && warnings.contains("zz-nolang"),
        "{}",
        out.data
    );

    // Ignored, but the unfiltered rows share one language (the live case):
    // the probe without the qualifier counts the same.
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
        json!({"keywords":["x"],"operation":"search","filters":{"language":"zz-nolang"}}),
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
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"total_count": 99, "incomplete_results": false, "items": []}),
            ),
        )
        .mount(&alias)
        .await;
    let out = run(
        &alias,
        json!({"keywords":["x"],"operation":"search","filters":{"language":"js"}}),
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
        json!({"message":"Not Found"}),
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
    let window = fail(json!({"keywords":["x"],"page":51,"operation":"search"})).await;
    assert!(window.hints.is_empty(), "{:?}", window.hints);
    let next = window.calls.expect("repair");
    assert_eq!(next["repair"]["tool"], "ghSearchRepo", "{next}");
    let repair = &next["repair"]["input"]["queries"][0];
    assert_eq!(repair["page"], 20, "{next}");
    assert_eq!(repair["keywords"], json!(["x"]), "{next}");
    let missing = fail(json!({"owner":"o","repo":"missing","operation":"get"})).await;
    assert!(missing.hints.is_empty(), "{:?}", missing.hints);
    let next = missing.calls.expect("find");
    assert_eq!(
        next["findRepository"]["input"]["queries"][0],
        json!({"keywords":["missing"],"operation":"search"}),
        "{next}"
    );
}
