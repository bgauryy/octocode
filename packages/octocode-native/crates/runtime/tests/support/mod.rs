//! Isolated runtime fixture for integration tests.
//!
//! Tests construct `ToolRuntime` from an explicit `ConfigInput` so they do not
//! mutate process environment or race under `cargo test`.
#![expect(clippy::panic, reason = "test assertions fail by panicking")]

use octocode_native::execution::RuntimeLimits;
use octocode_native::response::outcome::ToolOutcome;
use octocode_native::runtime::{Channel, RuntimeError, ToolRuntime};
use serde_json::{Value, json};
use std::path::Path;

// The contract, lead and page-walk checks the unit tests use: one copy, by path.
use octocode_native::contracts::{validate, validate_output};
#[path = "../../src/tools/test_support/checks.rs"]
#[expect(dead_code, reason = "the unit tests use the rest of the shared checks")]
pub mod checks;

mod cursor;
mod providers;
mod workspace;

pub use cursor::assert_cursor_only;
pub use providers::{PROVIDER_LIMITS, capture_allowed, clasify_runtime, github_api, provider_env};
pub use workspace::Workspace;

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
        self.execute(admission, tool, input, Channel::Cli).await
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
    let input = envelope(query);
    let outcome = runtime
        .run("test-1".into(), tool.into(), input.clone())
        .await?;
    checks::assert_output_contract(tool, &outcome.structured_content);
    Ok(outcome)
}

fn publish_for_assertion(_tool: &str, output: &Value) -> Value {
    output.clone()
}
fn validate_test_input(
    tool: &str,
    input: &Value,
) -> Result<Value, octocode_native::contracts::ContractValidationError> {
    validate(tool, input.clone())
}

pub fn envelope(input: Value) -> Value {
    if input.get("queries").is_some() || input.get("cursor").is_some() {
        input
    } else {
        json!({"queries":[input]})
    }
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

/// Execute a canonical row or complete request and validate the public output.
pub async fn run(runtime: &ToolRuntime, tool: &str, row: Value) -> Row {
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
        let output = outcome.structured_content.into_value();
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
