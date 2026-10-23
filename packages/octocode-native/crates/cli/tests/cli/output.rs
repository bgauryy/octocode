use super::*;

#[test]
fn tool_output_is_compact_json_on_a_pipe() {
    let workspace = Workspace::new();
    let query = serde_json::json!({"queries":[{
        "id":"decision",
        "mainGoal": "test", "reasoning":"Exercise output formatting.",
        "state":{"fact":"present"},
        "questions":{"relevant":{"type":"noul","instructions":"Is it relevant?"}}
    }]})
    .to_string();
    let compact = workspace
        .cli()
        .args(["clasify", &query, "--json"])
        .output()
        .unwrap();
    assert_eq!(
        stdout(&compact).trim_end().lines().count(),
        1,
        "{}",
        stdout(&compact)
    );
    let forced = workspace
        .cli()
        .args(["clasify", &query, "--json"])
        .output()
        .unwrap();
    assert_eq!(stdout(&forced), stdout(&compact));
    for retired in ["--compact", "--pretty"] {
        let output = workspace
            .cli()
            .args(["clasify", &query, retired, "--json"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "tool commands reject {retired}"
        );
    }
}

#[test]
fn localsearch_emits_structured_results() {
    let workspace = Workspace::new();
    let path = workspace.write("search.rs", "fn needle() {}\n");
    let query = serde_json::json!({"queries":[{
        "matchString": "needle",
        "path": path,
        "resultView": "matchOnly",
        "mainGoal": "test", "reasoning": "Verify structured lexical output."
    }]})
    .to_string();
    let output = workspace
        .cli()
        .args(["localSearch", &query, "--json"])
        .output()
        .expect("localSearch");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(stdout(&output)).expect("one JSON document");
    let data = &value["results"][0]["data"];
    // A complete single page carries no stats: the listed rows are the counts.
    assert!(data.get("stats").is_none(), "{data}");
    assert_eq!(data["files"][0]["matches"][0]["value"], "needle", "{data}");
}

/// A call whose every row failed prints its JSON (the same as `--json`) on
/// stderr, so a pipe never reads an error as data; its repair lead rides
/// in the row's hints.
#[test]
fn text_mode_failed_rows_print_their_json_on_stderr() {
    let workspace = Workspace::new();
    let file = workspace.workspace.join("short.rs");
    std::fs::write(&file, "fn a() {}\nfn b() {}\n").expect("fixture");
    let query = serde_json::json!({"path": file, "ranges": ["50-60"]}).to_string();
    let output = workspace
        .cli()
        .args(["localFetch", &query])
        .output()
        .expect("out-of-range read");
    let text = stderr(&output);
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let value: serde_json::Value = serde_json::from_str(text.trim()).expect("one JSON document");
    let data = &value["results"][0]["data"];
    assert_eq!(data["errorCode"], "invalidInput", "{value}");
    assert!(
        data["hints"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .any(|hint| hint.starts_with("repair: localFetch ")),
        "{value}"
    );
}

#[test]
fn json_mode_errors_go_to_stdout_without_stderr() {
    let workspace = Workspace::new();
    let missing = workspace.workspace.join("missing.rs");
    let query = serde_json::json!({"queries":[{
        "path": missing,
        "mainGoal": "test", "reasoning": "Verify native read errors."
    }]})
    .to_string();
    let output = workspace
        .cli()
        .args(["localFetch", &query, "--json"])
        .output()
        .expect("missing read");
    assert_eq!(exit_code(&output), Some(3));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_eq!(value["results"][0]["data"]["errorCode"], "pathNotFound");
    assert!(
        stderr(&output).is_empty(),
        "duplicate stderr: {}",
        stderr(&output)
    );

    let malformed = workspace
        .cli()
        .args(["localSearch", "{", "--json"])
        .output()
        .expect("malformed raw JSON");
    assert_eq!(exit_code(&malformed), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&malformed)).expect("machine-readable parse error");
    assert_bare_error(&error);
    assert_eq!(error["errorCode"], "invalidInput", "{error}");
    assert!(stderr(&malformed).is_empty(), "{}", stderr(&malformed));

    // A schema-invalid query answers with the same typed envelope MCP
    // returns as structuredContent, repair details kept.
    let typo = workspace
        .cli()
        .args([
            "localFetch",
            r#"{"queries":[{"path":"a.rs","matchstring":"x"}]}"#,
            "--json",
        ])
        .output()
        .expect("schema-invalid query");
    assert_eq!(exit_code(&typo), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&typo)).expect("machine-readable input error");
    assert_bare_error(&error);
    assert_eq!(error["errorCode"], "invalidInput", "{error}");
    // The single repair detail is the error itself; no extra details.
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("did you mean 'matchString'?")),
        "{error}"
    );
    assert!(error.get("details").is_none(), "{error}");

    let unknown = workspace
        .cli()
        .args(["notACommand", "--json"])
        .output()
        .expect("unknown subcommand");
    assert_eq!(exit_code(&unknown), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&unknown)).expect("machine-readable clap error");
    assert_bare_error(&error);
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("notACommand")),
        "{error}"
    );
    assert!(stderr(&unknown).is_empty(), "{}", stderr(&unknown));

    // A mistyped tool keeps clap's did-you-mean names in the envelope.
    let typo = workspace
        .cli()
        .args(["lokalSearch", "{}", "--json"])
        .output()
        .expect("typo subcommand");
    assert_eq!(exit_code(&typo), Some(2));
    let error: serde_json::Value = serde_json::from_str(stdout(&typo)).expect("JSON error");
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("localSearch")),
        "{error}"
    );

    // Retired global flags are argument errors, not aliases.
    for flag in ["--json-errors", "--redact-emails", "--no-color"] {
        let retired = workspace
            .cli()
            .args([flag, "localSearch", "{}"])
            .output()
            .expect("retired flag");
        assert_eq!(exit_code(&retired), Some(2), "{flag}");
    }
}

