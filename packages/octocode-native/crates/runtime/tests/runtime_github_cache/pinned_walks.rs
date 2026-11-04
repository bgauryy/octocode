//! Page walks of one pinned file: one fetch however many pages, redaction
//! kept out of retained state, and a walk that outlives its process.

use crate::support::{self, Workspace, call, row_data, row_status};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A server answering `file` as GitHub's raw media type does.
async fn raw_file_server(file: &str, body: &str) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/contents/{file}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(body.as_bytes(), "application/vnd.github.raw; charset=utf-8"),
        )
        .mount(&server)
        .await;
    server
}

async fn content_reads(server: &MockServer, file: &str) -> usize {
    let route = format!("/api/v3/repos/a/b/contents/{file}");
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

/// The source lines a read page shows, in the order `N\t<line>` states them.
fn numbered_lines(content: &str) -> Vec<(usize, String)> {
    content
        .lines()
        .map(|line| {
            let (number, text) = line.split_once('\t').expect("numbered line");
            (number.parse().expect("line number"), text.to_owned())
        })
        .collect()
}

/// Whether any file under `root` holds `needle`.
fn any_file_holds(root: &std::path::Path, needle: &[u8]) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            return any_file_holds(&path, needle);
        }
        std::fs::read(&path)
            .is_ok_and(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
    })
}

const WALKED_LINES: usize = 4_000;

fn walked_body(line: impl Fn(usize) -> String) -> String {
    (1..=WALKED_LINES).map(|n| line(n) + "\n").collect()
}

/// The pages a read walks by its cursors, from `first`, and each page's input.
async fn walk_pages(
    runtime: &octocode_native::runtime::ToolRuntime,
    first: serde_json::Value,
) -> (Vec<(usize, String)>, Vec<serde_json::Value>) {
    let mut lines = Vec::new();
    let mut inputs = Vec::new();
    let mut pending = Some(first);
    while let Some(input) = pending.take() {
        inputs.push(input.clone());
        let outcome = call(runtime, "ghGetFileContent", input)
            .await
            .expect("page");
        assert_eq!(
            row_status(&outcome),
            "success",
            "{}",
            outcome.structured_content
        );
        let data = row_data(&outcome);
        lines.extend(numbered_lines(data["content"].as_str().expect("content")));
        pending = data.pointer("/next/nextPage/input").cloned();
        assert!(inputs.len() < WALKED_LINES, "nonterminating walk");
    }
    (lines, inputs)
}

/// A walk of a clean pinned file fetches it once however many pages it takes,
/// and walking every page reconstructs the file exactly once.
#[tokio::test]
async fn a_walk_of_a_pinned_file_fetches_it_once() {
    let body = walked_body(|n| format!("line {n} of a long enough source file to fill a page"));
    let server = raw_file_server("big.rs", &body).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let query = json!({"owner":"a","repo":"b","path":"big.rs","ref":"a".repeat(40)});
    let (lines, inputs) = walk_pages(&runtime, query.clone()).await;
    assert!(inputs.len() > 1, "the file takes several pages");
    assert_eq!(content_reads(&server, "big.rs").await, 1);
    let expected = body
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.to_owned()))
        .collect::<Vec<_>>();
    assert_eq!(lines, expected);
    // Only a continuation owns what it observed: a fresh read asks GitHub.
    call(&runtime, "ghGetFileContent", query)
        .await
        .expect("fresh");
    assert_eq!(content_reads(&server, "big.rs").await, 2);
    runtime.close().await;
}

/// One redacted line must not cost the rest of the walk its fetch: the answer
/// the output gate would rewrite stays out of the store, yet the long-lived
/// process still serves every page from the one fetch. Each page shows what a
/// fresh read shows, and no retained file holds the credential.
#[tokio::test]
async fn a_walk_of_a_file_with_a_redacted_line_fetches_it_once() {
    let secret = format!("ghp_{}", "a1B2c3D4e5F6".repeat(3));
    let body = walked_body(|n| {
        if n == WALKED_LINES / 2 {
            format!("token = \"{secret}\"")
        } else {
            format!("line {n} of a long enough source file to fill a page")
        }
    });
    let server = raw_file_server("big.rs", &body).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let query = json!({"owner":"a","repo":"b","path":"big.rs","ref":"a".repeat(40)});
    let (lines, inputs) = walk_pages(&runtime, query).await;
    assert!(inputs.len() > 1, "the file takes several pages");
    assert_eq!(content_reads(&server, "big.rs").await, 1);
    assert_eq!(
        lines.iter().map(|(number, _)| *number).collect::<Vec<_>>(),
        (1..=WALKED_LINES).collect::<Vec<_>>(),
        "every line exactly once"
    );
    for (number, line) in &lines {
        assert!(!line.contains(&secret), "line {number} leaks: {line}");
        if *number != WALKED_LINES / 2 {
            assert_eq!(
                line,
                &format!("line {number} of a long enough source file to fill a page")
            );
        }
    }
    assert!(
        !any_file_holds(&workspace.home, secret.as_bytes()),
        "no retained state holds the credential"
    );
    runtime.close().await;
}

/// An answer held only in process memory is gone after a restart. The walk's
/// cursors still open: the next page fetches the pinned file again and the
/// walk completes exactly once, never truncated.
#[tokio::test]
async fn a_walk_past_a_restart_fetches_the_held_file_again_and_completes() {
    let secret = format!("ghp_{}", "a1B2c3D4e5F6".repeat(3));
    let body = walked_body(|n| {
        if n == 1 {
            format!("token = \"{secret}\"")
        } else {
            format!("line {n} of a long enough source file to fill a page")
        }
    });
    let server = raw_file_server("big.rs", &body).await;
    let workspace = Workspace::new();
    let settings = support::github_api(&server);
    let first = workspace.runtime(&settings);
    let query = json!({"owner":"a","repo":"b","path":"big.rs","ref":"a".repeat(40)});
    let outcome = call(&first, "ghGetFileContent", query)
        .await
        .expect("page 1");
    let data = row_data(&outcome);
    let mut lines = numbered_lines(data["content"].as_str().expect("content"));
    let next = data.pointer("/next/nextPage/input").cloned().expect("next");
    first.close().await;
    assert_eq!(content_reads(&server, "big.rs").await, 1);

    let second = workspace.runtime(&settings);
    let (rest, _) = walk_pages(&second, next).await;
    lines.extend(rest);
    assert_eq!(content_reads(&server, "big.rs").await, 2);
    assert_eq!(
        lines.iter().map(|(number, _)| *number).collect::<Vec<_>>(),
        (1..=WALKED_LINES).collect::<Vec<_>>(),
        "every line exactly once"
    );
    second.close().await;
}
