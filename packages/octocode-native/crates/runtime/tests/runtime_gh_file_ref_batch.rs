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

/// `count` numbered lines of the file `name`.
fn lines(name: &str, count: usize) -> String {
    (1..=count)
        .map(|line| format!("{name} line {line:04} of the source\n"))
        .collect()
}

/// Refs of `a/b` resolve to [`SHA`]; each of `files` reads as its raw text.
async fn files_server(files: &[(&str, String)]) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits/[^/]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for (file, body) in files {
        Mock::given(method("GET"))
            .and(wiremock::matchers::path(format!(
                "/api/v3/repos/a/b/contents/{file}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                body.clone().into_bytes(),
                "application/vnd.github.raw; charset=utf-8",
            ))
            .mount(&server)
            .await;
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", file).await;
    }
    server
}

/// Requests `server` received whose path holds `part`.
async fn sent(server: &MockServer, part: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().contains(part))
        .count()
}

/// The source text of a row's content, its line gutter stripped.
fn text(data: &Value) -> String {
    data["content"]
        .as_str()
        .unwrap_or_default()
        .split_inclusive('\n')
        .map(|line| line.split_once('\t').map_or(line, |(_, text)| text))
        .collect()
}

/// GR3: a GitHub file read fills its batch's page like water. A row that
/// finishes under its equal share leaves the rest to the row still
/// reading, which is read again with the wider share and finishes in the
/// same page; the re-read sends GitHub nothing (its body is the answer
/// pinned to the resolved commit, its ref the batch's resolution).
#[tokio::test]
async fn a_wider_reread_fills_the_page_and_sends_nothing() {
    let small = lines("small", 2);
    let big = lines("big", 90);
    let server = files_server(&[("small.txt", small.clone()), ("big.txt", big.clone())]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let input = json!({"responseChars": 5000, "queries": [
        {"owner": "a", "repo": "b", "path": "small.txt"},
        {"owner": "a", "repo": "b", "path": "big.txt"},
    ]});
    let out = call(&runtime, "ghGetFileContent", input)
        .await
        .expect("batch")
        .structured_content
        .into_value();
    runtime.close().await;
    let half = (5000 - 2 * "{\"index\":0,\"data\":{}}".len()) / 2;
    assert!(
        serde_json::to_string(&out["results"][1]["data"])
            .expect("row")
            .encode_utf16()
            .count()
            > half,
        "the big row outgrows an equal share: {out}"
    );
    assert_eq!(text(&out["results"][0]["data"]), small, "{out}");
    assert_eq!(text(&out["results"][1]["data"]), big, "{out}");
    assert!(
        out["results"][1]["data"]
            .pointer("/next/nextPage")
            .is_none(),
        "{out}"
    );
    assert!(out.pointer("/next").is_none(), "{out}");
    // Each file's body was asked for once: the wider re-read sent nothing.
    assert_eq!(sent(&server, "/contents/small.txt").await, 1);
    assert_eq!(sent(&server, "/contents/big.txt").await, 1);
    assert_eq!(sent(&server, "/commits/").await, 1);
}

/// GR3: GitHub file rows that both read on continue together, in one
/// call: no row keeps a walk of its own, and walking that one call reads
/// each file whole, once, with no body asked twice.
#[tokio::test]
async fn rows_still_reading_continue_together_in_one_call() {
    let sources = [
        ("one.txt", lines("one", 400)),
        ("two.txt", lines("two", 400)),
    ];
    let server = files_server(&sources).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let queries = sources
        .iter()
        .map(|(file, _)| json!({"owner": "a", "repo": "b", "path": file}))
        .collect::<Vec<_>>();
    let mut input = json!({"responseChars": 6000, "queries": queries});
    let mut read = vec![String::new(); sources.len()];
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 100, "the walk ends");
        let page = call(&runtime, "ghGetFileContent", input)
            .await
            .expect("page")
            .structured_content
            .into_value();
        let rows = page["results"].as_array().expect("rows");
        for row in rows {
            assert!(
                row["data"].pointer("/next/nextPage").is_none(),
                "one call continues both: {page}"
            );
            let index = row["index"].as_u64().expect("index") as usize;
            read[index].push_str(&text(&row["data"]));
        }
        match page.pointer("/next/input") {
            Some(next) => input = next.clone(),
            None => break,
        }
    }
    runtime.close().await;
    assert!(calls > 1, "both files outgrow one page");
    for ((file, source), read) in sources.iter().zip(&read) {
        assert_eq!(read, source, "{file} read whole, once");
        assert_eq!(
            sent(&server, &format!("/contents/{file}")).await,
            1,
            "{file}"
        );
    }
}

/// GR7: a repository named as its clone URL names it (`b.git`) that GitHub
/// does not resolve: the failure row's first related[] request is the same
/// read naming `b`, valid against the ghGetFileContent output contract
/// (`call` checks it).
#[tokio::test]
async fn a_clone_suffix_repository_leads_to_the_bare_name_read() {
    let server = MockServer::builder().start().await;
    // The owner's login exists: only the repository is missing.
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/v3/users/a"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login": "a"})))
        .mount(&server)
        .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&github_api(&server));
    let out = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner": "a", "repo": "b.git", "path": "README.md", "ref": SHA}),
    )
    .await
    .expect("failure row")
    .structured_content
    .into_value();
    runtime.close().await;
    let data = &out["results"][0]["data"];
    assert!(data["errorCode"].is_string(), "{out}");
    assert_ne!(data["errorCode"], "outputContractViolation", "{out}");
    let first = &data["related"][0];
    assert_eq!(first["tool"], "ghGetFileContent", "{out}");
    assert_eq!(first["input"]["queries"][0]["repo"], "b", "{out}");
    assert_eq!(first["input"]["queries"][0]["path"], "README.md", "{out}");
}
