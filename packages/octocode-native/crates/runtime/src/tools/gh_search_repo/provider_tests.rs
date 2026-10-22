//! Provider-backed ghSearchRepo tests: typed filters, archived disclosure,
//! and row facts.
use super::{GhSearchRepoQuery, execute};
use crate::response::result::ToolData;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use octocode_github::mock_http::mount_json;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn run(server: &MockServer, query: Value) -> ToolData {
    let provider = one_shot_provider(server);
    let query: GhSearchRepoQuery = serde_json::from_value(query).expect("query");
    let context = default_context();
    execute(&provider, &query, &context).await.expect("search")
}

fn repo_item(name: &str, archived: bool) -> Value {
    json!({
        "full_name": format!("o/{name}"), "name": name,
        "html_url": "https://x", "default_branch": "main",
        "archived": archived
    })
}

fn page(items: Vec<Value>) -> Value {
    json!({"total_count": items.len(), "incomplete_results": false, "items": items})
}

/// Typed date filters reach GitHub as `pushed:`/`created:` qualifiers; a
/// relative window resolves to an absolute lower bound.
#[tokio::test]
async fn typed_date_filters_reach_the_search_query() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param(
            "q",
            "x stars:>100 pushed:>2025-01-01 created:2020-01-01..2021-01-01 archived:false",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![repo_item("r", false)])))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json!({"keywords":["x"],"stars":"> 100","pushed":">2025-01-01",
               "created":"2020-01-01 .. 2021-01-01"}),
    )
    .await;
    assert_eq!(out.data["repositories"][0]["repo"], "r", "{}", out.data);

    let relative = MockServer::builder().start().await;
    mount_json(
        &relative,
        "/api/v3/search/repositories",
        200,
        page(vec![repo_item("r", false)]),
    )
    .await;
    run(&relative, json!({"pushed":"30d"})).await;
    let requests = relative.received_requests().await.expect("requests");
    let q = requests[0]
        .url
        .query_pairs()
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.into_owned())
        .expect("q");
    let pushed = q
        .split(' ')
        .find(|term| term.starts_with("pushed:"))
        .expect("pushed");
    assert!(pushed.starts_with("pushed:>="), "{q}");
    assert_eq!(pushed.len(), "pushed:>=2026-01-01".len(), "{q}");
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

    let first = run(&server, json!({"keywords":["x"]})).await;
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
    let second = run(&server, json!({"keywords":["x"],"page":2})).await;
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
        second.data["next"]["viewRepo"]["tool"], "ghStructure",
        "{}",
        second.data
    );
    let excluded = run(&server, json!({"keywords":["x"],"includeArchived":false})).await;
    assert!(
        excluded.data["next"].get("includeArchived").is_none(),
        "{}",
        excluded.data
    );

    let archived = run(&server, json!({"keywords":["x"],"includeArchived":true})).await;
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
            serde_json::from_value(json!({"keywords": keywords})).expect("query");
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
    let plain = run(&server, json!({"keywords":["error or warning"]})).await;
    assert_eq!(plain.data["repositories"][0]["repo"], "r", "{}", plain.data);
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

/// An exact lookup of a renamed repository says so: the row names the new
/// repository, and nothing else would tell the caller its name was stale.
#[tokio::test]
async fn an_exact_lookup_of_a_renamed_repository_warns() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/old",
        200,
        json!({"full_name":"o/new","name":"new","html_url":"h","default_branch":"main","archived":false}),
    )
    .await;
    let out = run(&server, json!({"owner":"o","repo":"old"})).await;
    assert_eq!(out.data["repositories"][0]["repo"], "new", "{}", out.data);
    assert_eq!(
        out.data["warnings"],
        json!(["Repository o/old was renamed to o/new; leads use the canonical name."]),
        "{}",
        out.data
    );
    let same = MockServer::builder().start().await;
    mount_json(
        &same,
        "/api/v3/repos/o/new",
        200,
        json!({"full_name":"O/New","name":"New","html_url":"h","default_branch":"main","archived":false}),
    )
    .await;
    let out = run(&same, json!({"owner":"o","repo":"new"})).await;
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
}

