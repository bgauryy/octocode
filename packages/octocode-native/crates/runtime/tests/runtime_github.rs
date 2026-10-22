//! The GitHub file, code, tree and repository tools through the runtime:
//! reads, recoveries, rate limits and bulk dispatch against a mock GitHub.

use crate::support::{self, Run as _};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_github::mock_http::{Arrivals, Delayed};
use serde_json::json;
use std::time::Duration;
use support::{ALL_EXTRAS, Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn malformed_remote_regex_keeps_original_error_and_required_file_path() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("example source\n"),"size":15,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for extras in [json!([]), json!(["diagnostics"])] {
        let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":"(","regex":"rust","extras":extras})).await.expect("error row");
        assert_eq!(row_status(&out), "error", "{}", out.structured_content);
        let data = row_data(&out);
        assert_ne!(data["errorCode"], "outputContractViolation", "{data}");
        assert_eq!(data["path"], "README.md", "{data}");
        assert!(
            data["error"]
                .as_str()
                .is_some_and(|error| error.contains("Invalid regex pattern")),
            "{data}"
        );
    }
    // A list with one unclosed call: the error names the item, and the one
    // lead is the same GitHub read with that item escaped; no static text.
    let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":["^example","source("],"regex":"rust"})).await.expect("error row");
    let data = row_data(&out);
    assert_eq!(data["errorCode"], "invalidPattern", "{data}");
    assert!(
        data["error"].to_string().contains("matchString[1]"),
        "{data}"
    );
    let repair = &crate::support::checks::hint(data, "repair");
    assert_eq!(repair["tool"], "ghGetFileContent", "{data}");
    let row = &repair["query"]["queries"][0];
    assert_eq!(
        row["matchString"],
        json!(["^example", "source\\("]),
        "{data}"
    );
    assert_eq!(
        (
            row["owner"].as_str(),
            row["repo"].as_str(),
            row["ref"].as_str()
        ),
        (Some("a"), Some("b"), Some(sha)),
        "{data}"
    );
    assert!(
        crate::support::checks::hint(data, "text").is_null(),
        "{data}"
    );
    runtime.close().await;
}

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
async fn github_tree_materialize_is_accepted_and_emits_location() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":4,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":4,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    for name in ["one.rs", "two.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn x(){}\n"),
                "size": 8,
                "sha": "a".repeat(40),
                "path": name
            })))
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({
            "owner": "a",
            "repo": "b",
            "materialize": true
        }),
    )
    .await
    .expect("tree materialize");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert!(data["location"]["localPath"].as_str().is_some(), "{}", data);
    // Only the path and coverage: kind/source/cached are constants, hasMore
    // restates `complete`, and the ref is the snapshot the path names.
    let mut keys: Vec<&str> = data["location"]
        .as_object()
        .expect("location")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["coverage", "localPath"], "{data}");
    let exhausted = call(&runtime, "ghStructure", json!({"owner":"a","repo":"b","ref":sha,"pageSize":1,"materialize":true,"materializeOffset":1})).await.expect("boundary offset");
    let boundary = row_data(&exhausted);
    assert_eq!(boundary["pagination"]["hasMore"], true, "{boundary}");
    // A materialize resumed inside its page does not re-send the
    // entries its first call listed; `location` anchors it.
    let resumed = call(
        &runtime,
        "ghStructure",
        json!({"owner":"a","repo":"b","ref":sha,"materialize":true,"materializeOffset":1}),
    )
    .await
    .expect("mid-page offset");
    let resumed = row_data(&resumed);
    assert!(resumed.get("files").is_none(), "{resumed}");
    assert!(
        resumed["location"]["localPath"].as_str().is_some(),
        "{resumed}"
    );
    assert_eq!(
        resumed["location"]["coverage"]["state"], "complete",
        "{resumed}"
    );
    let next = &boundary["next"]["continueMaterialize"]["query"]["queries"][0];
    assert_eq!(next["ref"], sha, "{boundary}");
    assert_eq!(next["page"], 2, "{boundary}");
    assert_eq!(next["materializeOffset"], 0, "{boundary}");
    let last = call(&runtime, "ghStructure", next.clone())
        .await
        .expect("replay boundary continuation");
    let completed = row_data(&last);
    assert_eq!(
        completed["location"]["coverage"]["state"], "complete",
        "{completed}"
    );
    runtime.close().await;
}

