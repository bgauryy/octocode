//! Candidate pages: each judges what the page before left unjudged, so a
//! chain judges every eligible ordinal once.
use super::*;
use octocode_contract_types::SelectionStatus;
use octocode_contract_types::SelectionUnit;
use octocode_test_support::json_fixture;
use octocode_tools_shared::selection::Candidate;
use octocode_tools_shared::selection::Decision;
use octocode_tools_shared::selection::JudgeCall;
use octocode_tools_shared::selection::Judgments;
use octocode_tools_shared::selection::SelectionJudge;
use octocode_tools_shared::selection::kept_note;
use octocode_tools_shared::selection::needs_candidate_states;
use octocode_tools_shared::test_support::selection_runtime::select_source;
use octocode_tools_shared::test_support::selection_runtime::selecting_runtime;
use octocode_tools_shared::test_support::selection_runtime::test_runtime;
use serde_json::Value;
use serde_json::json;

#[test]
fn candidate_pages_walk_in_source_order_and_a_large_state_runs_alone() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    runtime.limits.max_chars = 64;
    let candidates = [
        Candidate::new(Some(json_fixture(r#"{"content": "first"}"#))),
        Candidate::new(Some(json!({"content":"x".repeat(64)}))),
        Candidate::new(Some(json_fixture(r#"{"content": "second"}"#))),
        Candidate::new(None),
    ];
    let query = json_fixture(r#"{"classification": {"predicates": ["find evidence"]}}"#);
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
    // The last page counts the whole walk: the candidate with no state was
    // never judged, so the walk stays partial, for the first page's reason.
    assert_eq!(last.receipt.total_items, 4);
    assert_eq!(
        (last.receipt.selected_items, last.receipt.unknown_items),
        (3, 1)
    );
    assert_eq!(last.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        last.receipt.reason.as_deref(),
        Some("candidateEligibilityAndBudget")
    );
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
            json_fixture(r#"{"test": "recording"}"#)
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
    let query = json_fixture(r#"{"classification": {"predicates": ["evidence"]}}"#);
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
    assert_eq!(totals, [9, 9, 9], "each page counts the whole walk");
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
    assert_eq!(replay.receipt.total_items, 9);
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

/// The last candidate page tallies the whole walk: every unit judged, the
/// unsure ones unknown, and judging complete once nothing waits for a later
/// page. Its note separates units not judged yet from unknown answers.
#[test]
fn the_last_candidate_page_tallies_the_whole_walk() {
    struct Thirds;
    impl SelectionJudge for Thirds {
        fn identity(&self) -> Value {
            json_fixture(r#"{"test": "thirds"}"#)
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&i| {
                    let decision = match i % 3 {
                        0 => Decision::Selected,
                        1 => Decision::Excluded,
                        _ => Decision::Unknown,
                    };
                    (i, decision)
                })
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let mut runtime = test_runtime(std::sync::Arc::new(Thirds), executor.handle().clone());
    runtime.limits.max_chars = 75;
    let candidates = (0..9)
        .map(|i| Candidate::new(Some(json!({"content":format!("evidence {i}")}))))
        .collect::<Vec<_>>();
    let query = json_fixture(r#"{"classification": {"predicates": ["evidence"]}}"#);
    let run = |prior: Option<&str>| {
        select_source(&runtime, &query, prior, &candidates, SelectionUnit::Hits)
            .unwrap_or_else(|_| panic!("candidate page"))
    };
    let first = run(None);
    assert_eq!(first.receipt.status, SelectionStatus::Partial);
    assert_eq!(first.receipt.reason.as_deref(), Some("candidateBudget"));
    assert_eq!(
        (first.receipt.total_items, first.receipt.unknown_items),
        (9, 7)
    );
    assert_eq!(
        kept_note(&first, first.selected.len()).as_deref(),
        Some(
            "classification passed 1 of 9 hits; 6 hits not judged yet: next.nextCandidatePage judges them."
        )
    );
    let mut last = first;
    while let Some(next) = last.next_candidates().map(str::to_owned) {
        last = run(Some(&next));
    }
    let receipt = &last.receipt;
    assert_eq!(receipt.status, SelectionStatus::Complete);
    assert_eq!(receipt.reason.as_deref(), Some("classificationUnknown"));
    assert_eq!(
        (
            receipt.total_items,
            receipt.selected_items,
            receipt.excluded_items,
            receipt.unknown_items
        ),
        (9, 3, 3, 3)
    );
    assert_eq!(
        kept_note(&last, last.selected.len()).as_deref(),
        Some(
            "classification passed 3 of 9 hits (1 on this page); none is left to judge, and 3 hits stay unknown."
        )
    );
}

/// A complete selection that kept nothing because every hit was a near miss
/// says so, and where they are: complete is not full recall.
#[test]
fn the_kept_note_names_hidden_near_misses() {
    struct Related;
    impl SelectionJudge for Related {
        fn identity(&self) -> serde_json::Value {
            json_fixture(r#"{"test": "related"}"#)
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&i| (i, Decision::Related))
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = test_runtime(std::sync::Arc::new(Related), executor.handle().clone());
    let candidates = (0..3)
        .map(|i| Candidate::new(Some(json!({"content":format!("evidence {i}")}))))
        .collect::<Vec<_>>();
    let query = json_fixture(r#"{"classification": {"predicates": ["evidence"]}}"#);
    let outcome = select_source(&runtime, &query, None, &candidates, SelectionUnit::Hits)
        .unwrap_or_else(|_| panic!("selection"));
    assert_eq!(outcome.receipt.status, SelectionStatus::Complete);
    assert_eq!(
        kept_note(&outcome, 0).as_deref(),
        Some(
            "classification passed 0 of 3 hits; 3 hits judged related (near misses, hidden): next.unfiltered shows them. An empty classified result is not absence: next.unfiltered lists them all."
        )
    );
}

/// A ghSearchCode page whose evaluations the default window refused and the
/// long-context window answered warns on its row with the tokens billed.
#[test]
fn a_provider_page_discloses_its_long_context_retries() {
    use octocode_tools_shared::selection::long_window_fake::{BILLED, LongWindow};
    let window = LongWindow::start();
    let files: Vec<Value> = (0..2)
        .map(|i| json!({"path":format!("{i}.rs"),"matches":[{"line":1,"value":format!("fn candidate_{i}() {{}}")}]}))
        .collect();
    let mut output = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r","files":files}}]});
    let query = json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "keywords": ["fn"],
            "classification": {"predicates": ["Does this define a function?"], "maxRequests": 10}
        }"#,
    );
    apply_code_rows(
        &mut output,
        query.clone(),
        &mut query.clone(),
        &window.selection,
    )
    .expect("classified page");
    let retried = u64::try_from(window.long_requests()).expect("count");
    assert!(retried > 0, "the provider answered in the long window");
    let data = &output["results"][0]["data"];
    let notice = data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|note| note.contains("long-context window"))
        .unwrap_or_else(|| panic!("no retry notice: {data}"));
    let evaluations = octocode_engine::text::counted(retried, "evaluation", "evaluations");
    assert!(notice.starts_with(&evaluations), "{notice}");
    assert!(
        notice.contains(&format!("{} input tokens billed", retried * BILLED)),
        "{notice}"
    );
}

/// A candidate page judged through long-context retries says so again when
/// its cursor is replayed, but the replay bills nothing: the notice names the
/// original read as the one that billed the tokens, and no request is sent.
#[test]
fn a_replayed_candidate_page_keeps_its_retry_notice_without_claiming_the_billing() {
    use octocode_tools_shared::selection::long_window_fake::{BILLED, LongWindow};
    let mut window = LongWindow::start();
    window.selection.limits.max_chars = 1;
    let candidates = [
        Candidate::new(Some(json_fixture(r#"{"content": "first"}"#))),
        Candidate::new(Some(json_fixture(r#"{"content": "second"}"#))),
    ];
    let query = json_fixture(r#"{"classification": {"predicates": ["find evidence"], "maxRequests": 10}}"#);
    let run = |prior| {
        select_source(
            &window.selection,
            &query,
            prior,
            &candidates,
            SelectionUnit::Hits,
        )
        .unwrap_or_else(|_| panic!("candidate page"))
    };
    let first = run(None);
    let opening = first.next_candidates().expect("the rest").to_owned();
    let judged = run(Some(&opening));
    let requests = window.long_requests();
    assert!(requests > 0, "the provider answered in the long window");
    let notice = judged.long_context_notice.expect("the retry is disclosed");
    assert!(
        notice.contains(&format!("{BILLED} input tokens billed.")) && !notice.contains("original"),
        "{notice}"
    );
    let replay = run(Some(&opening));
    assert_eq!(window.long_requests(), requests, "the replay sends nothing");
    assert_eq!(
        replay.receipt.usage.as_ref().map(|usage| usage.calls),
        Some(0)
    );
    let notice = replay.long_context_notice.expect("the retry is still true");
    assert!(
        notice.contains("retried in the long-context window (slower, less accurate)")
            && notice.contains(&format!(
                "{BILLED} input tokens billed by the original read."
            )),
        "{notice}"
    );
}

/// The provider's response contract requires `usage`, so a long-window
/// answer without it never reaches the notice as a billed-token claim: the
/// page fails as an invalid provider response, counts those requests as
/// unknown usage and states no token total. (The notice's own "unknown"
/// wording is pinned at `judgment::tests::usage`.)
#[test]
fn a_long_window_answer_without_usage_is_refused_not_billed_as_a_number() {
    use octocode_tools_shared::selection::long_window_fake::LongWindow;
    let window = LongWindow::start_without_usage();
    let files: Vec<Value> = (0..2)
        .map(|i| json!({"path":format!("{i}.rs"),"matches":[{"line":1,"value":format!("fn candidate_{i}() {{}}")}]}))
        .collect();
    let mut output = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r","files":files}}]});
    let query = json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "keywords": ["fn"],
            "classification": {"predicates": ["Does this define a function?"], "maxRequests": 10}
        }"#,
    );
    apply_code_rows(
        &mut output,
        query.clone(),
        &mut query.clone(),
        &window.selection,
    )
    .expect("classified page");
    assert!(window.long_requests() > 0, "the long window was tried");
    let data = &output["results"][0]["data"];
    assert_eq!(data["errorCode"], "invalidClassificationResponse", "{data}");
    let usage = &data["selection"]["usage"];
    assert_eq!(usage["knownUsageCalls"], 0, "{data}");
    assert!(usage["unknownUsageCalls"].as_u64() > Some(0), "{data}");
    assert!(
        !data.to_string().contains("input tokens billed"),
        "no billed total is claimed: {data}"
    );
}

/// A query stating the contract default `page: 1` emits candidate pages
/// that omit it; each emitted call, replayed as is, digests the same
/// provider page, so the walk reaches every file instead of going stale.
#[test]
fn a_default_page_walk_follows_each_emitted_candidate_page() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    runtime.limits.max_chars = 1;
    let files: Vec<Value> = (0..3)
        .map(|i| json!({"path":format!("{i}.rs"),"matches":[{"line":1,"value":format!("fn candidate_{i}() {{}}")}]}))
        .collect();
    let original = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r","files":files}}]});
    let mut query = json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "page": 1,
            "keywords": ["fn"],
            "classification": {"predicates": ["Does this define a function?"]}
        }"#,
    );
    let mut seen = HashSet::new();
    loop {
        let mut output = original.clone();
        apply_code_rows(&mut output, query.clone(), &mut query.clone(), &runtime)
            .expect("candidate page");
        let data = &output["results"][0]["data"];
        assert!(data.get("errorCode").is_none(), "{query} replays: {data}");
        for file in data["files"].as_array().expect("selected files") {
            assert!(seen.insert(file["path"].as_str().unwrap().to_owned()));
        }
        let next = &data["next"]["nextCandidatePage"]["input"]["queries"][0];
        if next.is_null() {
            break;
        }
        assert!(
            next.get("page").is_none(),
            "the default page is dropped: {next}"
        );
        query = next.clone();
    }
    assert_eq!(seen.len(), 3, "every file reached once");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn provider_candidate_pages_cover_the_same_page_and_refuse_source_drift() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (mut runtime, calls) = selecting_runtime(&executor);
    runtime.limits.max_chars = 1;
    let mut files: Vec<Value> = (0..5)
        .map(|i| json!({"path":format!("{i}.rs"),"matches":[{"line":1,"value":format!("fn candidate_{i}() {{}}")}]}))
        .collect();
    files.push(json!({"path":"oversized.rs","matches":[{"line":1,"value":"x".repeat(2_000)}]}));
    let original = json!({"results":[{"index":0,"data":{
        "owner":"o","repo":"r","files":files,
        "next":{"nextPage":{"kind":"continue","tool":"ghSearchCode","input":{"queries":[{
            "owner":"o","repo":"r","keywords":["fn"],"page":4
        }]}}}
    }}]});
    let mut query = json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "page": 3,
            "keywords": ["fn"],
            "classification": {"predicates": ["Does this define a function?"]}
        }"#,
    );
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
    changed["results"][0]["data"]["files"][0]["matches"] =
        json_fixture(r#"[{"line": 1, "value": "changed"}]"#);
    apply_code_rows(&mut changed, query.clone(), &mut query.clone(), &runtime)
        .expect("typed stale error");
    assert_eq!(changed["results"][0]["data"]["errorCode"], "staleSnapshot");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        6,
        "drift cannot trigger inference"
    );
}