/// A search inside a renamed repository: GitHub search does not follow the
/// rename and answers the old name with a 422 "cannot be searched", so the
/// search reruns once under the canonical name, with the rename warning.
#[tokio::test]
async fn a_filtered_search_of_a_renamed_repository_reruns_under_its_new_name() {
    use wiremock::matchers::query_param_contains;
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param_contains("q", "repo:o/old"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({
            "message": "Validation Failed",
            "errors": [{"message": "The listed users and repositories cannot be searched either because the resources do not exist or you do not have permission to view them.",
                "resource": "Search", "field": "q", "code": "invalid"}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .and(query_param_contains("q", "repo:o/new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(vec![repo_item("new", false)])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/old"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"o/new"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let out = run(&server, json!({"owner":"o","repo":"old","keywords":["x"]})).await;
    assert_eq!(out.data["repositories"][0]["repo"], "new", "{}", out.data);
    assert_eq!(
        out.data["warnings"][0],
        crate::tools::gh_shared::renamed_warning("o", "old", &("o".into(), "new".into())),
        "{}",
        out.data
    );
}

/// Rows carry the facts that decide a repository choice: the last push by
/// default, the creation date as a `details` extra; forks and the
/// metadata-update date are `stats` extras.
#[tokio::test]
async fn rows_carry_creation_date_as_a_detail_and_counts_as_stats() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        page(vec![json!({
            "full_name":"o/r","name":"r","html_url":"h","default_branch":"main",
            "stargazers_count":5,"forks_count":0,"open_issues_count":3,
            "pushed_at":"2026-01-02T00:00:00Z","created_at":"2019-05-06T00:00:00Z",
            "updated_at":"2026-01-03T00:00:00Z","homepage":""
        })]),
    )
    .await;
    let out = run(&server, json!({"keywords":["x"]})).await;
    let row = &out.data["repositories"][0];
    assert_eq!(row["createdAt"], "2019-05-06", "{row}");
    assert_eq!(row["pushedAt"], "2026-01-02", "{row}");
    for absent in ["homepage", "openIssuesCount"] {
        assert!(row.get(absent).is_none(), "{absent}: {row}");
    }
    let kind_of = |field: &str| {
        let path = format!("results[].data.repositories[].{field}");
        crate::id::ToolId::GhSearchRepo
            .extra_paths()
            .iter()
            .find(|(_, paths)| paths.contains(&path.as_str()))
            .map(|(kind, _)| *kind)
    };
    assert_eq!(kind_of("createdAt"), Some("details"));
    assert_eq!(kind_of("pushedAt"), None, "the last push is default output");
    for field in ["forks", "updatedAt"] {
        assert_eq!(kind_of(field), Some("stats"), "{field}");
    }
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
        json!({"owner":"o","sort":"stars:desc","pageSize":1}),
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
        json!({"owner":"o","sort":"stars:desc","pageSize":1,"page":2}),
    )
    .await;
    crate::tools::test_support::checks::assert_absent(&second.data["warnings"], "4156");
}

/// owner+repo reads that one repository from the repository endpoint: one
/// row with its language and default branch, no search request, no page.
#[tokio::test]
async fn owner_and_repo_return_that_repository_without_searching() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/sindresorhus/is",
        200,
        json!({
            "full_name": "sindresorhus/is", "name": "is", "html_url": "https://x",
            "url": "https://x", "default_branch": "main", "language": "TypeScript",
            "stargazers_count": 1700, "description": "Type check values",
            "pushed_at": "2026-09-01T00:00:00Z"
        }),
    )
    .await;
    let out = run(&server, json!({"owner":"sindresorhus","repo":"is"})).await;
    assert_eq!(
        out.data["repositories"],
        json!([{
            "owner": "sindresorhus", "repo": "is", "stars": 1700,
            "language": "TypeScript", "pushedAt": "2026-09-01",
            "description": "Type check values", "forks": 0,
            "defaultBranch": "main"
        }]),
        "{}",
        out.data
    );
    assert!(out.data.get("pagination").is_none(), "{}", out.data);
    assert!(out.data.get("next").is_none(), "{}", out.data);
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].url.path(), "/api/v3/repos/sindresorhus/is");
}

/// Anchors: a search row carries exactly the next hop's coordinates
/// (`owner`, `repo`, `defaultBranch`) beside its decision facts, and no
/// query echo.
#[tokio::test]
async fn a_search_row_carries_owner_repo_and_default_branch() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/repositories",
        200,
        page(vec![repo_item("r", false)]),
    )
    .await;
    let out = run(&server, json!({"keywords":["x"]})).await;
    let row = &out.data["repositories"][0];
    assert_eq!(
        row,
        &json!({"owner":"o","repo":"r","defaultBranch":"main","stars":0,"forks":0}),
        "{}",
        out.data
    );
    for echo in ["keywords", "page", "sort"] {
        assert!(out.data.get(echo).is_none(), "{echo}: {}", out.data);
    }
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
    let out = run(&ignored, json!({"keywords":["x"],"language":"zz-nolang"})).await;
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
    let out = run(&agreeing, json!({"keywords":["x"],"language":"zz-nolang"})).await;
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
    let out = run(&alias, json!({"keywords":["x"],"language":"js"})).await;
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
            super::run(provider, &query, Ok(context))
                .await
                .expect_err("failure")
        }
    };
    let window = fail(json!({"keywords":["x"],"page":51,"pageSize":20})).await;
    assert!(window.hints.is_empty(), "{:?}", window.hints);
    let next = window.next.expect("repair");
    assert_eq!(next["repair"]["tool"], "ghSearchRepo", "{next}");
    let repair = &next["repair"]["query"]["queries"][0];
    assert_eq!(repair["page"], 50, "{next}");
    assert_eq!(repair["keywords"], json!(["x"]), "{next}");
    let missing = fail(json!({"owner":"o","repo":"missing"})).await;
    assert!(missing.hints.is_empty(), "{:?}", missing.hints);
    let next = missing.next.expect("find");
    assert_eq!(
        next["findRepository"]["query"]["queries"][0],
        json!({"keywords":["missing"]}),
        "{next}"
    );
}

mod backed;
