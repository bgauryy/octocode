use crate::support::{Workspace, call, checks, row_data, row_status};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// ghSearchCode on a repository that does not exist is notFound
/// (exit 3), not an empty partial search.
#[tokio::test]
async fn gh_search_code_on_a_missing_repository_is_not_found() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":true,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/psf/zz-nope-repo"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"psf","repo":"zz-nope-repo","keywords":["needle"]}),
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
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghCloneRepo",
        json!({"owner":"ghost","repo":"nope"}),
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
    // The two pinning reads answered; nothing followed them.
    assert_eq!(
        api_calls,
        [
            "/api/v3/repos/ghost/nope",
            "/api/v3/repos/ghost/nope/commits/HEAD"
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
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "message":"Not Found",
            "documentation_url":"https://docs.github.com/rest/commits/commits#get-a-commit"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"ghost","repo":"nope","path":"README.md"}),
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
    // As on ghSearchCode and ghStructure, the one repair is a search for
    // the repository's name, which names the owner that holds it.
    let find = &crate::support::checks::related_call(data, "ghSearchRepo", |_| true);
    assert_eq!(find["tool"], "ghSearchRepo", "{data}");
    assert_eq!(
        find["input"]["queries"][0]["keywords"],
        json!(["nope"]),
        "{data}"
    );
    assert_eq!(
        data["hints"],
        json!([
            "Verify the exact identifier, repository and revision, and access rights; a missing resource is not proof its contents do not exist elsewhere."
        ]),
        "{data}"
    );
    assert_eq!(
        data["related"],
        json!([
            {"kind":"related","tool":"ghSearchRepo","input":{"queries":[{"operation":"search","keywords":["nope"]}]}},
            {"kind":"related","tool":"ghSearchRepo","input":{"queries":[{"operation":"list","owner":"ghost"}]}}
        ]),
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
        json!({"operation":"list","owner":"ghost-login"}),
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
