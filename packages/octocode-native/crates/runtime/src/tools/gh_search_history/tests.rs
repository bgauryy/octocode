use super::filters::Filters;
use super::leads::{issue_links_read, issue_read_query, pr_read_query};
use super::query::{build_query_with_warnings, resolve_commit_window};
use super::rows::{
    is_completed, is_merged, item_number, map_commit, map_commit_list, map_issue, map_pr,
    read_target,
};
use super::shape::mark_empty;
use super::*;
use crate::providers::github::{GitHubTransport, ProviderError};
use crate::tools::gh_shared::test_support::mount_json;
use serde_json::{Value, json};

use crate::security::scan::Passthrough;

fn transport(
    server: &wiremock::MockServer,
) -> GitHubTransport<crate::providers::github::StaticCredentialResolver> {
    use crate::providers::github::{
        CredentialSource, GitHubEndpoint, RetryPolicy, StaticCredentialResolver,
    };
    GitHubTransport::new(
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("url"))
            .expect("endpoint"),
        std::sync::Arc::new(StaticCredentialResolver::new(
            "fixture",
            CredentialSource::Override,
        )),
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
    )
    .expect("transport")
}

fn context() -> RequestContext {
    RequestContext::with_timeout(std::time::Duration::from_secs(5), 1 << 20)
}

fn search(q: &GhSearchHistoryQuery) -> Result<HistorySearch, ProviderError> {
    HistorySearch::new(q.clone())
}
fn build_query(q: &GhSearchHistoryQuery) -> Result<String, ProviderError> {
    build_query_with_warnings(&search(q)?).map(|(terms, _)| terms)
}
fn should_use_search_for_prs(q: &GhSearchHistoryQuery) -> bool {
    query::should_use_search_for_prs(&search(q).expect("qualifiers"))
}
fn needs_issue_search_qualifiers(q: &GhSearchHistoryQuery) -> bool {
    query::needs_issue_search_qualifiers(&search(q).expect("qualifiers"))
}
async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhSearchHistoryQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    super::execute(transport, query.clone(), context, security).await
}

#[test]
fn canonical_issue_qualifier_order() {
    let q: GhSearchHistoryQuery=serde_json::from_str(r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"],"state":"closed","qualifiers":"in:title label:bug"}"#).expect("GitHub history search test data should be valid");
    assert_eq!(
        build_query(&q).expect("GitHub history search test data should be valid"),
        "x in:title is:issue repo:a/b is:closed label:\"bug\""
    );
    let archived: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"],"qualifiers":"archived:true"}"#,
    )
    .expect("valid");
    assert!(
        build_query(&archived)
            .expect("valid")
            .ends_with("archived:true")
    );
}
#[test]
fn report_probes_cannot_rewrite_the_history_scope() {
    // A leading-quote keyword, an owner carrying operators,
    // and a label with an interior quote.
    let q = parse(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"octocat","repo":"Hello-World","keywords":["\"hello\" NOT"]}"#,
    );
    assert_eq!(
        build_query(&q).expect("valid"),
        "\"hello NOT\" is:issue repo:octocat/Hello-World"
    );
    let q = parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","keywords":["repo:evil/x","-y"]}"#,
    );
    assert_eq!(
        build_query(&q).expect("valid"),
        "\"repo:evil/x\" \"-y\" is:pr user:a"
    );
    for raw in [
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"octocat OR is:public","repo":"b","keywords":["hello"]}"#,
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b:c","keywords":["hello"]}"#,
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","qualifiers":"author:\"x OR is:public\""}"#,
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","qualifiers":"label:x\" OR is:public\""}"#,
        r#"{"operation":"issue","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","qualifiers":"created:\"x OR is:public\""}"#,
        r#"{"operation":"commit","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","keywords":["x"],"committer":"a b"}"#,
    ] {
        // Rejected by the published pattern or by native validation.
        if let Ok(query) = serde_json::from_str::<GhSearchHistoryQuery>(raw) {
            let error = build_query(&query).expect_err(raw);
            assert_eq!(error.kind, ProviderErrorKind::Validation, "{raw}");
        }
    }
    let q = parse(
        r#"{"operation":"issue","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","qualifiers":"label:\"good first issue\" comments:>5 author:dependabot[bot]"}"#,
    );
    let built = build_query(&q).expect("valid");
    assert!(built.contains("label:\"good first issue\""), "{built}");
    assert!(built.contains("comments:>5"), "{built}");
    assert!(built.contains("author:dependabot[bot]"), "{built}");
}

