use super::plan::{Plan, VERSION, binding, decode};
use super::provider_page::strip_plan_from_tool_continuations;
use super::*;
use crate::tools::test_support::selection_fake;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_contract_types::ErrorCode;
use octocode_contract_types::SelectionScope;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn bounded_chunk_admission_reaches_relevant_evidence_after_the_prefix() {
    let executor = tokio::runtime::Runtime::new().unwrap();
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let mut runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
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
    let query = json!({"clasifyContext":{"classifiers":["retry connection"]}});
    let outcome = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &candidates,
        None,
        SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("selection failed"));
    assert_eq!(outcome.selected, [75, 99]);
    assert_eq!(outcome.receipt.unknown_items, 98);
    let replay = select(
        Some(&runtime),
        &query,
        "source",
        Some(&outcome.token),
        &candidates,
        None,
        SelectionUnit::Chunks,
    )
    .unwrap_or_else(|_| panic!("replay failed"));
    assert_eq!(replay.selected, outcome.selected);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

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
fn context_selection_binds_optional_research_context() {
    let limits = SelectionLimits {
        max_candidates: 4,
        max_chars: 100,
        timeout: Duration::from_secs(1),
    };
    let query = json!({
        "clasifyContext":{"classifiers":["Does this evidence describe retry termination?"]}
    });
    let ordinary = binding(&query, "source", limits, None);
    for field in ["mainGoal", "reasoning"] {
        let mut contextual = query.clone();
        contextual[field] = json!("Distinguish current behavior from archived behavior");
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
    let query = json!({"clasifyContext":{"classifiers":["Find retry termination"]},
        "ranges":["1-40"],"unit":"lines","length":10});
    let mut continued = query.clone();
    continued["unit"] = json!("bytes");
    continued["fullContent"] = json!(true);
    continued["length"] = json!(20);
    continued["offset"] = json!(10);
    assert_eq!(
        binding(&query, "same-source", limits, None),
        binding(&continued, "same-source", limits, None)
    );
    continued["ranges"] = json!(["41-80"]);
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
    let query = json!({"clasifyContext":{"classifiers":["Find retry termination"]}});
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
fn context_selection_activates_without_optional_research_metadata() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({
        "owner":"a", "repo":"b", "keywords":["retry"],
        "clasifyContext":{"classifiers":["Does this evidence state when retries stop?"]}
    });
    let mut structured = json!({"results":[{"index":0,"data":{"files":[
        {"path":"a.rs","matches":[{"line":1,"value":"stop after three attempts"}]},
        {"path":"b.rs","matches":[{"line":1,"value":"start a new attempt"}]}
    ]}}]});
    let mut response_query = query.clone();

    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut structured,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("classified page without optional metadata");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(structured["results"][0]["data"].get("selection").is_some());
    assert!(structured["results"][0]["data"].get("ranking").is_none());
    assert!(response_query.get("selection").is_some());
    assert!(response_query.get("ranking").is_none());
}

#[test]
fn provider_candidate_pages_cover_the_same_page_and_refuse_source_drift() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let mut runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    runtime.limits.max_candidates = 2;
    runtime.limits.max_chars = 1_024;
    let mut files: Vec<Value> = (0..5)
        .map(|i| json!({"path":format!("{i}.rs"),"lines":[format!("1\tfn candidate_{i}() {{}}")]}))
        .collect();
    files.push(json!({"path":"oversized.rs","lines":["x".repeat(2_000)]}));
    let original = json!({"results":[{"index":0,"data":{
        "owner":"o","repo":"r","files":files,
        "next":{"nextPage":{"tool":"ghSearchCode","query":{"queries":[{
            "owner":"o","repo":"r","keywords":["fn"],"page":4
        }]}}}
    }}]});
    let mut query = json!({"owner":"o","repo":"r","page":3,"keywords":["fn"],
        "clasifyContext":{"classifiers":["Does this define a function?"]}});
    let mut seen = HashSet::new();
    let mut first_next = None;
    for page in 0..3 {
        let mut output = original.clone();
        apply_sanitized_rows(
            ToolId::GhSearchCode,
            &mut output,
            std::slice::from_ref(&query),
            &[0],
            &mut query.clone(),
            &runtime,
        )
        .expect("candidate page");
        let data = &output["results"][0]["data"];
        for file in data["files"].as_array().expect("selected files") {
            assert!(seen.insert(file["path"].as_str().unwrap().to_owned()));
        }
        assert_eq!(data["next"]["nextPage"]["query"]["queries"][0]["page"], 4);
        if page == 0 {
            assert_eq!(data["selection"]["totalItems"], 6);
            assert_eq!(data["selection"]["unknownItems"], 4);
            assert_eq!(data["selection"]["reason"], "candidateStateTooLarge");
            assert!(data["next"]["unfiltered"].is_object());
        }
        let next = data["next"]["nextCandidatePage"]["query"]["queries"][0].clone();
        if page < 2 {
            assert_eq!(next["page"], 3);
            assert!(next["selection"].is_string());
            first_next.get_or_insert_with(|| next.clone());
            query = next;
        } else {
            assert!(next.is_null(), "oversized evidence cannot loop forever");
        }
    }
    assert_eq!(seen.len(), 5);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let query = first_next.expect("continuation");
    let mut changed = original;
    changed["results"][0]["data"]["files"][0]["lines"] = json!(["1\tchanged"]);
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut changed,
        std::slice::from_ref(&query),
        &[0],
        &mut query.clone(),
        &runtime,
    )
    .expect("typed stale error");
    assert_eq!(changed["results"][0]["data"]["errorCode"], "staleSnapshot");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "drift cannot trigger inference"
    );
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
    let query = json!({"clasifyContext":{"classifiers":["States the fact?"]}});
    let candidates: Vec<_> = (0..64)
        .map(|i| Candidate::new(Some(json!(format!("candidate {i}")))))
        .collect();
    let run = |token: Option<&str>| {
        select(
            Some(&runtime),
            &query,
            "source",
            token,
            &candidates,
            None,
            SelectionUnit::Chunks,
        )
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
    let max =
        crate::contracts::query_schema_number(ToolId::LocalFetch, None, "selection", "maxLength")
            .unwrap() as usize;
    assert!(fresh.token.len() <= max);
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
fn inline_usage_is_opt_in_and_plan_replay_reports_no_new_spend() {
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
    let query =
        json!({"extras":["stats"],"clasifyContext":{"classifiers":["Does this state the fact?"]}});
    let candidates = vec![Candidate::new(Some(json!("the fact")))];
    let run = |query: &Value, token: Option<&str>| {
        select(
            Some(&runtime),
            query,
            "source",
            token,
            &candidates,
            None,
            SelectionUnit::Files,
        )
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
    let mut compact = query;
    compact.as_object_mut().unwrap().remove("extras");
    assert!(run(&compact, Some(&fresh.token)).receipt.usage.is_none());
}

struct FailedJudge;
impl SelectionJudge for FailedJudge {
    fn identity(&self) -> Value {
        json!({"backend":"test"})
    }
    fn select(&self, _call: JudgeCall<'_>) -> Judgments {
        Err("providerUnavailable".into())
    }
}

#[test]
fn empty_selection_preserves_provider_page_and_unfiltered_recovery() {
    struct ExcludingJudge(std::sync::Arc<AtomicUsize>);
    impl SelectionJudge for ExcludingJudge {
        fn identity(&self) -> Value {
            json!({"test":"exclude"})
        }
        fn select(&self, call: JudgeCall<'_>) -> Judgments {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(call
                .admitted
                .iter()
                .map(|&i| (i, Decision::Excluded))
                .collect())
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(ExcludingJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"o","repo":"r","page":3,"keywords":["retry"],
        "clasifyContext":{"classifiers":["Does it state the termination condition?"]}});
    let original = json!({"results":[{"index":0,"data":{
        "files":[{"path":"retry.rs","lines":["UNRELATED_SENTINEL"]}],
        "next":{"nextPage":{"tool":"ghSearchCode","query":{"queries":[{
            "owner":"o","repo":"r","page":4,"keywords":["retry"],
            "clasifyContext":{"classifiers":["Does it state the termination condition?"]},
            "selection":"old-plan"
        }]}}}
    }}]});
    let mut selected = original.clone();
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut selected,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("selected provider page");
    let data = &selected["results"][0]["data"];
    assert_eq!(data["files"], json!([]));
    assert_eq!(data["selection"]["excludedItems"], 1);
    assert_eq!(data["selection"]["unknownItems"], 0);
    // The filtered-evidence notice is the response stage's, said once
    // on a first page (`selection_errors::decorate`), never per page.
    assert!(data.get("warnings").is_none(), "{data}");
    let provider_next = &data["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(provider_next["page"], 4);
    assert!(provider_next.get("selection").is_none());
    assert!(provider_next.get("clasifyContext").is_some());
    let recovery = data["next"]["unfiltered"]["query"]["queries"][0].clone();
    assert_eq!(recovery["page"], 3, "recover the same provider page");
    assert!(recovery.get("clasifyContext").is_none());
    assert!(recovery.get("selection").is_none());
    let mut unfiltered_page = original.clone();
    let mut unfiltered_query = recovery.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut unfiltered_page,
        &[recovery],
        &[0],
        &mut unfiltered_query,
        &runtime,
    )
    .expect("explicit ordinary recovery");
    assert_eq!(unfiltered_page, original);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "recovery makes no judgment"
    );
}

/// A provider page with no candidates left nothing out: no unfiltered
/// page re-runs the same empty search.
#[test]
fn an_empty_provider_page_offers_no_unfiltered_page() {
    struct NoJudge;
    impl SelectionJudge for NoJudge {
        fn identity(&self) -> Value {
            json!({"test":"none"})
        }
        fn select(&self, _: JudgeCall<'_>) -> Judgments {
            Ok(Vec::new())
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = test_runtime(std::sync::Arc::new(NoJudge), executor.handle().clone());
    let query = json!({"owner":"o","repo":"r","keywords":["nothing"],
        "clasifyContext":{"classifiers":["Relevant?"]}});
    let mut page = json!({"results":[{"index":0,"status":"empty","data":{"files":[]}}]});
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut page,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("empty provider page");
    let data = &page["results"][0]["data"];
    assert_eq!(data["selection"]["totalItems"], 0, "{data}");
    assert!(data["next"].get("unfiltered").is_none(), "{data}");
}

#[test]
fn path_only_candidates_are_unknown_without_provider_work() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"o","repo":"r","keywords":["retry"],
        "clasifyContext":{"classifiers":["Does this show a stopping condition?"]}});
    let mut output = json!({"results":[{"index":0,"data":{"files":[{"path":"retry.rs"}]}}]});
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut output,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("unknown path-only view");
    let data = &output["results"][0]["data"];
    assert_eq!(data["files"], json!([]));
    assert_eq!(data["selection"]["unknownItems"], 1);
    assert_eq!(data["selection"]["excludedItems"], 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn source_failure_keeps_selection_coverage_visible_without_returning_partial_bodies() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"o","keywords":["retry"],
        "clasifyContext":{"classifiers":["Does it describe retries?"]}});
    let mut output = json!({"results":[{"index":0,"status":"error","data":{
        "errorCode":"providerUnavailable","files":[{"path":"a.rs","lines":["UNJUDGED_BODY"]}]
    }}]});
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut output,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("failed source has a selection receipt");
    let data = &output["results"][0]["data"];
    assert_eq!(data["errorCode"], "providerUnavailable");
    assert!(data.get("files").is_none());
    assert_eq!(data["selection"]["status"], "unavailable");
    assert_eq!(data["selection"]["unknownItems"], 1);
    assert!(data["next"].get("unfiltered").is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_provider_failing_the_whole_page_fails_the_row_with_its_typed_error() {
    struct Unauthorized;
    impl SelectionJudge for Unauthorized {
        fn identity(&self) -> Value {
            json!({"backend":"unauthorized"})
        }
        fn select(&self, _call: JudgeCall<'_>) -> Judgments {
            Err(JudgeFailure::Provider(ProviderError {
                code: ErrorCode::ClassificationProviderError,
                message: "Classification provider returned HTTP 401 Unauthorized".into(),
                hints: vec!["Check OCTOCODE_CLASSIFICATION_API and account access.".into()],
            }))
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = test_runtime(std::sync::Arc::new(Unauthorized), executor.handle().clone());
    let query = json!({"owner":"o","repo":"r","page":2,"keywords":["retry"],
        "clasifyContext":{"classifiers":["Does it state when retries stop?"]}});
    let mut output = json!({"results":[{"index":0,"data":{"files":[
        {"path":"a.rs","matches":[{"line":1,"value":"UNJUDGED_BODY"}]}
    ]}}]});
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut output,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("a failed judgment is a row outcome");
    let row = &output["results"][0];
    assert_eq!(row["status"], "error");
    let data = &row["data"];
    assert_eq!(data["errorCode"], "classificationProviderError");
    assert!(
        data["hints"]
            .to_string()
            .contains("OCTOCODE_CLASSIFICATION_API")
    );
    assert!(!data.to_string().contains("UNJUDGED_BODY"));
    assert_eq!(data["selection"]["status"], "unavailable");
    assert_eq!(data["selection"]["reason"], "classificationProviderError");
    assert_eq!(data["selection"]["unknownItems"], 1);
    let recovery = &data["next"]["unfiltered"]["query"]["queries"][0];
    assert_eq!(recovery["page"], 2);
    assert!(recovery.get("clasifyContext").is_none());
    assert!(
        response_query.get("selection").is_none(),
        "no plan to replay"
    );
}

/// A provider that cannot answer now (down, overloaded, unreachable)
/// never withholds a provider page that succeeded: every file is shown
/// under an `unavailable` receipt naming the provider's code, the row
/// warns with its error and hint, no unfiltered lead repeats the page,
/// and the response pager's replay of the recorded plan asks no provider.
#[test]
fn a_provider_that_is_down_shows_the_whole_page_as_unavailable() {
    struct Down(std::sync::Arc<AtomicUsize>);
    impl SelectionJudge for Down {
        fn identity(&self) -> Value {
            json!({"backend":"down"})
        }
        fn select(&self, _call: JudgeCall<'_>) -> Judgments {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(JudgeFailure::Provider(ProviderError {
                code: crate::providers::classification::CIRCUIT_OPEN,
                message: "Classification provider returned HTTP 503 Service Unavailable".into(),
                hints: vec!["Retry later.".into()],
            }))
        }
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(Down(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"o","repo":"r","keywords":["retry"],
        "clasifyContext":{"classifiers":["Does it state when retries stop?"]}});
    let page = || {
        json!({"results":[{"index":0,"data":{"files":[
            {"path":"a.rs","matches":[{"line":1,"value":"FIRST_BODY"}]},
            {"path":"b.rs","matches":[{"line":2,"value":"SECOND_BODY"}]}
        ]}}]})
    };
    let mut output = page();
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut output,
        std::slice::from_ref(&query),
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("a provider that is down is a row outcome");
    let row = &output["results"][0];
    assert_ne!(row["status"], "error", "{row}");
    let data = &row["data"];
    let paths = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [json!("a.rs"), json!("b.rs")], "{data}");
    assert_eq!(data["selection"]["status"], "unavailable");
    assert_eq!(
        data["selection"]["reason"],
        crate::id::error_codes::as_str(crate::providers::classification::CIRCUIT_OPEN)
    );
    assert_eq!(data["selection"]["unknownItems"], 2);
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("HTTP 503") && warnings.contains("shown unfiltered"),
        "{data}"
    );
    assert!(data["next"].get("unfiltered").is_none(), "{data}");
    let token = response_query["selection"].clone();
    assert!(token.is_string(), "the plan replays: {response_query}");
    // The response pager replays the recorded plan: no provider call.
    let mut replayed_query = query.clone();
    replayed_query["selection"] = token;
    let mut replay = page();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut replay,
        std::slice::from_ref(&replayed_query),
        &[0],
        &mut replayed_query.clone(),
        &runtime,
    )
    .expect("replay");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one provider call");
    let data = &replay["results"][0]["data"];
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(data["files"].as_array().map(Vec::len), Some(2), "{data}");
}

