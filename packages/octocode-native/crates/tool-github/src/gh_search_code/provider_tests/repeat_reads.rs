//! A repeat search reads no file twice: a read is pinned to the commit the
//! index matched, so the transport keeps its answer for good
//! ([`octocode_github::SavedAnswers`]) and a new commit is read again.
use super::*;
use octocode_github::{GitHubBudget, GitHubProvider, GitHubTransport, RetryPolicy};
use octocode_tools_shared::gh_shared::test_support::mock_endpoint;
use std::sync::Arc;

const NEXT_SHA: &str = "89abcdef0123456789abcdef0123456789abcdef";

/// A provider whose transport keeps answers, as a persistent runtime does.
fn keeping_provider(server: &MockServer) -> GitHubProvider {
    let mut transport = GitHubTransport::new(
        mock_endpoint(server),
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    transport.set_saved_answers(Some(Arc::new(
        octocode_runtime_base::providers::saved_answers::KeptAnswers::new(None),
    )));
    GitHubProvider::new(transport)
}

/// One hit of `keyword` in `app.rs`, indexed at `commit`.
async fn mount_hit(server: &MockServer, keyword: &str, commit: &str) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", keyword))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"blob1",
                 "html_url":format!("https://github.com/a/b/blob/{commit}/app.rs"),
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })))
        .mount(server)
        .await;
}

async fn search(provider: &GitHubProvider, keyword: &str) -> serde_json::Value {
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":[keyword]}))
            .expect("query");
    execute(
        provider,
        &query,
        &default_context(),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
    .data
}

async fn content_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .count()
}

#[tokio::test]
async fn a_repeat_reads_no_file_and_a_new_commit_reads_it_again() {
    let server = MockServer::builder().start().await;
    mount_hit(&server, "alpha", SHA).await;
    mount_hit(&server, "beta", NEXT_SHA).await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "app.rs").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/app.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content":STANDARD.encode("fn alpha() {}\nfn beta() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = keeping_provider(&server);

    let first = search(&provider, "alpha").await;
    assert_eq!(first["files"][0]["matches"], json!(["1\tfn alpha() {}"]));
    assert_eq!(content_requests(&server).await, 1, "{first}");

    let repeat = search(&provider, "alpha").await;
    assert_eq!(repeat["files"], first["files"]);
    assert_eq!(
        content_requests(&server).await,
        1,
        "the same hit at the same commit is not read again: {repeat}"
    );

    let moved = search(&provider, "beta").await;
    assert_eq!(moved["files"][0]["commitSha"], NEXT_SHA, "{moved}");
    assert_eq!(
        content_requests(&server).await,
        2,
        "a hit indexed at another commit is read again: {moved}"
    );
}

/// The git object id of a blob holding `text`.
fn oid(text: &str) -> String {
    use sha1::Digest as _;
    let mut digest = sha1::Sha1::new();
    digest.update(format!("blob {}\0", text.len()).as_bytes());
    digest.update(text.as_bytes());
    hex::encode(digest.finalize())
}