/// `archived` means the same with or without keywords: only search
/// filters on repository archive state, so it always routes there.
#[test]
fn archived_pull_request_filter_always_routes_to_search() {
    let parse = |extra: &str| -> GhSearchHistoryQuery {
        serde_json::from_str(&format!(
            r#"{{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b"{extra}}}"#
        ))
        .expect("GitHub history search test data should be valid")
    };
    assert!(!should_use_search_for_prs(&parse("")));
    for extra in [
        r#","qualifiers":"archived:true""#,
        r#","qualifiers":"archived:false""#,
        r#","qualifiers":"archived:true","keywords":["README"]"#,
    ] {
        let query = parse(extra);
        assert!(should_use_search_for_prs(&query), "{extra}");
        let built = build_query(&query).expect("query");
        assert!(built.contains("archived:"), "{extra}: {built}");
    }
}

#[test]
fn quotes_multiword_history_keywords() {
    let q: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix login"]}"#,
    )
    .expect("GitHub history search test data should be valid");
    assert!(
        build_query(&q)
            .expect("GitHub history search test data should be valid")
            .starts_with("\"fix login\"")
    );
    assert!(needs_issue_search_qualifiers(&q));
    let listed: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"issue","mainGoal":"test","reasoning":"test","owner":"a","repo":"b"}"#,
    )
    .expect("GitHub history search test data should be valid");
    assert!(!needs_issue_search_qualifiers(&listed));
}
#[test]
fn commit_search_uses_email_and_committer_date() {
    let q: GhSearchHistoryQuery = serde_json::from_str(
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"author":"dev@example.com","since":"2026-01-01T00:00:00Z"}"#,
    )
    .expect("GitHub history search test data should be valid");
    let query = build_query(&q).expect("GitHub history search test data should be valid");
    assert!(query.contains("author-email:dev@example.com"));
    assert!(query.contains("committer-date:>=2026-01-01T00:00:00Z"));
}
#[test]
fn rejects_unscoped_commit() {
    // Commit history is repository-scoped by the wire contract.
    assert!(
        serde_json::from_str::<GhSearchHistoryQuery>(
            r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a"}"#
        )
        .is_err()
    );
}

#[test]
fn pull_request_rows_normalize_merged_state_from_list_and_search_shapes() {
    let listed = map_pr(json!({
        "number":1,
        "title":"listed",
        "state":"closed",
        "merged_at":"2026-09-20T10:00:00Z",
        "user":{"login":"dev"},
        "labels":[],
        "created_at":"2026-09-19T10:00:00Z",
        "comments":2
    }));
    assert_eq!(listed["state"], "merged");
    assert_eq!(listed["mergedAt"], "2026-09-20T10:00:00Z");
    assert_eq!(listed["author"], "dev");
    assert_eq!(listed["commentsCount"], 2);

    let searched = map_pr(json!({
        "number":2,
        "title":"searched",
        "state":"closed",
        "pull_request":{"merged_at":"2026-09-20T11:00:00Z"},
        "user":{"login":"dev"},
        "labels":[],
        "created_at":"2026-09-19T11:00:00Z",
        "comments":0
    }));
    assert_eq!(searched["state"], "merged");
    assert_eq!(searched["mergedAt"], "2026-09-20T11:00:00Z");

    let closed = map_pr(json!({
        "number":3,
        "title":"closed",
        "state":"closed",
        "merged_at":null,
        "user":{"login":"dev"},
        "labels":[],
        "created_at":"2026-09-19T12:00:00Z",
        "comments":0
    }));
    assert_eq!(closed["state"], "closed");
    assert!(closed.get("mergedAt").is_none());
}

