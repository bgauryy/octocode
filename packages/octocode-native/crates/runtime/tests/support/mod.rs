//! Isolated runtime fixture for integration tests.
//!
//! Tests construct `ToolRuntime` from an explicit `ConfigInput` so they do not
//! mutate process environment or race under `cargo test`.
#![expect(clippy::panic, reason = "test assertions fail by panicking")]

use octocode_native::runtime::{Channel, RuntimeError, ToolRuntime};
use octocode_runtime_base::execution::RuntimeLimits;
use octocode_runtime_base::response::outcome::ToolOutcome;
use serde_json::{Value, json};
use std::path::Path;

// The contract, lead and page-walk checks the unit tests use: one copy, by path.
use octocode_runtime_base::contracts::{partial_reasons, validate, validate_output};
#[path = "../../../runtime-base/src/test_support/checks.rs"]
#[expect(dead_code, reason = "the unit tests use the rest of the shared checks")]
pub mod checks;

mod cursor;
mod providers;
pub mod recorded;
mod workspace;

pub use cursor::assert_cursor_only;
pub use providers::{
    LONG_WINDOW_TOKENS, PROVIDER_LIMITS, WINDOW_TOKENS, capture_allowed, clasify_runtime,
    github_api, provider_env,
};
pub use workspace::Workspace;

/// The internal limits fixtures run under: no retries, a 5 s network
/// timeout so a test that reaches an endpoint it did not mock fails fast,
/// and no client-side GitHub spacing (only the throttle tests pay
/// production pacing, through [`PACED_LIMITS`]).
pub const FIXTURE_LIMITS: RuntimeLimits = RuntimeLimits {
    max_retries: 0,
    network_timeout: std::time::Duration::from_secs(5),
    classification_timeout: Some(std::time::Duration::from_secs(5)),
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
/// Every call a reader runs next: the reply's one `next`, present while more
/// of the request remains. No row carries a page walk of its own.
pub fn next_inputs(output: &Value) -> Vec<Value> {
    next_input(output).into_iter().collect()
}

/// The call a reader runs next: the reply's top-level `next`.
pub fn next_input(output: &Value) -> Option<Value> {
    for row in output["results"].as_array().into_iter().flatten() {
        assert!(
            row.pointer("/data/next/nextPage").is_none(),
            "a row carries its own page walk; the reply's next continues every row: {output}"
        );
    }
    output.pointer("/next/input").cloned()
}

/// A clasify call as this suite reads it: each fresh row that states no
/// `view` asks for `details`, the whole row, which is what every assertion
/// written before the lean `summary` default reads. A test of the default
/// states `view: "summary"`. Other tools, cursors and `models` rows pass
/// unchanged.
fn whole_clasify_rows(tool: &str, mut input: Value) -> Value {
    if tool != "clasify" {
        return input;
    }
    for row in input
        .get_mut("queries")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object_mut)
    {
        let judged = matches!(
            row.get("operation").and_then(Value::as_str),
            Some("evaluate" | "sections")
        );
        if judged && !row.contains_key("view") {
            row.insert("view".into(), json!("details"));
        }
    }
    input
}

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
        let input = whole_clasify_rows(&tool, input);
        self.execute(admission, tool, input, Channel::Cli).await
    }

    async fn run_mcp(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> Result<Value, RuntimeError> {
        let input = whole_clasify_rows(&tool, input);
        let result = match self.admit(request_id) {
            Ok(admission) => {
                self.execute(admission, tool.clone(), input, Channel::Mcp)
                    .await
            }
            Err(error) => Err(error),
        };
        octocode_native::runtime::error::mcp_result(&tool, result).map(|result| {
            json!({"content":[{"type":"text","text":result.text}],"isError":result.is_error})
        })
    }
}

/// Parse the one JSON text block of an actual text-only MCP result.
pub fn mcp_payload(result: &Value) -> Value {
    assert!(result.get("structuredContent").is_none(), "{result}");
    let content = result["content"].as_array().expect("MCP content");
    assert_eq!(content.len(), 1, "{result}");
    serde_json::from_str(content[0]["text"].as_str().expect("MCP JSON text"))
        .expect("valid MCP JSON text")
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
    assert_named_calls_carried(tool, &outcome.structured_content);
    Ok(outcome)
}

/// The keys whose strings are sentences said to the caller about a row.
const SENTENCE_KEYS: [&str; 7] = [
    "warnings", "hints", "notices", "error", "answer", "skipped", "recovery",
];

/// Every sentence under `value` (a string or strings held by one of
/// [`SENTENCE_KEYS`]), calls aside: a call's input is a request, not prose.
fn sentences<'a>(value: &'a Value, said: bool, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) if said => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| sentences(item, said, out)),
        Value::Object(fields) => {
            for (key, child) in fields {
                if matches!(key.as_str(), "next" | "related") {
                    continue;
                }
                let said = SENTENCE_KEYS.contains(&key.as_str())
                    || (said && !child.is_object() && !child.is_array());
                sentences(child, said, out);
            }
        }
        _ => {}
    }
}

