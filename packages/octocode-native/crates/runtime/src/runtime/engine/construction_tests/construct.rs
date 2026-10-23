use super::*;
use crate::runtime::engine::construction::startup_diagnostics;

async fn section_call(runtime: &ToolRuntime, query: Value, channel: Channel, token: &str) -> Value {
    let admission = runtime
        .admit("sections".into())
        .expect("admit")
        .with_github_token(SecretString::from(token.to_owned()));
    runtime
        .execute(admission, "clasify".into(), query, channel)
        .await
        .expect("sections response")
        .structured_content
        .into_value()
}

async fn section_fixture(
    home: &std::path::Path,
    mode: &str,
) -> (ToolRuntime, wiremock::MockServer) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::builder().start().await;
    Mock::given(method("POST")).and(path("/v1/systemone"))
            .respond_with(|request: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&request.body).expect("body");
                let answers: serde_json::Map<String, Value> = body["questions"].as_object().expect("questions")
                    .keys().map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
                ResponseTemplate::new(200).set_body_json(json!({"model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
            }).mount(&server).await;
    let runtime = runtime(
        home,
        &[
            ("OCTOCODE_BETA", "true"),
            ("OCTOCODE_CLASSIFICATION_API", "secret"),
            ("OCTOCODE_CLASSIFICATION_API_HOST", &server.uri()),
            ("OCTOCODE_STORAGE_MODE", mode),
        ],
    );
    let mut limits = RuntimeLimits::DEFAULT;
    limits.selection.max_candidates = 1;
    let runtime = ToolRuntime::with_limits(runtime.input.clone(), limits).expect("bounded runtime");
    (runtime, server)
}

fn section_query(home: &std::path::Path) -> Value {
    std::fs::write(
        home.join("first.txt"),
        "First source: restart the service.\n",
    )
    .expect("first");
    std::fs::write(
        home.join("second.txt"),
        "Second source: stop the service.\n",
    )
    .expect("second");
    json!({"queries":[{"id":"sections","operation":"sections","resources":{
            "first":{"tool":"localFetch","query":{"path":home.join("first.txt"),"fullContent":true}},
            "second":{"tool":"localFetch","query":{"path":home.join("second.txt"),"fullContent":true}}
        },"questions":{"restart":{"type":"noul","instructions":"Does this section require a restart?"}}}]})
}