/// The default PR read targets the merged fix, not the first
/// (unmerged) search row; the rows already name the others.
#[test]
fn read_pr_prefers_the_first_merged_row() {
    let rows = [
        json!({"number":13794,"state":"closed","pull_request":{"merged_at":null}}),
        json!({"number":13825,"state":"closed","pull_request":{"merged_at":"2026-09-25T00:29:36Z"}}),
        json!({"number":13787,"state":"open"}),
    ];
    let target = read_target(&rows, is_merged).expect("rows");
    assert_eq!(item_number(&rows[target]), Some(13825));
    // No merged row: the first row, unchanged.
    let open = [
        json!({"number":5,"state":"open"}),
        json!({"number":6,"state":"closed"}),
    ];
    assert_eq!(read_target(&open, is_merged), Some(0));
    assert_eq!(read_target(&[], is_merged), None);
    let read = pr_read_query("o", "r", 13825);
    assert_eq!(read["number"], 13825);
    assert_eq!(read["sections"], json!(["body", "files"]));
    assert!(read.get("pageSize").is_none(), "{read}");
}

/// The issue read prefers an issue closed as completed (its fix landed).
#[test]
fn read_issue_prefers_a_completed_issue() {
    let rows = [
        json!({"number":1,"state":"open"}),
        json!({"number":2,"state":"closed","state_reason":"not_planned"}),
        json!({"number":3,"state":"closed","state_reason":"completed"}),
    ];
    let target = read_target(&rows, is_completed).expect("rows");
    assert_eq!(item_number(&rows[target]), Some(3));
    let read = issue_read_query("o", "r", 3);
    assert_eq!(read["sections"], json!(["body", "comments"]), "{read}");
}

/// A bare issue number in the keywords links straight to that issue's
/// closing pull requests.
#[test]
fn bare_issue_number_keywords_offer_the_issue_links_read() {
    let q = parse(
        r##"{"operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","keywords":["#13786"]}"##,
    );
    let read = issue_links_read(&q).expect("bare number");
    assert_eq!(read["query"]["queries"][0]["operation"], "issue");
    assert_eq!(read["query"]["queries"][0]["number"], 13786);
    let words = parse(
        r#"{"operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","keywords":["fix 13786"]}"#,
    );
    assert!(issue_links_read(&words).is_none());
}

/// Row diet: commit authors are a login (else name) without email, list
/// rows drop the message body, issue rows drop updatedAt and empty
/// labels, PR rows drop empty labels and zero comment counts.
#[test]
fn history_rows_carry_no_emails_or_empty_fields() {
    let raw = json!({"sha":"abc","commit":{"message":"Fix (#1)\n\nlong body",
        "author":{"name":"Dev","email":"dev@example.com","date":"2026-01-01T00:00:00Z"},
        "committer":{"name":"GitHub","email":"noreply@github.com"}},
        "author":{"login":"dev"},"committer":{"login":"web-flow"}});
    for row in [map_commit(raw.clone()), map_commit_list(raw.clone())] {
        assert_eq!(row["author"], "dev", "{row}");
        assert!(!row.to_string().contains('@'), "{row}");
        assert!(row.get("messageBody").is_none(), "{row}");
        assert!(row.get("committer").is_none(), "{row}");
    }
    let nameless =
        json!({"sha":"abc","commit":{"message":"x","author":{"name":"Dev","email":"d@e.f"}}});
    assert_eq!(map_commit_list(nameless)["author"], "Dev");
    let other = json!({"sha":"abc","commit":{"message":"x","author":{"name":"A"},"committer":{"name":"B"}},
        "author":{"login":"a"},"committer":{"login":"b"}});
    assert_eq!(map_commit_list(other)["committer"], "b");
    let issue = map_issue(
        json!({"number":1,"title":"t","state":"open","user":{"login":"u"},
        "labels":[],"created_at":"c","updated_at":"u"}),
        false,
    );
    assert!(
        issue.get("updatedAt").is_none() && issue.get("labels").is_none(),
        "{issue}"
    );
    let by_update = map_issue(
        json!({"number":1,"updated_at":"u","labels":[{"name":"bug"}]}),
        true,
    );
    assert_eq!(by_update["updatedAt"], "u");
    assert_eq!(by_update["labels"], json!(["bug"]));
    let pr = map_pr(
        json!({"number":1,"title":"t","state":"open","user":{"login":"u"},
        "labels":[],"created_at":"c","comments":0}),
    );
    assert!(
        pr.get("labels").is_none() && pr.get("commentsCount").is_none(),
        "{pr}"
    );
}