struct SelectedJudge(std::sync::Arc<AtomicUsize>);
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

fn test_runtime(
    judge: std::sync::Arc<dyn SelectionJudge>,
    handle: tokio::runtime::Handle,
) -> Runtime {
    Runtime {
        judge,
        ..selection_fake::runtime(selection_fake::SelectAll, handle, Duration::from_secs(5))
    }
}

#[test]
fn candidate_budget_counts_serialized_characters_in_source_order() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let mut runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let candidates = [
        Candidate::new(Some(json!({"text":"é\n\""}))),
        Candidate::new(Some(json!({"text":"longer than the remaining budget"}))),
        Candidate::new(Some(json!({"text":"é\n\""}))),
    ];
    assert_eq!(candidates[0].evidence_chars, 16);
    runtime.limits.max_chars = 32;
    let outcome = select(
        Some(&runtime),
        &json!({"clasifyContext":{"classifiers":["find evidence"]}}),
        "source",
        None,
        &candidates,
        None,
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
    let mut runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(std::sync::Arc::new(AtomicUsize::new(0)))),
        executor.handle().clone(),
    );
    let candidates = [
        Candidate::new(Some(json!({"text":"first"}))),
        Candidate::new(Some(json!({"text":"second"}))),
        Candidate::new(None),
    ];
    let query = json!({"clasifyContext":{"classifiers":["find evidence"]}});
    let eligible = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &candidates,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("partial order"));
    assert_eq!(eligible.selected, [0, 1]);
    assert_eq!(
        eligible.receipt.reason.as_deref(),
        Some("candidateIneligible")
    );
    runtime.limits.max_candidates = 1;
    let budgeted = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &candidates,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("budgeted order"));
    assert_eq!(budgeted.selected, [0]);
    assert_eq!(
        budgeted.receipt.reason.as_deref(),
        Some("candidateEligibilityAndBudget")
    );
    runtime.limits.max_candidates = 64;
    let no_states = [Candidate::new(None), Candidate::new(None)];
    let skipped = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &no_states,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("ineligible order"));
    assert_eq!(skipped.receipt.status, SelectionStatus::Partial);
    assert_eq!(
        skipped.receipt.reason.as_deref(),
        Some("candidateIneligible")
    );
}