/// GraphQL answering `object(expression:)` reads of `files` at [`SHA`]: each
/// file's blob and each folder's entries, as GitHub answers them.
fn graphql_files(
    files: &'static [(&'static str, &'static str)],
) -> impl Fn(&wiremock::Request) -> ResponseTemplate {
    move |request: &wiremock::Request| {
        let body: serde_json::Value = serde_json::from_slice(&request.body).expect("body");
        let mut repository = serde_json::Map::new();
        for (alias, expression) in body["variables"].as_object().expect("variables") {
            let Some(expression) = expression
                .as_str()
                .filter(|_| alias.starts_with('f') || alias.starts_with('d'))
            else {
                continue;
            };
            let at = expression.strip_prefix(SHA).unwrap_or_default();
            let answer = if alias.starts_with('f') {
                let path = at.trim_start_matches(':');
                files
                    .iter()
                    .find(|(file, _)| *file == path)
                    .map_or(json!(null), |(_, text)| {
                        json!({"oid": oid(text), "byteSize": text.len(), "isBinary": false,
                           "isTruncated": false, "text": text})
                    })
            } else {
                let dir = at.trim_start_matches(':');
                let entries = files
                    .iter()
                    .filter_map(|(file, text)| {
                        let name = if dir.is_empty() { Some(*file) } else { file.strip_prefix(&format!("{dir}/")) }?;
                        (!name.contains('/')).then(|| json!({"name": name, "mode": 0o100_644, "type": "blob", "oid": oid(text)}))
                    })
                    .collect::<Vec<_>>();
                if dir.is_empty() {
                    json!({"tree": {"entries": entries}})
                } else {
                    json!({"entries": entries})
                }
            };
            repository.insert(alias.clone(), answer);
        }
        // One repository: its document reads it as `r0`.
        ResponseTemplate::new(200).set_body_json(json!({"data": {"r0": repository}}))
    }
}