/// The `qualifiers` string sets the search filters; scope and unknown
/// keys are rejected.
#[test]
fn qualifiers_set_the_search_filters() {
    let built = |row: Value| build_query(&serde_json::from_value(row).expect("typed"));
    let base = json!({"operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","keywords":["x"]});
    let mut flat = base.clone();
    flat["qualifiers"] = json!(
        r#"reviewed-by:dev review:approved comments:>5 label:"good first issue" -is:draft merged:>2026-01-01"#
    );
    let query = built(flat).expect("flat");
    for term in [
        "reviewed-by:dev",
        "review:approved",
        "comments:>5",
        "label:\"good first issue\"",
        "-is:draft",
        "merged:>2026-01-01",
    ] {
        assert!(query.contains(term), "{term}: {query}");
    }
    // Scope, unknown keys and free text fail the published pattern.
    for bad in ["repo:evil/x", "reviewd-by:dev", "loose words"] {
        let mut row = base.clone();
        row["qualifiers"] = json!(bad);
        assert!(
            serde_json::from_value::<GhSearchHistoryQuery>(row).is_err(),
            "{bad}"
        );
    }
    for (bad, needle) in [("is:pr", "operation"), ("author:a author:b", "set it once")] {
        let mut row = base.clone();
        row["qualifiers"] = json!(bad);
        let error = built(row).expect_err(bad);
        assert!(error.message.contains(needle), "{bad}: {}", error.message);
    }
    let issue: GhSearchHistoryQuery = serde_json::from_value(json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","qualifiers":"review:approved"})).expect("issue");
    assert!(Filters::parse(&issue).is_err());
}

/// The published schema admits exactly the negated qualifiers native
/// runs (a pull request's `-is:draft`); every other negation fails
/// contract preparation, before any provider work.
#[test]
fn schema_negation_matches_native_execution() {
    let row = |operation: &str, qualifiers: &str| {
        json!({"operation":operation,"mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "qualifiers":qualifiers})
    };
    let prepared =
        crate::contracts::validate_query("ghSearchHistory", row("pullRequest", "-is:draft"))
            .expect("-is:draft is published");
    let filters = Filters::parse(&serde_json::from_value(prepared).expect("typed"))
        .expect("native runs -is:draft");
    assert_eq!(filters.draft, Some(false));
    for (operation, qualifiers, message) in [
        (
            "pullRequest",
            "label:bug -label:wontfix",
            "\"-label:wontfix\": negation is not supported",
        ),
        ("pullRequest", "-is:open", "\"-is:open\": negation"),
        ("issue", "-label:bug", "\"-label:bug\": negation"),
        ("issue", "-is:draft", "\"-is:draft\": negation"),
        (
            "issue",
            r#"label:"good first issue" repo:x/y"#,
            "\"repo:x/y\" is not an allowed key:value filter",
        ),
    ] {
        let error = crate::contracts::validate_query("ghSearchHistory", row(operation, qualifiers))
            .expect_err(qualifiers);
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.message.contains(message)),
            "{operation} {qualifiers}: {error:?}"
        );
    }
}

#[test]
fn empty_history_rows_are_marked_empty() {
    for key in ["pullRequests", "issues", "commits"] {
        let mut value = json!({ key: [] });
        mark_empty(&mut value, false);
        assert_eq!(value["status"], "empty", "{key}");
    }
    let mut more = json!({"commits": []});
    mark_empty(&mut more, true);
    assert!(more.get("status").is_none());
    let mut rows = json!({"issues": [{"number": 1}]});
    mark_empty(&mut rows, false);
    assert!(rows.get("status").is_none());
}

#[tokio::test]
async fn cap_is_terminal_only_after_the_last_reachable_page() {
    use wiremock::MockServer;

    let server = MockServer::start().await;
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
    let transport = transport(&server);
    for page in [1, 1000] {
        let query = serde_json::from_value(json!({"operation":"pullRequest","mainGoal": "test", "reasoning":"test","keywords":["fix"],"pageSize":1,"page":page})).expect("query");
        let data = execute(&transport, &query, &context(), &Passthrough)
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
        // A search beyond one repository names each row's repository,
        // and the read targets it: a bare number is unreadable.
        assert_eq!(
            data["pullRequests"][0]["repository"], "acme/widget",
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

/// Commit rows are an index of headlines: the commit read targets the
/// first sha and takes any other row's sha; the rows already name them,
/// so the read carries no second list.
/// GitHub answers a search scoped to a missing or hidden repository with
/// a 422 "cannot be searched": that is `notFound`, not invalid input.
#[tokio::test]
async fn search_of_a_missing_repository_is_not_found() {
    use wiremock::MockServer;

    let server = MockServer::start().await;
    let unsearchable = json!({
        "message": "Validation Failed",
        "errors": [{"message": "The listed users and repositories cannot be searched either because the resources do not exist or you do not have permission to view them.",
            "resource": "Search", "field": "q", "code": "invalid"}]
    });
    for route in ["/api/v3/search/issues", "/api/v3/search/commits"] {
        mount_json(&server, route, 422, unsearchable.clone()).await;
    }
    let transport = transport(&server);
    for row in [
        json!({"operation":"pullRequest","owner":"o","repo":"missing","keywords":["fix"]}),
        json!({"operation":"commit","owner":"o","repo":"missing","keywords":["fix"]}),
    ] {
        let query = serde_json::from_value(row.clone()).expect("query");
        let error = execute(&transport, &query, &context(), &Passthrough)
            .await
            .expect_err("missing repository");
        assert_eq!(error.kind, ProviderErrorKind::NotFound, "{row}: {error:?}");
    }
}

#[tokio::test]
async fn commit_search_read_targets_the_first_sha_without_a_candidate_list() {
    use wiremock::MockServer;

    let server = MockServer::start().await;
    let commit = |sha: &str| {
        json!({"sha": sha, "commit": {"message": format!("Fix {sha}\n\nWhy: the body names the cache"),
            "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}})
    };
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({
            "total_count": 3, "incomplete_results": false,
            "items": [commit("aaa"), commit("bbb"), commit("ccc")]
        }),
    )
    .await;
    let transport = transport(&server);
    let query = serde_json::from_value(
        json!({"operation":"commit","mainGoal":"test","reasoning":"test",
        "owner":"o","repo":"r","keywords":["cache"]}),
    )
    .expect("query");
    let data = execute(&transport, &query, &context(), &Passthrough)
        .await
        .expect("history search");
    let read = &data["next"]["readCommit"];
    assert_eq!(read["query"]["queries"][0]["ref"], "aaa", "{data}");
    assert_eq!(
        read["query"]["queries"][0]["sections"],
        json!(["patches"]),
        "{data}"
    );
    assert!(read.get("candidates").is_none(), "{data}");
    assert_eq!(data["commits"].as_array().map(Vec::len), Some(3), "{data}");
    // Commit search pages use the shared `currentPage` key, so the
    // default projection drops page one like every other history page.
    assert_eq!(data["pagination"]["currentPage"], 1, "{data}");
    assert!(data["pagination"].get("page").is_none(), "{data}");
}

/// A commit listing names its row kind like commit search: `type` is
/// `commits`, whether `path` names a file, a directory, or nothing.
#[tokio::test]
async fn commit_listing_rows_are_typed_commits() {
    use wiremock::MockServer;

    let server = MockServer::start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits",
        200,
        json!([{
            "sha": "aaa", "commit": {"message": "Fix cache",
            "committer": {"date": "2026-01-01T00:00:00Z"}}, "author": {"login": "dev"}
        }]),
    )
    .await;
    let transport = transport(&server);
    for scope in [json!("src/lib.rs"), json!("src/"), Value::Null] {
        let mut query = json!({"operation":"commit","owner":"o","repo":"r"});
        if !scope.is_null() {
            query["path"] = scope.clone();
        }
        let query = serde_json::from_value(query).expect("query");
        let data = execute(&transport, &query, &context(), &Passthrough)
            .await
            .expect("commit listing");
        assert!(data.get("type").is_none(), "{scope}: {data}");
        assert_eq!(data["commits"][0]["sha"], "aaa", "{data}");
        // The listed path is the caller's own input: not restated.
        assert!(data.get("path").is_none(), "{scope}: {data}");
        // The commit read scopes its patches with the published `include`.
        let read = &data["next"]["readCommit"]["query"]["queries"][0];
        assert!(read.get("path").is_none(), "{read}");
        if scope.is_null() {
            assert!(read.get("include").is_none(), "{read}");
        } else {
            assert_eq!(read["include"], json!([scope]), "{read}");
        }
    }
}

/// D4: a plain issue listing pages `is:issue` search results, so every
/// page holds up to pageSize real issues (the REST /issues list
/// interleaves PRs) and page numbers advance by one. An empty or PR-only
/// repository is a clean empty row.
#[tokio::test]
async fn plain_issue_listing_pages_issue_search_without_pr_gaps() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    let issue = |n: u64| json!({"number": n, "title": format!("issue {n}"), "state": "open", "user": {"login": "dev"}});
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .and(query_param("q", "is:issue repo:o/full"))
        .and(query_param("page", "2"))
        .and(query_param("per_page", "5"))
        .and(query_param("sort", "created"))
        .and(query_param("order", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 11, "incomplete_results": false,
            "items": [issue(6), issue(7), issue(8), issue(9), issue(10)]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .and(query_param("q", "is:issue repo:o/prs-only"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 0, "incomplete_results": false, "items": []
        })))
        .mount(&server)
        .await;
    let transport = transport(&server);
    let run = |query: Value| {
        let transport = &transport;
        async move {
            execute(
                transport,
                &serde_json::from_value(query).expect("query"),
                &context(),
                &Passthrough,
            )
            .await
            .expect("issue listing")
        }
    };
    let data = run(json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"full","pageSize":5,"page":2})).await;
    assert_eq!(data["issues"].as_array().map(Vec::len), Some(5), "{data}");
    assert_eq!(data["pagination"]["currentPage"], 2, "{data}");
    assert_eq!(data["pagination"]["nextPage"], 3, "{data}");
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["page"], 3,
        "{data}"
    );
    assert_eq!(data["pagination"]["totalItems"], 11, "{data}");
    assert!(data.get("skippedPullRequestPages").is_none(), "{data}");

    let empty = run(json!({"operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"prs-only","pageSize":5})).await;
    assert_eq!(empty["issues"], json!([]), "{empty}");
    assert_eq!(empty["status"], "empty", "{empty}");
    assert!(empty.get("pagination").is_none(), "{empty}");
    assert!(empty["next"].get("nextPage").is_none(), "{empty}");
}

fn parse(json: &str) -> GhSearchHistoryQuery {
    serde_json::from_str(json).expect("GitHub history search test data should be valid")
}

#[test]
fn pull_request_search_allows_cross_repo_and_owner_scopes() {
    let both = build_query(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["x"]}"#,
    ))
    .expect("scoped");
    assert!(both.contains("repo:a/b"), "{both}");
    let owner = build_query(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","keywords":["x"]}"#,
    ))
    .expect("owner-scoped PR search");
    assert!(
        owner.contains("user:a") && !owner.contains("repo:"),
        "{owner}"
    );
    let global = build_query(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","keywords":["x"]}"#,
    ))
    .expect("cross-repo PR search");
    assert!(
        !global.contains("repo:") && !global.contains("user:"),
        "{global}"
    );
    assert!(!global.contains("archived:"), "{global}");
    // Without a full repo scope the REST list endpoint is unusable, so the
    // PR path must route through search.
    assert!(should_use_search_for_prs(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a"}"#
    )));
    assert!(!should_use_search_for_prs(&parse(
        r#"{"operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b"}"#
    )));
    // Issues still require the repository scope.
    assert!(
        serde_json::from_str::<GhSearchHistoryQuery>(
            r#"{"operation":"issue","mainGoal":"test","reasoning":"test","keywords":["x"]}"#
        )
        .is_err()
    );
}

