//! A classified history-item read that holds no discussion judged nothing:
//! its receipt counts zero, so the row says which sections hold the entries
//! classification judges instead of letting "complete" read as absence.
use super::*;
use octocode_runtime_base::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::Value;
use serde_json::json;

/// The classified read of `query` over `data`, with a judge that records
/// every candidate it is shown.
fn classified(query: Value, data: Value) -> (Value, Vec<String>) {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let judge = RecordingJudge::default();
    let runtime = runtime(judge.clone(), &executor);
    let mut structured = json!({"results":[{"index":0,"data":data}]});
    let mut response_query = query.clone();
    apply(
        ToolId::GhGetHistoryItem,
        &mut structured,
        &query,
        &mut response_query,
        &runtime,
    );
    let seen = judge.0.lock().expect("seen").clone();
    (structured["results"][0]["data"].clone(), seen)
}

fn query(operation: &str, sections: Option<&[&str]>) -> Value {
    let mut query = json!({"owner":"o","repo":"r","operation":operation,"number":2,
        "classification":{"predicates":["Does it mention a prefix?"]}});
    if let Some(sections) = sections {
        query["sections"] = json!(sections);
    }
    query
}

fn warnings(data: &Value) -> Vec<String> {
    data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

#[test]
fn an_issue_summary_with_counted_comments_says_nothing_was_judged_and_where_they_are() {
    let (data, seen) = classified(
        query("issue", None),
        json_fixture(
            r#"{
                "issues": [
                    {"number": 2, "body": "needs a prefix", "commentsCount": 1}
                ]
            }"#,
        ),
    );
    assert!(seen.is_empty(), "nothing judgeable was sent: {seen:?}");
    assert_eq!(data["selection"]["totalItems"], 0, "{data}");
    assert_eq!(
        data["selection"]["reason"], "unreadSections",
        "the receipt says the judged sections were not read, not that none matched: {data}"
    );
    assert_eq!(data["selection"]["status"], "complete", "{data}");
    let notes = warnings(&data);
    assert!(
        notes.iter().any(|note| note.contains("judged nothing")
            && note.contains("#2 counts 1 comment")
            && note.contains(r#"sections:["comments"]"#)),
        "{data}"
    );
}

#[test]
fn an_issue_section_read_that_restates_no_header_names_its_comments_uncounted() {
    // A section read restates only the number and state, so the row does not
    // say whether the issue has comments: they may hold judgeable entries.
    let (data, _) = classified(
        query("issue", Some(&["body"])),
        json_fixture(
            r#"{
                "issues": [{"number": 2, "state": "open", "body": "needs a prefix"}]
            }"#,
        ),
    );
    let notes = warnings(&data);
    assert!(
        notes.iter().any(|note| note.contains("judged nothing")
            && note.contains(r#"sections:["comments"]"#)
            && !note.contains("counts")),
        "{data}"
    );
}

#[test]
fn a_pull_request_files_read_names_every_discussion_section_it_left_out() {
    let (data, _) = classified(
        query("pullRequest", Some(&["files"])),
        json_fixture(
            r#"{
                "pullRequests": [
                    {"number": 2, "files": ["A +2 -1 a.ts"], "commentsCount": 3}
                ]
            }"#,
        ),
    );
    let notes = warnings(&data);
    assert!(
        notes.iter().any(|note| note.contains("judged nothing")
            && note.contains("#2 counts 3 comments")
            && note.contains(r#"sections:["comments","reviews","reviewComments"]"#)),
        "{data}"
    );
}

#[test]
fn a_pull_request_names_its_uncounted_reviews_but_not_sections_it_read() {
    // Reviews have no count on the row, so an unread reviews section may hold
    // entries; the comments it read were empty and stay unnamed.
    let (data, _) = classified(
        query("pullRequest", Some(&["comments", "reviewComments"])),
        json_fixture(r#"{"pullRequests": [{"number": 2}]}"#),
    );
    let notes = warnings(&data);
    assert!(
        notes.iter().any(|note| note.contains("judged nothing")
            && note.contains(r#"sections:["reviews"]"#)
            && !note.contains("counts")),
        "{data}"
    );
}

#[test]
fn a_read_with_nothing_left_to_judge_adds_no_note() {
    // An issue that counts no comment, and a read that requested every
    // discussion section, hold no unread entry: zero judged is the answer.
    for (query, data) in [
        (
            query("issue", None),
            json_fixture(r#"{"issues": [{"number": 2, "title": "t", "body": "b"}]}"#),
        ),
        (
            query("issue", Some(&["comments"])),
            json_fixture(r#"{"issues": [{"number": 2, "commentsCount": 1}]}"#),
        ),
        (
            query(
                "pullRequest",
                Some(&["comments", "reviews", "reviewComments"]),
            ),
            json_fixture(r#"{"pullRequests": [{"number": 2, "commentsCount": 1}]}"#),
        ),
    ] {
        let (data, _) = classified(query, data);
        assert_eq!(data["selection"]["reason"], "noCandidates", "{data}");
        assert!(
            !warnings(&data)
                .iter()
                .any(|note| note.contains("judged nothing")),
            "{data}"
        );
    }
}

#[test]
fn a_read_whose_discussion_was_judged_adds_no_note() {
    let (data, seen) = classified(
        query("issue", Some(&["comments"])),
        json_fixture(
            r#"{
                "issues": [
                    {
                        "number": 2,
                        "commentsCount": 1,
                        "comments": [{"body": "a prefix, yes"}]
                    }
                ]
            }"#,
        ),
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(
        !warnings(&data)
            .iter()
            .any(|note| note.contains("judged nothing")),
        "{data}"
    );
}
