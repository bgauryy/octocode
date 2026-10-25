//! Per-setting effect tests: each setting is written to the home
//! `.octocoderc` (the file layer, not the environment spelling other tests
//! use) and changes what a call through the runtime returns. Flipping the
//! value flips the outcome, so a setting that stops reaching its reader
//! fails here.
use crate::execution::RuntimeLimits;
use crate::response::outcome::ToolOutcome;
use crate::runtime::{Channel, ToolRuntime};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{ConfigInput, FileInput, RuntimeSurface};

/// A runtime whose home `.octocoderc` holds `rc`, run from `cwd`.
fn runtime(home: &Path, cwd: &Path, rc: &Value) -> ToolRuntime {
    runtime_with(home, cwd, rc, RuntimeLimits::DEFAULT)
}

/// [`runtime`] under the internal `limits` instead of the constants.
fn runtime_with(home: &Path, cwd: &Path, rc: &Value, limits: RuntimeLimits) -> ToolRuntime {
    let env = BTreeMap::from([(
        super::environment::OCTOCODE_HOME.to_owned(),
        home.to_string_lossy().into_owned(),
    )]);
    ToolRuntime::with_limits(
        ConfigInput {
            env,
            cwd: cwd.to_path_buf(),
            os_home: home.to_path_buf(),
            global_env: FileInput::Missing {
                path: home.join(".env"),
            },
            project_env: FileInput::Missing {
                path: cwd.join(".octocode/.env"),
            },
            config_file: FileInput::Read {
                path: home.join(".octocoderc"),
                text: rc.to_string(),
            },
            project_config_file: FileInput::Missing {
                path: cwd.join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Mcp,
        },
        limits,
    )
    .expect("runtime")
}

async fn call(runtime: &ToolRuntime, tool: &str, row: Value) -> ToolOutcome {
    let admission = runtime.admit("effect".into()).expect("admitted");
    runtime
        .execute(
            admission,
            tool.into(),
            json!({"queries":[row]}),
            Channel::Mcp,
        )
        .await
        .expect("call")
}

fn data(outcome: &ToolOutcome) -> &Value {
    &outcome.structured_content["results"][0]["data"]
}

/// `local.workspaceRoot` is where a relative path resolves, not the cwd.
#[tokio::test]
async fn workspace_root_anchors_relative_paths() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let root = tempfile::tempdir().expect("root");
    std::fs::write(cwd.path().join("a.txt"), "from-cwd\n").expect("cwd file");
    std::fs::write(root.path().join("a.txt"), "from-root\n").expect("root file");
    let content = |rc: Value| {
        let runtime = runtime(home.path(), cwd.path(), &rc);
        async move {
            let outcome = call(&runtime, "localFetch", json!({"path":"a.txt"})).await;
            data(&outcome)["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        }
    };
    let configured = content(json!({"local":{
        "workspaceRoot":root.path(),
        "allowedPaths":[root.path(), cwd.path()]
    }}))
    .await;
    assert!(configured.contains("from-root"), "{configured}");
    let unset = content(json!({})).await;
    assert!(unset.contains("from-cwd"), "{unset}");
}

/// `output.redactEmails` masks addresses in GitHub rows.
#[tokio::test]
async fn redact_emails_masks_github_rows() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[{
                "full_name":"o/r","name":"r","owner":{"login":"o"},
                "stargazers_count":1,"forks_count":0,"html_url":"https://github.com/o/r",
                "default_branch":"main","private":false,"archived":false,"fork":false,
                "topics":[],"description":"mail maintainer@example.com","language":"Rust"}]})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let api = format!("{}/api/v3", server.uri());
    let shown = |redact: bool| {
        let rc = json!({"github":{"apiUrl":api},"output":{"redactEmails":redact}});
        let runtime = runtime(home.path(), cwd.path(), &rc);
        async move {
            let outcome = call(
                &runtime,
                "ghSearchRepo",
                json!({"operation":"search","keywords":["r"]}),
            )
            .await;
            outcome.structured_content.to_string()
        }
    };
    let plain = shown(false).await;
    assert!(plain.contains("maintainer@example.com"), "{plain}");
    let redacted = shown(true).await;
    assert!(!redacted.contains("maintainer@example.com"), "{redacted}");
    assert!(redacted.contains("mail "), "the row stays: {redacted}");
}

/// The runtime's rate-limit wait decides whether a short provider rate limit
/// is waited out (the retry answers) or fails fast with `rateLimited`.
#[tokio::test]
async fn max_rate_limit_wait_decides_wait_or_fail_fast() {
    let outcome = |wait_ms: u64| async move {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("retry-after", "1")
                    .set_body_json(json!({"message":"You have exceeded a secondary rate limit."})),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":0,"incomplete_results":false,"items":[]})))
            .mount(&server)
            .await;
        let home = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        let rc = json!({"github":{"apiUrl":format!("{}/api/v3", server.uri())}});
        let limits = RuntimeLimits {
            max_rate_limit_wait: std::time::Duration::from_millis(wait_ms),
            ..RuntimeLimits::DEFAULT
        };
        let runtime = runtime_with(home.path(), cwd.path(), &rc, limits);
        let outcome = call(
            &runtime,
            "ghSearchRepo",
            json!({"operation":"search","keywords":["r"]}),
        )
        .await;
        outcome.structured_content["results"][0].clone()
    };
    let limited = outcome(0).await;
    assert_eq!(
        limited["data"]["errorCode"],
        json!("rateLimited"),
        "{limited}"
    );
    let waited = outcome(5_000).await;
    assert_eq!(waited["status"], "empty", "waited and answered: {waited}");
}

/// `lsp.trustProjectConfig` reaches the language-server settings every
/// lspSearch call and lead discovery reads.
#[test]
fn trust_project_config_reaches_server_discovery() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    for trusted in [true, false] {
        let rc = json!({"lsp":{"trustProjectConfig":trusted}});
        let runtime = runtime(home.path(), cwd.path(), &rc);
        let settings = runtime.lsp_execution_config();
        assert_eq!(settings.trust_project_config, trusted);
        assert_eq!(settings.discovery(None).trust_project_config, trusted);
    }
}

#[test]
fn lsp_trace_reaches_execution_without_changing_discovery() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    for enabled in [true, false] {
        let runtime = runtime(home.path(), cwd.path(), &json!({"lsp":{"trace":enabled}}));
        assert_eq!(runtime.lsp_execution_config().trace, enabled);
        assert!(!runtime.lsp_execution_config().trust_project_config);
    }
}
