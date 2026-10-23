//! Provider-page selection that keeps nothing, fails or cannot judge:
//! typed errors, unavailable receipts and unfiltered recovery.
use super::*;

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
    apply_code_rows(&mut selected, query, &mut response_query, &runtime)
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
    apply_code_rows(
        &mut unfiltered_page,
        recovery,
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
    apply_code_rows(&mut page, query, &mut response_query, &runtime).expect("empty provider page");
    let data = &page["results"][0]["data"];
    assert_eq!(data["selection"]["totalItems"], 0, "{data}");
    assert!(data["next"].get("unfiltered").is_none(), "{data}");
}

#[test]
fn path_only_candidates_are_unknown_without_provider_work() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let query = json!({"owner":"o","repo":"r","keywords":["retry"],
        "clasifyContext":{"classifiers":["Does this show a stopping condition?"]}});
    let mut output = json!({"results":[{"index":0,"data":{"files":[{"path":"retry.rs"}]}}]});
    let mut response_query = query.clone();
    apply_code_rows(&mut output, query, &mut response_query, &runtime)
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
    let (runtime, calls) = selecting_runtime(&executor);
    let query = json!({"owner":"o","keywords":["retry"],
        "clasifyContext":{"classifiers":["Does it describe retries?"]}});
    let mut output = json!({"results":[{"index":0,"status":"error","data":{
        "errorCode":"providerUnavailable","files":[{"path":"a.rs","lines":["UNJUDGED_BODY"]}]
    }}]});
    let mut response_query = query.clone();
    apply_code_rows(&mut output, query, &mut response_query, &runtime)
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
    apply_code_rows(&mut output, query, &mut response_query, &runtime)
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
    apply_code_rows(&mut output, query.clone(), &mut response_query, &runtime)
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
    apply_code_rows(
        &mut replay,
        replayed_query.clone(),
        &mut replayed_query.clone(),
        &runtime,
    )
    .expect("replay");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one provider call");
    let data = &replay["results"][0]["data"];
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(data["files"].as_array().map(Vec::len), Some(2), "{data}");
}

#[test]
fn malformed_code_page_fails_clearly_and_skips_judgment() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let query = json!({"owner":"a","repo":"b","keywords":["target"],"clasifyContext":{"classifiers":["wanted"]}});
    let files = json!({"unexpected":"shape"});
    let mut structured = json!({"results":[{"index":0,"data":{"files":files,"marker":"retain"}}]});
    let mut response_query = query.clone();
    apply_code_rows(&mut structured, query, &mut response_query, &runtime).expect("malformed page");
    assert!(structured["results"][0]["data"].get("files").is_none());
    assert_eq!(structured["results"][0]["status"], "error");
    assert_eq!(
        structured["results"][0]["data"]["selection"]["reason"],
        "invalidClassificationView"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
