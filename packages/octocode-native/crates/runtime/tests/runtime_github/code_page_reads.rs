//! A code search reads the files of the page it shows, not of every hit its
//! GitHub page listed: a page whose response window holds a few rows reads
//! those rows' files, and the walk reads every later row's file on the page
//! that shows it, from the one code-search answer the first page observed.
use crate::support::{self, Run as _, Workspace, checks};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// The commit the index matched every hit at.
const INDEXED: &str = "0123456789abcdef0123456789abcdef01234567";

/// A hit for `file` in `o/r`, indexed at [`INDEXED`], holding blob `n`.
fn hit(file: &str, n: usize) -> Value {
    json!({
        "name": file.rsplit('/').next(), "path": file, "sha": format!("{n:040x}"),
        "html_url": format!("https://github.com/o/r/blob/{INDEXED}/{file}"),
        "repository": {"full_name": "o/r", "html_url": "https://github.com/o/r",
                       "url": "https://api.github.com/repos/o/r"}
    })
}

/// `%XX` escapes of a request path segment, decoded.
fn unescape(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = bytes
            .get(at + 1..at + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[at], escaped) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).expect("utf-8 path")
}

/// The path a contents request reads.
fn read_path(request: &Request) -> String {
    let route = request.url.path();
    let (_, file) = route.split_once("/contents/").expect("a contents read");
    unescape(file)
}

/// Each file's text: three `needle` lines that name the file, so no two rows
/// list the same lines. `failing` paths answer 500.
struct Files {
    failing: Vec<String>,
}

impl Respond for Files {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let file = read_path(request);
        if self.failing.contains(&file) {
            return ResponseTemplate::new(500).set_body_json(json!({"message": "boom"}));
        }
        let text = format!(
            "fn needle() {{ /* {file} */ }}\nlet other = 1;\n// needle: the first note of {file}\n// needle: the last note of {file}\n"
        );
        ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode(text)
        }))
    }
}

/// A code index answering `page`/`per_page` the way GitHub does.
struct Index(Vec<Value>);

impl Respond for Index {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };
        let (page, per) = (param("page").unwrap_or(1), param("per_page").unwrap_or(30));
        let items = self.0.iter().skip((page - 1) * per).take(per).cloned();
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count": self.0.len(), "incomplete_results": false,
            "items": items.collect::<Vec<_>>()
        }))
    }
}

/// A repo-scoped code search of `files` whose file reads fail for `failing`.
async fn mount(server: &MockServer, files: &[String], failing: &[&str]) {
    let items = files
        .iter()
        .enumerate()
        .map(|(n, file)| hit(file, n + 1))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(Index(items))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v3/repos/o/r/contents/"))
        .respond_with(Files {
            failing: failing.iter().map(|file| (*file).to_owned()).collect(),
        })
        .mount(server)
        .await;
}

/// The server's requests so far: code searches, and the paths of file reads.
async fn requests(server: &MockServer) -> (usize, Vec<String>) {
    let received = server.received_requests().await.expect("recorded");
    let searches = received
        .iter()
        .filter(|request| request.url.path() == "/api/v3/search/code")
        .count();
    let reads = received
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .map(read_path)
        .collect();
    (searches, reads)
}

/// The paths one page lists (a row's `path` and every `alsoAt`); a file
/// row a response page split (`rowPart.continues` into `files`) is the one
/// the previous page began, not listed again.
fn listed(output: &Value) -> Vec<String> {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            let continued = row["rowPart"]["continues"]["array"]
                .as_str()
                .is_some_and(|array| array.ends_with("files"));
            let files = row["data"]["files"].as_array().cloned().unwrap_or_default();
            files.into_iter().skip(usize::from(continued))
        })
        .flat_map(|file| {
            file["path"]
                .as_str()
                .map(str::to_owned)
                .into_iter()
                .chain(
                    file["alsoAt"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|path| path.as_str().map(str::to_owned)),
                )
                .collect::<Vec<_>>()
        })
        .collect()
}

/// One call of a walk: what it listed, the files it read and its output.
struct Call {
    listed: Vec<String>,
    reads: Vec<String>,
    output: Value,
}

/// Every page of a ghSearchCode walk from `start`, with the file reads each
/// call sent ([`checks::Walker`]: each call once, each row once).
async fn walk(
    server: &MockServer,
    runtime: &octocode_native::runtime::ToolRuntime,
    start: Value,
) -> Vec<Call> {
    walk_with(server, runtime, start, |_| {}).await
}

/// [`walk`], calling `accepted` with each call's output once it is accepted.
async fn walk_with(
    server: &MockServer,
    runtime: &octocode_native::runtime::ToolRuntime,
    start: Value,
    mut accepted: impl FnMut(&Value),
) -> Vec<Call> {
    let mut walker = checks::Walker::new("ghSearchCode", start);
    let mut calls = Vec::new();
    while let Some(query) = walker.next_query() {
        let (_, before) = requests(server).await;
        let output = runtime
            .run("walk".into(), "ghSearchCode".into(), query.clone())
            .await
            .unwrap_or_else(|error| panic!("{query}: {error:?}"))
            .structured_content
            .into_value();
        let (_, after) = requests(server).await;
        let listed = listed(&output);
        walker.accept(checks::Page {
            rows: listed.clone(),
            output: output.clone(),
        });
        accepted(&output);
        calls.push(Call {
            listed,
            reads: after[before.len()..].to_vec(),
            output,
        });
    }
    calls
}

fn files(count: usize, name: impl Fn(usize) -> String) -> Vec<String> {
    (0..count).map(name).collect()
}

