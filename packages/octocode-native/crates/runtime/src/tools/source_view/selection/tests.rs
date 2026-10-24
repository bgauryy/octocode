use super::*;
use crate::security::ContentSecurity;
use crate::tools::selection::{Decision, JudgeCall, Runtime, SelectionJudge};
use crate::tools::source_view::ReadStatus;
use crate::tools::source_view::local_fetch_query;
use crate::tools::test_support::selection_fake::{self, DOWN, REFUSED, WordJudge};
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

struct BudgetJudge(Arc<AtomicUsize>);

impl SelectionJudge for BudgetJudge {
    fn identity(&self) -> serde_json::Value {
        json!({"test":"budget"})
    }

    fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
        self.0.fetch_add(1, Ordering::SeqCst);
        let sent_chars = call
            .admitted
            .iter()
            .map(|&ordinal| {
                call.candidates[ordinal]
                    .state
                    .as_ref()
                    .unwrap()
                    .to_string()
                    .chars()
                    .count()
            })
            .sum::<usize>();
        assert!(
            sent_chars <= call.limits.max_chars,
            "sent {sent_chars} serialized state characters with budget {}",
            call.limits.max_chars
        );
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

fn word_read(fail: Option<ErrorCode>) -> crate::tools::source_view::LocalFetchResult {
    word_read_with(
        json!({}),
        fail,
        &crate::tools::source_view::SourceFacts::default(),
    )
}

/// The five-chunk word source read with `extra` query fields under
/// `facts`.
fn word_read_with(
    extra: serde_json::Value,
    fail: Option<ErrorCode>,
    facts: &crate::tools::source_view::SourceFacts,
) -> crate::tools::source_view::LocalFetchResult {
    use crate::execution::cancel::NeverCancel;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail });
    runtime.limits.max_chars = 100_000;
    // One line per chunk: two lines never fit one 2,048-character window.
    let text = ["keep", "drop", "maybe", "maybe", "keep"]
        .iter()
        .map(|word| format!("{word} {}\n", "x".repeat(1_500)))
        .collect::<String>();
    let mut query = json!({
        "path":"source.txt",
        "classification":{"predicates":["Is it kept?"]}
    });
    for (key, value) in extra.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    let query: LocalFetchQuery = serde_json::from_value(query).unwrap();
    crate::tools::source_view::process_fetched_content_selected(
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
) -> crate::tools::source_view::LocalFetchResult {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::source_view::SourceFacts;
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = budget_runtime(executor.handle().clone(), Arc::new(AtomicUsize::new(0)));
    runtime.judge = Arc::new(WordJudge { fail: None });
    runtime.limits.max_chars = 1_000_000;
    runtime.limits.max_candidates = max;
    let query: LocalFetchQuery = serde_json::from_value(query.clone()).unwrap();
    crate::tools::source_view::process_fetched_content_selected(
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
mod provider;
mod state_budget;
