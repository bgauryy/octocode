//! Which candidates a judgment admits under the candidate and character
//! budgets, and the receipt each budget leaves.
use super::*;

#[test]
fn bounded_chunk_admission_reaches_relevant_evidence_after_the_prefix() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let (mut runtime, calls) = selecting_runtime(&executor);
    runtime.limits.max_candidates = 2;
    let mut candidates = (0..100)
        .map(|_| Candidate::new(Some(json!({"content":"unrelated rendering code"}))))
        .collect::<Vec<_>>();
    candidates[75] = Candidate::new(Some(
        json!({"content":"fn retryConnection() { retry_connection(); }"}),
    ));
    candidates[99] = Candidate::new(Some(
        json!({"content":"fn connectionRetry() { connection_retry(); }"}),
    ));
    let query = json!({"classification":{"predicates":["retry connection"]}});
    let outcome = select_source(&runtime, &query, None, &candidates, SelectionUnit::Chunks)
        .unwrap_or_else(|_| panic!("selection failed"));
    assert_eq!(outcome.selected, [75, 99]);
    assert_eq!(outcome.receipt.unknown_items, 98);
    let replay = select_source(
        &runtime,
        &query,
        Some(&outcome.token),
        &candidates,
        SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("replay failed"));
    assert_eq!(replay.selected, outcome.selected);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

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
        select_source(&runtime, query, token, &candidates, SelectionUnit::Files)
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
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("budgeted order"));
    assert_eq!(outcome.selected, [0, 2]);
    assert_eq!(outcome.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        outcome.receipt.reason.as_deref(),
        Some("candidateStateTooLarge")
    );
    assert_eq!(outcome.receipt.selected_items, 2);
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
    let eligible = select_source(&runtime, &query, None, &candidates, SelectionUnit::Files)
        .unwrap_or_else(|_| panic!("partial order"));
    assert_eq!(eligible.selected, [0, 1]);
    assert_eq!(
        eligible.receipt.reason.as_deref(),
        Some("candidateIneligible")
    );
    runtime.limits.max_candidates = 1;
    let budgeted = select_source(&runtime, &query, None, &candidates, SelectionUnit::Files)
        .unwrap_or_else(|_| panic!("budgeted order"));
    assert_eq!(budgeted.selected, [0]);
    assert_eq!(
        budgeted.receipt.reason.as_deref(),
        Some("candidateEligibilityAndBudget")
    );
    runtime.limits.max_candidates = 64;
    let no_states = [Candidate::new(None), Candidate::new(None)];
    let skipped = select_source(&runtime, &query, None, &no_states, SelectionUnit::Files)
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
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("single candidate should be judged"));
    assert!(outcome.selected.is_empty());
    assert_eq!(outcome.receipt.status, SelectionStatus::Complete);
    assert_eq!(outcome.receipt.excluded_items, 1);
    assert_eq!(outcome.receipt.unknown_items, 0);
}

#[test]
fn oversized_candidate_is_terminal_unknown_without_provider_work() {
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
    assert!(outcome.next_candidates(ToolId::LocalFetch).is_none());
    assert!(outcome.unjudged.is_empty());
    assert_eq!(outcome.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        outcome.receipt.reason.as_deref(),
        Some("candidateStateTooLarge")
    );
    assert_eq!(outcome.receipt.unknown_items, 1);
    assert_eq!(outcome.receipt.excluded_items, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn cancelled_judgment_never_becomes_a_fallback_page() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let mut runtime = selection_fake::runtime(
        FailedJudge,
        executor.handle().clone(),
        Duration::from_secs(1),
    );
    runtime.limits.max_candidates = 2;
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
            SelectionUnit::Files
        ),
        Err(Failure::Cancelled)
    ));
}
