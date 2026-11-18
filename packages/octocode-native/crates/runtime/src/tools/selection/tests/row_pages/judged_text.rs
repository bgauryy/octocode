//! The text of a history row that only the judge reads (a body) reaches the
//! judge and leaves the row, on every outcome of the selection.
use super::*;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use octocode_tools_shared::selection::SelectionJudge;
use serde_json::Value;
use serde_json::json;

fn pulls() -> Value {
    json_fixture(
        r#"[
            {"number": 1, "title": "tidy", "body": "fixes a crash"},
            {"number": 2, "title": "tidy", "body": "renames a field"},
            {"number": 3, "title": "tidy"}
        ]"#,
    )
}

fn pull_query(extra: Value) -> Value {
    let mut query = json_fixture(
        r#"{
            "owner": "o",
            "repo": "r",
            "operation": "pullRequest",
            "keywords": ["tidy"],
            "classification": {"predicates": ["Is this a bug fix?"]}
        }"#,
    );
    query
        .as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    query
}

fn run(judge: impl SelectionJudge + 'static, query: &Value, rows: Value) -> Value {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(judge, &executor);
    let mut structured = json!({"results":[{"index":0,"data":{"pullRequests":rows}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhSearchHistory,
        &mut structured,
        query,
        &mut response_query,
        &runtime,
    );
    structured["results"][0]["data"].clone()
}

fn bodies(data: &Value) -> String {
    data["pullRequests"].to_string()
}

#[test]
fn a_body_decides_the_selection_and_leaves_the_row() {
    let data = run(FixJudge, &pull_query(json!({})), pulls());
    let kept = data["pullRequests"].as_array().expect("rows");
    assert_eq!(kept.len(), 1, "{data}");
    assert_eq!(kept[0]["number"], 1, "{data}");
    assert!(!bodies(&data).contains("body"), "{data}");
}

#[test]
fn the_judge_reads_each_body_and_a_bodyless_row_judges_on_its_title() {
    let judge = RecordingJudge::default();
    run(judge.clone(), &pull_query(json!({})), pulls());
    let seen = judge.0.lock().expect("seen").join("\n");
    assert!(seen.contains("fixes a crash"), "{seen}");
    assert!(seen.contains("renames a field"), "{seen}");
    assert!(seen.contains(r#""title":"tidy""#), "{seen}");
}

#[test]
fn a_body_the_caller_picked_stays_on_the_row() {
    let data = run(
        RecordingJudge::default(),
        &pull_query(json_fixture(r#"{"fields": ["title", "body"]}"#)),
        pulls(),
    );
    assert!(bodies(&data).contains("fixes a crash"), "{data}");
}

#[test]
fn a_pick_without_the_body_still_judges_it_and_shows_none() {
    let judge = RecordingJudge::default();
    let data = run(
        judge.clone(),
        &pull_query(json_fixture(r#"{"fields": ["title"]}"#)),
        pulls(),
    );
    assert!(
        judge
            .0
            .lock()
            .expect("seen")
            .join("")
            .contains("fixes a crash")
    );
    assert!(!bodies(&data).contains("body"), "{data}");
}

#[test]
fn a_provider_that_is_down_shows_rows_without_their_bodies() {
    let data = run(DownJudge, &pull_query(json!({})), pulls());
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(data["pullRequests"].as_array().map(Vec::len), Some(3));
    assert!(!bodies(&data).contains("body"), "{data}");
}

#[test]
fn a_refused_credential_shows_no_body() {
    let data = run(RefusedJudge, &pull_query(json!({})), pulls());
    assert!(!data.to_string().contains("fixes a crash"), "{data}");
}

#[test]
fn a_commit_message_body_is_judged_and_leaves_the_row() {
    let judge = RecordingJudge::default();
    let rows = json_fixture(
        r#"[
            {
                "commitSha": "aaa",
                "messageHeadline": "tidy",
                "messageBody": "fixes a crash in the cache"
            }
        ]"#,
    );
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let runtime = runtime(judge.clone(), &executor);
    let query = query();
    let mut structured = json!({"results":[{"index":0,"data":{"commits":rows}}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhSearchHistory,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let data = &structured["results"][0]["data"];
    assert!(
        judge
            .0
            .lock()
            .expect("seen")
            .join("")
            .contains("fixes a crash in the cache")
    );
    assert!(data["commits"][0].get("messageBody").is_none(), "{data}");
}
