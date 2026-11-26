//! Provider outcomes require usable continuations before their exit class is reported.
use super::*;
use octocode_test_support::json_fixture;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn github(
    status: u16,
    headers: &[(&str, &str)],
    resumable: bool,
) -> (Workspace, MockServer, Output) {
    let server = MockServer::builder().start().await;
    let mut response =
        ResponseTemplate::new(status).set_body_json(json_fixture(r#"{"message": "fixture"}"#));
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
    let input = json_fixture(
        r#"{
            "queries": [
                {
                    "owner": "a",
                    "repo": "b",
                    "path": "x.rs",
                    "ref": "0123456789abcdef0123456789abcdef01234567"
                }
            ]
        }"#,
    );
    let output = tokio::task::spawn_blocking(move || run_json(command, "ghGetFileContent", &input))
        .await
        .unwrap();
    drop(persistent);
    (workspace, server, output)
}

#[tokio::test]
async fn github_failures_exit_by_failure_kind() {
    assert_exit(&github(404, &[], true).await.2, 2);
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"models": []}"#)))
        .mount(&server)
        .await;
    // Each question answered under its own key and type, as the provider
    // does: a text judged whole also asks where its answer sits.
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(move |request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
            let answers = body["questions"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(id, question)| {
                    let answer = match question["criteria"].as_object() {
                        Some(labels) if question["type"] == "choice" => {
                            let first = labels.keys().next().cloned().unwrap_or_default();
                            let probabilities = labels
                                .keys()
                                .map(|label| (label.clone(), json!(if *label == first { 1.0 } else { 0.0 })))
                                .collect::<serde_json::Map<_, _>>();
                            json!({"type":"choice","choice":first,"confidence":1,"probabilities":probabilities})
                        }
                        _ => json_fixture(r#"{"type": "noul", "noul": 0.8}"#),
                    };
                    (id.clone(), answer)
                })
                .collect::<serde_json::Map<_, _>>();
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "model":"resolved",
                    "answers":answers,
                    "usage":{"input_tokens":2,"output_tokens":1}
                }))
                .set_delay(delay)
        })
        .mount(&server)
        .await;
    server
}

fn clasify_command(mut command: Command, server: &MockServer) -> Command {
    command
        .env("OCTOCODE_BETA", "false")
        .env("OCTOCODE_TYPESAFE_KEY", "secret")
        .env("OCTOCODE_TYPESAFE_HOST", server.uri());
    command
}

/// One clasify row per state or source.
fn rows(inputs: Vec<Value>) -> Value {
    json!({"queries": inputs.into_iter().map(|input| {
        let mut row = json_fixture(r#"{
            "operation": "evaluate",
            "questions": {
                "q": {"type": "noul", "instructions": "Does it define a step?"}
            }
        }"#);
        for (key, value) in input.as_object().expect("input object") {
            row[key] = value.clone();
        }
        row
    }).collect::<Vec<_>>()})
}