#[test]
fn code_page_selects_in_source_order_and_replays_without_a_second_judgment() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"a","repo":"b","keywords":["target"],"clasifyContext":{"classifiers":["wanted evidence"]}});
    let baseline = json!({"results":[{"index":0,"data":{"files":[
        {"path":"one.rs","lines":["source evidence"]},{"path":"two.rs","lines":["source evidence"]},{"path":"three.rs","lines":["source evidence"]}
    ],"pagination":{"currentPage":1,"hasMore":true}}}]});
    let mut first = baseline.clone();
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut first,
        std::slice::from_ref(&query),
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("first page");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first["results"][0]["data"]["selection"]["scope"],
        "providerPage"
    );
    assert_eq!(
        first["results"][0]["data"]["files"],
        json!([{"path":"one.rs","lines":["source evidence"]},{"path":"two.rs","lines":["source evidence"]},{"path":"three.rs","lines":["source evidence"]}])
    );
    assert!(response_query.get("selection").is_some());

    let mut replay = baseline;
    let mut replay_query = response_query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut replay,
        &[response_query],
        &[0],
        &mut replay_query,
        &runtime,
    )
    .expect("replay");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        replay["results"][0]["data"]["files"],
        first["results"][0]["data"]["files"]
    );
}

#[test]
fn every_tool_continuation_starts_a_new_candidate_plan() {
    let mut data = json!({"next":{"nextPage":{"tool":"ghSearchCode","query":{"queries":[{"page":2,"selection":"old"}]}},
        "read":{"tool":"ghGetFileContent","query":{"queries":[{"path":"a.rs","clasifyContext":{"classifiers":["old"]},"selection":"old"}]}}},
        "hints":{"verify":{"tool":"ghGetFileContent","query":{"queries":[{"path":"a.rs","clasifyContext":{"classifiers":["old"]},"selection":"old"}]}}},
        "files":[{"evidence":{"tool":"ghSearchCode","query":{"queries":[{"clasifyContext":{"classifiers":["source"]},"selection":"source"}]},"next":{"sourceValue":{"tool":"ghSearchCode","query":{"queries":[{"selection":"source"}]}}}},"next":{"nextMatchPage":{"tool":"ghSearchCode","query":{"queries":[{"matchPage":2,"clasifyContext":{"classifiers":["old"]},"selection":"old"}]}}}}]});
    strip_plan_from_tool_continuations(&mut data, ToolId::GhSearchCode);
    assert!(
        data["next"]["nextPage"]["query"]["queries"][0]
            .get("selection")
            .is_none()
    );
    assert!(
        data["next"]["read"]["query"]["queries"][0]
            .get("selection")
            .is_none()
    );
    assert!(
        data["next"]["read"]["query"]["queries"][0]
            .get("clasifyContext")
            .is_none()
    );
    assert!(
        data["hints"]["verify"]["query"]["queries"][0]
            .get("clasifyContext")
            .is_none()
    );
    assert!(
        data["files"][0]["next"]["nextMatchPage"]["query"]["queries"][0]
            .get("selection")
            .is_none()
    );
    assert_eq!(
        data["files"][0]["next"]["nextMatchPage"]["query"]["queries"][0]["clasifyContext"]["classifiers"],
        json!(["old"])
    );
    assert_eq!(
        data["files"][0]["evidence"]["query"]["queries"][0]["selection"],
        "source"
    );
    assert_eq!(
        data["files"][0]["evidence"]["next"]["sourceValue"]["query"]["queries"][0]["selection"],
        "source"
    );
}

