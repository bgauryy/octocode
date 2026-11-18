use super::*;
use crate::selection::{Decision, JudgeCall, Runtime, SelectionJudge};
use crate::source_view::ReadStatus;
use crate::source_view::local_fetch_query;
use crate::test_support::selection_fake::{self, DOWN, REFUSED, WordJudge};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::security::ContentSecurity;
use octocode_test_support::json_fixture;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

struct BudgetJudge(Arc<AtomicUsize>);

impl SelectionJudge for BudgetJudge {
    fn identity(&self) -> serde_json::Value {
        json_fixture(r#"{"test": "budget"}"#)
    }

    fn select(&self, call: JudgeCall<'_>) -> crate::selection::Judgments {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| (ordinal, Decision::Selected))
            .collect())
    }
}

fn budget_runtime(handle: tokio::runtime::Handle, calls: Arc<AtomicUsize>) -> Runtime {
    let mut runtime = selection_fake::runtime(BudgetJudge(calls), handle, Duration::from_secs(5));
    runtime.limits.max_chars = 4_096;
    runtime
}

fn word_read(fail: Option<ErrorCode>) -> crate::source_view::LocalFetchResult {
    word_read_with(json!({}), fail, &crate::source_view::SourceFacts::default())
}

/// The five-chunk word source read with `extra` query fields under
/// `facts`.
fn word_read_with(
    extra: serde_json::Value,
    fail: Option<ErrorCode>,
    facts: &crate::source_view::SourceFacts,
) -> crate::source_view::LocalFetchResult {
    use octocode_runtime_base::execution::cancel::NeverCancel;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail });
    runtime.limits.max_chars = 100_000;
    // One line per chunk: two lines never fit one 2,048-character window.
    let text = ["keep", "drop", "maybe", "maybe", "keep"]
        .iter()
        .map(|word| format!("{word} {}\n", "x".repeat(1_500)))
        .collect::<String>();
    let mut query = json_fixture(
        r#"{
            "path": "source.txt",
            "classification": {"predicates": ["Is it kept?"]}
        }"#,
    );
    for (key, value) in extra.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    let query: LocalFetchQuery = serde_json::from_value(query).unwrap();
    crate::source_view::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        facts,
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    )
}

/// A classified read of `text` under a word judge, `max` candidates
/// per call.
fn budgeted_read(
    text: &str,
    query: &serde_json::Value,
    max: usize,
) -> crate::source_view::LocalFetchResult {
    use crate::source_view::SourceFacts;
    use octocode_runtime_base::execution::cancel::NeverCancel;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail: None });
    // `word_lines` chunks serialize to about 1,560 characters each.
    runtime.limits.max_chars = max * 1_600;
    let query: LocalFetchQuery = serde_json::from_value(query.clone()).unwrap();
    crate::source_view::process_fetched_content_selected(
        &query,
        text.as_bytes(),
        Path::new("source.txt"),
        "source.txt",
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
        Some(&runtime),
    )
}

/// One line per chunk: two lines never fit one 2,048-character window.
fn word_lines(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| format!("{word} {}\n", "x".repeat(1_500)))
        .collect()
}

mod chunks;
mod classified_reads;
mod hit_states;
mod long_window;
mod provider;
mod state_budget;
mod undecided;