/// The fail-first case: 50 hits, a response window that holds about five rows.
/// The first page reads no file it does not show; the walk shows every hit
/// once, reads each file once and sends one code search.
#[tokio::test]
async fn a_page_reads_only_the_files_it_shows_and_the_walk_reads_each_once() {
    let server = MockServer::builder().start().await;
    let names = files(50, |n| format!("src/f{n:03}.rs"));
    mount(&server, &names, &[]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json!({
        "queries": [{"owner": "o", "repo": "r", "keywords": ["needle"]}],
        "responseChars": 5000
    });
    let calls = walk(&server, &runtime, start).await;
    let first = &calls[0];
    assert!(
        !first.listed.is_empty() && first.listed.len() < names.len(),
        "the window holds a few rows: {}",
        first.output
    );
    assert!(
        first.reads.len() <= first.listed.len(),
        "page 1 read {} files to show {}: {:?}",
        first.reads.len(),
        first.listed.len(),
        first.reads
    );
    let shown = calls
        .iter()
        .flat_map(|call| call.listed.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        shown.iter().cloned().collect::<BTreeSet<_>>(),
        names.iter().cloned().collect::<BTreeSet<_>>(),
        "every hit is reached"
    );
    assert_eq!(shown.len(), names.len(), "no hit twice");
    let (searches, reads) = requests(&server).await;
    assert_eq!(searches, 1, "the walk sends one code search");
    assert_eq!(
        reads.iter().cloned().collect::<BTreeSet<_>>().len(),
        reads.len(),
        "no file is read twice: {reads:?}"
    );
    assert_eq!(
        reads.len(),
        names.len(),
        "every shown file is read: {reads:?}"
    );
    for call in &calls {
        let data = &call.output["results"][0]["data"];
        for file in data["files"].as_array().into_iter().flatten() {
            assert!(
                file["matches"].is_array(),
                "a shown row has its lines: {file}"
            );
        }
    }
    runtime.close().await;
}

/// Whatever the window, the part a page cuts fits it as published, the
/// response envelope and the saved page's `responsePagination` included: the
/// pager never cuts that part again, so the page reads no file it does not
/// show.
#[tokio::test]
async fn every_window_publishes_the_whole_part_its_page_read() {
    let names = files(50, |n| format!("src/f{n:03}.rs"));
    for chars in (1_500..=5_000).step_by(167) {
        let server = MockServer::builder().start().await;
        mount(&server, &names, &[]).await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&support::github_api(&server));
        let query = json!({
            "queries": [{"owner": "o", "repo": "r", "keywords": ["needle"]}],
            "responseChars": chars
        });
        let output = runtime
            .run("first".into(), "ghSearchCode".into(), query)
            .await
            .unwrap_or_else(|error| panic!("responseChars {chars}: {error:?}"))
            .structured_content
            .into_value();
        let shown = listed(&output);
        let (_, reads) = requests(&server).await;
        assert!(
            output["results"][0].get("rowPart").is_none(),
            "responseChars {chars}: the pager cut the page's part again: {output}"
        );
        assert!(
            !shown.is_empty() && reads.len() <= shown.len(),
            "responseChars {chars}: read {} files to show {}: {reads:?}",
            reads.len(),
            shown.len()
        );
        runtime.close().await;
    }
}

/// Unicode paths and a file whose read fails: every hit is listed once, the
/// failed one says its lines were not resolved and leads to its read.
#[tokio::test]
async fn unicode_paths_and_a_failed_read_are_listed_once_over_the_walk() {
    let server = MockServer::builder().start().await;
    let names = files(12, |n| format!("src/λ/ünï🦀{n:02}.rs"));
    let failing = names[3].clone();
    mount(&server, &names, &[failing.as_str()]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let start = json!({
        "queries": [{"owner": "o", "repo": "r", "keywords": ["needle"]}],
        "responseChars": 2000
    });
    let calls = walk(&server, &runtime, start).await;
    assert!(calls.len() > 1, "the window pages the hits");
    let shown = calls
        .iter()
        .flat_map(|call| call.listed.clone())
        .collect::<Vec<_>>();
    assert_eq!(shown.len(), names.len(), "{shown:?}");
    assert_eq!(
        shown.into_iter().collect::<BTreeSet<_>>(),
        names.iter().cloned().collect::<BTreeSet<_>>()
    );
    let row = calls
        .iter()
        .flat_map(|call| {
            call.output["results"][0]["data"]["files"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .find(|file| file["path"] == failing.as_str())
        .expect("the failed file is listed");
    assert_eq!(row["lineResolved"], false, "{row}");
    let (searches, _) = requests(&server).await;
    assert_eq!(searches, 1);
    // A batch read past the window costs at most the rows already shown
    // (`part::read`), so a part reads at most twice what it shows.
    for call in &calls {
        assert!(
            call.reads.len() <= 2 * call.listed.len(),
            "a page reads at most twice the files it shows: read {:?}, listed {:?}",
            call.reads,
            call.listed
        );
    }
    runtime.close().await;
}

/// The tiniest window still moves: each page shows at least one row, and the
/// walk shows every hit once with one code search.
#[tokio::test]
async fn a_tiny_window_walks_every_hit_once() {
    let server = MockServer::builder().start().await;
    let names = files(6, |n| format!("src/t{n}.rs"));
    mount(&server, &names, &[]).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    for chars in [1, 300] {
        let start = json!({
            "queries": [{"owner": "o", "repo": "r", "keywords": ["needle"]}],
            "responseChars": chars
        });
        let calls = walk(&server, &runtime, start).await;
        let shown = calls
            .iter()
            .flat_map(|call| call.listed.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            shown.into_iter().collect::<BTreeSet<_>>(),
            names.iter().cloned().collect::<BTreeSet<_>>(),
            "responseChars {chars}"
        );
    }
    runtime.close().await;
}

mod moved_ref;
mod parts;
