//! The replay token: sealed partitions, finished order, malformed or
//! drifted tokens, and opening tokens for candidate pages.
use super::*;

#[test]
fn context_selection_plan_returns_only_selected_ordinals_in_source_order() {
    let mut plan = Plan::new(
        "source-and-context".into(),
        4,
        SelectionStatus::Partial,
        SelectionScope::SelectedView,
        SelectionUnit::Files,
        Some("candidateBudget"),
    );
    plan.selected = vec![2, 0];
    let outcome = plan.finish();

    assert_eq!(
        outcome.selected,
        [0, 2],
        "unjudged units must not be appended or selected evidence reordered"
    );
}

#[test]
fn context_selection_failure_withholds_all_unknown_bodies() {
    let plan = Plan::new(
        "source-and-context".into(),
        3,
        SelectionStatus::Unavailable,
        SelectionScope::SelectedView,
        SelectionUnit::Files,
        Some("providerUnavailable"),
    );
    let outcome = plan.finish();

    assert!(
        outcome.selected.is_empty(),
        "a failed judgment is unknown and must not become an unfiltered body"
    );
    let receipt = serde_json::to_value(outcome.receipt).expect("receipt");
    assert_eq!(receipt["unknownItems"], 3);
    assert_eq!(receipt["excludedItems"], 0);
}

#[test]
fn partial_provider_failures_survive_replay_without_losing_successful_evidence() {
    struct PartialJudge;
    impl SelectionJudge for PartialJudge {
        fn identity(&self) -> Value {
            json!({"test":"partial-failures"})
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            for &ordinal in call.admitted.iter().skip(1) {
                call.failures.push(SelectionFailure {
                    candidate: ordinal as i64,
                    error_code:
                        octocode_contract_types::ErrorCode::ClassificationProviderError,
                    error: format!(
                        "Candidate {ordinal}: provider returned 403; access denied for ghp_abcdefghijklmnopqrstuvwxyz0123456789."
                    )
                    .try_into()
                    .unwrap(),
                    hints: Some(
                        vec!["Check provider credentials and access.".parse().unwrap()].into(),
                    ),
                });
            }
            Ok(vec![(0, Decision::Selected)])
        }
    }
    let executor = tokio::runtime::Runtime::new().unwrap();
    let mut runtime = test_runtime(std::sync::Arc::new(PartialJudge), executor.handle().clone());
    runtime.limits.max_candidates = 64;
    runtime.limits.max_chars = 100_000;
    let query = json!({"classification":{"predicates":["States the fact?"]}});
    let candidates: Vec<_> = (0..64)
        .map(|i| Candidate::new(Some(json!(format!("candidate {i}")))))
        .collect();
    let run = |token: Option<&str>| {
        select_source(&runtime, &query, token, &candidates, SelectionUnit::Chunks)
            .ok()
            .unwrap()
    };
    let fresh = run(None);
    assert_eq!(fresh.selected, [0]);
    assert_eq!(fresh.receipt.unknown_items, 63);
    assert_eq!(
        fresh.receipt.reason.as_deref(),
        Some("classificationProviderError")
    );
    assert_eq!(fresh.receipt.failures.len(), 63);
    let stored = serde_json::to_string(&decode(&fresh.token).unwrap()).unwrap();
    assert!(
        !stored.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
        "opaque replay tokens must not retain provider secrets"
    );
    // The token fits whatever bound the published `selection` field declares.
    let max =
        crate::contracts::query_schema_number(ToolId::LocalFetch, None, "selection", "maxLength");
    assert!(max.is_none_or(|max| fresh.token.len() as u64 <= max));
    let replay = run(Some(&fresh.token));
    assert_eq!(
        serde_json::to_value(&replay.receipt.failures).unwrap(),
        serde_json::to_value(&fresh.receipt.failures).unwrap()
    );
    assert_eq!(replay.selected, fresh.selected);
    let mut plan = decode(&fresh.token).unwrap();
    plan.failures[0].candidate = 0;
    plan.seal();
    assert!(
        !plan.valid(
            &plan.binding,
            candidates.len(),
            SelectionScope::SelectedView,
            SelectionUnit::Chunks
        ),
        "failure cannot overlap a successful decision"
    );
}

