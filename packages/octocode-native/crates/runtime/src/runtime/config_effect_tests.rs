//! Per-setting effect tests: each setting is written to the home
//! `.octocoderc` (the file layer, not the environment spelling other tests
//! use) and changes what a call through the runtime returns. Flipping the
//! value flips the outcome, so a setting that stops reaching its reader
//! fails here.
use crate::runtime::{Channel, ToolRuntime};
use octocode_runtime_base::execution::RuntimeLimits;
use octocode_runtime_base::response::outcome::ToolOutcome;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use octocode_runtime_base::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_test_support::json_fixture;

/// A runtime whose home `.octocoderc` holds `rc`, run from `cwd`.
fn runtime(home: &Path, cwd: &Path, rc: &Value) -> ToolRuntime {
    runtime_with(home, cwd, rc, RuntimeLimits::DEFAULT)
}

/// [`runtime`] under the internal `limits` instead of the constants.
fn runtime_with(home: &Path, cwd: &Path, rc: &Value, limits: RuntimeLimits) -> ToolRuntime {
    // An MCP host lists the local tools only once it turns them on.
    let env = BTreeMap::from([
        (
            octocode_runtime_base::config::environment::OCTOCODE_HOME.to_owned(),
            home.to_string_lossy().into_owned(),
        ),
        ("OCTOCODE_ENABLE_LOCAL".to_owned(), "true".to_owned()),
    ]);
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
            let outcome = call(&runtime, "localFetch", json_fixture(r#"{"path": "a.txt"}"#)).await;
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{
                "total_count": 1,
                "incomplete_results": false,
                "items": [
                    {
                        "full_name": "o/r",
                        "name": "r",
                        "owner": {"login": "o"},
                        "stargazers_count": 1,
                        "forks_count": 0,
                        "html_url": "https://github.com/o/r",
                        "default_branch": "main",
                        "private": false,
                        "archived": false,
                        "fork": false,
                        "topics": [],
                        "description": "mail maintainer@example.com",
                        "language": "Rust"
                    }
                ]
            }"#,
        )))
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
                json_fixture(r#"{"operation": "search", "keywords": ["r"]}"#),
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

/// `output.redactUuid` masks UUIDs in a local tool's rows; off, they read.
#[tokio::test]
async fn redact_uuid_masks_uuids_in_local_rows() {
    const ID: &str = "123e4567-e89b-12d3-a456-426614174000";
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    std::fs::write(cwd.path().join("ids.txt"), format!("request {ID} done\n")).expect("file");
    let content = |redact: bool| {
        let rc = json!({"output":{"redactUuid":redact}});
        let runtime = runtime(home.path(), cwd.path(), &rc);
        async move {
            let outcome = call(
                &runtime,
                "localFetch",
                json_fixture(r#"{"path": "ids.txt"}"#),
            )
            .await;
            data(&outcome)["content"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        }
    };
    let plain = content(false).await;
    assert!(plain.contains(ID), "{plain}");
    let masked = content(true).await;
    assert!(!masked.contains(ID), "{masked}");
    assert!(masked.contains("request [REDACTED-UUID] done"), "{masked}");
}

/// A GitHub read of a credential-named file follows the local rule: refused
/// with `pathPolicyDenied` before any request by default; `security.
/// credentialFiles: allow` opens it with its values redacted and nothing of
/// it written under the home. A sensitive directory never opens.
#[tokio::test]
async fn credential_files_setting_governs_github_file_reads() {
    use base64::Engine as _;
    let server = MockServer::builder().start().await;
    let sha = "2222222222222222222222222222222222222222";
    let token = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
    let text = format!("TOKEN={token}\nDB_PASS=CowSaysMoo\nneedle_line=1\n");
    for file in [".env", ".ssh/id_rsa"] {
        octocode_test_support::mock_http::list_in_trees(&server, "o", "r", file).await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/o/r/contents/{file}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","sha":"3333333333333333333333333333333333333333",
                "content":base64::engine::general_purpose::STANDARD.encode(&text)})))
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let api = format!("{}/api/v3", server.uri());
    let read = |allow: bool, file: &'static str| {
        let mut rc = json!({"github":{"apiUrl":api}});
        if allow {
            rc["security"] = json_fixture(r#"{"credentialFiles": "allow"}"#);
        }
        let runtime = runtime(home.path(), cwd.path(), &rc);
        async move {
            let outcome = call(
                &runtime,
                "ghGetFileContent",
                json!({"owner":"o","repo":"r","path":file,"ref":sha}),
            )
            .await;
            data(&outcome).clone()
        }
    };
    for file in [".env", ".ssh/id_rsa"] {
        let refused = read(false, file).await;
        assert_eq!(refused["errorCode"], "pathPolicyDenied", "{refused}");
        assert!(
            refused
                .to_string()
                .contains("withheld by the security path policy"),
            "{refused}"
        );
    }
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path().contains("/contents/"))
            .count(),
        0,
        "refused before any request"
    );
    let opened = read(true, ".env").await;
    let shown = opened.to_string();
    // A dotenv file is a known credential store: every value goes, each name
    // stays.
    assert!(shown.contains("needle_line="), "{opened}");
    for (what, secret) in [
        ("the TOKEN value", token.as_str()),
        ("the DB_PASS value", "CowSaysMoo"),
    ] {
        assert!(!shown.contains(secret), "{what} leaked: {opened}");
    }
    assert!(shown.contains("DB_PASS="), "{opened}");
    let still = read(true, ".ssh/id_rsa").await;
    assert_eq!(still["errorCode"], "pathPolicyDenied", "{still}");
    let on_disk = walkdir_text(home.path());
    assert!(
        !on_disk.contains("needle_line="),
        "an opened credential file is kept in memory only"
    );
}

