//! Recovery reads for a number read under the wrong operation.
use crate::providers::github::ProviderErrorReason;
use crate::tools::id::ToolId;
use crate::tools::result::Continuation;
use serde_json::{Map, Value, json};

/// The read that recovers a failed item read: a pull-request number read as
/// an issue reruns as operation:"pullRequest" (issue selections are a subset
/// of the pull-request ones, so `sections` carry over); an issue number read
/// as a pull request reruns as operation:"issue".
pub fn attach_recovery(data: &mut Value, reason: Option<ProviderErrorReason>, query: &Value) {
    let (operation, fields, name, hint): (&str, &[&str], &str, &str) = match reason {
        Some(ProviderErrorReason::IssueIsPullRequest) => (
            "pullRequest",
            &["owner", "repo", "number", "sections"],
            "readPullRequest",
            "This number is a pull request; run the readPullRequest continuation.",
        ),
        Some(ProviderErrorReason::PullRequestIsIssue) => (
            "issue",
            &["owner", "repo", "number"],
            "readIssue",
            "This number is an issue; run the readIssue continuation.",
        ),
        _ => return,
    };
    let mut next = Map::new();
    for field in fields {
        if let Some(value) = query.get(*field).filter(|value| !value.is_null()) {
            next.insert((*field).into(), value.clone());
        }
    }
    next.insert("operation".into(), json!(operation));
    data["hints"] = json!([hint]);
    let read = Continuation::new(ToolId::GhGetHistoryItem, Value::Object(next))
        .confidence("exact")
        .build();
    data["next"] = Value::Object(Map::from_iter([(name.to_owned(), read)]));
}