#[test]
fn selection_tokens_reject_malformed_and_excessively_expanded_data() {
    use std::io::Write;

    assert!(decode("not base64").is_none());
    assert!(decode(&URL_SAFE_NO_PAD.encode(b"not compressed")).is_none());
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    assert!(decode(&URL_SAFE_NO_PAD.encode(encoder.finish().unwrap())).is_none());
}

#[test]
fn token_preserves_disjoint_partitions_and_rejects_drift() {
    let limits = SelectionLimits {
        max_candidates: 2,
        max_chars: 100,
        timeout: Duration::from_secs(1),
    };
    let bound = binding(
        &json!({"classification":{"predicates":["find it"]},"page":1}),
        "source",
        limits,
        None,
    );
    let mut plan = Plan::new(
        bound.clone(),
        4,
        SelectionStatus::Partial,
        SelectionScope::SelectedView,
        SelectionUnit::Files,
        Some("candidateBudget"),
    );
    plan.selected = vec![2, 0];
    plan.seal();
    assert!(plan.valid(
        &bound,
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
    let outcome = plan.clone().finish();
    assert_eq!(outcome.selected, vec![0, 2]);
    assert_eq!(outcome.receipt.selected_items, 2);
    let decoded: Plan = decode(&outcome.token).unwrap();
    assert!(decoded.valid(
        &bound,
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
    assert!(!decoded.valid(
        "changed",
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
    assert!(!decoded.valid(
        &bound,
        4,
        SelectionScope::ProviderPage,
        SelectionUnit::Files
    ));
    let mut previous_policy = decoded.clone();
    previous_policy.version = VERSION - 1;
    previous_policy.seal();
    assert!(!previous_policy.valid(
        &bound,
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
    plan.selected = vec![2, 2];
    plan.seal();
    assert!(!plan.valid(
        &bound,
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
    plan.selected = vec![2, 4];
    plan.seal();
    assert!(!plan.valid(
        &bound,
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
    plan.selected = vec![0];
    plan.excluded = vec![0];
    plan.seal();
    assert!(
        !plan.valid(
            &bound,
            4,
            SelectionScope::SelectedView,
            SelectionUnit::Files
        ),
        "selected and excluded partitions must be disjoint"
    );
    assert!(
        !decoded.valid(
            &bound,
            4,
            SelectionScope::SelectedView,
            SelectionUnit::Chunks
        ),
        "the same ordinals cannot be reinterpreted as another evidence unit"
    );
    let mut tampered = decoded;
    tampered.selected = vec![2, 0];
    assert!(!tampered.valid(
        &bound,
        4,
        SelectionScope::SelectedView,
        SelectionUnit::Files
    ));
}

/// An opening token carries no decisions: one that claims any is stale.
#[test]
fn an_opening_token_with_decisions_is_stale() {
    let limits = crate::execution::RuntimeLimits::DEFAULT.selection;
    let bound = binding(&json!({}), "source", limits, None);
    let mut plan = Plan::new(
        bound.clone(),
        4,
        SelectionStatus::Partial,
        SelectionScope::SelectedView,
        SelectionUnit::Chunks,
        None,
    );
    plan.pool = vec![[1, 3]];
    plan.opens = true;
    plan.seal();
    let valid = |plan: &Plan| {
        plan.valid(
            &bound,
            4,
            SelectionScope::SelectedView,
            SelectionUnit::Chunks,
        )
    };
    assert!(valid(&plan));
    let decoded = decode(&plan.clone().finish().token).expect("round trip");
    assert_eq!(decoded.pool, [[1, 3]]);
    plan.selected = vec![1];
    plan.seal();
    assert!(!valid(&plan));
    plan.selected = vec![0];
    plan.opens = false;
    plan.seal();
    assert!(!valid(&plan), "a decision outside the pool");
    plan.pool = vec![[3, 5]];
    plan.selected.clear();
    plan.seal();
    assert!(!valid(&plan), "a pool past the candidates");
}