#[test]
fn rejected_rows_do_not_misalign_goal_queries() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let plain = json!({"owner":"a","repo":"b","keywords":["target"]});
    let goal = json!({"owner":"a","repo":"b","keywords":["target"],"clasifyContext":{"classifiers":["wanted"]}});
    let mut structured = json!({"results":[
        {"index":0,"status":"error","data":{"errorCode":"invalidInput"}},
        {"index":1,"data":{"files":[{"path":"one.rs","lines":["source evidence"]},{"path":"two.rs","lines":["source evidence"]}]}},
        {"index":2,"data":{"files":[{"path":"three.rs","lines":["source evidence"]},{"path":"four.rs","lines":["source evidence"]}]}}
    ]});
    let mut response_query = json!({"queries":[plain.clone(),goal.clone()]});
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut structured,
        &[plain, goal],
        &[1, 2],
        &mut response_query,
        &runtime,
    )
    .expect("mixed rows");
    assert!(structured["results"][1]["data"].get("selection").is_none());
    assert_eq!(
        structured["results"][2]["data"]["selection"]["status"],
        "complete"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(response_query["queries"][0].get("selection").is_none());
    assert!(response_query["queries"][1].get("selection").is_some());
}

#[test]
fn malformed_code_page_fails_clearly_and_skips_judgment() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"a","repo":"b","keywords":["target"],"clasifyContext":{"classifiers":["wanted"]}});
    let files = json!({"unexpected":"shape"});
    let mut structured = json!({"results":[{"index":0,"data":{"files":files,"marker":"retain"}}]});
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut structured,
        &[query],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("malformed page");
    assert!(structured["results"][0]["data"].get("files").is_none());
    assert_eq!(structured["results"][0]["status"], "error");
    assert_eq!(
        structured["results"][0]["data"]["selection"]["reason"],
        "invalidClassificationView"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
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
    let outcome = select(
        Some(&runtime),
        &json!({"clasifyContext":{"classifiers":["find evidence"]}}),
        "source",
        None,
        &candidate,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("single candidate should be judged"));
    assert!(outcome.selected.is_empty());
    assert_eq!(outcome.receipt.status, SelectionStatus::Complete);
    assert_eq!(outcome.receipt.excluded_items, 1);
    assert_eq!(outcome.receipt.unknown_items, 0);
}

