//! Isolated runtime fixture for integration tests.
//!
//! Tests construct `ToolRuntime` from an explicit `ConfigInput` so they do not
//! mutate process environment or race under `cargo test`.
#![allow(dead_code)]
use octocode_native::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_native::runtime::{RuntimeError, ToolOutcome, ToolRuntime};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Workspace {
    _root: tempfile::TempDir,
    pub workspace: PathBuf,
    pub home: PathBuf,
}

impl Workspace {
    pub fn new() -> Self {
        let root = tempfile::TempDir::new().expect("temporary workspace");
        let workspace = root.path().join("work");
        let home = root.path().join("home");
        std::fs::create_dir_all(&workspace).expect("workspace directory");
        std::fs::create_dir_all(&home).expect("home directory");
        Self {
            _root: root,
            workspace,
            home,
        }
    }

    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.workspace.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parents");
        }
        std::fs::write(&path, contents).expect("fixture write");
        path
    }

    pub fn config(&self, extra: &[(&str, String)]) -> ConfigInput {
        let mut env = BTreeMap::from([
            ("ENABLE_LOCAL".into(), "true".into()),
            ("ENABLE_CLONE".into(), "false".into()),
            (
                "ALLOWED_PATHS".into(),
                self.workspace.to_string_lossy().into_owned(),
            ),
            (
                "WORKSPACE_ROOT".into(),
                self.workspace.to_string_lossy().into_owned(),
            ),
            (
                "OCTOCODE_HOME".into(),
                self.home.to_string_lossy().into_owned(),
            ),
            ("OCTOCODE_TOKEN".into(), "fixture-token".into()),
            ("OCTOCODE_ENABLE_STATS".into(), "false".into()),
            ("MAX_RETRIES".into(), "0".into()),
            ("REQUEST_TIMEOUT".into(), "5000".into()),
        ]);
        for (key, value) in extra {
            env.insert((*key).into(), value.clone());
        }
        ConfigInput {
            env,
            cwd: self.workspace.clone(),
            os_home: self.home.clone(),
            trusted_project: true,
            global_env: FileInput::Missing {
                path: self.home.join(".env"),
            },
            project_env: FileInput::Missing {
                path: self.workspace.join(".octocode/.env"),
            },
            config_file: FileInput::Missing {
                path: self.home.join(".octocoderc"),
            },
            runtime_surface: RuntimeSurface::Cli,
            revision: 1,
        }
    }

    pub fn runtime(&self, extra: &[(&str, String)]) -> ToolRuntime {
        ToolRuntime::new(self.config(extra)).expect("native runtime")
    }

    pub fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_octocode"));
        command
            .current_dir(&self.workspace)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("OCTOCODE_HOME", &self.home)
            .env("WORKSPACE_ROOT", &self.workspace)
            .env("ALLOWED_PATHS", &self.workspace)
            .env("ENABLE_LOCAL", "true")
            .env("ENABLE_CLONE", "false")
            .env("NO_COLOR", "1")
            .env("OCTOCODE_ENABLE_STATS", "false");
        command
    }
}

pub async fn call(
    runtime: &ToolRuntime,
    tool: &str,
    query: Value,
) -> Result<ToolOutcome, RuntimeError> {
    runtime.execute("test-1".into(), tool.into(), query).await
}

pub fn row_data(outcome: &ToolOutcome) -> &Value {
    outcome
        .structured_content
        .pointer("/results/0/data")
        .unwrap_or(&Value::Null)
}

pub fn row_status(outcome: &ToolOutcome) -> &str {
    outcome
        .structured_content
        .pointer("/results/0/status")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if outcome
                .structured_content
                .pointer("/results/0/data")
                .is_some()
            {
                "success"
            } else {
                ""
            }
        })
}

pub fn query_path(path: &Path, extra: Value) -> Value {
    let mut query = json!({"path": path});
    if let Some(object) = extra.as_object() {
        for (key, value) in object {
            query[key] = value.clone();
        }
    }
    query
}
