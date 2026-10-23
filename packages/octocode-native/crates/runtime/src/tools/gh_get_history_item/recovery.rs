//! A failed history-item read as the tool states it: the shared wording,
//! the recovery read (the right operation, the listing that names the
//! right item, the refs listing) and, for a ref that is not there, the same
//! call at the default branch.
use crate::id::ToolId;
use crate::response::result::Continuation;
use crate::tools::gh_shared::{
    DefaultBranchRetry, GhFailure, RenameCheck, canonical_repo, default_branch_retry,
    renamed_warning, unresolved_refs, with_repo,
};
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::{Map, Value, json};

/// The failure of a history-item read of `query` (its wire row): the
/// shared history wording ([`crate::tools::gh_shared::history::history_failure`]),
/// [`recovery`]'s lead and, for a missing ref, [`missing_ref`]'s message
/// and `atDefaultBranch` retry. A repository renamed since the query was
/// written is named once on a missing-ref failure ([`renamed_warning`]), and
/// every lead names it as it is now.
pub(crate) async fn recover(
    provider: &GitHubProvider,
    query: &Value,
    mut error: ProviderError,
    context: &RequestContext,
) -> GhFailure {
    let missing = error.reason == Some(ProviderErrorReason::RefNotFound);
    let renamed = if missing {
        renamed_repository(provider, query, context).await
    } else {
        None
    };
    let canonical = renamed
        .as_ref()
        .and_then(|(_, to)| with_repo(query, &to.0, &to.1).ok());
    let query = canonical.as_ref().unwrap_or(query);
    let mut next = recovery(error.kind, error.reason, query);
    if missing && let Some((message, retry)) = missing_ref(provider, query, context).await {
        error.message = retry
            .message(message.as_deref().unwrap_or(&error.message))
            .into_boxed_str();
        let (name, lead) = retry.lead();
        next.get_or_insert_with(|| json!({}))[name] = lead;
    }
    if let Some((from, to)) = &renamed {
        error.message = format!(
            "{} {}",
            error.message,
            renamed_warning(&from.0, &from.1, to)
        )
        .into_boxed_str();
    }
    let (message, hint) = crate::tools::gh_shared::history::history_failure(&error, false);
    let mut failure = GhFailure::new(error, message);
    if let Some(hint) = hint {
        failure = failure.hint(hint);
    }
    failure.next = next.map(Box::new);
    failure
}

/// The repository `query` names as `((owner, repo), canonical)` when GitHub
/// redirected it to another name (the memoized rename check: free for a
/// repository GitHub never redirected).
async fn renamed_repository(
    provider: &GitHubProvider,
    query: &Value,
    context: &RequestContext,
) -> Option<((String, String), (String, String))> {
    let text = |name: &str| query.get(name).and_then(Value::as_str);
    let (owner, repo) = (text("owner")?, text("repo")?);
    let to = canonical_repo(provider, owner, repo, context, RenameCheck::Redirected)
        .await
        .ok()
        .flatten()?;
    Some(((owner.to_owned(), repo.to_owned()), to))
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
    let read = Continuation::new(ToolId::GhGetHistoryItem, Value::Object(next)).build();
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
    .build();
    Some(Value::Object(Map::from_iter([(name.to_owned(), listing)])))
}

#[cfg(test)]
mod tests;
