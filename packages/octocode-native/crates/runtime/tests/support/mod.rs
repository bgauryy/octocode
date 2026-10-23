//! Isolated runtime fixture for integration tests.
//!
//! Tests construct `ToolRuntime` from an explicit `ConfigInput` so they do not
//! mutate process environment or race under `cargo test`.
#![allow(clippy::panic, reason = "test assertions fail by panicking")]

use octocode_native::execution::RuntimeLimits;
use octocode_native::response::outcome::ToolOutcome;
use octocode_native::runtime::{Channel, RuntimeError, ToolRuntime};
use serde_json::{Value, json};
use std::path::Path;

// The contract, lead and page-walk checks the unit tests use: one copy, by path.
use octocode_native::contracts::{parsed_contract, validate, validate_output};
#[path = "../../src/tools/test_support/checks.rs"]
#[expect(dead_code, reason = "the unit tests use the rest of the shared checks")]
pub mod checks;

mod cursor;
mod providers;
mod workspace;

pub use cursor::{assert_cursor_only, cursor_at_offset, cursor_offset, cursor_response};
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
