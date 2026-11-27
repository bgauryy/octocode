//! A listing that shows an entry's name with a secret replaced says so the
//! one way every tool does: its result carries `redacted: true`. A name that
//! only spells a placeholder replaced nothing. Read from the call's result;
//! all secrets are fake.
use crate::runtime::{Channel, ToolRuntime};
use octocode_runtime_base::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_runtime_base::execution::RuntimeLimits;
use serde_json::{Value, json};
use std::path::Path;

const TOKEN: &str = "ghp_FAKEnameTok3n00000000000000000000xyz";

fn runtime(home: &Path, cwd: &Path) -> ToolRuntime {
    // An MCP host lists the local tools only once it turns them on.
    let env = std::collections::BTreeMap::from([
        (
            "OCTOCODE_HOME".to_owned(),
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
            config_file: FileInput::Missing {
                path: home.join(".octocoderc"),
            },
            project_config_file: FileInput::Missing {
                path: cwd.join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Mcp,
        },
        RuntimeLimits::DEFAULT,
    )
    .expect("runtime")
}

/// The data of one structureSearch call's row, as an agent receives it.
async fn listed(runtime: &ToolRuntime, row: Value) -> Value {
    let admission = runtime.admit("names".into()).expect("admitted");
    runtime
        .execute(
            admission,
            "structureSearch".into(),
            json!({ "queries": [row] }),
            Channel::Mcp,
        )
        .await
        .expect("call")
        .structured_content
        .into_value()["results"][0]["data"]
        .clone()
}

#[tokio::test]
async fn a_listing_that_shows_a_replaced_name_is_flagged() {
    let home = tempfile::tempdir().expect("home");
    let root = tempfile::tempdir().expect("root");
    let named = root.path().join("named");
    let literal = root.path().join("literal");
    for dir in [&named, &literal] {
        std::fs::create_dir(dir).expect("dir");
        std::fs::write(dir.join("plain.txt"), "x\n").expect("fixture");
    }
    std::fs::write(named.join(format!("deploy-{TOKEN}.txt")), "x\n").expect("fixture");
    std::fs::write(literal.join("[REDACTED-NOTE].txt"), "x\n").expect("fixture");
    let runtime = runtime(home.path(), root.path());
    for operation in ["files", "tree"] {
        let data = listed(&runtime, json!({"path": named, "operation": operation})).await;
        let text = data.to_string();
        assert!(!text.contains(TOKEN), "{text}");
        assert!(text.contains("deploy-[REDACTED-"), "{text}");
        assert_eq!(data["redacted"], json!(true), "{operation}: {text}");
        let data = listed(&runtime, json!({"path": literal, "operation": operation})).await;
        assert!(data.to_string().contains("[REDACTED-NOTE].txt"), "{data}");
        assert!(data.get("redacted").is_none(), "{operation}: {data}");
    }
    runtime.close().await;
}
