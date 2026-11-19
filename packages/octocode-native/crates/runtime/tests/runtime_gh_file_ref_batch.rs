//! A ghGetFileContent batch resolves each repository ref once, and reads
//! only the files: when a file last changed is its history, one documented
//! call away (ghSearchHistory), never a request or a lead of the read.

use crate::support::{Workspace, call, github_api};
use serde_json::{Value, json};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Refs of `owner/repo` resolve to [`SHA`] and its files read as raw text.
async fn server(owner: &str, repo: &str) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/api/v3/repos/{owner}/{repo}/commits/[^/]+$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(format!(
            "^/api/v3/repos/{owner}/{repo}/contents/.+$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"hello\n".as_slice(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    server
}

async fn read_batch(server: &MockServer, owner: &str, repo: &str, files: &[&str]) -> Value {
    // GitHub's tree of the commit lists each file: the symlink check a read asks first.
    for file in files {
        octocode_test_support::mock_http::list_in_trees(server, owner, repo, file).await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(server));
    let queries = files
        .iter()
        .map(|file| json!({"owner":owner,"repo":repo,"path":file}))
        .collect::<Vec<_>>();
    let outcome = call(&runtime, "ghGetFileContent", json!({"queries":queries}))
        .await
        .expect("batch");
    runtime.close().await;
    outcome.structured_content.into_value()
}

/// Three rows at one ref: one resolution, one GraphQL read of the three files
/// together (TE6; this server answers no GraphQL, so each file is then read by
/// REST as before), then the three contents reads and nothing else. No row
/// states its file's last change or leads to it.
#[tokio::test]
async fn three_rows_resolve_their_ref_once_and_ask_only_for_their_files() {
    let server = server("a", "b").await;
    let files = ["a.txt", "b.txt", "c.txt"];
    let out = read_batch(&server, "a", "b", &files).await;
    let requests = server.received_requests().await.unwrap();
    let paths = requests
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.starts_with("/api/v3/repos/a/b/commits/"))
            .count(),
        1,
        "{paths:?}"
    );
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.contains("/contents/"))
            .count(),
        3,
        "{paths:?}"
    );
    // The batch's one read of its files together.
    assert_eq!(
        paths.iter().filter(|path| *path == "/api/graphql").count(),
        1,
        "{paths:?}"
    );
    // The tree read that says a path is no link is no last-commit request.
    let asked = paths
        .iter()
        .filter(|path| !path.contains("/git/trees/") && *path != "/api/graphql")
        .count();
    assert_eq!(asked, 4, "no last-commit request: {paths:?}");
    for (index, file) in files.iter().enumerate() {
        let data = &out["results"][index]["data"];
        assert!(data.get("modifiedAt").is_none(), "{out}");
        assert!(data.get("lastModifiedBy").is_none(), "{out}");
        assert!(
            data["related"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|lead| lead["tool"] != "ghSearchHistory"),
            "{file}: {out}"
        );
    }
}

/// The resolution keeps the spelling of the row's repository, and a
/// withheld path asks GitHub nothing.
#[tokio::test]
async fn the_resolution_keeps_the_rows_spelling_and_withheld_paths_send_nothing() {
    let server = server("Ab", "Cd").await;
    let out = read_batch(&server, "Ab", "Cd", &["a.txt", ".env", "b.txt"]).await;
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| !request.url.path().contains(".env")),
        "{requests:?}"
    );
    // Nor does the batch's GraphQL read of its files name it.
    assert!(
        requests
            .iter()
            .all(|request| !String::from_utf8_lossy(&request.body).contains(".env")),
        "{requests:?}"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request
                .url
                .path()
                .starts_with("/api/v3/repos/Ab/Cd/commits/"))
            .count(),
        1,
        "{out}"
    );
    assert_eq!(out["results"][0]["data"]["content"], "1\thello\n", "{out}");
}
