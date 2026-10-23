use super::plan::{Plan, VERSION, decode};
use super::provider_page::strip_plan_from_tool_continuations;
use super::run::binding;
use super::*;
use crate::security::CheckedQuery;
use crate::tools::test_support::selection_fake;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_contract_types::ErrorCode;
use octocode_contract_types::SelectionScope;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod admission;
mod candidate_pages;
mod code_page;
mod code_page_failures;
mod plan_binding;
mod plan_token;

struct FailedJudge;
impl SelectionJudge for FailedJudge {
    fn identity(&self) -> Value {
        json!({"backend":"test"})
    }
    fn select(&self, _call: JudgeCall<'_>) -> Judgments {
        Err("providerUnavailable".into())
    }
}

struct SelectedJudge(std::sync::Arc<AtomicUsize>);
impl SelectionJudge for SelectedJudge {
    fn identity(&self) -> Value {
        json!({"backend":"reverse-test"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| (ordinal, Decision::Selected))
            .collect())
    }
}

fn test_runtime(
    judge: std::sync::Arc<dyn SelectionJudge>,
    handle: tokio::runtime::Handle,
) -> Runtime {
    Runtime {
        judge,
        ..selection_fake::runtime(selection_fake::SelectAll, handle, Duration::from_secs(5))
    }
}

/// A runtime whose judge selects every admitted candidate, and its call count.
fn selecting_runtime(executor: &tokio::runtime::Runtime) -> (Runtime, std::sync::Arc<AtomicUsize>) {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    (runtime, calls)
}

/// Selects among `candidates` of the source named `"source"`.
fn select_source(
    runtime: &Runtime,
    query: &Value,
    prior: Option<&str>,
    candidates: &[Candidate],
    unit: SelectionUnit,
) -> Result<Outcome, Failure> {
    select(
        Some(runtime),
        query,
        "source",
        prior,
        candidates,
        None,
        unit,
    )
}

/// Selects the one `ghSearchCode` row of `output`, judged for `query`.
fn apply_code_rows(
    output: &mut Value,
    query: Value,
    response_query: &mut Value,
    runtime: &Runtime,
) -> Result<(), crate::execution::ExecutionError> {
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        output,
        &[CheckedQuery::checked(query)],
        &[0],
        response_query,
        runtime,
    )
}
