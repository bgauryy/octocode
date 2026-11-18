//! Fixtures shared by this crate's tests and, through the `test-support`
//! feature, by the tests of the crates above it (`octocode-native`'s tools):
//! fixture directories, contract and page-walk checks, staged responses.
#![cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        clippy::string_slice,
        reason = "fixtures for tests, compiled only for them (`test`, `test-support`): they build processes, files and timings on purpose; the source scans slice ASCII syntax"
    )
)]
#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    clippy::new_without_default,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once; a fixture directory is made by `new`, never defaulted"
)]
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::security::ContentSecurity;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

// Contract, lead and page-walk checks; the integration tests include the same
// file (tests/support), so both kinds of test assert one way.
use crate::contracts::{partial_reasons, validate, validate_output};
pub mod billing;
pub mod checks;

/// The build identity (hex, as a real one) tests mint and replay cursors
/// under: they call the tools directly, without the runtime that passes its own.
pub const BUILD_IDENTITY: &str = "feedfacecafebeef";

/// A fresh temporary directory, removed (with everything under it) on drop.
/// `.0` is its path.
pub struct Fixture(
    pub PathBuf,
    #[expect(dead_code, reason = "held only to remove the directory on drop")] tempfile::TempDir,
);

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("octocode-tool-test-")
            .tempdir()
            .expect("fixture");
        Self(dir.path().to_path_buf(), dir)
    }

    /// Write `contents` to `relative` under the root, creating its parents.
    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parents");
        }
        std::fs::write(&path, contents).expect("fixture write");
        path
    }

    /// Date `relative` (a file or a directory) an hour ahead: a file whose last write is in the future
    /// is inside every settle window, however the clock reads, so it stays
    /// unsettled with no sleep.
    pub fn date_ahead(&self, relative: &str) {
        let path = self.0.join(relative);
        // A directory opens read-only (Unix; Windows has no such open); a
        // file needs write access for `set_modified` on Windows.
        std::fs::File::options()
            .read(true)
            .write(!path.is_dir())
            .open(&path)
            .expect("open fixture")
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(3600))
            .expect("date fixture ahead");
    }
}

/// Creates a FIFO (named pipe) at `path`, owner read/write only.
#[cfg(unix)]
pub fn make_fifo(path: &Path) {
    let name = std::ffi::CString::new(path.to_string_lossy().as_bytes()).expect("path has no NUL");
    // SAFETY: `name` is a valid NUL-terminated C string that outlives the call.
    let made = unsafe { libc::mkfifo(name.as_ptr(), 0o600) };
    assert_eq!(made, 0, "mkfifo {}", path.display());
}

/// Whether `node` runs on this machine.
#[cfg(unix)]
pub fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Gate for a test that needs `node`: `true` when it can run. Without node a
/// local run skips the test; under `CI` it fails instead, so a runner that
/// lacks node cannot silently lose the coverage.
#[cfg(unix)]
pub fn require_node() -> bool {
    if node_available() {
        return true;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "node is required to run this test in CI"
    );
    eprintln!("skipping: node not available");
    false
}

thread_local! { static SAVED_REQUESTS: std::cell::RefCell<Vec<(ToolId, Value)>> = const { std::cell::RefCell::new(Vec::new()) }; }

/// A query row `tool`'s contract accepts as saved work with `fields` set:
/// the first of core's accepted contract fixtures for `tool` that stays
/// valid with them. For tests whose subject is call plumbing, not the row.
pub fn accepted_row(tool: ToolId, fields: &Value) -> Value {
    let fixtures: Vec<Value> =
        serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
            .expect("core contract fixtures");
    fixtures
        .iter()
        .filter(|fixture| fixture["tool"] == tool.as_str() && fixture["accepted"] == true)
        .filter_map(|fixture| fixture["input"]["queries"].get(0))
        .map(|row| crate::response::result::with_changes(row.clone(), fields))
        .find(|row| crate::contracts::validate_built_query(tool.as_str(), row.clone()).is_ok())
        .unwrap_or_else(|| panic!("no accepted {} fixture takes {fields}", tool.as_str()))
}

pub fn opened(tool: ToolId, row: &Value) -> Value {
    crate::contracts::validate_built_query(tool.as_str(), row.clone())
        .expect("valid internal producer row")
}

pub fn opened_input(tool: ToolId, input: &Value) -> Value {
    let saved = if let Some(token) = input.get("cursor").and_then(Value::as_str) {
        validate(tool.as_str(), input.clone()).expect("valid public cursor input");
        let id = token
            .strip_prefix("test-publication-")
            .expect("issued test cursor")
            .parse::<usize>()
            .expect("cursor index");
        SAVED_REQUESTS.with(|saved| {
            let saved = saved.borrow();
            let (target, input) = saved.get(id).expect("issued cursor member");
            assert_eq!(*target, tool, "cursor target binding");
            input.clone()
        })
    } else {
        input.clone()
    };
    crate::contracts::validate_saved(tool.as_str(), saved).expect("valid saved producer input")
}

