#![allow(clippy::expect_used, clippy::unwrap_used)]

//! One shell exit code per tool-call outcome: 0 success, 1 empty, 2 rejected
//! input, 3 not found, 4 authentication/permission, 5 execution failure,
//! 6 more to read, 7 rate limited, 130 interrupted.

use crate::support;
use serde_json::{Value, json};
use std::process::{Command, Output};
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn run(mut command: Command, tool: &str, input: &Value) -> Output {
    command.args([tool, &input.to_string()]).output().unwrap()
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
    json!({"path":path,"matchString":pattern})
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
    let page = json!({"path":long,"unit":"lines","length":3});
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
        ("localFetch", json!({"queries":[page.clone()]}), 6),
        ("localFetch", json!({"queries":[fetch(&missing)]}), 3),
        (
            "localFetch",
            json!({"queries":[fetch(&missing), fetch(&missing)]}),
            3,
        ),
        (
            "localFetch",
            json!({"queries":[fetch(&missing), fetch(&file)]}),
            0,
        ),
        ("localFetch", json!({"queries":[fetch(&missing), page]}), 6),
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
            json!({"queries":[{"path":root,"matchString":"(","mode":"regex"}]}),
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
fn disabled_tool_is_an_execution_failure() {
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command.env("OCTOCODE_ENABLE_LOCAL", "false");
    assert_exit(
        &run(
            command,
            "localSearch",
            &json!({"queries":[search(".", "x")]}),
        ),
        5,
    );
}

async fn github(status: u16, headers: &[(&str, &str)]) -> (Workspace, MockServer, Output) {
    let server = MockServer::start().await;
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
        .env("GITHUB_TOKEN", &workspace.token)
        .env("REQUEST_TIMEOUT", support::MOCK_PROVIDER_TIMEOUT_MS);
    let input = json!({"queries":[{"owner":"a","repo":"b","path":"x.rs",
        "ref":"0123456789abcdef0123456789abcdef01234567","forceRefresh":true}]});
    let output = tokio::task::spawn_blocking(move || run(command, "ghGetFileContent", &input))
        .await
        .unwrap();
    (workspace, server, output)
}

#[tokio::test]
async fn github_failures_exit_by_failure_kind() {
    assert_exit(&github(404, &[]).await.2, 3);
    assert_exit(&github(401, &[]).await.2, 4);
    let limited = [
        ("x-ratelimit-remaining", "0"),
        ("x-ratelimit-reset", "4102444800"),
    ];
    assert_exit(&github(403, &limited).await.2, 7);
}

async fn classification_provider(delay: std::time::Duration) -> MockServer {
    let server = MockServer::start().await;
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
        .env("OCTOCODE_CLASSIFICATION_API", "secret")
        .env("OCTOCODE_CLASSIFICATION_API_HOST", server.uri())
        .env("REQUEST_TIMEOUT", support::MOCK_PROVIDER_TIMEOUT_MS);
    command
}

fn matrix(resources: Value) -> Value {
    json!({"queries":[{
        "reasoning":"Triage files.","mainGoal":"Which files define steps.",
        "resources":resources,
        "questions":[{"id":"q","type":"yesno","ask":"Does it define a step?"}]
    }]})
}

#[tokio::test]
async fn clasify_exits_by_resource_outcome() {
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
    let read = |id: &str, path: &str| json!({"id":id,"tool":"localFetch","query":{"path":path,"fullContent":true}});
    let cases = [
        (matrix(json!([read("ok", &file)])), 0),
        (matrix(json!([read("x", &missing)])), 3),
        (matrix(json!([read("ok", &file), read("x", &missing)])), 0),
        (matrix(json!([{"id":"v","value":{"fact":"present"}}])), 0),
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
async fn interrupted_call_exits_130() {
    let server = classification_provider(std::time::Duration::from_secs(30)).await;
    let workspace = Workspace::new();
    let mut command = clasify_command(&workspace, &server);
    command
        .args([
            "clasify",
            &matrix(json!([{"id":"v","value":{"fact":"present"}}])).to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    while server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty()
    {
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
}
