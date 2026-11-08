//! Selection of the listed rows of ghSearchHistory, ghGetHistoryItem and
//! structureSearch at the response boundary: each row, entry or discussion
//! item is a candidate.
use super::*;

mod discussion;

/// Keeps the candidates whose state mentions `fix`; excludes the rest.
struct FixJudge;
impl SelectionJudge for FixJudge {
    fn identity(&self) -> Value {
        json!({"test":"fix"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| {
                let kept = call.candidates[ordinal]
                    .state
                    .as_ref()
                    .is_some_and(|state| state.to_string().contains("fix"));
                (
                    ordinal,
                    if kept {
                        Decision::Selected
                    } else {
                        Decision::Excluded
                    },
                )
            })
            .collect())
    }
}

/// A judge whose provider is rate limited.
struct DownJudge;
impl SelectionJudge for DownJudge {
    fn identity(&self) -> Value {
        json!({"test":"down"})
    }
    fn select(&self, _call: JudgeCall<'_>) -> Judgments {
        Err(JudgeFailure::Provider(ProviderError {
            code: ErrorCode::ClassificationRateLimited,
            message: "slow down".into(),
            hints: Vec::new(),
        }))
    }
}

/// A judge that keeps every candidate and records what it was shown.
#[derive(Clone, Default)]
struct RecordingJudge(std::sync::Arc<std::sync::Mutex<Vec<String>>>);
impl SelectionJudge for RecordingJudge {
    fn identity(&self) -> Value {
        json!({"test":"record"})
    }
    fn select(&self, call: JudgeCall<'_>) -> Judgments {
        let mut seen = self.0.lock().expect("seen");
        Ok(call
            .admitted
            .iter()
            .map(|&ordinal| {
                seen.extend(call.candidates[ordinal].state.iter().map(Value::to_string));
                (ordinal, Decision::Selected)
            })
            .collect())
    }
}

/// A judge whose provider refuses the credential: a failure the caller must fix.
struct RefusedJudge;
impl SelectionJudge for RefusedJudge {
    fn identity(&self) -> Value {
        json!({"test":"refused"})
    }
    fn select(&self, _call: JudgeCall<'_>) -> Judgments {
        Err(JudgeFailure::Provider(ProviderError {
            code: ErrorCode::ClassificationProviderError,
            message: "401".into(),
            hints: Vec::new(),
        }))
    }
}

fn runtime(judge: impl SelectionJudge + 'static, executor: &tokio::runtime::Runtime) -> Runtime {
    test_runtime(std::sync::Arc::new(judge), executor.handle().clone())
}

fn apply(
    tool: ToolId,
    structured: &mut Value,
    query: &Value,
    response_query: &mut Value,
    runtime: &Runtime,
) {
    apply_sanitized_rows(
        tool,
        tool.output(),
        structured,
        &[CheckedQuery::checked(query.clone())],
        &[0],
        response_query,
        runtime,
    )
    .expect("classified page");
}

fn commits() -> Value {
    let titles = [
        "fix: crash",
        "refactor: rename",
        "docs: intro",
        "fix: leak",
        "chore: bump",
        "style: fmt",
    ];
    json!(titles.map(|title| json!({"commitSha": title.len(), "messageHeadline": title})))
}

fn query() -> Value {
    json!({"owner":"o","repo":"r","operation":"commit","keywords":["x"],
        "classification":{"predicates":["Is this a bug fix?"]}})
}