#[test]
fn skipped_and_replayed_plans_keep_the_order_without_candidate_states() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let mut runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"clasifyContext":{"classifiers":["find evidence"]},"mainGoal":"Explain retry policy","reasoning":"Locate stopping conditions"});
    let with_states = [
        Candidate::new(Some(json!({"text":"first"}))),
        Candidate::new(Some(json!({"text":"second"}))),
    ];
    let without_states = [Candidate::new(None), Candidate::new(None)];
    runtime.available = false;
    let disabled = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &with_states,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("disabled plan"));
    let skipped = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &without_states,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("same disabled plan"));
    assert_eq!(disabled.token, skipped.token);
    assert!(skipped.selected.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    runtime.available = true;
    let selected = select(
        Some(&runtime),
        &query,
        "source",
        None,
        &with_states,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("selected plan"));
    assert_eq!(selected.selected, [0, 1]);
    assert!(!needs_candidate_states(
        Some(&runtime),
        Some(&selected.token),
        None,
        2
    ));
    let replay = select(
        Some(&runtime),
        &query,
        "source",
        Some(&selected.token),
        &without_states,
        None,
        SelectionUnit::Files,
    )
    .unwrap_or_else(|_| panic!("replay without evidence states"));
    assert_eq!(replay.token, selected.token);
    assert_eq!(replay.selected, selected.selected);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for field in ["mainGoal", "reasoning"] {
        let mut changed = query.clone();
        changed[field] = json!("different research context");
        assert!(
            matches!(
                select(
                    Some(&runtime),
                    &changed,
                    "source",
                    Some(&selected.token),
                    &without_states,
                    None,
                    SelectionUnit::Files
                ),
                Err(Failure::Stale)
            ),
            "changing {field} must invalidate a saved selection"
        );
    }
    let mut changed_capability = query.clone();
    changed_capability["clasifyContext"]["longContext"] = json!(true);
    assert!(matches!(
        select(
            Some(&runtime),
            &changed_capability,
            "source",
            Some(&selected.token),
            &without_states,
            None,
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
fn only_code_search_selects_at_the_response_boundary() {
    // Local search/fetch and GitHub file reads rank before their own
    // source pagination; the engine calls `apply_sanitized_rows` only for
    // a tool whose output says it ranks one provider page.
    let boundary = ToolId::ALL
        .into_iter()
        .filter(|tool| tool.output().selects_provider_page())
        .collect::<Vec<_>>();
    assert_eq!(boundary, [ToolId::GhSearchCode]);
}

#[test]
fn binding_uses_canonical_source_identity_across_path_spellings() {
    let limits = SelectionLimits {
        max_candidates: 0,
        max_chars: 0,
        timeout: Duration::ZERO,
    };
    let absolute = binding(
        &json!({"path":"/workspace/docs/readme.md","clasifyContext":{"classifiers":["find evidence"]}}),
        "/workspace/docs/readme.md:abc123",
        limits,
        None,
    );
    let relative = binding(
        &json!({"path":"docs/readme.md","clasifyContext":{"classifiers":["find evidence"]}}),
        "/workspace/docs/readme.md:abc123",
        limits,
        None,
    );
    assert_eq!(absolute, relative);
    assert_ne!(
        absolute,
        binding(
            &json!({"path":"docs/readme.md","clasifyContext":{"classifiers":["find evidence"]}}),
            "/workspace/other/readme.md:abc123",
            limits,
            None,
        )
    );
}

#[test]
fn selected_page_rejects_null_evidence_drift_but_accepts_key_reordering() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let query = json!({"owner":"o","repo":"r","keywords":["queue"],"clasifyContext":{"classifiers":["find queue"]}});
    let original = json!({"results":[{"index":0,"data":{"files":[
        {"path":"a.rs","detail":null,"lines":["source evidence"]},
        {"path":"b.rs","lines":["source evidence"]}
    ]}}]});
    let mut first = original.clone();
    let mut replay_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut first,
        std::slice::from_ref(&query),
        &[0],
        &mut replay_query,
        &runtime,
    )
    .expect("initial selected page");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first["results"][0]["data"]["selection"]["status"],
        "complete"
    );

    let mut same_evidence = json!({"results":[{"index":0,"data":{"files":[
        {"lines":["source evidence"],"detail":null,"path":"a.rs"},
        {"path":"b.rs","lines":["source evidence"]}
    ]}}]});
    let mut same_response = replay_query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut same_evidence,
        std::slice::from_ref(&replay_query),
        &[0],
        &mut same_response,
        &runtime,
    )
    .expect("same source with reordered object keys");
    assert_eq!(
        same_evidence["results"][0]["data"]["selection"]["status"],
        "complete"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1, "replay does not rescore");

    let mut changed = original;
    changed["results"][0]["data"]["files"][0]
        .as_object_mut()
        .expect("file object")
        .remove("detail");
    let mut changed_response = replay_query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut changed,
        &[replay_query],
        &[0],
        &mut changed_response,
        &runtime,
    )
    .expect("changed source is reported in its row");
    assert_eq!(changed["results"][0]["status"], "error");
    assert_eq!(changed["results"][0]["data"]["errorCode"], "staleSnapshot");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "stale replay does not rescore"
    );
}