#[tokio::test]
async fn clasify_exits_by_worst_row_outcome_and_states_its_ceiling() {
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
    let state = json_fixture(r#"{"content": {"fact": "present"}}"#);
    let cases = [
        (rows(vec![read(&file)]), 0),
        (rows(vec![read(&missing)]), 2),
        (rows(vec![read(&file), read(&missing)]), 2),
        (rows(vec![read(&missing), read(&file)]), 2),
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
    // A one-shot memory runtime answers without an immutable source handle.
    // Its answer lines and the known source identity form a fresh read.
    let memory_server = classification_provider(std::time::Duration::ZERO).await;
    let command = clasify_command(workspace.cli(), &memory_server);
    let input = rows(vec![read(&file)]);
    let memory = tokio::task::spawn_blocking(move || run_json(command, "clasify", &input))
        .await
        .unwrap();
    assert_exit(&memory, 0);
    let answered = json_output(&memory);
    let data = &answered["results"][0]["data"];
    assert_eq!(data["answers"]["q"]["noul"], 0.8, "{answered}");
    // A summary states the requests sent, the tokens they cost and the
    // ceiling: the one request and the row's 3 shared transient retries (the
    // CLI's default retry count).
    assert_eq!(
        (&data["usage"]["calls"], &data["usage"]["requestsCeiling"]),
        (&serde_json::json!(1), &serde_json::json!(4)),
        "{answered}"
    );
    for tokens in ["inputTokens", "outputTokens"] {
        assert!(data["usage"][tokens].as_u64() > Some(0), "{answered}");
    }
    assert!(data.get("related").is_none(), "{answered}");
    assert!(data.get("next").is_none(), "{answered}");
    assert!(!answered.to_string().contains("cursor"), "{answered}");
    let lines = data["answers"]["q"]["lines"].clone();
    assert_eq!(
        lines,
        json_fixture(r#"{"startLine": 1, "endLine": 1}"#),
        "{answered}"
    );
    let fresh = run_json(
        workspace.cli(),
        "localFetch",
        &json!({"queries":[{"path":file,"ranges":[lines]}]}),
    );
    assert_exit(&fresh, 0);
    let source = json_output(&fresh);
    assert_eq!(
        source["results"][0]["data"]["content"]
            .as_str()
            .expect("fresh source")
            .lines()
            .collect::<Vec<_>>(),
        ["1\tfn step() -> u32 {"]
    );
    let posts = memory_server.received_requests().await.unwrap();
    let posts = posts
        .iter()
        .filter(|request| request.method.as_str() == "POST");
    assert_eq!(posts.count(), 1, "one provider call answers the row");
    // A section plan needs its next batch kept, so memory refuses it before
    // any read or provider request.
    let plan_server = classification_provider(std::time::Duration::ZERO).await;
    let command = clasify_command(workspace.cli(), &plan_server);
    let input = rows(vec![json!({"operation":"sections",
        "resources":{"local":{"tool":"localFetch","query":{"path":&file}}}})]);
    let plan = tokio::task::spawn_blocking(move || run_json(command, "clasify", &input))
        .await
        .unwrap();
    assert_exit(&plan, 5);
    let error = json_output(&plan);
    assert_eq!(
        error["results"][0]["data"]["errorCode"], "classificationContextUnavailable",
        "{error}"
    );
    assert!(!error.to_string().contains("cursor2."), "{error}");
    assert!(
        plan_server.received_requests().await.unwrap().is_empty(),
        "a memory-only section-plan refusal must precede every provider request"
    );
}

/// `clasify --content -` sends stdin, byte for byte, as the one row's
/// `content`, and the answer names its deciding lines of that text. A row
/// that names its own input is refused before any provider request.
#[tokio::test]
async fn clasify_reads_a_rows_content_from_stdin() {
    use std::io::Write as _;
    let server = classification_provider(std::time::Duration::ZERO).await;
    let workspace = Workspace::new();
    let piped = |query: Value, stdin: &'static str| {
        let mut command = clasify_command(workspace.cli(), &server);
        command
            .args(["clasify", "--content", "-", &query.to_string(), "--json"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        move || {
            let mut child = command.spawn().unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        }
    };
    let text = "test a ... ok\ntest b ... FAILED\n";
    let output = tokio::task::spawn_blocking(piped(rows(vec![json!({})]), text))
        .await
        .unwrap();
    assert_exit(&output, 0);
    let answered = json_output(&output);
    let data = &answered["results"][0]["data"];
    assert_eq!(
        data["answers"]["q"],
        json_fixture(r#"{"noul": 0.8, "lines": {"startLine": 1, "endLine": 1}}"#),
        "{answered}"
    );
    let posts = || async {
        server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| request.method.as_str() == "POST")
            .collect::<Vec<_>>()
    };
    let body: Value = serde_json::from_slice(&posts().await[0].body).unwrap();
    assert_eq!(
        body["state"],
        json!(
            "<lines 1-1>\ntest a ... ok\n</lines 1-1>\n<lines 2-2>\ntest b ... FAILED\n</lines 2-2>\n"
        )
    );
    let refused = tokio::task::spawn_blocking(piped(
        rows(vec![json_fixture(
            r#"{
                "source": {"tool": "localFetch", "query": {"path": "a.txt"}}
            }"#,
        )]),
        text,
    ))
    .await
    .unwrap();
    assert_exit(&refused, 2);
    let stdout = String::from_utf8_lossy(&refused.stdout);
    assert!(stdout.contains("drop its `source`"), "{stdout}");
    assert_eq!(posts().await.len(), 1, "the refusal sent nothing");
}

#[tokio::test]
async fn interrupted_call_exits_130() -> Result<(), String> {
    let server = classification_provider(std::time::Duration::from_secs(30)).await;
    let workspace = Workspace::new();
    let mut command = clasify_command(workspace.cli(), &server);
    command
        .args([
            "clasify",
            &rows(vec![json_fixture(r#"{"content": {"fact": "present"}}"#)]).to_string(),
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