#[tokio::test]
async fn sections_runtime_resumes_replays_and_partitions_access_and_policy() {
    for mode in ["memory", "persistent"] {
        let home = tempfile::tempdir().expect("home");
        let (runtime, server) = section_fixture(home.path(), mode).await;
        let channel = if mode == "memory" {
            Channel::Mcp
        } else {
            Channel::Cli
        };
        let first = section_call(&runtime, section_query(home.path()), channel, "caller-a").await;
        let next = first["queries"][0]["next"]["nextSectionPage"]["query"].clone();
        assert!(next.is_object(), "{first}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        let denied = section_call(&runtime, next.clone(), channel, "caller-b").await;
        assert!(
            denied
                .to_string()
                .contains("classificationContextUnavailable"),
            "{denied}"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        let runtime = if mode == "persistent" {
            let mut changed = runtime.input.clone();
            changed
                .env
                .insert("OCTOCODE_ENABLE_LOCAL".into(), "false".into());
            let other = ToolRuntime::with_limits(changed, runtime.limits).expect("other policy");
            let denied = section_call(&other, next.clone(), channel, "caller-a").await;
            assert!(
                denied
                    .to_string()
                    .contains("classificationContextUnavailable"),
                "{denied}"
            );
            runtime.close().await;
            ToolRuntime::with_limits(runtime.input.clone(), runtime.limits).expect("reopened")
        } else {
            runtime
        };
        let second = section_call(&runtime, next.clone(), channel, "caller-a").await;
        assert_eq!(
            second["queries"][0]["resources"]["second"]["sections"][0]["answers"]["restart"]["status"],
            "answered",
            "{second}"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        let replay = section_call(&runtime, next, channel, "caller-a").await;
        assert_eq!(
            replay["queries"][0]["resources"],
            second["queries"][0]["resources"]
        );
        assert_eq!(replay["queries"][0]["replayed"], true);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
        runtime.close().await;
        if mode == "memory" {
            assert!(!home.path().join("tmp/section-assessments").exists());
        }
    }
}

#[tokio::test]
async fn sections_runtime_refuses_changed_sources_and_one_shot_memory_continuations() {
    let home = tempfile::tempdir().expect("home");
    let (runtime, server) = section_fixture(home.path(), "memory").await;
    let query = section_query(home.path());
    let one_shot = section_call(&runtime, query.clone(), Channel::Cli, "caller").await;
    assert!(one_shot.to_string().contains("unassessed"), "{one_shot}");
    assert!(one_shot["queries"][0].get("next").is_none());
    assert!(server.received_requests().await.unwrap().is_empty());
    let first = section_call(&runtime, query, Channel::Mcp, "caller").await;
    let next = first["queries"][0]["next"]["nextSectionPage"]["query"].clone();
    assert!(next.is_object(), "{first}");
    std::fs::write(home.path().join("second.txt"), "Changed source.").expect("change");
    let changed = section_call(&runtime, next, Channel::Mcp, "caller").await;
    assert!(changed.to_string().contains("sourceMismatch"), "{changed}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    runtime.close().await;
}

#[tokio::test]
async fn sections_runtime_checks_paths_and_sanitizes_before_inference() {
    let home = tempfile::tempdir().expect("home");
    let outside = tempfile::NamedTempFile::new().expect("outside");
    let (runtime, server) = section_fixture(home.path(), "memory").await;
    let secret = format!("ghp_{}", "A".repeat(36));
    let file = home.path().join("secret.txt");
    std::fs::write(&file, format!("Restart with token {secret}.\n")).expect("source");
    let result = section_call(&runtime, json!({"queries":[{"id":"secure","operation":"sections","resources":{
            "secret":{"tool":"localFetch","query":{"path":file,"fullContent":true}},
            "outside":{"tool":"localFetch","query":{"path":outside.path(),"fullContent":true}}
        },"questions":{"check":{"type":"noul","instructions":"Does this section require a restart?"}}}]}), Channel::Mcp, "caller").await;
    assert_eq!(
        result["queries"][0]["resources"]["outside"]["status"], "error",
        "{result}"
    );
    assert_eq!(
        result["queries"][0]["resources"]["secret"]["sections"][0]["answers"]["check"]["status"],
        "answered",
        "{result}"
    );
    let sent = server.received_requests().await.unwrap();
    assert_eq!(sent.len(), 1);
    let body = String::from_utf8(sent[0].body.clone()).unwrap();
    assert!(!body.contains(&secret), "provider received secret");
    assert!(body.contains("REDACTED"), "{body}");
    assert!(!result.to_string().contains(&secret));
    let mut context = ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() - std::time::Duration::from_secs(1),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    };
    assert_eq!(
        runtime
            .dispatcher()
            .capture_local_text(&json!({"path":file}), &context)
            .unwrap_err()
            .code,
        ErrorCode::Timeout
    );
    context.deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    context.cancellation.cancel();
    assert_eq!(
        runtime
            .dispatcher()
            .capture_local_text(&json!({"path":file}), &context)
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
    runtime.close().await;
}

/// Keys no setting reads (unknown or removed) are left to `octocode
/// config`; an ignored value of a live setting prints at every start.
#[test]
fn startup_prints_only_ignored_values_of_live_settings() {
    let diagnostic = |code: &str| config::ConfigDiagnostic {
        severity: config::Severity::Warning,
        code: code.into(),
        field_path: None,
        message: code.into(),
        source_path: None,
    };
    let diagnostics = [
        diagnostic(config::UNKNOWN_CONFIG_CODE),
        diagnostic(config::REMOVED_SETTING_CODE),
        diagnostic("invalid_config"),
        diagnostic("config_load_error"),
    ];
    let printed: Vec<&str> = startup_diagnostics(&diagnostics)
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert_eq!(printed, ["invalid_config", "config_load_error"]);
}
