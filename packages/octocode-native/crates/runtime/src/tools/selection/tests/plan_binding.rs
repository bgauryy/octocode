//! What a plan binds: research context and canonical source
//! identity; paging controls do not, and a changed binding is stale.
use super::*;

/// A field stated at its contract default binds as its omission: a
/// continuation drops it, so its replay binds the same plan.
#[test]
fn a_field_at_its_contract_default_binds_as_its_omission() {
    let classified = json!({"matchString":["needle"],
        "classification":{"predicates":["Is this kept?"]}});
    let mut stated = classified.clone();
    stated["contextLines"] = json!({"before":0,"after":0});
    let tool = crate::id::ToolId::LocalSearch;
    assert_eq!(
        binding(tool, &stated, "source", None),
        binding(tool, &classified, "source", None)
    );
    let mut widened = classified.clone();
    widened["contextLines"] = json!({"before":2,"after":2});
    assert_ne!(
        binding(tool, &widened, "source", None),
        binding(tool, &classified, "source", None)
    );
}

#[test]
fn context_selection_binds_optional_research_context() {
    let query = json!({
        "classification":{"predicates":["Does this evidence describe retry termination?"]}
    });
    let ordinary = binding(crate::id::ToolId::LocalFetch, &query, "source", None);
    for (field, value) in [
        ("context", json!("different research context")),
        ("predicates", json!(["different predicate"])),
    ] {
        let mut contextual = query.clone();
        contextual["classification"][field] = value;
        assert_ne!(
            ordinary,
            binding(crate::id::ToolId::LocalFetch, &contextual, "source", None),
            "changing supplied {field} must invalidate the prior judgment"
        );
    }
}

#[test]
fn selected_output_paging_controls_do_not_change_assessment_binding() {
    let query = json!({"classification":{"predicates":["Find retry termination"]},
        "ranges":[{"startLine":1,"endLine":40}]});
    let mut continued = query.clone();
    continued["readProgress"] = json!({"unit":"bytes","offset":10});
    continued["selection"] = json!("saved-plan");
    assert_eq!(
        binding(crate::id::ToolId::LocalFetch, &query, "same-source", None),
        binding(
            crate::id::ToolId::LocalFetch,
            &continued,
            "same-source",
            None
        )
    );
    continued["ranges"] = json!([{"startLine":41,"endLine":80}]);
    assert_ne!(
        binding(crate::id::ToolId::LocalFetch, &query, "same-source", None),
        binding(
            crate::id::ToolId::LocalFetch,
            &continued,
            "same-source",
            None
        )
    );
}

#[test]
fn binding_uses_canonical_source_identity_across_path_spellings() {
    let absolute = binding(
        crate::id::ToolId::LocalFetch,
        &json!({"path":"/workspace/docs/readme.md","classification":{"predicates":["find evidence"]}}),
        "/workspace/docs/readme.md:abc123",
        None,
    );
    let relative = binding(
        crate::id::ToolId::LocalFetch,
        &json!({"path":"docs/readme.md","classification":{"predicates":["find evidence"]}}),
        "/workspace/docs/readme.md:abc123",
        None,
    );
    assert_eq!(absolute, relative);
    assert_ne!(
        absolute,
        binding(
            crate::id::ToolId::LocalFetch,
            &json!({"path":"docs/readme.md","classification":{"predicates":["find evidence"]}}),
            "/workspace/other/readme.md:abc123",
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
    let disabled = select_source(&runtime, &query, None, &with_states, SelectionUnit::Hits)
        .unwrap_or_else(|_| panic!("disabled plan"));
    let skipped = select_source(&runtime, &query, None, &without_states, SelectionUnit::Hits)
        .unwrap_or_else(|_| panic!("same disabled plan"));
    assert_eq!(disabled.token, skipped.token);
    assert!(skipped.selected.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    runtime.available = true;
    let selected = select_source(&runtime, &query, None, &with_states, SelectionUnit::Hits)
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
        SelectionUnit::Hits,
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
                    SelectionUnit::Hits
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
            SelectionUnit::Hits
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
            crate::id::ToolId::LocalFetch,
            &query,
            "changed source",
            Some(&selected.token),
            &without_states,
            None,
            SelectionUnit::Hits
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