/// E21: an unparseable `since`/`until` is invalid input, never a silently
/// unfiltered listing (search or REST list).
#[test]
fn commit_window_with_an_invalid_date_is_rejected() {
    for row in [
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"since":"yesterday-ish"}"#,
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","since":"zz"}"#,
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","until":"2026-13-01"}"#,
    ] {
        let q = parse(row);
        let error = resolve_commit_window(&q, &mut Vec::new()).expect_err(row);
        assert_eq!(error.kind, ProviderErrorKind::Validation, "{row}");
        assert!(
            error.message.contains("since") || error.message.contains("until"),
            "{}",
            error.message
        );
        assert!(error.message.contains("30d"), "{}", error.message);
        assert!(!error.message.contains("skipped"), "{}", error.message);
    }
}

#[test]
fn inverted_since_until_is_a_validation_error() {
    let q = parse(
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","keywords":["fix"],"since":"2026-05-01","until":"2026-01-01"}"#,
    );
    let error = build_query(&q).expect_err("since after until");
    assert_eq!(error.kind, ProviderErrorKind::Validation);
    assert!(error.message.contains("since"), "{}", error.message);
    let listed = parse(
        r#"{"operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","since":"2026-05-01","until":"2026-01-01"}"#,
    );
    assert!(build_query(&listed).is_err());
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
    let found = MockServer::start().await;
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
    let data = execute(&transport(&found), &query, &context(), &Passthrough)
        .await
        .expect("search");
    assert_eq!(data["pullRequests"][0]["number"], 7, "{data}");

    let renamed = MockServer::start().await;
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
    let data = execute(&transport(&renamed), &query, &context(), &Passthrough)
        .await
        .expect("renamed search");
    assert_eq!(data["pullRequests"][0]["number"], 7, "{data}");
    assert!(
        data["warnings"][0]
            .as_str()
            .is_some_and(|warning| warning.contains("renamed to new/r")),
        "{data}"
    );
    let read = &data["next"]["readPullRequest"]["query"]["queries"][0];
    assert_eq!(
        (&read["owner"], &read["repo"]),
        (&json!("new"), &json!("r")),
        "{data}"
    );
}