/// Every regular file's text under `root`, concatenated (lossy).
fn walkdir_text(root: &Path) -> String {
    let mut text = String::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                text.push_str(&String::from_utf8_lossy(&bytes));
            }
        }
    }
    text
}

/// A provider rate limit that ends inside the call's deadline is waited out
/// (the retry answers); no fixed wait cap turns it into `rateLimited`.
#[tokio::test]
async fn a_rate_limit_inside_the_deadline_is_waited_out() {
    let outcome = || async move {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("retry-after", "1")
                    .set_body_json(json_fixture(
                        r#"{"message": "You have exceeded a secondary rate limit."}"#,
                    )),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/search/repositories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
                r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
            )))
            .mount(&server)
            .await;
        let home = tempfile::tempdir().expect("home");
        let cwd = tempfile::tempdir().expect("cwd");
        let rc = json!({"github":{"apiUrl":format!("{}/api/v3", server.uri())}});
        let runtime = runtime_with(home.path(), cwd.path(), &rc, RuntimeLimits::DEFAULT);
        let outcome = call(
            &runtime,
            "ghSearchRepo",
            json_fixture(r#"{"operation": "search", "keywords": ["r"]}"#),
        )
        .await;
        outcome.structured_content["results"][0].clone()
    };
    let started = std::time::Instant::now();
    let waited = outcome().await;
    assert_eq!(waited["status"], "empty", "waited and answered: {waited}");
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));
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
        assert_eq!(settings.discovery().trust_project_config, trusted);
    }
}

/// `lsp.trustFetchedCode` empties the roots discovery refuses; unset, they are
/// the directories local tools read fetched code from.
#[test]
fn trust_fetched_code_reaches_server_discovery() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let roots = |rc: Value| {
        runtime(home.path(), cwd.path(), &rc)
            .lsp_execution_config()
            .discovery()
            .fetched_code_roots
    };
    let refused = roots(json!({}));
    for root in [
        octocode_tool_github::gh_clone_repo::storage::clones_root(home.path()),
        octocode_tool_github::gh_structure::materialize::materialize_root(home.path()),
    ] {
        assert!(refused.contains(&root), "{root:?} in {refused:?}");
    }
    assert!(roots(json_fixture(r#"{"lsp": {"trustFetchedCode": true}}"#)).is_empty());
}

/// `security.credentialFiles: allow` opens credential-named files to local
/// reads and searches, with secret values still redacted, and keeps saved work
/// off disk. Graphs and classifier reads never read them.
#[tokio::test]
async fn credential_files_setting_opens_local_reads_only() {
    let home = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let token = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
    std::fs::write(
        cwd.path().join(".env"),
        format!("TOKEN={token}\nneedle=1\n"),
    )
    .expect("env");
    std::fs::write(cwd.path().join("a.ts"), "export const a = 1;\n").expect("source");
    let run = |rc: Value| {
        let runtime = runtime(home.path(), cwd.path(), &rc);
        async move {
            let fetched = call(&runtime, "localFetch", json_fixture(r#"{"path": ".env"}"#)).await;
            let found = call(
                &runtime,
                "localSearch",
                json_fixture(
                    r#"{
                        "path": ".",
                        "matchString": "needle",
                        "includeHidden": true
                    }"#,
                ),
            )
            .await;
            let built = call(&runtime, "graphBuild", json_fixture(r#"{"path": "."}"#)).await;
            (
                fetched.structured_content["results"][0].clone(),
                found.structured_content["results"][0].clone(),
                built.structured_content["results"][0].clone(),
            )
        }
    };
    let (fetched, found, _) = run(json_fixture(r#"{"local": {"beta": true}}"#)).await;
    assert_eq!(
        fetched["data"]["errorCode"], "pathPolicyDenied",
        "{fetched}"
    );
    // Withheld by default: an empty row with no files, the credential file
    // disclosed in skippedPaths.
    assert_eq!(found["status"], "empty", "{found}");
    assert_eq!(found["data"]["files"], json!([]), "{found}");
    assert_eq!(
        found["data"]["skippedPaths"]["entries"][0]["cause"], "withheld",
        "{found}"
    );

    let (fetched, found, built) = run(json_fixture(
        r#"{
            "local": {"beta": true},
            "security": {"credentialFiles": "allow"}
        }"#,
    ))
    .await;
    let content = fetched["data"]["content"].as_str().unwrap_or_default();
    // A dotenv file's every value goes; its names stay.
    assert!(content.contains("needle=[REDACTED-"), "{fetched}");
    assert!(
        !content.contains(&token),
        "the value stays redacted: {fetched}"
    );
    assert_eq!(found["data"]["matchCount"], 1, "{found}");
    assert_eq!(
        built["data"]["diagnostics"]["path-policy-withheld"], 1,
        "a graph never reads them: {built}"
    );
}

mod graph_home;
