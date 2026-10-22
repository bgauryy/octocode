//! Isolated runtime fixture for integration tests.
//!
//! Tests construct `ToolRuntime` from an explicit `ConfigInput` so they do not
//! mutate process environment or race under `cargo test`.
#![allow(clippy::panic, reason = "test assertions fail by panicking")]

use octocode_native::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_native::execution::RuntimeLimits;
use octocode_native::response::outcome::ToolOutcome;
use octocode_native::runtime::{Channel, RuntimeError, ToolRuntime};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use wiremock::MockServer;

// The contract, lead and page-walk checks the unit tests use: one copy, by path.
use octocode_native::contracts::{parsed_contract, validate, validate_output};
#[path = "../../src/tools/test_support/checks.rs"]
#[expect(dead_code, reason = "the unit tests use the rest of the shared checks")]
pub mod checks;

/// Per-fixture token suffix. The GitHub rate-limit budget is process-wide and
/// keyed by API host + token digest; wiremock pools its servers, so a later
/// test can reuse an earlier test's `127.0.0.1:<port>`. A distinct token per
/// workspace keeps one test's simulated limits, cooldowns, pacing, and circuit
/// state from leaking into another.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

/// The internal limits fixtures run under: no retries, a 5 s network
/// timeout so a test that reaches an endpoint it did not mock fails fast,
/// and no client-side GitHub spacing (only the throttle tests pay
/// production pacing, through [`PACED_LIMITS`]).
pub const FIXTURE_LIMITS: RuntimeLimits = RuntimeLimits {
    max_retries: 0,
    network_timeout: std::time::Duration::from_secs(5),
    github_pacing: octocode_github::GitHubPacing::Unpaced,
    ..RuntimeLimits::DEFAULT
};

/// [`FIXTURE_LIMITS`] under production (octokit) GitHub pacing.
pub const PACED_LIMITS: RuntimeLimits = RuntimeLimits {
    github_pacing: octocode_github::GitHubPacing::Octokit,
    ..FIXTURE_LIMITS
};

/// The GitHub API base every fixture starts from: a closed loopback port, so
/// a test that reaches GitHub without mounting a mock fails fast with a
/// connection error and never calls api.github.com. Mocked tests override it.
pub const UNMOCKED_GITHUB_API_URL: &str = "http://127.0.0.1:1/api/v3";

// Mock servers are bare (`MockServer::builder().start()`), never wiremock's
// pooled `MockServer::start()`: process-wide state keyed by the endpoint
// (the clasify judgment cache, the GitHub budget's per-host limiter) must not
// carry one test's answers or pacing into another test that reuses a pooled
// server's address.

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
        let script = self.write("bin/gh", "#!/bin/sh\n[ \"$1 $2 $3 $4\" = 'auth token --hostname 127.0.0.1' ] || exit 2\n[ -z \"$GH_TOKEN$GITHUB_TOKEN\" ] || exit 3\nprintf x >> \"$OCTOCODE_HOME/gh-calls\"\nprintf synthetic-gh-credential\n");
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
            ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
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
            // The OS home (`os_home` below) as child processes see it: without
            // it, a spawned `gh` or `git` resolves an empty home and writes its
            // state relative to the test's working directory.
            ("HOME".into(), self.home.to_string_lossy().into_owned()),
            ("GITHUB_TOKEN".into(), self.token.clone()),
            ("GITHUB_API_URL".into(), UNMOCKED_GITHUB_API_URL.into()),
            ("OCTOCODE_ENABLE_STATS".into(), "false".into()),
            // Fixture workspaces trust their own language-server definitions.
            (
                octocode_native::config::field::LSP_TRUST_PROJECT_CONFIG
                    .env_name()
                    .expect("lsp.trustProjectConfig is bound")
                    .into(),
                "true".into(),
            ),
        ]);
        for (key, value) in extra {
            env.insert((*key).into(), value.clone());
        }
        ConfigInput {
            env,
            cwd: self.workspace.clone(),
            os_home: self.home.clone(),
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
        self.runtime_with(extra, FIXTURE_LIMITS)
    }

    /// [`Self::runtime`] under the internal `limits` (the typed test seam;
    /// limits are code constants, never configuration).
    pub fn runtime_with(&self, extra: &[(&str, String)], limits: RuntimeLimits) -> ToolRuntime {
        ToolRuntime::with_limits(self.config(extra), limits).expect("native runtime")
    }
}