/// Every row date is UTC `Z`: commit search reports the committer's own
/// offset, the REST listings already use `Z`.
#[test]
fn row_dates_are_utc() {
    let commit = json!({"sha":"abc","commit":{"message":"Fix",
        "author":{"name":"Dev","date":"2024-03-01T01:30:00.000+02:00"}}});
    assert_eq!(map_commit(commit.clone())["date"], "2024-02-29T23:30:00Z");
    assert_eq!(map_commit_list(commit)["date"], "2024-02-29T23:30:00Z");
    let pr = map_pr(json!({"number":1,"state":"closed","user":{"login":"u"},
        "created_at":"2024-01-01T10:00:00-01:00","merged_at":"2024-01-02T00:00:00+00:30"}));
    assert_eq!(pr["createdAt"], "2024-01-01T11:00:00Z", "{pr}");
    assert_eq!(pr["mergedAt"], "2024-01-01T23:30:00Z", "{pr}");
    let issue = map_issue(
        json!({"number":1,"created_at":"2024-01-01T10:00:00Z","updated_at":"2024-01-01T10:00:00+01:00"}),
        true,
    );
    assert_eq!(issue["createdAt"], "2024-01-01T10:00:00Z", "{issue}");
    assert_eq!(issue["updatedAt"], "2024-01-01T09:00:00Z", "{issue}");
}

