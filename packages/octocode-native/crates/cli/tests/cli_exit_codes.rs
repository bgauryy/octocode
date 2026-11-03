//! One shell exit code per tool-call outcome: 0 success (a next page is still
//! success), 1 empty, 2 rejected input, 3 not found, 4
//! authentication/permission, 5 execution failure, 7 rate limited, 130
//! interrupted.

use crate::support;
use serde_json::{Value, json};
use std::process::{Command, Output};
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn run(mut command: Command, tool: &str, input: &Value) -> Output {
    command.args([tool, &input.to_string()]).output().unwrap()
}

fn run_json(mut command: Command, tool: &str, input: &Value) -> Output {
    command
        .args([tool, "--json", &input.to_string()])
        .output()
        .unwrap()
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("JSON output: {error}; {output:?}"))
}

fn first_data(output: &Output) -> Value {
    json_output(output)["results"][0]["data"].clone()
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn search(path: &str, pattern: &str) -> Value {
    json!({"path":path,"matchString":[pattern]})
}

fn fetch(path: &str) -> Value {
    json!({"path":path})
}

#[test]
fn local_rows_exit_by_outcome() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "needle\n");
    let root = file.parent().unwrap().to_string_lossy().into_owned();
    let file = file.to_string_lossy().into_owned();
    let missing = workspace
        .workspace
        .join("src/missing.txt")
        .to_string_lossy()
        .into_owned();
    let lines: String = (1..40).map(|n| format!("line {n}\n")).collect();
    let long = workspace
        .write("long.txt", lines)
        .to_string_lossy()
        .into_owned();
    let page = json!({"path":long});
    let cases = [
        (
            "localSearch",
            json!({"queries":[search(&root, "needle")]}),
            0,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "zzqqxx")]}),
            1,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "zzqqxx"), search(&root, "qqzzxx")]}),
            1,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "needle"), search(&root, "zzqqxx")]}),
            0,
        ),
        // Paging is success: the page's `next.*` is the continuation.
        ("localFetch", json!({"queries":[page.clone()]}), 0),
        ("localFetch", json!({"queries":[fetch(&missing)]}), 3),
        (
            "localFetch",
            json!({"queries":[fetch(&missing), fetch(&missing)]}),
            3,
        ),
        // A failed row is never hidden by an answering one: the worst row
        // decides (error class over a continuation).
        (
            "localFetch",
            json!({"queries":[fetch(&missing), fetch(&file)]}),
            3,
        ),
        ("localFetch", json!({"queries":[fetch(&missing), page]}), 3),
        (
            "localSearch",
            json!({"queries":[search(&root, "needle"), {"path":root,"bogus":1}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[search(&root, "zzqqxx"), {"path":root,"bogus":1}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":root,"bogus":1}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":root,"bogus":1}, {"path":root,"bogus":2}]}),
            2,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":root,"matchString": ["("],"regexEngine":"rust"}]}),
            2,
        ),
    ];
    for (tool, input, expected) in cases {
        let output = run(workspace.cli(), tool, &input);
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{tool} {input}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn invalid_provider_configuration_uses_the_one_config_code() {
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command
        .env("OCTOCODE_STORAGE_MODE", "persistent")
        .env("GITHUB_API_URL", "https://user:pass@example.com/api/v3");
    let output = run_json(
        command,
        "ghCloneRepo",
        &json!({"queries":[{"owner":"a","repo":"b"}]}),
    );
    assert_exit(&output, 5);
    assert_eq!(first_data(&output)["errorCode"], "config");
}

#[test]
fn source_size_gaps_share_the_file_too_large_reason() {
    let workspace = Workspace::new();
    let utf16 = workspace.workspace.join("large-utf16.txt");
    let mut head = vec![0xff, 0xfe];
    for unit in "a".repeat(8192).encode_utf16() {
        head.extend(unit.to_le_bytes());
    }
    std::fs::write(&utf16, head).expect("UTF-16 text sample");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&utf16)
        .expect("open UTF-16 source")
        .set_len(10 * 1024 * 1024 + 2)
        .expect("grow UTF-16 source past the whole-file decode guard");
    let fetched = run_json(
        workspace.cli(),
        "localFetch",
        &json!({"queries":[fetch(&utf16.to_string_lossy())]}),
    );
    assert_exit(&fetched, 5);
    let data = first_data(&fetched);
    assert_eq!(data["errorCode"], "fileTooLarge", "{data}");
    assert_eq!(data["partialReasons"], json!(["fileTooLarge"]), "{data}");

    let searched = workspace.workspace.join("oversize-search.txt");
    std::fs::File::create(&searched)
        .expect("create search source")
        .set_len(20 * 1024 * 1024 + 1)
        .expect("grow search source past the multiline ceiling");
    let output = run_json(
        workspace.cli(),
        "localSearch",
        &json!({"queries":[{"path":searched,"matchString":["needle"],"multiline":"on"}]}),
    );
    assert_exit(&output, 1);
    let data = first_data(&output);
    assert_eq!(data["partialReasons"], json!(["fileTooLarge"]), "{data}");
}

