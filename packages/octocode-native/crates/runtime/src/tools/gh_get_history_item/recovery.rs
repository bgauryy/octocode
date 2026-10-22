//! Recovery reads for a number read under the wrong operation.
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::Continuation;
use octocode_github::{ProviderErrorKind, ProviderErrorReason};
use serde_json::{Map, Value, json};

/// The read that recovers a failed item read, as the row's `next`: a
/// pull-request number read as an issue reruns as operation:"pullRequest"
/// (issue sections are a subset of the pull-request ones, so they carry
/// over); an issue number read as a pull request reruns as
/// operation:"issue" with its `body`/`comments`; a comparison whose refs did
/// not resolve lists the repository's refs; a pull request, issue, or commit
/// SHA the repository does not have lists its recent ones (`ghSearchHistory`),
/// where the right number or SHA is. The lead runs the fix, so the row states
/// no hint beside it.
pub fn recovery(
    kind: ProviderErrorKind,
    reason: Option<ProviderErrorReason>,
    query: &Value,
) -> Option<Value> {
    // A commit read of a name (not a SHA) that did not resolve, or a
    // comparison ref: the refs listing names the branches and tags.
    let operation = query.get("operation").and_then(Value::as_str);
    let named_ref = query
        .get("ref")
        .and_then(Value::as_str)
        .is_some_and(|reference| !reference.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if reason == Some(ProviderErrorReason::RefNotFound)
        && (operation == Some("compare") || (operation == Some("commit") && named_ref))
        && let (Some(owner), Some(repo)) = (
            query.get("owner").and_then(Value::as_str),
            query.get("repo").and_then(Value::as_str),
        )
    {
        return Some(json!({"viewRefs": crate::tools::gh_shared::ref_recovery(owner, repo)}));
    }
    if kind == ProviderErrorKind::NotFound
        && matches!(reason, None | Some(ProviderErrorReason::RefNotFound))
        && let Some(found) = find_item(operation, query)
    {
        return Some(found);
    }
    let (operation, fields, name): (&str, &[&str], &str) = match reason {
        Some(ProviderErrorReason::IssueIsPullRequest) => (
            "pullRequest",
            &["owner", "repo", "number", "sections"],
            "readPullRequest",
        ),
        // An issue reads `body` and `comments`: those sections carry over.
        Some(ProviderErrorReason::PullRequestIsIssue) => (
            "issue",
            &["owner", "repo", "number", "sections"],
            "readIssue",
        ),
        _ => return None,
    };
    let mut next = Map::new();
    for field in fields {
        if let Some(value) = query.get(*field).filter(|value| !value.is_null()) {
            next.insert((*field).into(), value.clone());
        }
    }
    if operation == "issue"
        && let Some(Value::Array(sections)) = next.get_mut("sections")
    {
        sections.retain(|section| matches!(section.as_str(), Some("body" | "comments")));
        if sections.is_empty() {
            next.remove("sections");
        }
    }
    next.insert("operation".into(), json!(operation));
    let read = Continuation::new(ToolId::GhGetHistoryItem, Value::Object(next))
        .confidence(Confidence::Exact)
        .build();
    Some(Value::Object(Map::from_iter([(name.to_owned(), read)])))
}

/// The history listing a missing item is found in: the repository's recent
/// pull requests, issues, or commits, newest first. `None` for a comparison
/// (its refs listing is the recovery) or a query without owner/repo.
fn find_item(operation: Option<&str>, query: &Value) -> Option<Value> {
    let (search, name) = match operation? {
        "pullRequest" => ("pullRequest", "findPullRequest"),
        "issue" => ("issue", "findIssue"),
        "commit" => ("commit", "findCommit"),
        _ => return None,
    };
    let owner = query.get("owner").and_then(Value::as_str)?;
    let repo = query.get("repo").and_then(Value::as_str)?;
    let listing = Continuation::new(
        ToolId::GhSearchHistory,
        json!({"operation": search, "owner": owner, "repo": repo}),
    )
    .confidence(Confidence::High)
    .build();
    Some(Value::Object(Map::from_iter([(name.to_owned(), listing)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing pull request, issue, or commit SHA lists the repository's
    /// recent ones; another failure kind (rate limit, auth) gets no lead.
    #[test]
    fn missing_items_list_where_the_right_one_is() {
        for (operation, name) in [
            ("pullRequest", "findPullRequest"),
            ("issue", "findIssue"),
            ("commit", "findCommit"),
        ] {
            let query =
                json!({"operation":operation,"owner":"o","repo":"r","number":9,"ref":"0000000"});
            let next = recovery(ProviderErrorKind::NotFound, None, &query).expect(operation);
            assert_eq!(next[name]["tool"], "ghSearchHistory", "{next}");
            assert_eq!(
                next[name]["query"]["queries"][0],
                json!({"operation":operation,"owner":"o","repo":"r"}),
                "{next}"
            );
            assert!(recovery(ProviderErrorKind::RateLimited, None, &query).is_none());
        }
        let commit = json!({"operation":"commit","owner":"o","repo":"r","ref":"0000000000000000000000000000000000000000"});
        let next = recovery(
            ProviderErrorKind::NotFound,
            Some(ProviderErrorReason::RefNotFound),
            &commit,
        )
        .expect("commit");
        assert!(next.get("findCommit").is_some(), "{next}");
    }
}
