//! ghGetFileContent reads: redaction, directories, binary blobs and full-content paging.

use crate::support::{self};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
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
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/lib.rs").await;
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
        json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "path": "src/lib.rs",
                "ref": "main",
                "ranges": [{"startLine": 1, "endLine": 2}]
            }"#,
        ),
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
    assert!(
        file.get("next").is_none(),
        "the requested range is complete: {file}"
    );
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
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/lib.rs").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"[
                {
                    "name": "lib.rs",
                    "path": "src/lib.rs",
                    "type": "file",
                    "size": 3,
                    "sha": "1"
                }
            ]"#,
        )))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json_fixture(r#"{"owner": "a", "repo": "b", "path": "src", "ref": "main"}"#),
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
        crate::support::checks::related_call(
            data,
            "ghStructure",
            |input| input["queries"][0]["operation"] != "refs"
        )["input"]["queries"][0]["path"],
        "src",
        "{data}"
    );
    runtime.close().await;
}

/// A binary file is content the text read cannot serve: it names the size
/// and blob SHA under the code localFetch refuses binary bytes with,
/// neither a malformed request nor a decode failure.
#[tokio::test]
async fn gh_file_read_of_binary_content_reports_size_and_blob() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let blob = "b".repeat(40);
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "icon.png").await;
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
        json!({"owner":"a","repo":"b","path":"icon.png","ref":sha}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "binaryFileUnsupported", "{data}");
    let message = data["error"].as_str().unwrap_or_default();
    assert!(message.contains("12 bytes"), "{data}");
    assert!(message.contains(&blob), "{data}");
    // The refusal is the same class and row status as localFetch's own.
    let local = workspace.write("blob.ts", b"\x00\x01\x02binary\x00".as_slice());
    let refused = call(&runtime, "localFetch", json!({"path": local}))
        .await
        .expect("typed binary row");
    assert_eq!(row_data(&refused)["errorCode"], data["errorCode"]);
    assert_eq!(refused.exit_class(), outcome.exit_class());
    assert_eq!(row_status(&refused), row_status(&outcome));
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
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/big.rs").await;
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
        octocode_runtime_base::execution::RuntimeLimits {
            auto_page_chars: window,
            ..support::FIXTURE_LIMITS
        },
    );
    (server, workspace, runtime)
}

/// Raw source pages preserve comments, blank lines and exact line order.
#[tokio::test]
async fn gh_full_content_pages_stay_under_the_configured_window_and_cover_the_file() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = source_with_comments_and_blank_lines();
    let total = text.lines().count();
    for window in [support::FIXTURE_LIMITS.auto_page_chars, 40_000] {
        let (_server, _workspace, runtime) = big_file_runtime(&text, sha, window).await;
        let start =
            support::envelope(json!({"owner":"a","repo":"b","path":"src/big.rs","ref":sha,}));
        let walk = support::walk(&runtime, "ghGetFileContent", start, |output| {
            // Per-call provider metadata rides outside the page measure.
            let chars =
                octocode_runtime_base::response::pager::budgeted_chars(output).expect("page");
            assert_eq!(
                output.get("oversized") == Some(&json!(true)),
                octocode_runtime_base::response::pager::holds_value_over(
                    output.as_object().expect("envelope"),
                    octocode_runtime_base::id::ToolId::GhGetFileContent,
                    window
                ),
                "a page is {chars} chars: overflow is disclosed by the value it holds"
            );
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
            support::assert_cursor_only(continuation);
        }
        runtime.close().await;
    }
}

/// An explicit symbols view survives replay and reproduces its unpaged view.
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
                "view":"symbols"
            }));
            let walk = support::walk(&runtime, "ghGetFileContent", start, |output| {
                let data = &output["results"][0]["data"];
                // The fresh page repeats no unchanged request field; a resumed one states it.
                assert!(
                    data.get("view").is_none_or(|view| view == "symbols"),
                    "{data}"
                );
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
        support::assert_cursor_only(continuation);
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

/// A GitHub read of a code file is judged as a local read of the same bytes
/// is: its windows with the file's grammar, so a code line a configuration
/// rule would read as an entry (an interface member `password: Type;`) stays,
/// and its twin, a quoted credential the same file holds, is hidden. Every window
/// returns the text the local read of that window returns.
#[tokio::test]
async fn a_github_code_window_is_judged_as_the_local_read_of_its_file() {
    let token = format!("ghp_{}", "Zq7xK9mP".repeat(5));
    let source = format!(
        "interface Cfg {{\n  password: \"hunter2hunter2hunter2\";\n  mode: string;\n}}\nconst fallback = \"{token}\";\n"
    );
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/config.ts").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fconfig.ts"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(&source),
            "size": source.len(),
            "sha": "f".repeat(40),
            "path": "src/config.ts"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    workspace.write("src/config.ts", &source);
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for read in [
        json_fixture(r#"{"ranges": [{"startLine": 2, "endLine": 2}]}"#),
        json_fixture(
            r#"{
                "matchString": ["mode"],
                "contextLines": {"before": 1, "after": 0}
            }"#,
        ),
        json_fixture(r#"{"ranges": [{"startLine": 1, "endLine": 5}]}"#),
        json!({}),
    ] {
        let row = |fields: Value| {
            let mut row = fields;
            for (key, value) in read.as_object().expect("read fields") {
                row[key] = value.clone();
            }
            row
        };
        let local = call(
            &runtime,
            "localFetch",
            row(json_fixture(r#"{"path": "src/config.ts"}"#)),
        )
        .await
        .expect("local read");
        let github = call(
            &runtime,
            "ghGetFileContent",
            row(json_fixture(
                r#"{
                    "owner": "a",
                    "repo": "b",
                    "path": "src/config.ts",
                    "ref": "main"
                }"#,
            )),
        )
        .await
        .expect("github read");
        let local = row_data(&local)["content"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let github = row_data(&github)["content"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        // Only the value goes: the label and the `;` read as written.
        assert!(
            local.contains("password: \"[REDACTED-PASSWORDASSIGNMENT]\";"),
            "{read}: {local}"
        );
        assert!(!local.contains("hunter2hunter2hunter2"), "{read}: {local}");
        assert_eq!(github, local, "{read}");
        assert!(!github.contains(&token), "{read}: {github}");
    }
    runtime.close().await;
}
