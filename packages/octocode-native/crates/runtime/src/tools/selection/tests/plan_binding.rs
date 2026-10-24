//! What a plan binds: research context, limits and canonical source
//! identity; paging controls do not, and a changed binding is stale.
use super::*;

#[test]
fn context_selection_binds_optional_research_context() {
    let limits = SelectionLimits {
        max_candidates: 4,
        max_chars: 100,
        timeout: Duration::from_secs(1),
    };
    let query = json!({
        "classification":{"predicates":["Does this evidence describe retry termination?"]}
    });
    let ordinary = binding(&query, "source", limits, None);
    for (field, value) in [
        ("context", json!("different research context")),
        ("predicates", json!(["different predicate"])),
    ] {
        let mut contextual = query.clone();
        contextual["classification"][field] = value;
        assert_ne!(
            ordinary,
            binding(&contextual, "source", limits, None),
            "changing supplied {field} must invalidate the prior judgment"
        );
    }
}

#[test]
fn selected_output_paging_controls_do_not_change_assessment_binding() {
    let limits = SelectionLimits {
        max_candidates: 4,
        max_chars: 100,
        timeout: Duration::from_secs(1),
    };
    let query = json!({"classification":{"predicates":["Find retry termination"]},
        "ranges":[{"startLine":1,"endLine":40}]});
    let mut continued = query.clone();
    continued["readProgress"] = json!({"unit":"bytes","offset":10});
    continued["selection"] = json!("saved-plan");
    assert_eq!(
        binding(&query, "same-source", limits, None),
        binding(&continued, "same-source", limits, None)
    );
    continued["ranges"] = json!([{"startLine":41,"endLine":80}]);
    assert_ne!(
        binding(&query, "same-source", limits, None),
        binding(&continued, "same-source", limits, None)
    );
}

/// A plan binds the limits it was judged under: another candidate,
/// character or deadline budget is a different binding, so its replay is
/// stale. Replaces the local-ranking harness case that varied the removed
/// OCTOCODE_CLASSIFICATION_* budget variables per process.
#[test]
fn changed_selection_limits_invalidate_the_plan_binding() {
    let limits = crate::execution::RuntimeLimits::DEFAULT.selection;
    let query = json!({"classification":{"predicates":["Find retry termination"]}});
    let base = binding(&query, "source", limits, None);
    assert_eq!(base, binding(&query, "source", limits, None));
    for changed in [
        SelectionLimits {
            max_candidates: 2,
            ..limits
        },
        SelectionLimits {
            max_chars: 4_096,
            ..limits
        },
        SelectionLimits {
            timeout: Duration::from_secs(9),
            ..limits
        },
    ] {
        assert_ne!(
            base,
            binding(&query, "source", changed, None),
            "{changed:?}"
        );
    }
}

#[test]
fn binding_uses_canonical_source_identity_across_path_spellings() {
    let limits = SelectionLimits {
        max_candidates: 0,
        max_chars: 0,
        timeout: Duration::ZERO,
    };
    let absolute = binding(
        &json!({"path":"/workspace/docs/readme.md","classification":{"predicates":["find evidence"]}}),
        "/workspace/docs/readme.md:abc123",
        limits,
        None,
    );
    let relative = binding(
        &json!({"path":"docs/readme.md","classification":{"predicates":["find evidence"]}}),
        "/workspace/docs/readme.md:abc123",
        limits,
        None,
    );
    assert_eq!(absolute, relative);
    assert_ne!(
        absolute,
        binding(
            &json!({"path":"docs/readme.md","classification":{"predicates":["find evidence"]}}),
            "/workspace/other/readme.md:abc123",
            limits,
            None,
        )
    );
}

#[test]
fn skipped_and_replayed_plans_keep_the_order_without_candidate_states() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    let query = json!({"classification":{"predicates":["find evidence"],"context":"Explain retry policy and stopping conditions"}});
    let with_states = [
        Candidate::new(Some(json!({"text":"first"}))),
        Candidate::new(Some(json!({"text":"second"}))),
    ];
    let without_states = [Candidate::new(None), Candidate::new(None)];
    runtime.available = false;
    let disabled = select_source(&runtime, &query, None, &with_states, SelectionUnit::Files)
        .unwrap_or_else(|_| panic!("disabled plan"));
    let skipped = select_source(
        &runtime,
        &query,
        None,
        &without_states,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("same disabled plan"));
    assert_eq!(disabled.token, skipped.token);
    assert!(skipped.selected.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    runtime.available = true;
    let selected = select_source(&runtime, &query, None, &with_states, SelectionUnit::Files)
        .unwrap_or_else(|_| panic!("selected plan"));
    assert_eq!(selected.selected, [0, 1]);
    assert!(!needs_candidate_states(
        Some(&runtime),
        Some(&selected.token),
        None,
        2
    ));
    let replay = select_source(
        &runtime,
        &query,
        Some(&selected.token),
        &without_states,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("replay without evidence states"));
    assert_eq!(replay.token, selected.token);
    assert_eq!(replay.selected, selected.selected);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for (field, value) in [
        ("context", json!("different research context")),
        ("predicates", json!(["different predicate"])),
    ] {
        let mut changed = query.clone();
        changed["classification"][field] = value;
        assert!(
            matches!(
                select_source(
                    &runtime,
                    &changed,
                    Some(&selected.token),
                    &without_states,
                    SelectionUnit::Files
                ),
                Err(Failure::Stale)
            ),
            "changing {field} must invalidate a saved selection"
        );
    }
    let mut changed_capability = query.clone();
    changed_capability["classification"]["longContext"] = json!(true);
    assert!(matches!(
        select_source(
            &runtime,
            &changed_capability,
            Some(&selected.token),
            &without_states,
            SelectionUnit::Files
        ),
        Err(Failure::Stale)
    ));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "stale context does not rejudge one page"
    );
    assert!(matches!(
        select(
            Some(&runtime),
            &query,
            "changed source",
            Some(&selected.token),
            &without_states,
            None,
            SelectionUnit::Files
        ),
        Err(Failure::Stale)
    ));
}

#[test]
fn context_selection_still_assesses_a_single_candidate() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = test_runtime(std::sync::Arc::new(FailedJudge), executor.handle().clone());

    assert!(
        needs_candidate_states(Some(&runtime), None, None, 1),
        "one unread candidate can be irrelevant and must be assessed"
    );
    assert!(!needs_candidate_states(
        Some(&runtime),
        Some("saved-plan"),
        None,
        1
    ));
}
