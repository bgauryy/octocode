//! Provider outcomes require usable continuations before their exit class is reported.
use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn github(
    status: u16,
    headers: &[(&str, &str)],
    resumable: bool,
) -> (Workspace, MockServer, Output) {
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
    let persistent = workspace.persistent_cli();
    let mut command = if resumable {
        persistent.cli()
    } else {
        workspace.cli()
    };
    command
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .env("GITHUB_TOKEN", &workspace.token);
    let input = json!({"queries":[{"owner":"a","repo":"b","path":"x.rs",
        "ref":"0123456789abcdef0123456789abcdef01234567"}]});
    let output = tokio::task::spawn_blocking(move || run_json(command, "ghGetFileContent", &input))
        .await
        .unwrap();
    drop(persistent);
    (workspace, server, output)
}

#[tokio::test]
async fn github_failures_exit_by_failure_kind() {
    assert_exit(&github(404, &[], true).await.2, 3);
    assert_exit(&github(401, &[], true).await.2, 4);
    let denied = github(403, &[], true).await;
    assert_exit(&denied.2, 4);
    assert_eq!(first_data(&denied.2)["errorCode"], "permissionDenied");
    let limited = [
        ("x-ratelimit-remaining", "0"),
        ("x-ratelimit-reset", "4102444800"),
    ];
    assert_exit(&github(403, &limited, true).await.2, 7);
    // A retry is retained work too: a one-shot memory call cannot publish it.
    let memory = github(403, &limited, false).await;
    assert_exit(&memory.2, 5);
    let error = json_output(&memory.2);
    assert_eq!(error["errorCode"], "executionFailed", "{error}");
    assert!(!error.to_string().contains("cursor2."), "{error}");
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

fn clasify_command(mut command: Command, server: &MockServer) -> Command {
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
    let persistent = workspace.persistent_cli();
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
        // Source-backed answers retain next.read; preserve that usable lifetime
        // while checking the row's exit class, including mixed failed batches.
        let command = clasify_command(persistent.cli(), &server);
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
    let memory_server = classification_provider(std::time::Duration::ZERO).await;
    let command = clasify_command(workspace.cli(), &memory_server);
    let input = rows(vec![read(&file)]);
    let memory = tokio::task::spawn_blocking(move || run_json(command, "clasify", &input))
        .await
        .unwrap();
    assert_exit(&memory, 5);
    let error = json_output(&memory);
    assert_eq!(
        error["results"][0]["data"]["errorCode"], "classificationContextUnavailable",
        "{error}"
    );
    assert!(!error.to_string().contains("cursor2."), "{error}");
    assert!(
        memory_server.received_requests().await.unwrap().is_empty(),
        "memory-only source refusal must precede every provider request"
    );
}

#[tokio::test]
async fn interrupted_call_exits_130() -> Result<(), String> {
    let server = classification_provider(std::time::Duration::from_secs(30)).await;
    let workspace = Workspace::new();
    let mut command = clasify_command(workspace.cli(), &server);
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
