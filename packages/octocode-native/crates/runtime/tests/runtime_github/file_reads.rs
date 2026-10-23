//! ghGetFileContent reads: redaction, directories, binary blobs and full-content paging.

use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_github::mock_http::{Arrivals, Delayed};
use serde_json::json;
use std::time::Duration;
use support::{ALL_EXTRAS, Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_file_read_goes_through_execute_and_redacts() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(format!("one\nneedle ghp_{}\nthree\n", "a".repeat(37))),
            "size": 22,
            "sha": "f".repeat(40),
            "path": "src/lib.rs"
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({
            "owner": "a",
            "repo": "b",
            "path": "src/lib.rs",
            "ref": "main",
            "forceRefresh": true,
            "unit": "lines",
            "length": 2
        }),
    )
    .await
    .expect("github read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let file = row_data(&outcome);
    let content = file["content"].as_str().unwrap_or("");
    assert!(content.contains("one\n"), "{content}");
    assert!(
        content.contains("[REDACTED"),
        "expected secret redaction, got {content}"
    );
    assert!(file["next"]["continueWindow"]["query"].is_object());
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_file_content_on_directory_returns_tree_recovery() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"}
        ])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"src","ref":"main","forceRefresh":true}),
    )
    .await
    .expect("directory read is a row error, not a contract violation");
    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert!(
        data["error"]
            .as_str()
            .unwrap_or_default()
            .contains("is a directory"),
        "{data}"
    );
    assert_eq!(
        crate::support::checks::hint(data, "viewTree")["query"]["queries"][0]["path"],
        "src",
        "{data}"
    );
    runtime.close().await;
}

/// A binary file is a request the text read cannot serve: it names the
/// size and blob SHA under an invalid-input code, not a decode failure.
#[tokio::test]
async fn gh_file_read_of_binary_content_reports_size_and_blob() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let blob = "b".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/icon.png"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(b"\x89PNG\0\0\0\rIHDR"),
            "size": 12,
            "sha": blob,
            "path": "icon.png"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"icon.png","ref":sha,"forceRefresh":true}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "invalidInput", "{data}");
    let message = data["error"].as_str().unwrap_or_default();
    assert!(message.contains("12 bytes"), "{data}");
    assert!(message.contains(&blob), "{data}");
    assert!(
        octocode_native::response::rows::is_invalid_input_code("invalidInput"),
        "binary reads must exit as caller errors"
    );
    runtime.close().await;
}

/// A source whose standard (minified) view differs from the bytes: line
/// comments, a block comment and blank lines among long code lines.
fn source_with_comments_and_blank_lines() -> String {
    (0..6000)
        .map(|n| match n % 10 {
            0 => "\n".to_owned(),
            3 => format!("// comment number {n} explains the next function\n"),
            6 => format!("/* block comment {n} */\n"),
            _ => format!("fn function_number_{n}(value: usize) -> usize {{ value * {n} }}\n"),
        })
        .collect()
}

/// Mount `text` as `src/big.rs` at `sha` and start a runtime under `window`.
async fn big_file_runtime(
    text: &str,
    sha: &str,
    window: usize,
) -> (MockServer, Workspace, octocode_native::runtime::ToolRuntime) {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fbig.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode(text),
            "size": text.len(), "sha": "f".repeat(40), "path": "src/big.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime_with(
        &[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))],
        octocode_native::execution::RuntimeLimits {
            auto_page_chars: window,
            ..support::FIXTURE_LIMITS
        },
    );
    (server, workspace, runtime)
}

