use crate::support::{Workspace, call, checks, row_data, row_status};
use octocode_test_support::json_fixture;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `GET /users/{login}` answers: the login exists, so a repository under it
/// that does not resolve is a missing repository, not a missing owner.
async fn owner_exists(server: &MockServer, login: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/users/{login}")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "login": login })),
        )
        .mount(server)
        .await;
}

/// ghSearchCode on a repository that does not exist is notFound
/// (exit 3), not an empty partial search.
#[tokio::test]
async fn gh_search_code_on_a_missing_repository_is_not_found() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": true, "items": []}"#,
        )))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/psf/zz-nope-repo"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json_fixture(
            r#"{
                "owner": "psf",
                "repo": "zz-nope-repo",
                "keywords": ["needle"]
            }"#,
        ),
    )
    .await
    .expect("search");
    let row = &outcome.structured_content["results"][0];
    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    assert_eq!(row["data"]["errorCode"], "notFound", "{row}");
    runtime.close().await;
}

/// ghCloneRepo pins its commit through the API before git runs: an
/// unbranched row reads the repository's default branch beside its `HEAD`
/// commit, so a missing repository is classified by those two concurrent
/// reads, with no git, and leads to a search for it.
#[tokio::test]
async fn gh_clone_repo_classifies_a_missing_repository_before_git() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghost/nope"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    owner_exists(&server, "ghost").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghCloneRepo",
        json_fixture(r#"{"owner": "ghost", "repo": "nope"}"#),
    )
    .await
    .expect("clone error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    assert_eq!(data["errorCode"], "notFound", "{data}");
    checks::assert_absent(data, "defaultBranchUnavailable");
    // The missing repository leads to a search for it (a failed row keeps
    // its recovery lead).
    assert_eq!(
        crate::support::checks::related_call(data, "ghSearchRepo", |_| true)["tool"],
        "ghSearchRepo",
        "{data}"
    );
    let mut api_calls: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| request.url.path().to_owned())
        .filter(|path| path.starts_with("/api/"))
        .collect();
    api_calls.sort();
    // The two pinning reads answered; only the read of the owner's login
    // (a missing owner answers like a missing repository) followed them.
    assert_eq!(
        api_calls,
        [
            "/api/v3/repos/ghost/nope",
            "/api/v3/repos/ghost/nope/commits/HEAD",
            "/api/v3/users/ghost"
        ],
        "{api_calls:?}"
    );
    runtime.close().await;
}

/// A repository-level 404 (the ref resolution itself 404s) says the
/// repository is missing, private, or inaccessible to the token and leads to
/// a search for its name; it does not blame path case, offer a viewTree of
/// the same missing repository, or echo GitHub's unrelated commits
/// documentation link.
#[tokio::test]
async fn gh_file_read_on_a_missing_repository_reports_repository_access() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghost/nope/commits/HEAD"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json_fixture(
            r#"{
                "message": "Not Found",
                "documentation_url": "https://docs.github.com/rest/commits/commits#get-a-commit"
            }"#,
        )))
        .mount(&server)
        .await;
    owner_exists(&server, "ghost").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json_fixture(r#"{"owner": "ghost", "repo": "nope", "path": "README.md"}"#),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert_eq!(
        data["error"],
        "Repository ghost/nope not found, or private and not accessible to this token",
        "{data}"
    );
    assert!(
        !data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "ghStructure"
                && (call["input"]["queries"][0]["operation"] != "refs")),
        "{data}"
    );
    // As on every GitHub tool (`gh_shared::failure::repository_recovery`):
    // a search for the repository's name, which names the owner that holds
    // it, and the owner's listing; the hint adds only the caution.
    let find = &crate::support::checks::related_call(data, "ghSearchRepo", |_| true);
    assert_eq!(find["tool"], "ghSearchRepo", "{data}");
    assert_eq!(
        find["input"]["queries"][0]["keywords"],
        json_fixture(r#"["nope"]"#),
        "{data}"
    );
    assert_eq!(
        data["hints"],
        json_fixture(r#"["Confirm the owner before using a related[] repository."]"#),
        "{data}"
    );
    // Each lead asks for a page smaller than the tool's default: a lead
    // never costs more than the failed call; its listing pages on.
    let mut related = data["related"].clone();
    for call in related.as_array_mut().into_iter().flatten() {
        let query = &mut call["input"]["queries"][0];
        let limit = query["limit"]
            .as_u64()
            .expect("a recovery lead states its page");
        assert!(limit > 0, "{data}");
        query.as_object_mut().map(|row| row.remove("limit"));
    }
    assert_eq!(
        related,
        json_fixture(
            r#"[
                {
                    "tool": "ghSearchRepo",
                    "input": {"queries": [{"operation": "search", "keywords": ["nope"]}]}
                },
                {
                    "tool": "ghSearchRepo",
                    "input": {"queries": [{"operation": "list", "owner": "ghost"}]}
                }
            ]"#
        ),
        "{data}"
    );
    assert!(data.get("next").is_none(), "{data}");
    assert!(data.get("retryable").is_none(), "{data}");
    checks::assert_absent(data, "exact case");
    checks::assert_absent(data, "commits#get-a-commit");
    runtime.close().await;
}

