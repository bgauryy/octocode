//! Candidate pages: each judges what the page before left unjudged, so a
//! chain judges every eligible ordinal once.
use super::*;

#[test]
fn candidate_pages_walk_in_source_order_and_a_large_state_runs_alone() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    runtime.limits.max_chars = 64;
    let candidates = [
        Candidate::new(Some(json!({"content":"first"}))),
        Candidate::new(Some(json!({"content":"x".repeat(64)}))),
        Candidate::new(Some(json!({"content":"second"}))),
        Candidate::new(None),
    ];
    let query = json!({"classification":{"predicates":["find evidence"]}});
    let run = |prior| {
        select_source(&runtime, &query, prior, &candidates, SelectionUnit::Hits)
            .unwrap_or_else(|_| panic!("mixed candidate page"))
    };
    let first = run(None);
    assert_eq!(first.selected, [0]);
    assert_eq!(first.unjudged, [1, 2]);
    assert_eq!(
        first.receipt.reason.as_deref(),
        Some("candidateEligibilityAndBudget")
    );
    let second = run(Some(first.next_candidates().expect("the rest")));
    assert_eq!(second.selected, [1]);
    assert_eq!(second.unjudged, [2]);
    let last = run(Some(second.next_candidates().expect("the rest")));
    assert_eq!(last.selected, [2]);
    assert!(last.next_candidates().is_none());
    assert_eq!(last.receipt.total_items, 1);
    assert_eq!(last.receipt.status, SelectionStatus::Complete);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

/// Candidate pages: each judges the budget's worth of what the page
/// before left unjudged, so a chain judges every eligible ordinal
/// exactly once; a later page's plan replays like the first, and a
/// candidate without state stays unknown on the first page only.
#[test]
fn a_chain_of_candidate_pages_judges_every_ordinal_once() {
    struct Recording(std::sync::Mutex<Vec<usize>>);
    impl SelectionJudge for Recording {
        fn identity(&self) -> Value {
            json!({"test":"recording"})
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            self.0.lock().expect("lock").extend(call.admitted);
            Ok(call
                .admitted
                .iter()
                .map(|&i| {
                    let decision = if i % 2 == 0 {
                        Decision::Selected
                    } else {
                        Decision::Excluded
                    };
                    (i, decision)
                })
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = std::sync::Arc::new(Recording(std::sync::Mutex::new(Vec::new())));
    let mut runtime = test_runtime(judge.clone(), executor.handle().clone());
    // Three candidates of 24 serialized characters a page.
    runtime.limits.max_chars = 75;
    let mut candidates = (0..8)
        .map(|i| Candidate::new(Some(json!({"content":format!("evidence {i}")}))))
        .collect::<Vec<_>>();
    candidates.push(Candidate::new(None));
    let query = json!({"classification":{"predicates":["evidence"]}});
    let run = |prior: Option<&str>| {
        select_source(&runtime, &query, prior, &candidates, SelectionUnit::Chunks)
            .unwrap_or_else(|_| panic!("candidate page"))
    };
    let mut prior: Option<String> = None;
    let mut totals = Vec::new();
    let mut shown = Vec::new();
    let mut second_page = None;
    loop {
        assert!(needs_candidate_states(
            Some(&runtime),
            prior.as_deref(),
            None,
            candidates.len()
        ));
        let outcome = run(prior.as_deref());
        totals.push(outcome.receipt.total_items);
        shown.extend(outcome.selected.iter().copied());
        if totals.len() == 2 {
            second_page = Some((outcome.token.clone(), outcome.selected.clone()));
        }
        match outcome.next_candidates() {
            Some(next) => prior = Some(next.to_owned()),
            None => break,
        }
        assert!(totals.len() < 10, "the chain ends");
    }
    assert_eq!(totals, [9, 5, 2], "each page counts its own pool");
    let mut judged = judge.0.lock().expect("lock").clone();
    judged.sort_unstable();
    assert_eq!(judged, (0..8).collect::<Vec<_>>(), "each judged once");
    shown.sort_unstable();
    assert_eq!(shown, [0, 2, 4, 6]);
    let (token, selected) = second_page.expect("second page");
    assert!(!needs_candidate_states(
        Some(&runtime),
        Some(&token),
        None,
        9
    ));
    let replay = run(Some(&token));
    assert_eq!(replay.selected, selected);
    assert_eq!(replay.receipt.total_items, 5);
    assert!(
        (0..9).filter(|&o| replay.considers(o)).count() == 5,
        "the replay keeps its pool"
    );
    assert_eq!(
        judge.0.lock().expect("lock").len(),
        8,
        "a replay never judges"
    );
}

#[test]
fn provider_candidate_pages_cover_the_same_page_and_refuse_source_drift() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    runtime.limits.max_chars = 1;
    let mut files: Vec<Value> = (0..5)
        .map(|i| json!({"path":format!("{i}.rs"),"lines":[format!("1\tfn candidate_{i}() {{}}")]}))
        .collect();
    files.push(json!({"path":"oversized.rs","lines":[format!("1\t{}", "x".repeat(2_000))]}));
    let original = json!({"results":[{"index":0,"data":{
        "owner":"o","repo":"r","files":files,
        "next":{"nextPage":{"kind":"continue","tool":"ghSearchCode","input":{"queries":[{
            "owner":"o","repo":"r","keywords":["fn"],"page":4
        }]}}}
    }}]});
    let mut query = json!({"owner":"o","repo":"r","page":3,"keywords":["fn"],
        "classification":{"predicates":["Does this define a function?"]}});
    let mut seen = HashSet::new();
    let mut first_next = None;
    for page in 0..6 {
        let mut output = original.clone();
        apply_code_rows(&mut output, query.clone(), &mut query.clone(), &runtime)
            .expect("candidate page");
        let data = &output["results"][0]["data"];
        for file in data["files"].as_array().expect("selected files") {
            assert!(seen.insert(file["path"].as_str().unwrap().to_owned()));
        }
        assert_eq!(data["next"]["nextPage"]["input"]["queries"][0]["page"], 4);
        if page == 0 {
            assert_eq!(data["selection"]["totalItems"], 6);
            assert_eq!(data["selection"]["unknownItems"], 5);
            assert_eq!(data["selection"]["reason"], "candidateBudget");
            assert!(data["next"]["unfiltered"].is_object());
        }
        let next = data["next"]["nextCandidatePage"]["input"]["queries"][0].clone();
        if page < 5 {
            assert_eq!(next["page"], 3);
            assert!(next["selection"].is_string());
            first_next.get_or_insert_with(|| next.clone());
            query = next;
        } else {
            assert!(next.is_null(), "the walk ends");
        }
    }
    assert_eq!(
        seen.len(),
        6,
        "every file reached once, the oversized one too"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    let query = first_next.expect("continuation");
    let mut changed = original;
    changed["results"][0]["data"]["files"][0]["lines"] = json!(["1\tchanged"]);
    apply_code_rows(&mut changed, query.clone(), &mut query.clone(), &runtime)
        .expect("typed stale error");
    assert_eq!(changed["results"][0]["data"]["errorCode"], "staleSnapshot");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        6,
        "drift cannot trigger inference"
    );
}
