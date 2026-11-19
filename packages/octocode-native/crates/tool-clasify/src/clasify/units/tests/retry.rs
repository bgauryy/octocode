//! A units row held to its ceiling whose shared transient retries run out:
//! the units left unanswered are transient error cells, never dropped or
//! guessed, and `next.retry` asks only them again, priced; run, it judges
//! only them.
use super::*;
use octocode_runtime_base::config::ConfigOutput;
use octocode_runtime_base::execution::availability::ToolAvailability;
use octocode_runtime_base::execution::{ExecutionError, RuntimeLimits};
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::outcome::DomainResult;
use octocode_runtime_base::security::{CheckedQuery, ContentSecurity};
use octocode_tools_shared::judgment::Evaluator;
use octocode_tools_shared::selection::transient_fake::Transient;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A reader that only judges: a `content` row reads no source.
struct Judging {
    evaluator: Evaluator,
    handle: tokio::runtime::Handle,
}

impl SourceReader for Judging {
    fn paths(&self) -> &PathPolicy {
        unreachable!("a content row reads no path")
    }
    fn read_paths(&self) -> &PathPolicy {
        unreachable!("a content row reads no path")
    }
    fn security(&self) -> &ContentSecurity {
        unreachable!("a content row scans nothing here")
    }
    fn config(&self) -> &ConfigOutput {
        unreachable!("a content row reads no config")
    }
    fn limits(&self) -> &RuntimeLimits {
        unreachable!("a content row reads no limits")
    }
    fn home(&self) -> &Path {
        unreachable!("a content row reads no home")
    }
    fn handle(&self) -> &tokio::runtime::Handle {
        &self.handle
    }
    fn availability(&self) -> &ToolAvailability {
        unreachable!("a content row checks no availability")
    }
    fn evaluator(&self) -> &Evaluator {
        &self.evaluator
    }
    fn output(
        &self,
        _tool: ToolId,
    ) -> &'static dyn octocode_runtime_base::response::output::ToolOutput {
        unreachable!("a content row finishes no tool output")
    }
    fn execute(
        &self,
        _: ToolId,
        _: &CheckedQuery,
        _: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        unreachable!("a content row runs no tool")
    }
    fn retained_github_identity(
        &self,
        _: &ExecutionContext,
    ) -> Result<Option<String>, ClassificationError> {
        Ok(None)
    }
    fn validate_retained_github_identity(
        &self,
        _: Option<&str>,
        _: &ExecutionContext,
    ) -> Result<(), ClassificationError> {
        Ok(())
    }
    fn text_capture_limit(&self) -> usize {
        unreachable!("a content row captures nothing")
    }
    fn capture_local_text(
        &self,
        _: &Value,
        _: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), ClassificationError> {
        unreachable!("a content row captures nothing")
    }
    fn capture_github_content(
        &self,
        _: &Value,
        _: &ExecutionContext,
        _: usize,
    ) -> Result<octocode_github::ContentResponse, ClassificationError> {
        unreachable!("a content row captures nothing")
    }
}

/// The row's data, as the call states it.
fn judged(reader: &Judging, query: &Value, can_resume: bool) -> Value {
    let row = super::super::super::rows::parse_rows(std::slice::from_ref(query))
        .pop()
        .expect("one row");
    let super::super::super::Input::Judge(super::super::super::Target::Content(content)) =
        &row.input
    else {
        panic!("a content row");
    };
    let context = ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(60)),
        response_window: None,
        github_credential: None,
    };
    let gate = reader.evaluator.gate();
    match judge_items(
        &reader.evaluator,
        &Evaluations::default(),
        &row,
        content,
        reader,
        &context,
        &gate,
        can_resume,
    ) {
        RowOutcome::Units(Ok(units)) => units.finish(),
        _ => panic!("a units row"),
    }
}

