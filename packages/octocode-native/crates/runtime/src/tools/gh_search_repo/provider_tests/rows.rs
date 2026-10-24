//! Exact repository lookups, renamed repositories, and the facts a row carries.
use super::*;

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
    let out = run(&server, json!({"owner":"o","repo":"old","operation":"get"})).await;
    assert_eq!(out.data["repositories"][0]["repo"], "new", "{}", out.data);
    // Leads name the repository as it is now.
    assert_eq!(
        crate::tools::test_support::checks::related_call(
            &out.data,
            "ghStructure",
            |input| input["queries"][0]["operation"] == "tree"
        )["input"]["queries"][0],
        json!({"owner":"o","repo":"new","operation":"tree"})
    );
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
    let out = run(&same, json!({"owner":"o","repo":"new","operation":"get"})).await;
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
    let out = run(
        &server,
        json!({"owner":"o","repo":"old","keywords":["x"],"operation":"search"}),
    )
    .await;
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
    let out = run(&server, json!({"keywords":["x"],"operation":"search"})).await;
    let row = &out.data["repositories"][0];
    assert_eq!(row["createdAt"], "2019-05-06", "{row}");
    assert_eq!(row["pushedAt"], "2026-01-02", "{row}");
    for absent in ["homepage", "openIssuesCount"] {
        assert!(row.get(absent).is_none(), "{absent}: {row}");
    }
    for field in ["createdAt", "pushedAt", "forks", "updatedAt"] {
        assert!(
            row.get(field).is_some(),
            "provider evidence is retained: {field}"
        );
    }
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
    let out = run(
        &server,
        json!({"owner":"sindresorhus","repo":"is","operation":"get"}),
    )
    .await;
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
    // No page; the repository's natural next reads are leads (shown with
    // extras hints): its root tree and its refs, on the default branch.
    let tree =
        crate::tools::test_support::checks::related_call(&out.data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "tree"
        });
    assert_eq!(
        tree["input"]["queries"][0],
        json!({"owner":"sindresorhus","repo":"is","operation":"tree"})
    );
    let refs =
        crate::tools::test_support::checks::related_call(&out.data, "ghStructure", |input| {
            input["queries"][0]["operation"] == "refs"
        });
    assert_eq!(
        refs["input"]["queries"][0],
        json!({"owner":"sindresorhus","repo":"is","operation":"refs"})
    );
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
    let out = run(&server, json!({"keywords":["x"],"operation":"search"})).await;
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