/// A materialize that lists folders below its depth without writing
/// them is not complete, and `next.expandDepth` writes them.
#[tokio::test]
async fn github_tree_materialize_with_unwritten_folders_is_not_complete() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":4,"sha":"1".repeat(40)},
            {"name":"router","path":"router","type":"dir","sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode("fn x(){}\n"),
            "size": 8, "sha": "a".repeat(40), "path": "one.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "materialize": true}),
    )
    .await
    .expect("tree materialize");
    let data = row_data(&outcome);
    assert_eq!(data["location"]["coverage"]["state"], "partial", "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("materializeDepth"))),
        "{data}"
    );
    let deeper = &data["next"]["expandDepth"];
    assert_eq!(deeper["tool"], "ghStructure", "{data}");
    let row = &deeper["query"]["queries"][0];
    assert_eq!(row["maxDepth"], 20, "{data}");
    assert_eq!(row["materialize"], true, "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    runtime.close().await;
}

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

#[tokio::test]
async fn github_clone_is_cli_only() {
    let workspace = Workspace::new();
    let cli = workspace.runtime(&[]);
    assert!(cli.is_available("ghCloneRepo"));
    let mcp_call = cli
        .run_mcp(
            "mcp-clone".into(),
            "ghCloneRepo".into(),
            json!({"queries":[{"owner":"a","repo":"b","mainGoal": "test", "reasoning":"check MCP gate"}]}),
        )
        .await
        .expect("MCP answers a refused call as a tool error");
    assert_eq!(mcp_call["isError"], true, "{mcp_call}");
    assert_eq!(
        mcp_call["structuredContent"]["errorCode"], "toolUnavailable",
        "MCP channel cannot clone through a CLI runtime: {mcp_call}"
    );
    cli.close().await;

    let mut input = workspace.config(&[]);
    input.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let mcp =
        octocode_native::runtime::ToolRuntime::with_limits(input, crate::support::FIXTURE_LIMITS)
            .expect("MCP runtime");
    assert!(!mcp.is_available("ghCloneRepo"));
    let error = call(&mcp, "ghCloneRepo", json!({"owner":"a","repo":"b"}))
        .await
        .expect_err("clone disabled for MCP");
    assert_eq!(error.code, "toolUnavailable");
    mcp.close().await;
}

/// Verifies that independent GitHub API calls run concurrently while output
/// rows retain their input order.
#[tokio::test]
async fn three_github_bulk_queries_are_concurrent_and_preserve_order() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let arrivals = Arrivals::default();

    for name in ["alpha.rs", "beta.rs", "gamma.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(Delayed {
                arrivals: arrivals.clone(),
                ..Delayed::new(
                    Duration::from_millis(200),
                    ResponseTemplate::new(200).set_body_json(json!({
                        "type": "file",
                        "encoding": "base64",
                        "content": STANDARD.encode("fn placeholder(){}"),
                        "size": 18,
                        "sha": "a".repeat(40),
                        "path": name
                    })),
                )
            })
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    let queries = ["alpha.rs", "beta.rs", "gamma.rs"].map(|name| {
        json!({
            "owner": "a",
            "repo": "b",
            "path": name,
            "ref": sha,
            "forceRefresh": true,
            "mainGoal": "test", "reasoning": format!("Read {name} through the GitHub bulk path."),
            "extras": ALL_EXTRAS
        })
    });
    let outcome = runtime
        .run(
            "github-bulk".into(),
            "ghGetFileContent".into(),
            json!({"queries": queries}),
        )
        .await
        .expect("GitHub bulk result");
    let rows = outcome.structured_content["results"]
        .as_array()
        .expect("GitHub result rows");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row["index"].as_u64())
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2)]
    );
    assert!(
        rows.iter().all(|row| row.get("status").is_none()),
        "{rows:?}"
    );
    {
        let arrivals = arrivals.times();
        assert_eq!(arrivals.len(), 3);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "GitHub API calls were dispatched serially: {arrivals:?}"
        );
    }

    runtime.close().await;
}