#[test]
fn spent_shared_retries_leave_transient_units_that_next_retry_asks_again_alone() {
    let provider = Transient::start();
    let reader = Judging {
        // One transient retry, shared by the row.
        evaluator: provider.evaluator(1),
        handle: provider.executor.handle().clone(),
    };
    let query = json!({
        "operation": "evaluate",
        "perItem": true,
        "content": ["a fine entry", "b FAIL entry", "c FAIL entry"],
        "questions": {"keep": {"type": "noul", "instructions": "Is the entry fine?"}}
    });
    let first = judged(&reader, &query, true);
    // Three requests and the one shared retry: the ceiling, all sent.
    assert_eq!(provider.bodies().len(), 4, "{first:#}");
    assert_eq!(first["usage"]["calls"], 4, "{first:#}");
    assert_eq!(first["usage"]["requestsPlanned"], 3, "{first:#}");
    assert_eq!(first["usage"]["requestsCeiling"], 4, "{first:#}");
    assert_eq!(
        first["units"]["0"]["keep"],
        json!({"noul": 0.9}),
        "{first:#}"
    );
    for key in ["1", "2"] {
        let cell = &first["units"][key]["keep"];
        assert_eq!(cell["status"], "error", "{first:#}");
        let code = cell["error"]["errorCode"].as_str().expect("code");
        assert_eq!(
            octocode_runtime_base::id::error_class(code),
            octocode_runtime_base::id::error_codes::ErrorClass::Transient,
            "{first:#}"
        );
    }
    assert_eq!(first["coverage"]["error"], 2, "{first:#}");
    // The retry asks only the two, at their own ceiling: two requests and
    // the shared retry.
    let retry = &first["next"]["retry"];
    assert_eq!(retry["tool"], "clasify", "{first:#}");
    let again = &retry["input"]["queries"][0];
    assert_eq!(again["retryUnits"], json!(["1", "2"]), "{first:#}");
    assert_eq!(again["maxRequests"], 3, "{first:#}");
    assert_eq!(again["content"], query["content"]);
    assert!(
        first["hints"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|hint| hint
                .as_str()
                .is_some_and(|hint| hint.contains("next.retry asks only them"))),
        "{first:#}"
    );

    // Run, once the provider answers, it judges only those two.
    provider.heal();
    let before = provider.bodies().len();
    let second = judged(&reader, again, true);
    assert_eq!(provider.bodies().len() - before, 2, "{second:#}");
    assert_eq!(second["coverage"]["totalUnits"], 2, "{second:#}");
    assert_eq!(second["coverage"]["answered"], 2, "{second:#}");
    assert!(second["units"].get("0").is_none(), "{second:#}");
    for key in ["1", "2"] {
        assert_eq!(
            second["units"][key]["keep"],
            json!({"noul": 0.9}),
            "{second:#}"
        );
    }
    assert!(second["next"].get("retry").is_none(), "{second:#}");
}

/// A runtime that keeps nothing cannot carry `retryUnits`: its related
/// retry is the whole row at the row's ceiling, and its hint says so.
#[test]
fn without_saved_work_the_retry_is_the_whole_row_at_its_ceiling() {
    let provider = Transient::start();
    let reader = Judging {
        evaluator: provider.evaluator(1),
        handle: provider.executor.handle().clone(),
    };
    let query = json!({
        "operation": "evaluate",
        "perItem": true,
        "content": ["a fine entry", "b FAIL entry"],
        "questions": {"keep": {"type": "noul", "instructions": "Is the entry fine?"}}
    });
    let first = judged(&reader, &query, false);
    assert!(first["next"].get("retry").is_none(), "{first:#}");
    let related = first["related"]
        .as_array()
        .and_then(|calls| calls.iter().find(|call| call["tool"] == "clasify"))
        .expect("a related retry");
    let again = &related["input"]["queries"][0];
    assert!(again.get("retryUnits").is_none(), "{first:#}");
    assert_eq!(
        again["maxRequests"], first["usage"]["requestsCeiling"],
        "{first:#}"
    );
    assert!(
        first["hints"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|hint| hint
                .as_str()
                .is_some_and(|hint| hint.contains("asks every unit"))),
        "{first:#}"
    );
}
