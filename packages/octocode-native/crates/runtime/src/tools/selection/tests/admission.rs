//! Which candidates a judgment admits under the candidate and character
//! budgets, and the receipt each budget leaves.
use super::*;

#[test]
fn inline_usage_is_always_disclosed_and_plan_replay_reports_no_new_spend() {
    struct UsageJudge;
    impl SelectionJudge for UsageJudge {
        fn identity(&self) -> Value {
            json!({"test":"usage"})
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            call.usage.add_usage(2, 1, 10, 3);
            Ok(call
                .admitted
                .iter()
                .map(|&i| (i, Decision::Selected))
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = test_runtime(std::sync::Arc::new(UsageJudge), executor.handle().clone());
    let query = json!({"classification":{"predicates":["Does this state the fact?"]}});
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    let run = |query: &Value, token: Option<&str>| {
        select_source(&runtime, query, token, &candidates, SelectionUnit::Hits)
            .ok()
            .expect("selection")
    };
    let fresh = run(&query, None);
    let usage = serde_json::to_value(&fresh.receipt.usage).unwrap();
    assert_eq!(usage["calls"], 2);
    assert_eq!(usage["knownUsageCalls"], 1);
    assert_eq!(usage["unknownUsageCalls"], 1);
    assert_eq!(usage["inputTokens"], 10);
    let replay = run(&query, Some(&fresh.token));
    let usage = serde_json::to_value(&replay.receipt.usage).unwrap();
    assert_eq!(usage["calls"], 0);
    assert_eq!(usage["inputTokens"], 0);
    assert_eq!(replay.selected, fresh.selected);
}

#[test]
fn candidate_budget_counts_serialized_characters_in_source_order() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    let candidates = [
        Candidate::new(Some(json!({"text":"é\n\""}))),
        Candidate::new(Some(json!({"text":"longer than the remaining budget"}))),
        Candidate::new(Some(json!({"text":"é\n\""}))),
    ];
    assert_eq!(candidates[0].evidence_chars, 16);
    runtime.limits.max_chars = 32;
    let outcome = select_source(
        &runtime,
        &json!({"classification":{"predicates":["find evidence"]}}),
        None,
        &candidates,
        SelectionUnit::Hits,
    )
    .unwrap_or_else(|_| panic!("budgeted order"));
    // Source order: the page stops at the candidate over its budget, and the
    // next page takes the rest.
    assert_eq!(outcome.selected, [0]);
    assert_eq!(outcome.receipt.status, SelectionStatus::Partial);
    assert_eq!(outcome.receipt.reason.as_deref(), Some("candidateBudget"));
    assert_eq!(outcome.receipt.selected_items, 1);
    assert_eq!(outcome.unjudged, [1, 2]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn partial_receipts_distinguish_ineligible_evidence_from_budgets() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, _) = selecting_runtime(&executor);
    let candidates = [
        Candidate::new(Some(json!({"text":"first"}))),
        Candidate::new(Some(json!({"text":"second"}))),
        Candidate::new(None),
    ];
    let query = json!({"classification":{"predicates":["find evidence"]}});
    let eligible = select_source(&runtime, &query, None, &candidates, SelectionUnit::Hits)
        .unwrap_or_else(|_| panic!("partial order"));
    assert_eq!(eligible.selected, [0, 1]);
    assert_eq!(
        eligible.receipt.reason.as_deref(),
        Some("candidateIneligible")
    );
    runtime.limits.max_chars = 1;
    let budgeted = select_source(&runtime, &query, None, &candidates, SelectionUnit::Hits)
        .unwrap_or_else(|_| panic!("budgeted order"));
    assert_eq!(budgeted.selected, [0]);
    assert_eq!(
        budgeted.receipt.reason.as_deref(),
        Some("candidateEligibilityAndBudget")
    );
    let no_states = [Candidate::new(None), Candidate::new(None)];
    let skipped = select_source(&runtime, &query, None, &no_states, SelectionUnit::Hits)
        .unwrap_or_else(|_| panic!("ineligible order"));
    assert_eq!(skipped.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        skipped.receipt.reason.as_deref(),
        Some("candidateIneligible")
    );
}

#[test]
fn single_candidate_is_assessed_and_can_be_excluded() {
    struct ExcludingJudge;
    impl SelectionJudge for ExcludingJudge {
        fn identity(&self) -> Value {
            json!({"backend":"exclude"})
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            assert_eq!(call.admitted, [0]);
            Ok(vec![(0, Decision::Excluded)])
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = test_runtime(
        std::sync::Arc::new(ExcludingJudge),
        executor.handle().clone(),
    );
    let candidate = [Candidate::new(Some(json!({"text":"sole evidence"})))];
    let outcome = select_source(
        &runtime,
        &json!({"classification":{"predicates":["find evidence"]}}),
        None,
        &candidate,
        SelectionUnit::Hits,
    )
    .unwrap_or_else(|_| panic!("single candidate should be judged"));
    assert!(outcome.selected.is_empty());
    assert_eq!(outcome.receipt.status, SelectionStatus::Complete);
    assert_eq!(outcome.receipt.excluded_items, 1);
    assert_eq!(outcome.receipt.unknown_items, 0);
}

/// A candidate larger than a whole page still runs, alone: the provider's own
/// window decides whether it can be judged.
#[test]
fn a_candidate_over_the_page_budget_runs_alone() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let candidates = [Candidate::new(Some(json!({
        "content": "x".repeat(runtime.limits.max_chars)
    })))];
    let outcome = select_source(
        &runtime,
        &json!({"classification":{"predicates":["find evidence"]}}),
        None,
        &candidates,
        SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("oversized candidate"));
    assert_eq!(outcome.selected, [0]);
    assert_eq!(outcome.receipt.status, SelectionStatus::Complete);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn cancelled_judgment_never_becomes_a_fallback_page() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let cancellation = octocode_io::cancellation::Cancellation::new();
    cancellation.cancel();
    let mut runtime = selection_fake::runtime(
        FailedJudge,
        executor.handle().clone(),
        Duration::from_secs(1),
    );
    runtime.limits.max_chars = 100;
    runtime.context.cancellation = cancellation;
    let candidates = [
        Candidate::new(Some(json!({"text":"first evidence"}))),
        Candidate::new(Some(json!({"text":"second evidence"}))),
    ];
    assert!(matches!(
        select_source(
            &runtime,
            &json!({"classification":{"predicates":["find evidence"]}}),
            None,
            &candidates,
            SelectionUnit::Hits
        ),
        Err(Failure::Cancelled)
    ));
}

/// A fresh call over `source`, judged by `runtime`.
fn fresh(runtime: &Runtime, candidates: &[Candidate], query: &Value, source: &str) -> Outcome {
    select(
        Some(runtime),
        crate::id::ToolId::LocalFetch,
        query,
        source,
        None,
        candidates,
        None,
        SelectionUnit::Hits,
    )
    .ok()
    .expect("selection")
}

fn fact_query(predicate: &str) -> Value {
    json!({"classification":{"predicates":[predicate]}})
}

/// A fresh call that repeats a judged one (same query, source and judge)
/// replays its saved judgment instead of billing again.
#[test]
fn an_identical_fresh_call_replays_the_saved_judgment_without_a_provider_call() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    let query = fact_query("Does this state the fact?");
    let first = fresh(&runtime, &candidates, &query, "source");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let again = fresh(&runtime, &candidates, &query, "source");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the repeat bills nothing");
    assert_eq!(again.selected, first.selected);
    assert_eq!(again.token, first.token);
    let usage = serde_json::to_value(&again.receipt.usage).unwrap();
    assert_eq!(usage["calls"], 0);
}

/// The replay is bound to the query.
#[test]
fn a_changed_query_judges_again_after_a_saved_judgment() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    fresh(
        &runtime,
        &candidates,
        &fact_query("Does this state the fact?"),
        "source",
    );
    fresh(
        &runtime,
        &candidates,
        &fact_query("Does this state another fact?"),
        "source",
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// The replay is bound to the source digest: an edited file is another source.
#[test]
fn a_changed_source_judges_again_after_a_saved_judgment() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    let query = fact_query("Does this state the fact?");
    fresh(&runtime, &candidates, &query, "source digest one");
    fresh(&runtime, &candidates, &query, "source digest two");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// The replay is bound to the judge, though the saved pages are shared.
#[test]
fn a_changed_judge_judges_again_after_a_saved_judgment() {
    struct OtherJudge(std::sync::Arc<AtomicUsize>);
    impl SelectionJudge for OtherJudge {
        fn identity(&self) -> Value {
            json!({"backend":"other-test"})
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
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    let query = fact_query("Does this state the fact?");
    fresh(&runtime, &candidates, &query, "source");
    let other = Runtime {
        judge: std::sync::Arc::new(OtherJudge(calls.clone())),
        saved: runtime.saved.clone(),
        ..test_runtime(std::sync::Arc::new(FailedJudge), executor.handle().clone())
    };
    fresh(&other, &candidates, &query, "source");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// The receipt says a page repeats an earlier judgment, and a judged page
/// does not.
#[test]
fn a_replayed_page_discloses_replayed_and_a_judged_page_omits_it() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, _) = selecting_runtime(&executor);
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    let query = fact_query("Does this state the fact?");
    let first = fresh(&runtime, &candidates, &query, "source");
    let usage = serde_json::to_value(&first.receipt.usage).unwrap();
    assert!(usage.get("replayed").is_none(), "{usage}");
    let again = fresh(&runtime, &candidates, &query, "source");
    let usage = serde_json::to_value(&again.receipt.usage).unwrap();
    assert_eq!(usage["replayed"], true, "{usage}");
}