/// A page of hit files is read in one GraphQL request with its folders'
/// trees (no REST tree or contents read), lists the same lines as REST, and
/// a later read of a hit at its commit sends nothing.
#[tokio::test]
async fn a_page_of_hits_is_read_in_one_graphql_request() {
    static FILES: &[(&str, &str)] = &[
        ("app.rs", "fn alpha() {}\n"),
        ("src/a.rs", "// alpha one\nfn x() {}\n"),
        ("src/b.rs", "fn y() {}\nlet alpha = 1;\n"),
    ];
    let server = MockServer::builder().start().await;
    let items = FILES
        .iter()
        .map(|(file, _)| {
            json!({"name": file, "path": file, "sha": oid(file),
                   "html_url": format!("https://github.com/a/b/blob/{SHA}/{file}"),
                   "repository": {"full_name": "a/b", "html_url": "https://x", "url": "https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"total_count": 3, "incomplete_results": false, "items": items}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(graphql_files(FILES))
        .mount(&server)
        .await;
    let provider = keeping_provider(&server);
    let data = search(&provider, "alpha").await;
    let lines = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|row| {
            (
                row["path"].clone(),
                crate::test_support::backed::listing(row),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        [
            (json!("app.rs"), json!(["1\tfn alpha() {}"])),
            (json!("src/a.rs"), json!(["1\t// alpha one"])),
            (json!("src/b.rs"), json!(["2\tlet alpha = 1;"])),
        ],
        "{data}"
    );
    let asked = server.received_requests().await.expect("recorded");
    let rest_reads = asked
        .iter()
        .filter(|request| {
            request.url.path().contains("/contents/") || request.url.path().contains("/git/")
        })
        .count();
    let graphql = asked
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count();
    assert_eq!((graphql, rest_reads), (1, 0), "{asked:?}");
    // A ghGetFileContent read of a hit at its commit is answered as kept.
    let read = provider
        .get_file_bytes(
            &octocode_github::ContentRequest {
                owner: "a".into(),
                repo: "b".into(),
                path: "src/b.rs".into(),
                reference: Some(SHA.into()),
            },
            &default_context(),
        )
        .await
        .expect("read");
    assert_eq!(read.bytes.as_ref(), b"fn y() {}\nlet alpha = 1;\n");
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        asked.len()
    );
}

/// A secret on a hit line of a file read by GraphQL is redacted in the served
/// `"<line>\t<text>"` entry, exactly as the REST read's lines are: the
/// GraphQL text is the file's own bytes, kept as its contents answer and
/// sealed by the same pass. The kept scan of a repeat serves the same
/// redacted lines.
#[tokio::test]
async fn a_secret_on_a_graphql_read_hit_line_is_redacted_in_the_served_string() {
    static FILES: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    let files: &'static [(&'static str, &'static str)] = FILES.get_or_init(|| {
        let secret: &'static str = Box::leak(
            format!("fn x() {{}}\nlet alpha = \"ghp_{}\";\n", "Zq7".repeat(12)).into_boxed_str(),
        );
        vec![("app.rs", secret), ("src/a.rs", "// alpha\n")]
    });
    let server = MockServer::builder().start().await;
    let items = files
        .iter()
        .map(|(file, _)| {
            json!({"name": file, "path": file, "sha": oid(file),
                   "html_url": format!("https://github.com/a/b/blob/{SHA}/{file}"),
                   "repository": {"full_name": "a/b", "html_url": "https://x", "url": "https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"total_count": 2, "incomplete_results": false, "items": items}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(graphql_files(files))
        .mount(&server)
        .await;
    let provider = keeping_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["alpha"]}))
            .expect("query");
    let security = octocode_runtime_base::security::ContentSecurity::new();
    for round in ["first", "repeat"] {
        let data = execute(
            &provider,
            &query,
            &default_context(),
            &security,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
        )
        .await
        .expect("search")
        .data;
        let app = data["files"]
            .as_array()
            .expect("files")
            .iter()
            .find(|row| row["path"] == "app.rs")
            .unwrap_or_else(|| panic!("{round}: {data}"));
        let line = app["matches"][0].as_str().expect("a numbered-line entry");
        assert!(line.starts_with("2\t"), "{round}: {line}");
        assert!(
            !line.contains("ghp_"),
            "{round}: the secret is served: {line}"
        );
        assert!(line.contains("let alpha = "), "{round}: {line}");
    }
    let graphql = server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count();
    assert_eq!(graphql, 1, "the repeat read nothing again");
}

/// A secret only the context detector finds (a `password = "…"` value, no
/// token shape) on a hit line of a GraphQL-read file never reaches the served
/// `"<line>\t<text>"` entry, on the first call or on the kept-scan repeat.
#[tokio::test]
async fn a_context_detected_secret_on_a_hit_line_is_redacted_in_the_served_string() {
    const VALUE: &str = "Kx9mVb2Qp7Lr4Tz8Wn3";
    static FILES: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    let line = format!("let password = \"{VALUE}\"; // alpha");
    use octocode_runtime_base::security::scan::ContentScan as _;
    let security = octocode_runtime_base::security::ContentSecurity::new();
    // The detector fires on the bare line: by context, not by a token shape.
    let (bare, _) = security
        .sanitize(&line, std::path::Path::new("conf.rs"))
        .expect("sanitized");
    assert!(!bare.contains(VALUE), "the context detector fires: {bare}");
    let files: &'static [(&'static str, &'static str)] = FILES.get_or_init(|| {
        let text: &'static str = Box::leak(format!("fn x() {{}}\n{line}\n").into_boxed_str());
        vec![("conf.rs", text)]
    });
    let server = MockServer::builder().start().await;
    let items = files
        .iter()
        .map(|(file, _)| {
            json!({"name": file, "path": file, "sha": oid(file),
                   "html_url": format!("https://github.com/a/b/blob/{SHA}/{file}"),
                   "repository": {"full_name": "a/b", "html_url": "https://x", "url": "https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"total_count": 1, "incomplete_results": false, "items": items}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(graphql_files(files))
        .mount(&server)
        .await;
    let provider = keeping_provider(&server);
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["alpha"]}))
            .expect("query");
    for round in ["first", "repeat"] {
        let data = execute(
            &provider,
            &query,
            &default_context(),
            &security,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
        )
        .await
        .expect("search")
        .data;
        let served = data["files"][0]["matches"][0]
            .as_str()
            .unwrap_or_else(|| panic!("{round}: a numbered-line entry: {data}"));
        assert!(served.starts_with("2\t"), "{round}: {served}");
        assert!(served.contains("let password = "), "{round}: {served}");
        assert!(
            !data.to_string().contains(VALUE),
            "{round}: the value is served: {data}"
        );
    }
}