/// An owner-only ghSearchRepo listing of a login GitHub does not know
/// (bare 404) names the owner instead of "Repository, resource, or path".
#[tokio::test]
async fn gh_search_repo_missing_owner_is_named() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json_fixture(r#"{"operation": "list", "owner": "ghost-login"}"#),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["error"]
            .as_str()
            .unwrap_or_default()
            .contains("\"ghost-login\""),
        "{data}"
    );
    runtime.close().await;
}

/// A missing repository's row reads the same on every tool that
/// takes a repository: `notFound`, which names the 404 (the clone row
/// too), the one message naming the repository, and the one hint the
/// related[] calls do not state (confirm the owner).
#[tokio::test]
async fn a_missing_repository_row_is_the_same_on_every_repository_tool() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("^/api/v3/repos/ghost/nope"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": true, "items": []}"#,
        )))
        .mount(&server)
        .await;
    owner_exists(&server, "ghost").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, input) in [
        (
            "ghCloneRepo",
            json_fixture(r#"{"owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghGetFileContent",
            json_fixture(r#"{"owner": "ghost", "repo": "nope", "path": "README.md"}"#),
        ),
        (
            "ghStructure",
            json_fixture(r#"{"operation": "tree", "owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghSearchCode",
            json_fixture(r#"{"owner": "ghost", "repo": "nope", "keywords": ["needle"]}"#),
        ),
        (
            "ghSearchHistory",
            json_fixture(r#"{"operation": "commit", "owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghGetHistoryItem",
            json_fixture(
                r#"{"operation": "pullRequest", "owner": "ghost", "repo": "nope", "number": 1}"#,
            ),
        ),
    ] {
        let outcome = call(&runtime, tool, input).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data.get("httpStatus").is_none(),
            "notFound already names the 404: {tool}: {data}"
        );
        assert_eq!(
            data["error"],
            "Repository ghost/nope not found, or private and not accessible to this token",
            "{tool}: {data}"
        );
        assert_eq!(
            data["hints"],
            json_fixture(r#"["Confirm the owner before using a related[] repository."]"#),
            "{tool}: {data}"
        );
    }
    runtime.close().await;
}

/// G9: a repository under a login GitHub does not know reads the same on
/// every tool that takes a repository: the row names the owner (GitHub
/// answers it exactly like a missing repository), and no related[] call
/// asks for that owner, since each would fail the same way.
#[tokio::test]
async fn a_missing_owner_row_is_the_same_on_every_repository_tool() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": true, "items": []}"#,
        )))
        .mount(&server)
        .await;
    // Every other request, the login's read too, answers 404.
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, input) in [
        (
            "ghCloneRepo",
            json_fixture(r#"{"owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghGetFileContent",
            json_fixture(r#"{"owner": "ghost", "repo": "nope", "path": "README.md"}"#),
        ),
        (
            "ghStructure",
            json_fixture(r#"{"operation": "tree", "owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghStructure",
            json_fixture(r#"{"operation": "refs", "owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghSearchCode",
            json_fixture(r#"{"owner": "ghost", "repo": "nope", "keywords": ["needle"]}"#),
        ),
        (
            "ghSearchHistory",
            json_fixture(r#"{"operation": "commit", "owner": "ghost", "repo": "nope"}"#),
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{"operation": "commit", "owner": "ghost", "repo": "nope", "ref": "main"}"#,
            ),
        ),
        (
            "ghGetHistoryItem",
            json_fixture(
                r#"{"operation": "pullRequest", "owner": "ghost", "repo": "nope", "number": 1}"#,
            ),
        ),
        (
            "ghSearchRepo",
            json_fixture(r#"{"operation": "get", "owner": "ghost", "repo": "nope"}"#),
        ),
    ] {
        let outcome = call(&runtime, tool, input.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool} {input}: {data}");
        assert_eq!(
            data["error"],
            octocode_tools_shared::gh_shared::owner_not_found_message("ghost"),
            "{tool} {input}: {data}"
        );
        for lead in data["related"].as_array().into_iter().flatten() {
            for row in lead["input"]["queries"].as_array().into_iter().flatten() {
                assert!(
                    row.get("owner").is_none(),
                    "a lead asks for the missing owner: {tool} {input}: {data}"
                );
            }
        }
        assert!(
            data.get("related").is_some() || data.get("hints").is_some(),
            "the row leads somewhere: {tool} {input}: {data}"
        );
    }
    runtime.close().await;
}