/// Verifies that three sequential single-query calls each produce exactly one
/// server-side HTTP request — `.expect(1)` on each mock endpoint is the
/// observable proof.
#[tokio::test]
async fn three_sequential_queries_each_hit_the_server() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";

    // One SHA endpoint shared by all three queries.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;

    // Three distinct content endpoints — each MUST be hit exactly once.
    for name in ["p.rs", "q.rs", "r.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn ok(){}"),
                "size": 9,
                "sha": "b".repeat(40),
                "path": name
            })))
            .expect(1) // exactly 1 request per file endpoint
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    for name in ["p.rs", "q.rs", "r.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name,
                   "ref": "main", "forceRefresh": true}),
        )
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_ne!(
            row_status(&outcome),
            "error",
            "{name} failed: {}",
            outcome.structured_content
        );
    }
    // WireMock verifies `.expect(1)` on drop: each content endpoint hit exactly once.

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
        crate::support::checks::hint(data, "findRepository")["tool"],
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

/// A wrong-case path recovers to the case-corrected file, and a missing
/// file to its nearest existing directory, never to another missing path.
#[tokio::test]
async fn gh_file_read_of_a_missing_path_recovers_to_what_exists() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let listing = |entries: serde_json::Value| ResponseTemplate::new(200).set_body_json(entries);
    for (dir, entries) in [
        (
            "",
            json!([
                {"name":"tokio","path":"tokio","type":"dir"},
                {"name":"README.md","path":"README.md","type":"file","size":3,"sha":"1"}
            ]),
        ),
        (
            "/tokio",
            json!([{"name":"src","path":"tokio/src","type":"dir"}]),
        ),
        (
            "/tokio%2Fsrc",
            json!([
                {"name":"lib.rs","path":"tokio/src/lib.rs","type":"file","size":3,"sha":"2"}
            ]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents{dir}")))
            .respond_with(listing(entries))
            .mount(&server)
            .await;
    }
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"}));
    for missing in [
        "Tokio%2Fsrc%2Flib.rs",
        "Tokio%2Fsrc",
        "Tokio",
        "tokio%2Fsrc%2Fnope.rs",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{missing}")))
            .respond_with(not_found())
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read =
        |file: &str| json!({"owner":"a","repo":"b","path":file,"ref":"main","forceRefresh":true});
    let outcome = call(&runtime, "ghGetFileContent", read("Tokio/src/lib.rs"))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert_eq!(
        crate::support::checks::hint(data, "read")["query"]["queries"][0]["path"],
        "tokio/src/lib.rs",
        "{data}"
    );
    // Recovery leads pin the commit the read resolved, not the movable ref.
    assert_eq!(
        crate::support::checks::hint(data, "read")["query"]["queries"][0]["ref"],
        sha,
        "{data}"
    );
    // The case-fixed read is the one repair; no listing beside it.
    assert!(
        crate::support::checks::hint(data, "viewTree").is_null(),
        "{data}"
    );
    let outcome = call(&runtime, "ghGetFileContent", read("tokio/src/nope.rs"))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert!(
        crate::support::checks::hint(data, "read").is_null(),
        "{data}"
    );
    assert_eq!(
        crate::support::checks::hint(data, "viewTree")["query"]["queries"][0]["path"],
        "tokio/src",
        "{data}"
    );
    // A hint is not proof, so no continuation carries confidence:"exact".
    assert!(
        crate::support::checks::hint(data, "viewTree")
            .get("confidence")
            .is_none(),
        "{data}"
    );
    runtime.close().await;
}

/// A ghStructure listing of a missing path recovers like
/// ghGetFileContent: `next.viewTree` lists the nearest existing directory
/// (case-corrected), never another missing path.
#[tokio::test]
async fn gh_structure_of_a_missing_path_recovers_to_the_nearest_directory() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let listing = |entries: serde_json::Value| ResponseTemplate::new(200).set_body_json(entries);
    for (dir, entries) in [
        ("", json!([{"name":"src","path":"src","type":"dir"}])),
        (
            "/src",
            json!([{"name":"Tools","path":"src/Tools","type":"dir"}]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents{dir}")))
            .respond_with(listing(entries))
            .mount(&server)
            .await;
    }
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"}));
    for missing in [
        "src%2Fnope",
        "src%2Ftools",
        "no",
        "no%2Fsuch",
        "no%2Fsuch%2Fdir",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{missing}")))
            .respond_with(not_found())
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let tree = |dir: &str| json!({"owner":"a","repo":"b","path":dir,"ref":"main"});
    for (requested, nearest) in [
        ("src/nope", "src"),
        ("src/tools", "src/Tools"),
        ("no/such/dir", "."),
    ] {
        let outcome = call(&runtime, "ghStructure", tree(requested))
            .await
            .expect("error row, not a contract violation");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{}",
            outcome.structured_content
        );
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{data}");
        assert_eq!(
            crate::support::checks::hint(data, "viewTree")["tool"],
            "ghStructure",
            "{data}"
        );
        assert_eq!(
            crate::support::checks::hint(data, "viewTree")["query"]["queries"][0]["path"],
            nearest,
            "{data}"
        );
        // Recovery leads pin the commit the listing resolved.
        assert_eq!(
            crate::support::checks::hint(data, "viewTree")["query"]["queries"][0]["ref"],
            sha,
            "{data}"
        );
        // A hint is not proof, so no continuation carries confidence:"exact".
        assert!(
            crate::support::checks::hint(data, "viewTree")
                .get("confidence")
                .is_none(),
            "{data}"
        );
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains(requested),
            "{data}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_picked_repositories_are_contract_valid() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [{"full_name":"o/r","name":"r","html_url":"https://x","default_branch":"main"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    // A pick keeps the row's id (owner+repo) beside the picked keys, so the
    // picked row still satisfies the output contract.
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json!({"keywords":["x"],"fields":["stars"]}),
    )
    .await
    .expect("picked repositories");
    assert_eq!(
        row_data(&outcome)["repositories"],
        json!([{"owner":"o","repo":"r","stars":0}])
    );
    // The top repository's tree lead is a hint, not a page.
    let lead = &crate::support::checks::hint(row_data(&outcome), "viewRepo");
    assert_eq!(lead["tool"], "ghStructure", "{}", row_data(&outcome));
    assert_eq!(
        (
            &lead["query"]["queries"][0]["owner"],
            &lead["query"]["queries"][0]["repo"]
        ),
        (&json!("o"), &json!("r"))
    );
    assert!(row_data(&outcome).get("next").is_none());
    runtime.close().await;
}

/// A primary rate limit surfaces as a contract-valid error row carrying
/// rate-limit metadata (incl. resource), and the blocking fact is mirrored to
/// `<home>/tmp/ratelimit/` for other processes.
#[tokio::test]
async fn gh_primary_rate_limit_is_contract_valid_and_persisted_for_other_processes() {
    let server = MockServer::builder().start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_STORAGE_MODE", "persistent".into()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for name in ["one.rs", "two.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name, "ref": sha, "forceRefresh": true}),
        )
        .await
        .expect("row-level error");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{}",
            outcome.structured_content
        );
        checks::assert_output_contract("ghGetFileContent", &outcome.structured_content);
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "rateLimited", "{data}");
        assert_eq!(data["rateLimit"]["resetEpochSeconds"], reset, "{data}");
        assert_eq!(data["rateLimit"]["resource"], "core", "{data}");
        // An authenticated caller is not told to authenticate.
        checks::assert_absent(data, "auth login");
    }
    // The second read was refused before sending (mock expects one hit).
    let dir = workspace.home.join("tmp").join("ratelimit");
    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("ratelimit dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files[0].path()).unwrap()).unwrap();
    assert_eq!(state["buckets"]["core"]["remaining"], 0, "{state}");
    assert_eq!(state["buckets"]["core"]["reset"], reset, "{state}");
    runtime.close().await;
}

