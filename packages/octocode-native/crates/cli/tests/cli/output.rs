use super::*;

#[test]
fn localsearch_emits_structured_results() {
    let workspace = Workspace::new();
    let path = workspace.write("search.rs", "fn needle() {}\n");
    let query = serde_json::json!({"queries":[{
        "matchString": ["needle"],
        "path": path,
        "view": "matchOnly"
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

/// Execution failures stay off the text data stream and print every field of
/// their row as text; `--json` keeps the typed row.
#[test]
fn text_mode_failed_rows_print_their_text_on_stderr() {
    let workspace = Workspace::new();
    let query = serde_json::json!({"queries":[{"path":"missing.rs"}]}).to_string();
    let output = workspace
        .cli()
        .args(["localFetch", &query])
        .output()
        .expect("missing read");
    assert_eq!(exit_code(&output), Some(3));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    let text = stderr(&output);
    assert!(
        serde_json::from_str::<serde_json::Value>(text.trim()).is_err(),
        "{text}"
    );
    for line in ["errorCode: pathNotFound\n", "# status: error"] {
        assert!(text.contains(line), "{text}");
    }
}

#[test]
fn json_mode_errors_go_to_stdout_without_stderr() {
    let workspace = Workspace::new();
    let missing = workspace.workspace.join("missing.rs");
    let query = serde_json::json!({"queries":[{
        "path": missing
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
    // returns as structuredContent.
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
    // Rejection names the actual invalid field without guessing another request.
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("matchstring")),
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
            r#"{"queries":[{"owner":"fixture","repo":"fixture","path":"README","refresh":true}]}"#,
            "--json",
        ]);
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
    assert!(message.contains("[REDACTED-"), "{message}");

    let text_output = workspace
        .cli()
        .args(["config", token.as_str()])
        .output()
        .expect("text argument error");
    assert_eq!(exit_code(&text_output), Some(2));
    assert!(stdout(&text_output).is_empty(), "{}", stdout(&text_output));
    let text = stderr(&text_output);
    assert!(!text.contains(&token));
    assert!(text.contains("[REDACTED-"), "{text}");
}