/// One call admitted under `request_id`, as the CLI (`run`: structured JSON)
/// or the N-API adapter (`run_mcp`: the MCP `CallToolResult`) makes it.
pub trait Run {
    fn run(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> impl Future<Output = Result<ToolOutcome, RuntimeError>> + Send;
    fn run_mcp(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> impl Future<Output = Result<Value, RuntimeError>> + Send;
}

impl Run for ToolRuntime {
    async fn run(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> Result<ToolOutcome, RuntimeError> {
        let admission = self.admit(request_id)?;
        self.execute(admission, tool, input, Channel::Structured)
            .await
    }

    async fn run_mcp(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> Result<Value, RuntimeError> {
        let result = match self.admit(request_id) {
            Ok(admission) => {
                self.execute(admission, tool.clone(), input, Channel::Mcp)
                    .await
            }
            Err(error) => Err(error),
        };
        octocode_native::runtime::error::mcp_result(&tool, result)
    }
}

pub async fn call(
    runtime: &ToolRuntime,
    tool: &str,
    query: Value,
) -> Result<ToolOutcome, RuntimeError> {
    runtime
        .run("test-1".into(), tool.into(), envelope(query))
        .await
}

/// Every opt-in output kind: the full view integration tests inspect.
pub const ALL_EXTRAS: [&str; 6] = [
    "hints",
    "details",
    "stats",
    "receipts",
    "echo",
    "diagnostics",
];

/// A test row (or a `{queries}` input) as the one input shape tools take:
/// `{queries:[row]}`, each row briefed, response-paging fields on the
/// envelope. A row without `extras` asks for every kind (the full view); a
/// row with `"extras": []` sends none (the default view).
pub fn envelope(mut input: Value) -> Value {
    let brief = |row: &mut Value| {
        if let Some(object) = row.as_object_mut() {
            object
                .entry("mainGoal")
                .or_insert_with(|| json!("Exercise the native runtime integration path."));
            object
                .entry("reasoning")
                .or_insert_with(|| json!("Exercise the native runtime integration path."));
            match object.get("extras") {
                None => {
                    object.insert("extras".into(), json!(ALL_EXTRAS));
                }
                Some(Value::Array(kinds)) if kinds.is_empty() => {
                    object.remove("extras");
                }
                Some(_) => {}
            }
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
    for key in [
        "responseOffset",
        "responseLength",
        "responseScope",
        "responseSnapshot",
    ] {
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

/// The environment pointing clasify at the mock classification provider
/// `server`, plus `extra`.
pub fn provider_env<'a>(
    server: &MockServer,
    extra: &[(&'a str, String)],
) -> Vec<(&'a str, String)> {
    let mut env = vec![
        ("OCTOCODE_BETA", "true".to_owned()),
        ("OCTOCODE_CLASSIFICATION_API", "secret".to_owned()),
        ("OCTOCODE_CLASSIFICATION_API_HOST", server.uri()),
    ];
    env.extend(extra.iter().cloned());
    env
}

/// Runtime whose classification provider is the mock `server`, plus `extra`,
/// at the production network timeout: the fixture's 5 s is too narrow during
/// a cold native build with several mock servers active.
pub fn clasify_runtime(
    workspace: &Workspace,
    server: &MockServer,
    extra: &[(&str, String)],
) -> ToolRuntime {
    workspace.runtime_with(&provider_env(server, extra), PROVIDER_LIMITS)
}

/// The workspace plus the URL capture directory, as `ALLOWED_PATHS` grants.
pub fn capture_allowed(workspace: &Workspace) -> (&'static str, String) {
    (
        "ALLOWED_PATHS",
        format!(
            "{},{}",
            workspace.workspace.display(),
            workspace.home.join("tmp/capture/url").display()
        ),
    )
}

/// The `GITHUB_API_URL` setting that points GitHub tools at the mock `server`.
pub fn github_api(server: &MockServer) -> [(&'static str, String); 1] {
    [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]
}

/// [`FIXTURE_LIMITS`] at the production network timeout, for HTTP-backed
/// provider fixtures.
pub const PROVIDER_LIMITS: RuntimeLimits = RuntimeLimits {
    network_timeout: RuntimeLimits::DEFAULT.network_timeout,
    ..FIXTURE_LIMITS
};

/// One executed row as an agent sees it: the whole outcome (`Row` derefs to
/// it), and its first result row's `status` and `data`.
pub struct Row {
    pub outcome: ToolOutcome,
}

impl std::ops::Deref for Row {
    type Target = ToolOutcome;

    fn deref(&self) -> &ToolOutcome {
        &self.outcome
    }
}

impl Row {
    pub fn data(&self) -> &Value {
        row_data(&self.outcome)
    }

    pub fn status(&self) -> &str {
        row_status(&self.outcome)
    }
}

/// Run one `row` of `tool` through the full runtime (validation, response
/// stages, path shaping) with exactly the output kinds `extras` (`&[]` is the
/// default view; the row itself names none). The row is briefed when it is
/// not. A runtime error fails the test, and so does an output that breaks the
/// output contract.
pub async fn run(runtime: &ToolRuntime, tool: &str, mut row: Value, extras: &[&str]) -> Row {
    assert!(
        row.get("extras").is_none(),
        "pass extras as run's argument, not on the row: {row}"
    );
    row["extras"] = json!(extras);
    let outcome = call(runtime, tool, row.clone())
        .await
        .unwrap_or_else(|error| panic!("{tool} {row}: {error:?}"));
    checks::assert_output_contract(tool, &outcome.structured_content);
    Row { outcome }
}

/// [`checks::Walker`] over the runtime: each call runs as the whole input,
/// verbatim (no envelope, no brief added), and `rows` names the row ids of
/// each output.
pub async fn walk(
    runtime: &ToolRuntime,
    tool: &str,
    start: Value,
    rows: impl Fn(&Value) -> Vec<String>,
) -> checks::Walk {
    let mut walker = checks::Walker::new(tool, start);
    while let Some(query) = walker.next_query() {
        let outcome = runtime
            .run("walk".into(), tool.into(), query.clone())
            .await
            .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"));
        let output = outcome.structured_content;
        walker.accept(checks::Page {
            rows: rows(&output),
            output,
        });
    }
    walker.finish()
}

/// Changed-file rows in a patch-free inventory: a `"M +1 -1 path"` row is
/// one, a `{"dir/": [rows]}` group is its rows.
pub fn inventory_len(files: &Value) -> usize {
    files.as_array().map_or(0, |rows| {
        rows.iter()
            .map(|row| match row {
                Value::Object(group) => group
                    .values()
                    .filter_map(Value::as_array)
                    .map(Vec::len)
                    .sum(),
                _ => 1,
            })
            .sum()
    })
}

/// Every `tests/*.rs` file of the crate at `manifest_dir` is declared in its
/// `tests/main.rs`: with `autotests = false`, an undeclared file never runs.
pub fn assert_every_test_file_is_a_module(manifest_dir: &str) {
    let dir = std::path::Path::new(manifest_dir).join("tests");
    let main = std::fs::read_to_string(dir.join("main.rs")).expect("read tests/main.rs");
    for entry in std::fs::read_dir(&dir).expect("read tests/") {
        let path = entry.expect("tests/ entry").path();
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if stem != "main" && path.extension().is_some_and(|ext| ext == "rs") {
            assert!(
                main.contains(&format!("mod {stem};")),
                "tests/{stem}.rs is not declared in tests/main.rs"
            );
        }
    }
}