/// Whole-file pages respect the default and a configured 40k host window and
/// deliver the source byte for byte, every line exactly once, through copied
/// continuations: comments and blank lines are kept, because `fullContent`
/// is the verbatim view, which every continuation keeps by omitting `minify`.
#[tokio::test]
async fn gh_full_content_pages_stay_under_the_configured_window_and_cover_the_file() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = source_with_comments_and_blank_lines();
    let total = text.lines().count();
    for window in [support::FIXTURE_LIMITS.auto_page_chars, 40_000] {
        let (_server, _workspace, runtime) = big_file_runtime(&text, sha, window).await;
        let start = support::envelope(
            json!({"owner":"a","repo":"b","path":"src/big.rs","ref":sha,"fullContent":true,"extras":[]}),
        );
        let walk = support::walk(&runtime, "ghGetFileContent", start, |output| {
            let chars = output.to_string().encode_utf16().count();
            assert!(chars <= window, "a page is {chars} chars");
            // Numbered records `N\t<text>`: the walk fails on a record
            // delivered twice.
            output["results"][0]["data"]["content"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .filter(|record| *record != "--")
                .map(str::to_owned)
                .collect()
        })
        .await;
        assert!(walk.queries.len() > 1, "one page held the whole file");
        let mut joined = String::new();
        for (index, record) in walk.rows.iter().enumerate() {
            let (number, line) = record
                .split_once('\t')
                .unwrap_or_else(|| panic!("unnumbered record {record:?}"));
            assert_eq!(number.parse::<usize>().ok(), Some(index + 1), "{record:?}");
            joined.push_str(line);
            joined.push('\n');
        }
        assert_eq!(walk.rows.len(), total, "every line once at window {window}");
        assert!(joined == text, "byte-for-byte source at window {window}");
        for continuation in &walk.queries[1..] {
            let row = &continuation["queries"][0];
            // The raw view is the default: omitted.
            assert!(row.get("minify").is_none(), "{continuation}");
            assert!(row.get("fullContent").is_none(), "{continuation}");
        }
        runtime.close().await;
    }
}

/// A view the caller names explicitly survives the window's shortened
/// whole-file reread: the paged walk carries the caller's `minify` in every
/// continuation and joins to the same text as one unpaged read. (`standard`
/// is the default view, which a whole-file read already resolves to
/// verbatim, so the explicit view here is the outline.)
#[tokio::test]
async fn gh_full_content_paging_keeps_an_explicit_view() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = source_with_comments_and_blank_lines();
    let read_all = |window: usize| {
        let text = text.clone();
        async move {
            let (_server, _workspace, runtime) = big_file_runtime(&text, sha, window).await;
            let pages = std::sync::Mutex::new(Vec::new());
            let start = support::envelope(json!({
                "owner":"a","repo":"b","path":"src/big.rs","ref":sha,
                "fullContent":true,"minify":"symbols","extras":[]
            }));
            let walk = support::walk(&runtime, "ghGetFileContent", start, |output| {
                let data = &output["results"][0]["data"];
                assert_eq!(data["contentView"], "symbols", "{data}");
                pages
                    .lock()
                    .expect("pages")
                    .push(data["content"].as_str().unwrap_or_default().to_owned());
                Vec::new()
            })
            .await;
            runtime.close().await;
            (walk.queries, pages.into_inner().expect("pages"))
        }
    };
    let (_, whole) = read_all(10_000_000).await;
    assert_eq!(whole.len(), 1, "a large window reads the view in one page");
    let (queries, pages) = read_all(support::FIXTURE_LIMITS.auto_page_chars).await;
    assert!(queries.len() > 1, "one page held the whole view");
    for continuation in &queries[1..] {
        assert_eq!(
            continuation["queries"][0]["minify"], "symbols",
            "{continuation}"
        );
    }
    let joined: String = pages
        .iter()
        .map(|page| {
            if page.ends_with('\n') {
                page.clone()
            } else {
                format!("{page}\n")
            }
        })
        .collect();
    let whole = if whole[0].ends_with('\n') {
        whole[0].clone()
    } else {
        format!("{}\n", whole[0])
    };
    assert!(!whole.contains("// comment"), "the outline drops comments");
    assert!(
        joined == whole,
        "the paged outline equals the unpaged outline"
    );
}