/// Only an anonymous caller is told that authenticating raises quota.
#[tokio::test]
async fn anonymous_rate_limit_advises_authentication() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "4102444800")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("GITHUB_TOKEN", String::new()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner": "a", "repo": "b", "path": "x.rs", "ref": sha, "forceRefresh": true}),
    )
    .await
    .expect("row-level error");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "rateLimited", "{data}");
    assert!(
        crate::support::checks::hint(data, "text")[0]
            .as_str()
            .is_some_and(|hint| hint.contains("auth login")),
        "{data}"
    );
    runtime.close().await;
}

/// Every hint string anywhere in a row.
fn all_hints(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "hints"
                    && let Some(hints) = child.as_array()
                {
                    out.extend(hints.iter().filter_map(|h| h.as_str().map(str::to_owned)));
                }
                out.extend(all_hints(child));
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| out.extend(all_hints(v))),
        _ => {}
    }
    out
}

/// Authentication and binary-file hints arrive whole, never cut
/// mid-sentence with an ellipsis.
#[tokio::test]
async fn github_recovery_hints_are_never_cut_mid_sentence() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/locked"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/img.png"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64",
            "content": STANDARD.encode([0x89u8, b'P', b'N', b'G', 0, 0, 1]),
            "size": 7, "sha": "f".repeat(40), "path": "img.png"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for query in [
        json!({"owner":"a","repo":"b","path":"README.md","ref":"locked","extras":[]}),
        json!({"owner":"a","repo":"b","path":"img.png","ref":sha,"extras":[]}),
    ] {
        let outcome = call(&runtime, "ghGetFileContent", query)
            .await
            .expect("error row");
        let data = row_data(&outcome);
        let hints = all_hints(data);
        assert!(!hints.is_empty(), "{data}");
        for hint in hints {
            assert!(!hint.contains('…'), "truncated hint {hint:?} in {data}");
        }
    }
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
        json!({"owner":"ghost","repo":"nope","path":"README.md","extras":[]}),
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
        crate::support::checks::hint(data, "viewTree").is_null(),
        "{data}"
    );
    // As on ghSearchCode and ghStructure, the one repair is a search for
    // the repository's name, which names the owner that holds it.
    let find = &crate::support::checks::hint(data, "findRepository");
    assert_eq!(find["tool"], "ghSearchRepo", "{data}");
    assert_eq!(
        find["query"]["queries"][0]["keywords"],
        json!(["nope"]),
        "{data}"
    );
    assert!(
        crate::support::checks::hint(data, "text").is_null(),
        "{data}"
    );
    checks::assert_absent(data, "exact case");
    checks::assert_absent(data, "commits#get-a-commit");
    runtime.close().await;
}