#[tokio::test]
async fn github_authentication_failure_uses_exit_four_and_actionable_hint() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "message": "Bad credentials"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .env("GITHUB_TOKEN", "invalid-fixture-token")
        .args([
            "ghGetFileContent",
            r#"{"queries":[{"owner":"fixture","repo":"fixture","path":"README","forceRefresh":true,"mainGoal":"Read the fixture README.","reasoning":"Exercise the authentication failure path."}]}"#, "--json"]);
    let output = tokio::task::spawn_blocking(move || command.output().expect("tool output"))
        .await
        .expect("tool worker");
    assert_eq!(output.status.code(), Some(4), "{}", stdout(&output));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
    assert_eq!(result["results"][0]["data"]["errorCode"], "authentication");
    assert!(stdout(&output).contains("octocode auth login"));
    assert!(stdout(&output).contains("invalid env token"));
    assert!(!stdout(&output).contains("invalid-fixture-token"));
}

#[test]
fn argument_parse_errors_redact_secrets_in_json_and_text() {
    let workspace = Workspace::new();
    let token = format!("ghp_{}", "a".repeat(37));
    let json_output = workspace
        .cli()
        .args(["localFetch", "{}", token.as_str(), "--json"])
        .output()
        .expect("JSON argument error");
    assert_eq!(exit_code(&json_output), Some(2));
    assert!(stderr(&json_output).is_empty(), "{}", stderr(&json_output));
    assert!(!stdout(&json_output).contains(&token));
    let error: serde_json::Value = serde_json::from_str(stdout(&json_output)).expect("error JSON");
    assert_bare_error(&error);
    assert_eq!(error["errorCode"], "invalidInput");
    let message = error["error"].as_str().expect("error message");
    assert!(message.starts_with("unexpected argument '"), "{message}");
    assert!(message.contains("[REDACTED-"), "{message}");
    assert!(message.contains("' found"), "{message}");

    let text_output = workspace
        .cli()
        .args(["config", token.as_str()])
        .output()
        .expect("text argument error");
    assert_eq!(exit_code(&text_output), Some(2));
    assert!(stdout(&text_output).is_empty(), "{}", stdout(&text_output));
    let text = stderr(&text_output);
    assert!(!text.contains(&token));
    assert!(
        text.starts_with("error: unrecognized subcommand '"),
        "{text}"
    );
    assert!(text.contains("[REDACTED-"), "{text}");
    assert!(
        text.contains("\n\nUsage: octocode config [OPTIONS]\n"),
        "{text}"
    );
    assert!(
        text.ends_with("For more information, try '--help'.\n"),
        "{text}"
    );
}