/// The response stage's `related[]` rule, for the shared checks.
fn keep_workflow_lead(tool: &str, output: &mut Value) {
    crate::response::workflow_lead::retain_guidance(
        crate::id::ToolId::from_name(tool).expect("a runtime tool"),
        output,
        &Value::Null,
    );
}
use crate::response::workflow_lead::carried_identities;

fn validate_test_input(
    tool: &str,
    input: &Value,
) -> Result<Value, crate::contracts::ContractValidationError> {
    if input.get("cursor").is_some() {
        validate(tool, input.clone())
    } else {
        crate::contracts::validate_saved(tool, input.clone())
    }
}

fn publish_for_assertion(tool: &str, output: &Value) -> Value {
    let mut output = output.clone();
    crate::response::pages::publish_rows(&mut output);
    if checks::leads(&output)
        .iter()
        .all(|lead| lead.kind != "continue" || lead.query.get("cursor").is_some())
    {
        return output;
    }
    crate::response::continuations::finalize(
        &mut output,
        ToolId::from_name(tool).expect("tool"),
        &crate::response::continuations::Scope {
            is_available: &|_| true,
        },
        &mut |target, input, _| {
            SAVED_REQUESTS.with(|saved| {
                let mut saved = saved.borrow_mut();
                // The same continuation (tool and complete input) is the same
                // handle, as in the issuing runtime.
                let id = saved
                    .iter()
                    .position(|(tool, saved)| *tool == target && *saved == input)
                    .unwrap_or_else(|| {
                        saved.push((target, input));
                        saved.len() - 1
                    });
                Some(format!("test-publication-{id}"))
            })
        },
    )
    .expect("valid producer continuations");
    output
}

pub fn validate_sealed(
    tool: &str,
    output: &Value,
) -> Result<(), crate::contracts::ContractValidationError> {
    validate_output(tool, &publish_for_assertion(tool, output))
}

/// Asserts that a row part (`data` with its `rowPart`), as published, meets
/// the output schema exactly as an MCP client checks `structuredContent`:
/// a later part is held to the same schema as the first.
pub fn assert_part_contract(tool: ToolId, part: &Value) {
    let mut output = published(tool, &part["data"]);
    if let Some(row_part) = part.get("rowPart") {
        output["results"][0]["rowPart"] = row_part.clone();
    }
    if let Err(error) = validate_output(tool.as_str(), &output) {
        panic!("{tool:?}: {error}\n{part}");
    }
}

pub fn published(tool: ToolId, row: &Value) -> Value {
    let shaped = crate::response::rows::result_row(0, row.clone(), row["status"].as_str());
    let mut output = serde_json::json!({"results":[shaped]});
    crate::response::notices::place(&mut output, "results");
    publish_for_assertion(tool.as_str(), &output)
}

/// A lead a tool builds on purpose stays served: the row `row` (a tool's raw
/// executor row) builds a related call `intended` accepts, and the served row
/// (after the response stage drops advisory leads) keeps that same call. A tool
/// names its recovery leads in its own output-contract tests, so a lead built
/// and then dropped fails where the lead is built, not in a live retest. Leads
/// that are advisory are simply not named; no rule bounds how many a row builds.
pub fn assert_lead_served(tool: ToolId, row: &Value, intended: impl Fn(&Value) -> bool) {
    let calls_of = |data: &Value| -> Vec<Value> {
        data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| {
                call.get("kind").is_none_or(|kind| kind == "related") && intended(&call["input"])
            })
            .cloned()
            .collect()
    };
    let built = calls_of(row);
    assert!(
        !built.is_empty(),
        "{tool:?}: the row builds no intended related call\n{row}"
    );
    let mut served = published(tool, row);
    keep_workflow_lead(tool.as_str(), &mut served);
    let kept = calls_of(&served["results"][0]["data"]);
    assert!(
        !kept.is_empty(),
        "{tool:?}: an intended related call was built and dropped by the response stage; \
         the row's warning must promise it (name `related`) or the lead carry new identity\n{served}"
    );
}

/// One call's response over `rows` (each a row's `data`), as a caller
/// receives it: the sealed envelope through the response stage (notices,
/// validation, paging) that every tool's rows pass.
pub fn staged_response(tool: ToolId, rows: Vec<Value>) -> Value {
    use crate::response::stage::{StageInput, respond, settle, state_call_notes};
    let queries = serde_json::json!({ "queries": rows.iter().map(|_| serde_json::json!({})).collect::<Vec<_>>() });
    let rows = rows
        .into_iter()
        .enumerate()
        .map(|(index, data)| {
            let status = data["status"].as_str().map(str::to_owned);
            crate::response::rows::result_row(index, data, status.as_deref())
        })
        .collect::<Vec<_>>();
    let mut structured = crate::security::Sanitized::sealed(serde_json::json!({ "results": rows }));
    let mut failure = None;
    let call_notes = settle(tool, &mut structured, &queries, &mut failure).expect("settled");
    let context = crate::execution::ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: None,
        response_window: None,
        github_credential: None,
    };
    let mut outcome = respond(
        StageInput {
            tool,
            structured,
            options: crate::response::page_model::ResponsePageOptions::default(),
            failure,
            auto_page_chars: 20_000,
            allow_auto_paging: true,
        },
        None,
        &context,
    )
    .expect("staged")
    .result
    .expect("valid response");
    // The call that executed the rows answers with them.
    state_call_notes(&mut outcome, &call_notes);
    outcome.structured_content.into_value()
}