#[test]
fn a_history_page_keeps_two_of_six_rows_and_leaves_the_page_reachable() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = query();
    let mut structured = json!({"results":[{"index":0,"data":{"commits":commits()}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhSearchHistory,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let data = &structured["results"][0]["data"];
    let kept = data["commits"].as_array().expect("commits");
    assert_eq!(kept.len(), 2, "{data}");
    assert_eq!(kept[0]["messageHeadline"], "fix: crash");
    assert_eq!(kept[1]["messageHeadline"], "fix: leak");
    let selection = &data["selection"];
    assert_eq!(selection["scope"], "providerPage");
    assert_eq!(selection["unit"], "hits");
    assert_eq!(
        (
            selection["totalItems"].as_i64(),
            selection["selectedItems"].as_i64()
        ),
        (Some(6), Some(2))
    );
    // The unfiltered lead is the same page without classification.
    let unfiltered = &data["next"]["unfiltered"]["input"]["queries"][0];
    assert!(unfiltered.get("classification").is_none());
    assert_eq!(unfiltered["keywords"], query["keywords"]);
}

#[test]
fn candidate_pages_judge_every_history_row_exactly_once() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = RecordingJudge::default();
    let mut runtime = runtime(judge.clone(), &executor);
    runtime.limits.max_chars = 120;
    let mut query = query();
    let mut pages = 0;
    let mut ended = false;
    for _ in 0..12 {
        let mut structured = json!({"results":[{"index":0,"data":{"commits":commits()}}]});
        let mut response_query = query.clone();
        apply(
            ToolId::GhSearchHistory,
            &mut structured,
            &query,
            &mut response_query,
            &runtime,
        );
        pages += 1;
        let data = &structured["results"][0]["data"];
        match data["next"]["nextCandidatePage"]["input"]["queries"][0].get("selection") {
            Some(token) => query["selection"] = token.clone(),
            None => {
                ended = true;
                break;
            }
        }
    }
    assert!(pages > 1, "the budget splits the page into candidate pages");
    assert!(
        ended,
        "the walk ends on the absence of a next candidate lead"
    );
    let mut judged = judge.0.lock().expect("seen").clone();
    judged.sort();
    let mut expected = commits()
        .as_array()
        .expect("commits")
        .iter()
        .map(|row| json!({"title":row["messageHeadline"],"body":null}).to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(judged, expected, "every row is judged, once");
}

#[test]
fn a_provider_that_is_down_shows_every_row_with_a_warning() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(DownJudge, &executor);
    let query = query();
    let mut structured = json!({"results":[{"index":0,"data":{"commits":commits()}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhSearchHistory,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let data = &structured["results"][0]["data"];
    assert_eq!(data["commits"].as_array().map(Vec::len), Some(6));
    assert_eq!(data["selection"]["status"], "unavailable");
    assert!(
        data["warnings"].to_string().contains("shown unfiltered"),
        "{data}"
    );
}

#[test]
fn a_structure_listing_keeps_the_entries_that_pass_and_drops_emptied_groups() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = json!({"path":".","classification":{"predicates":["Is it a fix?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"files":[
        {"dir":"a","files":["fix.rs (1)","other.rs (2)"]},
        {"dir":"b","files":["plain.rs (3)"]},
    ]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::StructureSearch,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let data = &structured["results"][0]["data"];
    assert_eq!(data["files"], json!([{"dir":"a","files":["fix.rs (1)"]}]));
    assert_eq!(data["selection"]["totalItems"], 3);
}

#[test]
fn history_item_discussion_is_judged_per_comment_and_review() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = json!({"owner":"o","repo":"r","operation":"pullRequest","number":1,
        "classification":{"predicates":["Does it say fix?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"pullRequests":[{
        "number":1,"title":"t",
        "comments":[{"body":"please fix"},{"body":"lgtm"}],
        "reviews":[{"body":"fix the leak"},{"body":"ok"}],
    }]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhGetHistoryItem,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let row = &structured["results"][0]["data"]["pullRequests"][0];
    assert_eq!(row["comments"], json!([{"body":"please fix"}]));
    assert_eq!(row["reviews"], json!([{"body":"fix the leak"}]));
    assert_eq!(row["title"], "t", "the item itself stays");
}

#[test]
fn a_credential_named_structure_path_is_withheld_from_the_judge_and_disclosed() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = RecordingJudge::default();
    let runtime = runtime(judge.clone(), &executor);
    let query = json!({"path":".","classification":{"predicates":["Is it code?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"files":[
        {"dir":"src","files":[".env (9)","lib.rs (2)"]},
    ]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::StructureSearch,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let seen = judge.0.lock().expect("seen").join(" ");
    assert!(seen.contains("src/lib.rs"), "{seen}");
    assert!(
        !seen.contains(".env"),
        "the judge saw a credential name: {seen}"
    );
    let data = &structured["results"][0]["data"];
    assert!(
        data["warnings"].to_string().contains("src/.env"),
        "the withheld path is disclosed: {data}"
    );
}

#[test]
fn a_credential_named_review_comment_path_is_withheld_like_a_listed_path() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = RecordingJudge::default();
    let runtime = runtime(judge.clone(), &executor);
    let query = json!({"owner":"o","repo":"r","operation":"pullRequest","number":1,
        "classification":{"predicates":["Does it say fix?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"pullRequests":[{
        "number":1,
        "reviewComments":[
            {"body":"rotate this","path":"config/.env"},
            {"body":"rename this","path":"src/lib.rs"},
        ],
    }]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhGetHistoryItem,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let seen = judge.0.lock().expect("seen").join(" ");
    assert!(seen.contains("rename this"), "{seen}");
    assert!(
        !seen.contains(".env") && !seen.contains("rotate this"),
        "the judge saw a credential-named review comment: {seen}"
    );
    let data = &structured["results"][0]["data"];
    assert!(
        data["warnings"].to_string().contains("config/.env"),
        "{data}"
    );
}

#[test]
fn a_refused_credential_fails_a_history_row_and_a_structure_row_with_the_unfiltered_lead() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(RefusedJudge, &executor);
    let history = query();
    let structure = json!({"path":".","classification":{"predicates":["Is it a fix?"]}});
    let cases = [
        (
            ToolId::GhSearchHistory,
            history,
            json!({"commits":commits()}),
        ),
        (
            ToolId::StructureSearch,
            structure,
            json!({"files":[{"dir":"a","files":["x (1)"]}]}),
        ),
    ];
    for (tool, query, data) in cases {
        let mut structured = json!({"results":[{"index":0,"data":data}]});
        let mut response_query = query.clone();
        apply(tool, &mut structured, &query, &mut response_query, &runtime);
        let row = &structured["results"][0];
        assert_eq!(row["status"], "error", "{tool:?}: {row}");
        assert_eq!(row["data"]["errorCode"], "classificationProviderError");
        let lead = &row["data"]["next"]["unfiltered"]["input"]["queries"][0];
        assert!(lead.get("classification").is_none(), "{tool:?}: {row}");
        assert_eq!(lead["path"].as_str(), query["path"].as_str());
    }
}

#[test]
fn a_later_comment_page_stays_classified_and_its_unfiltered_lead_keeps_the_page() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = json!({"owner":"o","repo":"r","operation":"pullRequest","number":1,
        "sections":["comments"],"page":2,
        "classification":{"predicates":["Does it say fix?"]}});
    // The continuation to page 3 was built from this call's query, so it
    // carries the judge's token when this page was reached by a candidate walk.
    let mut next_query = query.clone();
    next_query["page"] = json!(3);
    next_query["selection"] = json!("token-of-this-page");
    let lead =
        crate::response::result::Continuation::continued(ToolId::GhGetHistoryItem, next_query)
            .build();
    let mut structured = json!({"results":[{"index":0,"data":{"pullRequests":[{
        "number":1,
        "comments":[{"body":"please fix"},{"body":"lgtm"}],
        "contentPagination":{"comments":{"page":2,"hasMore":true,"next":{"nextPage":lead}}},
    }]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhGetHistoryItem,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let data = &structured["results"][0]["data"];
    let row = &data["pullRequests"][0];
    assert_eq!(row["comments"], json!([{"body":"please fix"}]));
    // The unjudged comments of this page: the same page, no judge.
    let unfiltered = &data["next"]["unfiltered"]["input"]["queries"][0];
    assert_eq!(unfiltered["page"], 2);
    assert!(unfiltered.get("classification").is_none());
    // The next page keeps the judge and starts a fresh judgment.
    let next = &row["contentPagination"]["comments"]["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(next["page"], 3);
    assert!(next.get("classification").is_some(), "{next}");
    assert!(
        next.get("selection").is_none(),
        "stale token carried: {next}"
    );
}

#[test]
fn the_unfiltered_lead_of_a_later_comment_window_keeps_its_position() {
    // The item read pages its comment bodies by `offset` and `pageSize`:
    // the lead for the dropped comments is that window, not the first.
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = json!({"owner":"o","repo":"r","operation":"pullRequest","number":1,
        "sections":["comments"],"page":2,"pageSize":2,"offset":400,
        "classification":{"predicates":["Does it say fix?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"pullRequests":[{
        "number":1,"comments":[{"body":"please fix"},{"body":"lgtm"}],
    }]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhGetHistoryItem,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let lead = &structured["results"][0]["data"]["next"]["unfiltered"]["input"]["queries"][0];
    assert!(lead.get("classification").is_none() && lead.get("selection").is_none());
    for field in ["page", "pageSize", "offset"] {
        assert_eq!(lead[field], query[field], "{field}: {lead}");
    }
}

#[test]
fn credential_named_directories_and_units_between_judged_ones_are_withheld() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = RecordingJudge::default();
    let runtime = runtime(judge.clone(), &executor);
    let query = json!({"path":".","classification":{"predicates":["Is it code?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"files":[
        {"dir":"src","files":["a.rs (1)",".env (2)","b.rs (3)"]},
        {"dir":".ssh","files":["config (4)"]},
        {"dir":"lib","files":["c.rs (5)"]},
    ]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::StructureSearch,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let seen = judge.0.lock().expect("seen").join(" ");
    for judged in ["src/a.rs", "src/b.rs", "lib/c.rs"] {
        assert!(seen.contains(judged), "{judged} not judged: {seen}");
    }
    for withheld in [".env", ".ssh"] {
        assert!(
            !seen.contains(withheld),
            "{withheld} reached the judge: {seen}"
        );
    }
    let data = &structured["results"][0]["data"];
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("src/.env") && warnings.contains(".ssh/config"),
        "{data}"
    );
    // The judge boundary (`SelectionJudge`) is the last seam before the
    // provider request: the request builder takes exactly these states.
    let shown = data["files"].to_string();
    assert!(
        shown.contains("a.rs") && shown.contains("b.rs") && shown.contains("c.rs"),
        "{shown}"
    );
}

#[test]
fn the_unfiltered_lead_of_a_later_code_page_keeps_its_position_and_drops_the_judge() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = json!({"owner":"o","repo":"r","keywords":["retry"],"page":2,
        "listed":"0123456789abcdef","matched":3,"indexHead":"0123456789abcdef0123456789abcdef01234567",
        "classification":{"predicates":["Does it say fix?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"files":[
        {"path":"a.rs","matches":[{"line":1,"value":"fix it"}]},
        {"path":"b.rs","matches":[{"line":1,"value":"other"}]},
    ]}}]});
    let mut response_query = query.clone();
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        ToolId::GhSearchCode.output(),
        &mut structured,
        &[CheckedQuery::checked(query.clone())],
        &[0],
        &mut response_query,
        &runtime,
    )
    .expect("classified code page");
    let data = &structured["results"][0]["data"];
    let lead = &data["next"]["unfiltered"]["input"]["queries"][0];
    for field in ["page", "listed", "matched", "indexHead"] {
        assert_eq!(lead[field], query[field], "{field}: {data}");
    }
    assert!(lead.get("selection").is_none(), "{lead}");
    assert!(lead.get("classification").is_none(), "{lead}");
}

#[test]
fn the_unfiltered_lead_of_a_history_item_keeps_its_head_sha() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(FixJudge, &executor);
    let query = json!({"owner":"o","repo":"r","operation":"pullRequest","number":1,
        "headSha":"deadbeef","classification":{"predicates":["Does it say fix?"]}});
    let mut structured = json!({"results":[{"index":0,"data":{"pullRequests":[{
        "number":1,"comments":[{"body":"please fix"},{"body":"lgtm"}],
    }]}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhGetHistoryItem,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let lead = &structured["results"][0]["data"]["next"]["unfiltered"]["input"]["queries"][0];
    assert_eq!(lead["headSha"], "deadbeef", "{lead}");
    assert!(lead.get("classification").is_none(), "{lead}");
}