/// The `next.<key>` channels `sentence` names, and whether it names
/// `related` as a word of its own (`related[]` too).
fn named_calls(sentence: &str) -> (Vec<&str>, bool) {
    let word = |text: &'_ str| -> usize {
        text.find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(text.len())
    };
    let channels = sentence
        .match_indices("next.")
        .filter(|(at, _)| {
            sentence[..*at]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_ascii_alphanumeric())
        })
        .map(|(at, token)| &sentence[at + token.len()..])
        .map(|rest| &rest[..word(rest)])
        .filter(|key| key.starts_with(|c: char| c.is_ascii_lowercase()))
        .collect();
    let related = sentence
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == "related");
    (channels, related)
}

/// A served reply names no call it does not carry: a sentence of a row
/// (warning, hint, notice, error, answer) that says `related` sits on a row
/// with a `related` call, and one that says `next.<key>` on a row whose
/// `next` holds that key (the reply's `next` counts for a row's page walk,
/// `next.nextPage`). A row split into parts carries its calls on its last
/// part, so only that part is judged; a sentence the response says once
/// for all rows is carried when any row carries its call.
pub fn assert_named_calls_carried(tool: &str, envelope: &Value) {
    let hoisted_page = envelope.get("next").is_some();
    let carries = |row: &Value, sentence: &str| -> Result<(), String> {
        let (channels, related) = named_calls(sentence);
        let leads = checks::leads(row);
        if related && !leads.iter().any(|lead| lead.channel == "related") {
            return Err("related".to_owned());
        }
        for key in channels {
            let held = leads
                .iter()
                .any(|lead| lead.channel == "next" && lead.name == key);
            if !held && !(key == "nextPage" && hoisted_page) {
                return Err(format!("next.{key}"));
            }
        }
        Ok(())
    };
    let whole = |row: &&Value| {
        let part = &row["rowPart"];
        part.is_null() || part["part"] == part["of"]
    };
    let rows = envelope["results"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for row in rows.iter().filter(whole) {
        let mut said = Vec::new();
        sentences(row, false, &mut said);
        for sentence in said {
            if let Err(missing) = carries(row, sentence) {
                panic!(
                    "{tool}: a sentence names {missing}, which its row does not carry: {sentence:?} in {row}"
                );
            }
        }
    }
    let mut shared = Vec::new();
    for (key, value) in envelope.as_object().into_iter().flatten() {
        if key != "results" {
            sentences(value, SENTENCE_KEYS.contains(&key.as_str()), &mut shared);
        }
    }
    for sentence in shared {
        let (channels, related) = named_calls(sentence);
        if channels.is_empty() && !related {
            continue;
        }
        assert!(
            rows.iter().any(|row| carries(row, sentence).is_ok())
                || checks::leads(&serde_json::json!({
                    "next": {"reply": envelope.get("next")},
                    "related": envelope.get("related"),
                }))
                .iter()
                .any(|lead| !related || lead.channel == "related"),
            "{tool}: a shared sentence names a call no row carries: {sentence:?} in {envelope}"
        );
    }
}

fn publish_for_assertion(_tool: &str, output: &Value) -> Value {
    output.clone()
}
/// The response stage's `related[]` rule, for the shared checks.
fn keep_workflow_lead(tool: &str, output: &mut Value) {
    octocode_runtime_base::response::workflow_lead::retain_guidance(
        octocode_runtime_base::id::ToolId::from_name(tool).expect("a runtime tool"),
        output,
        &Value::Null,
    );
}
fn validate_test_input(
    tool: &str,
    input: &Value,
) -> Result<Value, octocode_runtime_base::contracts::ContractValidationError> {
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
    assert_workflow_leads(tool, &outcome.structured_content);
    Row { outcome }
}

/// A served response holds nothing the stage's guidance rule
/// ([`octocode_runtime_base::response::workflow_lead`]) would still remove,
/// its `related` calls aside: a call its producer marked as needed is served
/// without the mark, so only the stage that read the mark can judge it.
pub fn assert_workflow_leads(tool: &str, envelope: &Value) {
    let without_related = |mut envelope: Value| {
        for row in envelope["results"].as_array_mut().into_iter().flatten() {
            if let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) {
                data.remove("related");
            }
        }
        envelope
    };
    let mut again = envelope.clone();
    keep_workflow_lead(tool, &mut again);
    assert_eq!(
        without_related(again),
        without_related(envelope.clone()),
        "{tool}: the rule would still drop guidance"
    );
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
