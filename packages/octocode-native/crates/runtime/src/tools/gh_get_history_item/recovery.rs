//! A failed history-item read as the tool states it: the shared wording,
//! the recovery read (the right operation, the listing that names the
//! right item, the refs listing) and, for a ref that is not there, the same
//! call at the default branch.
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::Continuation;
use crate::tools::gh_shared::{
    DefaultBranchRetry, GhFailure, default_branch_retry, unresolved_refs,
};
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::{Map, Value, json};

/// The failure of a history-item read of `query` (its wire row): the
/// shared history wording ([`crate::tools::gh_shared::history::history_failure`]),
/// [`recovery`]'s lead and, for a missing ref, [`missing_ref`]'s message
/// and `atDefaultBranch` retry.
pub(crate) async fn recover(
    provider: &GitHubProvider,
    query: &Value,
    mut error: ProviderError,
    context: &RequestContext,
) -> GhFailure {
    let mut next = recovery(error.kind, error.reason, query);
    if error.reason == Some(ProviderErrorReason::RefNotFound)
        && let Some((message, retry)) = missing_ref(provider, query, context).await
    {
        error.message = retry
            .message(message.as_deref().unwrap_or(&error.message))
            .into_boxed_str();
        let (name, lead) = retry.lead();
        next.get_or_insert_with(|| json!({}))[name] = lead;
    }
    let (message, hint) = crate::tools::gh_shared::history::history_failure(&error, false);
    let mut failure = GhFailure::new(error, message);
    if let Some(hint) = hint {
        failure = failure.hint(hint);
    }
    failure.next = next.map(Box::new);
    failure
}

/// A history item read at a ref that is not there, named against the
/// default branch: a commit's `ref`, or the comparison sides GitHub's one
/// not-found answer leaves unnamed (probed, so the message says which).
/// The retry sets each missing ref to the default branch; a comparison
/// whose other side already is the default branch has no useful retry.
/// `Some(message)` replaces the provider's message before the default
/// branch is named.
async fn missing_ref(
    provider: &GitHubProvider,
    query: &Value,
    context: &RequestContext,
) -> Option<(Option<String>, DefaultBranchRetry)> {
    let text = |name: &str| query.get(name).and_then(Value::as_str).unwrap_or_default();
    let retry = async |fields: &[&str]| {
        default_branch_retry(
            provider,
            ToolId::GhGetHistoryItem,
            query.clone(),
            fields,
            context,
        )
        .await
    };
    match text("operation") {
        "commit" => Some((None, retry(&["ref"]).await?)),
        "compare" => {
            let (owner, repo) = (text("owner"), text("repo"));
            let sides = [("base", text("base")), ("head", text("head"))];
            let missing = unresolved_refs(provider, owner, repo, &sides, context).await;
            if missing.is_empty() {
                return None;
            }
            let retry = retry(&missing).await?;
            // The retry must still compare two refs: a side that
            // resolved and already is the default branch leaves none.
            if sides
                .iter()
                .all(|(field, reference)| missing.contains(field) || *reference == retry.branch())
            {
                return None;
            }
            let named = missing
                .iter()
                .map(|field| format!("\"{}\" ({field})", text(field)))
                .collect::<Vec<_>>()
                .join(" and ");
            let message = format!(
                "Cannot compare {}...{} in {owner}/{repo}: {named} not found",
                text("base"),
                text("head")
            );
            Some((Some(message), retry))
        }
        _ => None,
    }
}

/// The read that recovers a failed item read, as the row's `next`: a
/// pull-request number read as an issue reruns as operation:"pullRequest"
/// (issue sections are a subset of the pull-request ones, so they carry
/// over); an issue number read as a pull request reruns as
/// operation:"issue" with its `body`/`comments`; a comparison whose refs did
/// not resolve lists the repository's refs; a pull request, issue, or commit
/// SHA the repository does not have lists its recent ones (`ghSearchHistory`),
/// where the right number or SHA is. The lead runs the fix, so the row states
/// no hint beside it.
fn recovery(
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

    /// [`super::super::run`] states a missing ref exactly as the runtime
    /// dispatch did (tests/runtime_gh_default_branch.rs): the message names
    /// the default branch, `atDefaultBranch` retries there, `viewRefs` stays;
    /// a comparison names its missing side, and offers no retry that would
    /// compare the default branch with itself.
    #[tokio::test]
    async fn a_missing_ref_is_recovered_inside_the_tool() {
        use crate::security::scan::ContentScan;
        use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
        use octocode_contract_types::ErrorCode;
        use wiremock::matchers::{method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        struct Passthrough;
        impl ContentScan for Passthrough {
            fn sanitize(
                &self,
                text: &str,
                _path: &std::path::Path,
            ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
                Ok((text.to_owned(), Vec::new()))
            }
        }
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"default_branch":"master","full_name":"a/b","private":false,"archived":false}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/commits/main"))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_json(json!({"message":"No commit found for SHA: main"})),
            )
            .mount(&server)
            .await;
        for resolved in ["next", "master"] {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/commits/{resolved}")))
                .respond_with(ResponseTemplate::new(200).set_body_string("a".repeat(40)))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path_regex("^/api/v3/repos/a/b/compare/"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
            .mount(&server)
            .await;
        let provider = one_shot_provider(&server);
        let fail = async |row: Value| {
            let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
            super::super::run(&provider, &request, &row, &default_context(), &Passthrough)
                .await
                .expect_err("missing ref")
        };
        let lead = |failure: &GhFailure, name: &str| {
            failure
                .next
                .as_deref()
                .and_then(|next| next.get(name))
                .cloned()
                .unwrap_or(Value::Null)
        };

        let commit = fail(json!({"operation":"commit","owner":"a","repo":"b","ref":"main"})).await;
        assert_eq!(commit.error.kind, ProviderErrorKind::NotFound);
        assert!(commit.message.contains("\"main\""), "{}", commit.message);
        assert!(
            commit
                .message
                .ends_with("; the default branch is \"master\"."),
            "{}",
            commit.message
        );
        let retry = lead(&commit, "atDefaultBranch");
        assert_eq!(retry["tool"], "ghGetHistoryItem");
        assert_eq!(retry["query"]["queries"][0]["ref"], "master");
        assert_eq!(lead(&commit, "viewRefs")["tool"], "ghStructure");

        let compare =
            fail(json!({"operation":"compare","owner":"a","repo":"b","base":"main","head":"next"}))
                .await;
        assert_eq!(
            compare.message,
            "Cannot compare main...next in a/b: \"main\" (base) not found; the default branch is \"master\"."
        );
        let row = &lead(&compare, "atDefaultBranch")["query"]["queries"][0];
        assert_eq!(
            (&row["base"], &row["head"]),
            (&json!("master"), &json!("next"))
        );
        assert_eq!(lead(&compare, "viewRefs")["tool"], "ghStructure");

        let same = fail(
            json!({"operation":"compare","owner":"a","repo":"b","base":"main","head":"master"}),
        )
        .await;
        assert!(lead(&same, "atDefaultBranch").is_null());
        assert!(!lead(&same, "viewRefs").is_null());
    }
}
