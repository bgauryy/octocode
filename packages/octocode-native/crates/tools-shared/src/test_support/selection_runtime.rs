//! A selection runtime over a counting judge, for the tests of selection
//! and of the tools that select.
#![cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "fixtures for tests, compiled only for them (`test`, `test-support`): they build processes, files and timings on purpose"
    )
)]
use crate::selection::{
    Candidate, Decision, Failure, JudgeCall, Judgments, Outcome, Runtime, SelectionJudge, select,
};
use crate::test_support::selection_fake;
use octocode_contract_types::SelectionUnit;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub struct SelectedJudge(std::sync::Arc<AtomicUsize>);

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

pub fn test_runtime(
    judge: std::sync::Arc<dyn SelectionJudge>,
    handle: tokio::runtime::Handle,
) -> Runtime {
    Runtime {
        judge,
        ..selection_fake::runtime(selection_fake::SelectAll, handle, Duration::from_secs(5))
    }
}

/// A runtime whose judge selects every admitted candidate, and its call count.
pub fn selecting_runtime(
    executor: &tokio::runtime::Runtime,
) -> (Runtime, std::sync::Arc<AtomicUsize>) {
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    (runtime, calls)
}

/// Selects among `candidates` of the source named `"source"`.
pub fn select_source(
    runtime: &Runtime,
    query: &Value,
    prior: Option<&str>,
    candidates: &[Candidate],
    unit: SelectionUnit,
) -> Result<Outcome, Failure> {
    select(
        Some(runtime),
        octocode_runtime_base::id::ToolId::LocalFetch,
        query,
        "source",
        prior,
        candidates,
        None,
        unit,
    )
}