#[test]
fn cli_tool_executes_despite_mcp_local_and_tool_filters() {
    let workspace = Workspace::new();
    let source = workspace.write("source.txt", "verified needle\n");
    let mut command = workspace.cli();
    command
        .env("OCTOCODE_ENABLE_LOCAL", "false")
        .env("TOOLS_TO_RUN", "ghSearchCode")
        .env("DISABLE_TOOLS", "localSearch");
    let output = run(
        command,
        "localSearch",
        &json!({"queries":[search(source.to_str().expect("source path"), "needle")]}),
    );
    assert_exit(&output, 0);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("verified needle"),
        "{output:?}"
    );
}

async fn github(status: u16, headers: &[(&str, &str)]) -> (Workspace, MockServer, Output) {
    let server = MockServer::builder().start().await;
    let mut response = ResponseTemplate::new(status).set_body_json(json!({"message": "fixture"}));
    for (name, value) in headers {
        response = response.insert_header(*name, *value);
    }
    Mock::given(method("GET"))
        .respond_with(response)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .env("GITHUB_TOKEN", &workspace.token);
    let input = json!({"queries":[{"owner":"a","repo":"b","path":"x.rs",
        "ref":"0123456789abcdef0123456789abcdef01234567"}]});
    let output = tokio::task::spawn_blocking(move || run_json(command, "ghGetFileContent", &input))
        .await
        .unwrap();
    (workspace, server, output)
}

#[tokio::test]
async fn github_failures_exit_by_failure_kind() {
    assert_exit(&github(404, &[]).await.2, 3);
    assert_exit(&github(401, &[]).await.2, 4);
    let denied = github(403, &[]).await;
    assert_exit(&denied.2, 4);
    assert_eq!(first_data(&denied.2)["errorCode"], "permissionDenied");
    let limited = [
        ("x-ratelimit-remaining", "0"),
        ("x-ratelimit-reset", "4102444800"),
    ];
    assert_exit(&github(403, &limited).await.2, 7);
}

async fn classification_provider(delay: std::time::Duration) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models":[]})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "model":"resolved",
                    "answers":{"answer":{"type":"noul","noul":0.8}},
                    "usage":{"input_tokens":2,"output_tokens":1}
                }))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

fn clasify_command(workspace: &Workspace, server: &MockServer) -> Command {
    let mut command = workspace.cli();
    command
        .env("OCTOCODE_BETA", "false")
        .env("OCTOCODE_CLASSIFICATION_API", "secret")
        .env("OCTOCODE_CLASSIFICATION_API_HOST", server.uri());
    command
}

/// One clasify row per state or source.
fn rows(inputs: Vec<Value>) -> Value {
    json!({"queries": inputs.into_iter().map(|input| {
        let mut row = json!({"operation":"evaluate","questions":{"q":{"type":"noul","instructions":"Does it define a step?"}}});
        for (key, value) in input.as_object().expect("input object") {
            row[key] = value.clone();
        }
        row
    }).collect::<Vec<_>>()})
}

#[tokio::test]
async fn clasify_exits_by_worst_row_outcome() {
    let server = classification_provider(std::time::Duration::ZERO).await;
    let workspace = Workspace::new();
    let file = workspace
        .write("src/steps.rs", "fn step() -> u32 {\n    1\n}\n")
        .to_string_lossy()
        .into_owned();
    let missing = workspace
        .workspace
        .join("src/nope.rs")
        .to_string_lossy()
        .into_owned();
    let read = |path: &str| json!({"source":{"tool":"localFetch","query":{"path":path}}});
    let state = json!({"content":{"fact":"present"}});
    let cases = [
        (rows(vec![read(&file)]), 0),
        (rows(vec![read(&missing)]), 3),
        (rows(vec![read(&file), read(&missing)]), 3),
        (rows(vec![read(&missing), read(&file)]), 3),
        (rows(vec![state.clone()]), 0),
    ];
    for (input, expected) in cases {
        let command = clasify_command(&workspace, &server);
        let output = tokio::task::spawn_blocking({
            let input = input.clone();
            move || run(command, "clasify", &input)
        })
        .await
        .unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{input}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[tokio::test]
async fn interrupted_call_exits_130() -> Result<(), String> {
    let server = classification_provider(std::time::Duration::from_secs(30)).await;
    let workspace = Workspace::new();
    let mut command = clasify_command(&workspace, &server);
    command
        .args([
            "clasify",
            &rows(vec![json!({"content":{"fact":"present"}})]).to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    let started = std::time::Instant::now();
    while !server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .any(|request| request.method.as_str() == "POST")
    {
        if let Some(status) = child.try_wait().unwrap() {
            return Err(format!(
                "clasify exited before reaching the provider: {status}"
            ));
        }
        if started.elapsed() > std::time::Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("clasify did not reach the provider within 15 seconds".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let interrupted = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(interrupted.success());
    let status = tokio::task::spawn_blocking(move || child.wait().unwrap())
        .await
        .unwrap();
    assert_eq!(status.code(), Some(130));
    Ok(())
}