/// A failed read's provider status, retry flag, request id and
/// documentation link are support diagnostics (`diagnostics` extra): a
/// default row drops them; `extras: ["diagnostics"]` keeps them.
#[tokio::test]
async fn gh_file_error_keeps_provider_diagnostics_for_the_diagnostics_extra() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fmissing.rs"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-github-request-id", "R1")
                .set_body_json(json!({
                    "message":"Not Found",
                    "documentation_url":"https://docs.github.com/rest/repos/contents#get-repository-content"
                })),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for debug in [false, true] {
        let extras = if debug {
            json!(["diagnostics"])
        } else {
            json!([])
        };
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"src/missing.rs","ref":sha,"extras":extras}),
        )
        .await
        .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{data}");
        assert_eq!(data.get("httpStatus").is_some(), debug, "{data}");
        // A 404 is not retryable, so no mode states `retryable`.
        assert!(data.get("retryable").is_none(), "{data}");
        assert_eq!(data.get("requestId").is_some(), debug, "{data}");
        assert_eq!(data.get("documentationUrl").is_some(), debug, "{data}");
    }
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

/// Through the whole response stage, a code search pinned to a non-default
/// ref warns with the index commit and keeps its ghStructure lead to the ref
/// first among the leads.
#[tokio::test]
async fn pinned_ref_code_search_warns_and_leads_to_the_ref_listing() {
    let server = MockServer::builder().start().await;
    let at_ref = "0123456789abcdef0123456789abcdef01234567";
    let head = "fedcba9876543210fedcba9876543210fedcba98";
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"base.py","path":"pkg/base.py","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"class Base:","matches":[{"text":"class","indices":[0,5]}]}]}
            ]
        })))
        .mount(&server)
        .await;
    for (reference, sha) in [("dev", at_ref), ("HEAD", head)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(sha))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/pkg%2Fbase.py"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("class Base:\n    pass\n")
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","repo":"b","keywords":["class"],"path":"pkg","ref":"dev"}),
    )
    .await
    .expect("search");
    let data = row_data(&out);
    // The warning states the per-call facts only; the route is the lead.
    assert_eq!(
        data["warnings"],
        json!(["Files from the default-branch index at fedcba9, not dev; `lines` read at dev."]),
        "{data}"
    );
    let lead = &crate::support::checks::hint(data, "viewRepo");
    assert_eq!(lead["tool"], "ghStructure", "{data}");
    assert_eq!(lead["query"]["queries"][0]["ref"], at_ref, "{data}");
    assert_eq!(lead["query"]["queries"][0]["path"], "pkg", "{data}");
    assert_eq!(
        data["hints"][0]
            .as_str()
            .and_then(|text| text.split_once(": "))
            .map(|(name, _)| name),
        Some("viewRepo"),
        "{data}"
    );
    runtime.close().await;
}