pub fn published_next_page(tool: ToolId, row: &Value) -> Value {
    let public = published(tool, row);
    opened_input(
        tool,
        &public["results"][0]["data"]["next"]["nextPage"]["input"],
    )["queries"][0]
        .clone()
}

/// A tool's raw executor `row`, as [`published`], satisfies the
/// tool's output contract.
pub fn assert_row_contract(tool: ToolId, row: &Value) {
    checks::assert_output_contract(tool.as_str(), &published(tool, row));
}

/// The path policy of a workspace rooted at `root`.
pub fn workspace_policy(root: &Path) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("path policy")
}

/// [`workspace_policy`] with default content security.
pub fn simple_policy(root: &Path) -> (PathPolicy, ContentSecurity) {
    (workspace_policy(root), ContentSecurity::new())
}

/// A workspace with a credentials directory, a built-in sensitive file name
/// (ignored by the path policy, not by config) and one visible source file.
pub fn sensitive_fixture() -> Fixture {
    let root = Fixture::new();
    root.write(".aws/credentials", "hidden\n".repeat(50));
    root.write("terraform.tfstate", "ignored\n".repeat(70));
    root.write("visible.rs", "pub fn visible() {\n}\n");
    root
}

/// Cancels from its third check on: the root is admitted, its descendants
/// are not.
pub struct AfterRoot(pub AtomicUsize);

impl CancellationCheck for AfterRoot {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_add(1, Ordering::Relaxed) >= 2 {
            Err("stop traversal".into())
        } else {
            Ok(())
        }
    }
}

include!("test_support/production_source.rs");

#[cfg(test)]
mod tests;

/// The `skippedPaths` entries of `cause` in a tool result `out`.
fn skipped_of<'a>(out: &'a Value, cause: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    out["skippedPaths"]["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |entry| entry["cause"] == cause)
}

/// How many paths `out`'s `skippedPaths` skipped by `cause`: each entry's
/// `count`, else one.
pub fn skipped_count(out: &Value, cause: &str) -> u64 {
    skipped_of(out, cause)
        .map(|entry| entry["count"].as_u64().unwrap_or(1))
        .sum()
}

/// The final names of `out`'s `skippedPaths` entries of `cause`, in order.
pub fn skipped_names(out: &Value, cause: &str) -> Vec<String> {
    skipped_of(out, cause)
        .filter_map(|entry| {
            entry["path"]
                .as_str()?
                .rsplit('/')
                .next()
                .map(str::to_owned)
        })
        .collect()
}

/// `out`'s `skippedPaths` entries of `cause` as (reason, count).
pub fn skipped_reasons(out: &Value, cause: &str) -> Vec<(String, u64)> {
    skipped_of(out, cause)
        .map(|entry| {
            (
                entry["reason"].as_str().unwrap_or_default().to_owned(),
                entry["count"].as_u64().unwrap_or(1),
            )
        })
        .collect()
}

/// The snapshot a raw tool row's continuations pin: the `snapshot` of the
/// first `next` call that carries one (a row states none itself), or
/// `Null` when no continuation pins one.
pub fn pinned_snapshot(out: &Value) -> &Value {
    static NONE: Value = Value::Null;
    out["next"]
        .as_object()
        .into_iter()
        .flat_map(|calls| calls.values())
        .map(|call| &call["input"]["queries"][0]["snapshot"])
        .find(|snapshot| snapshot.is_string())
        .unwrap_or(&NONE)
}

/// A call's outcome over `structured_content`, sealed by the output gate as a
/// tool result is.
pub fn tool_outcome(
    structured_content: Value,
    failure: Option<crate::response::result::ErrorClass>,
    all_failed: bool,
) -> crate::response::outcome::ToolOutcome {
    crate::response::outcome::ToolOutcome {
        structured_content: crate::security::Sanitized::sealed(structured_content),
        failure,
        all_failed,
        continued: None,
    }
}

/// A fresh canonical workspace and its path policy; the directory is removed
/// when the returned [`Fixture`] drops.
pub fn temp_workspace() -> (
    crate::test_support::Fixture,
    std::path::PathBuf,
    crate::policy::path::PathPolicy,
) {
    let dir = crate::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    let paths = crate::test_support::workspace_policy(&root);
    (dir, root, paths)
}

/// The `src` directory of every crate the tool runtime is built from: the
/// facade (`runtime`), this crate (`runtime-base`), the shared tool layer
/// (`tools-shared`) and each tool family (`tool-*`), sorted. Source scans that
/// hold for the runtime as a whole walk these.
pub fn runtime_crate_sources() -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates directory");
    let mut sources: Vec<PathBuf> = std::fs::read_dir(crates)
        .expect("read the crates directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            matches!(name.as_str(), "runtime" | "runtime-base" | "tools-shared")
                || name.starts_with("tool-")
        })
        .map(|entry| entry.path().join("src"))
        .collect();
    sources.sort();
    sources
}