/// An empty search offers one runnable recovery: the same search without
/// its keywords, else without its qualifiers, else a commit listing
/// without its date window.
#[test]
fn empty_searches_offer_a_broader_search() {
    let broadened = |row: Value| {
        let query: GhSearchHistoryQuery = serde_json::from_value(row).expect("query");
        super::leads::broaden_search(&query).map(|lead| lead["query"]["queries"][0].clone())
    };
    let keywords = broadened(json!({"operation":"pullRequest","owner":"o","repo":"r",
        "keywords":["fix"],"qualifiers":"author:x","page":2}))
    .expect("keywords dropped");
    assert!(keywords.get("keywords").is_none(), "{keywords}");
    assert!(keywords.get("page").is_none(), "{keywords}");
    assert_eq!(keywords["qualifiers"], "author:x", "{keywords}");
    let qualifiers = broadened(json!({"operation":"issue","owner":"o","repo":"r",
        "qualifiers":"label:bug"}))
    .expect("qualifiers dropped");
    assert!(qualifiers.get("qualifiers").is_none(), "{qualifiers}");
    let window = broadened(json!({"operation":"commit","owner":"o","repo":"r",
        "path":"src","since":"30d"}))
    .expect("window dropped");
    assert!(window.get("since").is_none(), "{window}");
    assert_eq!(window["path"], "src", "{window}");
    assert!(broadened(json!({"operation":"commit","owner":"o","repo":"r"})).is_none());
}

/// Branch and label filters are qualifiers: `head:`/`base:` route a pull
/// request search through search, labels add up, and issues reject the
/// pull-request-only branch keys.
#[test]
fn branch_and_label_filters_come_from_qualifiers() {
    let pr = parse(
        r#"{"operation":"pullRequest","owner":"o","repo":"r","qualifiers":"head:feature base:main label:bug label:ui"}"#,
    );
    assert!(should_use_search_for_prs(&pr));
    let built = build_query(&pr).expect("query");
    for term in ["head:feature", "base:main", "label:\"bug\"", "label:\"ui\""] {
        assert!(built.contains(term), "{term}: {built}");
    }
    let issue = parse(r#"{"operation":"issue","owner":"o","repo":"r","qualifiers":"head:x"}"#);
    assert!(Filters::parse(&issue).is_err());
    for hidden in ["label", "sourceBranch", "targetBranch"] {
        let mut row = json!({"operation":"pullRequest","owner":"o","repo":"r"});
        row[hidden] = json!(if hidden == "label" {
            json!(["bug"])
        } else {
            json!("x")
        });
        assert!(
            serde_json::from_value::<GhSearchHistoryQuery>(row).is_err(),
            "{hidden} is a qualifier now"
        );
    }
}