/// A ref that does not resolve is `notFound` (not `invalidInput`) on
/// both GitHub read tools, with one runnable lead to the repository's refs.
#[tokio::test]
async fn gh_bad_ref_is_not_found_with_a_refs_lead() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-ref"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-ref"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, query) in [
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":"no-such-ref","forceRefresh":true}),
        ),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","ref":"no-such-ref"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains("\"no-such-ref\""),
            "{tool}: {data}"
        );
        let lead = &crate::support::checks::hint(data, "viewRefs");
        assert_eq!(lead["tool"], "ghStructure", "{tool}: {data}");
        let row = &lead["query"]["queries"][0];
        for (field, value) in [("owner", "a"), ("repo", "b"), ("operation", "refs")] {
            assert_eq!(row[field], value, "{tool}: {data}");
        }
    }
    runtime.close().await;
}

/// A full 40-hex SHA that GitHub does not have skips ref resolution, so
/// the listing's tree read is a bare 404. ghStructure must still name the
/// ref and lead to the repository's refs, as ghGetFileContent does, instead
/// of the generic "Repository, resource, or path not found".
#[tokio::test]
async fn gh_missing_full_sha_is_named_with_a_refs_lead() {
    const MISSING: &str = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{MISSING}")))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":format!("No commit found for SHA: {MISSING}")})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"message":format!("No commit found for the ref {MISSING}")})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, query) in [
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":MISSING,"forceRefresh":true}),
        ),
        ("ghStructure", json!({"owner":"a","repo":"b","ref":MISSING})),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","ref":MISSING,"path":"src"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data["error"].as_str().unwrap_or_default().contains(MISSING),
            "{tool}: {data}"
        );
        assert_eq!(
            crate::support::checks::hint(data, "viewRefs")["tool"],
            "ghStructure",
            "{tool}: {data}"
        );
    }
    runtime.close().await;
}

/// An owner-only ghSearchRepo listing of a login GitHub does not know
/// (bare 404) names the owner instead of "Repository, resource, or path".
#[tokio::test]
async fn gh_search_repo_missing_owner_is_named() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(&runtime, "ghSearchRepo", json!({"owner":"ghost-login"}))
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

/// A read of a renamed repository (GitHub answers 301 to
/// `/repositories/<id>`) warns once and names the canonical repository; a
/// later process reading from the immutable cache (no request, so no
/// redirect) still knows the rename from the memo.
#[tokio::test]
async fn gh_file_read_of_a_renamed_repository_names_the_canonical_repository() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (from, to) in [
        (
            "/api/v3/repos/a/b/commits/main",
            "/api/v3/repositories/1/commits/main",
        ),
        (
            "/api/v3/repos/a/b/contents/README.md",
            "/api/v3/repositories/1/contents/README.md",
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(from))
            .respond_with(
                ResponseTemplate::new(301)
                    .insert_header("location", format!("{}{to}?ref={sha}", server.uri())),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),
            "size":6,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let renamed = |data: &serde_json::Value| {
        let renames = data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|warning| {
                warning
                    .as_str()
                    .is_some_and(|text| text.contains("renamed"))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            renames,
            [&json!(
                "Repository a/b was renamed to c/d; leads use the canonical name."
            )],
            "one rename warning: {data}"
        );
        assert_eq!(data["owner"], "c", "{data}");
        assert_eq!(data["repo"], "d", "{data}");
    };
    let first = workspace.runtime(&settings);
    let outcome = call(
        &first,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":"main"}),
    )
    .await
    .expect("read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    renamed(row_data(&outcome));
    first.close().await;

    let second = workspace.runtime(&settings);
    let outcome = call(
        &second,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":sha}),
    )
    .await
    .expect("cached read");
    renamed(row_data(&outcome));
    second.close().await;
}

/// A repository GitHub never redirected costs no metadata request.
#[tokio::test]
async fn gh_file_read_of_a_standing_repository_reads_no_metadata() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),
            "size":6,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":sha}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    checks::assert_absent(data, "renamed");
    runtime.close().await;
}
