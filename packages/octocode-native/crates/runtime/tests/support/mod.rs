//! Isolated runtime fixture for integration tests.
//!
//! Tests construct `ToolRuntime` from an explicit `ConfigInput` so they do not
//! mutate process environment or race under `cargo test`.
#![allow(
    dead_code,
    reason = "each integration-test binary compiles this module and uses a different subset"
)]
use octocode_native::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_native::runtime::{RuntimeError, ToolOutcome, ToolRuntime};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use wiremock::{MockServer, Request, Respond, ResponseTemplate};

/// Per-fixture token suffix. The GitHub rate-limit budget is process-wide and
/// keyed by API host + token digest; wiremock pools its servers, so a later
/// test can reuse an earlier test's `127.0.0.1:<port>`. A distinct token per
/// workspace keeps one test's simulated limits, cooldowns, pacing, and circuit
/// state from leaking into another.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Production default request timeout for HTTP-backed provider fixtures. The
/// workspace default of 5 seconds keeps unrelated timeout tests fast but is too
/// narrow during a cold native build with several mock servers active.
pub const MOCK_PROVIDER_TIMEOUT_MS: &str = "30000";

pub struct Workspace {
    _root: tempfile::TempDir,
    pub workspace: PathBuf,
    pub home: PathBuf,
    pub token: String,
}

impl Workspace {
    pub fn new() -> Self {
        let root = tempfile::TempDir::new().expect("temporary workspace");
        let root_path = root
            .path()
            .canonicalize()
            .expect("canonical temporary root");
        let workspace = root_path.join("work");
        let home = root_path.join("home");
        std::fs::create_dir_all(&workspace).expect("workspace directory");
        std::fs::create_dir_all(&home).expect("home directory");
        let token = format!(
            "fixture-token-{}",
            FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        Self {
            _root: root,
            workspace,
            home,
            token,
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

    /// `PATH` whose `gh` answers only `gh auth token --hostname 127.0.0.1`
    /// (with no token env set) and logs each call to `$OCTOCODE_HOME/gh-calls`.
    #[cfg(unix)]
    pub fn fake_gh_path(&self) -> String {
        use std::os::unix::fs::PermissionsExt;
        let script = self.write("bin/gh", "#!/bin/sh\n[ \"$1 $2 $3 $4\" = 'auth token --hostname 127.0.0.1' ] || exit 2\n[ -z \"$GH_TOKEN$GITHUB_TOKEN$OCTOCODE_TOKEN\" ] || exit 3\nprintf x >> \"$OCTOCODE_HOME/gh-calls\"\nprintf synthetic-gh-credential\n");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("fake gh permissions");
        format!(
            "{}:/usr/bin:/bin",
            script.parent().expect("fake gh dir").display()
        )
    }

    pub fn write_outside_allowed_roots(
        &self,
        relative: &str,
        contents: impl AsRef<[u8]>,
    ) -> PathBuf {
        let path = self._root.path().join("outside").join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("outside fixture parents");
        }
        std::fs::write(&path, contents).expect("outside fixture write");
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
            ("OCTOCODE_TOKEN".into(), self.token.clone()),
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
            project_config_file: FileInput::Missing {
                path: self.workspace.join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Cli,
        }
    }

    pub fn runtime(&self, extra: &[(&str, String)]) -> ToolRuntime {
        ToolRuntime::new(self.config(extra)).expect("native runtime")
    }
}

pub async fn call(
    runtime: &ToolRuntime,
    tool: &str,
    query: Value,
) -> Result<ToolOutcome, RuntimeError> {
    runtime
        .execute("test-1".into(), tool.into(), envelope(query))
        .await
}

/// A test row (or a `{queries}` input) as the one input shape tools take:
/// `{queries:[row]}`, each row briefed, response-paging fields on the
/// envelope.
pub fn envelope(mut input: Value) -> Value {
    let brief = |row: &mut Value| {
        if let Some(object) = row.as_object_mut() {
            object
                .entry("mainGoal")
                .or_insert_with(|| json!("Exercise the native runtime integration path."));
            object
                .entry("reasoning")
                .or_insert_with(|| json!("Exercise the native runtime integration path."));
            object.entry("debug").or_insert_with(|| json!(true));
        }
    };
    if let Some(rows) = input.get_mut("queries").and_then(Value::as_array_mut) {
        rows.iter_mut().for_each(brief);
        return input;
    }
    let Some(row) = input.as_object_mut() else {
        return input;
    };
    let mut envelope = serde_json::Map::new();
    for key in ["responseOffset", "responseLength", "responseSnapshot"] {
        if let Some(value) = row.remove(key) {
            envelope.insert(key.into(), value);
        }
    }
    let mut row = Value::Object(std::mem::take(row));
    brief(&mut row);
    envelope.insert("queries".into(), json!([row]));
    Value::Object(envelope)
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

/// Runtime whose classification provider is the mock `server`.
pub fn provider_runtime(workspace: &Workspace, server: &MockServer) -> ToolRuntime {
    workspace.runtime(&[
        ("OCTOCODE_CLASSIFICATION_API", "secret".into()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
        ("REQUEST_TIMEOUT", MOCK_PROVIDER_TIMEOUT_MS.into()),
    ])
}

/// Locate provider stub: the choice question puts `top` on the first
/// passage ID it was offered; the existence question answers `exists`.
#[derive(Clone)]
pub struct LocateTopPassage {
    pub top: f64,
    pub exists: f64,
}

impl Respond for LocateTopPassage {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        fn passage_ids(value: &Value, ids: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    for (key, value) in map {
                        if key.len() == 4 && key.starts_with('P') && !ids.contains(key) {
                            ids.push(key.clone());
                        }
                        passage_ids(value, ids);
                    }
                }
                Value::Array(items) => {
                    items.iter().for_each(|item| passage_ids(item, ids));
                }
                _ => {}
            }
        }
        let body: Value = serde_json::from_slice(&request.body).expect("ranking request body");
        let mut ids = Vec::new();
        passage_ids(&body, &mut ids);
        ids.sort();
        let rest = if ids.len() > 1 {
            (1.0 - self.top) / (ids.len() - 1) as f64
        } else {
            0.0
        };
        let probabilities = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), json!(if index == 0 { self.top } else { rest })))
            .collect::<serde_json::Map<_, _>>();
        ResponseTemplate::new(200).set_body_json(json!({
            "model":"resolved",
            "answers":{
                "answer_0":{"type":"choice","choice":ids.first(),"confidence":self.top,
                    "probabilities":probabilities},
                "answer_1":{"type":"noul","noul":self.exists}
            },
            "usage":{"input_tokens":5,"output_tokens":2}
        }))
    }
}