#[test]
fn token_preserves_disjoint_partitions_and_rejects_drift() {
    let limits = SelectionLimits {
        max_candidates: 2,
        max_chars: 100,
        timeout: Duration::from_secs(1),
    };
    let bound = binding(
        &json!({"clasifyContext":{"classifiers":["find it"]},"page":1}),
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

#[test]
fn oversized_candidate_is_terminal_unknown_without_provider_work() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    let candidates = [Candidate::new(Some(json!({
        "content": "x".repeat(runtime.limits.max_chars)
    })))];
    let outcome = select(
        Some(&runtime),
        &json!({"clasifyContext":{"classifiers":["find evidence"]}}),
        "source",
        None,
        &candidates,
        None,
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
fn candidate_pages_skip_oversized_states_but_judge_every_fitting_state_once() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let mut runtime = test_runtime(
        std::sync::Arc::new(SelectedJudge(calls.clone())),
        executor.handle().clone(),
    );
    runtime.limits.max_candidates = 1;
    runtime.limits.max_chars = 64;
    let candidates = [
        Candidate::new(Some(json!({"content":"first"}))),
        Candidate::new(Some(json!({"content":"x".repeat(64)}))),
        Candidate::new(Some(json!({"content":"second"}))),
        Candidate::new(None),
    ];
    let query = json!({"clasifyContext":{"classifiers":["find evidence"]}});
    let run = |prior| {
        select(
            Some(&runtime),
            &query,
            "source",
            prior,
            &candidates,
            None,
            SelectionUnit::Files,
        )
        .unwrap_or_else(|_| panic!("mixed candidate page"))
    };
    let first = run(None);
    assert_eq!(first.selected, [0]);
    assert_eq!(first.unjudged, [2]);
    assert_eq!(
        first.receipt.reason.as_deref(),
        Some("candidateStateTooLarge")
    );
    assert_eq!(first.receipt.unknown_items, 3);
    assert!(first.excluded.is_empty());
    let next = first
        .next_candidates(ToolId::LocalSearch)
        .expect("fitting candidate page");
    let last = run(Some(next));
    assert_eq!(last.selected, [2]);
    assert!(last.next_candidates(ToolId::LocalSearch).is_none());
    assert_eq!(last.receipt.total_items, 1);
    assert_eq!(last.receipt.status, SelectionStatus::Complete);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
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
    runtime.limits.max_candidates = 3;
    let mut candidates = (0..8)
        .map(|i| Candidate::new(Some(json!({"content":format!("evidence {i}")}))))
        .collect::<Vec<_>>();
    candidates.push(Candidate::new(None));
    let query = json!({"clasifyContext":{"classifiers":["evidence"]}});
    let run = |prior: Option<&str>| {
        select(
            Some(&runtime),
            &query,
            "source",
            prior,
            &candidates,
            None,
            SelectionUnit::Chunks,
        )
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
        match outcome.next_candidates(ToolId::LocalSearch) {
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
        select(
            Some(&runtime),
            &json!({"clasifyContext":{"classifiers":["find evidence"]}}),
            "source",
            None,
            &candidates,
            None,
            SelectionUnit::Files
        ),
        Err(Failure::Cancelled)
    ));
}
