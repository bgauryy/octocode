//! A failed history-item read as the tool states it: the shared wording,
//! the recovery read (the right operation, the listing that names the
//! right item, the refs listing) and, for a ref that is not there, the
//! default branch named (a comparison also retries there).
use octocode_contract_types::{GhGetHistoryItemQuery, GhSearchHistoryQuery};
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use octocode_runtime_base::response::result::wire_query;
use octocode_tools_shared::gh_shared::failed_read::name_missing_owner;
use octocode_tools_shared::gh_shared::{
    DefaultBranchRetry, GhFailure, default_branch_retry, failure_scope, renamed_warning,
    repository_failure, repository_not_found, unresolved_refs, with_repo,
};
use serde_json::{Map, Value, json};

/// The failure of a history-item read of `query` (its wire row): the
/// shared history wording ([`octocode_tools_shared::gh_shared::history::history_failure`]),
/// [`recovery`]'s lead and, for a missing ref, [`missing_ref`]'s message
/// (and a comparison's `atDefaultBranch` retry). A bare 404 of a repository that does not
/// resolve is the shared repository failure, whose leads search for it
/// ([`failure_scope`]), or names the owner when no such login exists
/// ([`name_missing_owner`]). A repository renamed since the query was written is
/// named once in the message ([`renamed_warning`]), and every lead names it
/// as it is now.
pub(crate) async fn recover(
    provider: &GitHubProvider,
    query: &Value,
    error: ProviderError,
    context: &RequestContext,
) -> GhFailure {
    let text = |name: &str| query.get(name).and_then(Value::as_str);
    let (mut error, renamed) = match (text("owner"), text("repo")) {
        (Some(owner), Some(repo)) => {
            let scope = failure_scope(provider, owner, repo, None, error, context).await;
            if repository_not_found(&scope.error) {
                let failure = repository_failure(scope.error, owner, repo);
                return name_missing_owner(provider, failure, owner, context).await;
            }
            let renamed = scope
                .canonical
                .map(|to| ((owner.to_owned(), repo.to_owned()), to));
            let mut error = scope.error;
            // GitHub answered for the repository: only the item is missing.
            if (context.repository_resolves(owner, repo)
                || context.read_full_name(owner, repo).is_some())
                && let Ok(request) = super::HistoryItemRequest::from_row(query)
                && super::bare_not_found(&error, &request)
            {
                error.message =
                    super::not_found_message(&request, error.reason, true).into_boxed_str();
            }
            (error, renamed)
        }
        _ => (error, None),
    };
    let missing = error.reason == Some(ProviderErrorReason::RefNotFound);
    let canonical = renamed
        .as_ref()
        .and_then(|(_, to)| with_repo(query, &to.0, &to.1).ok());
    let query = canonical.as_ref().unwrap_or(query);
    let mut next = recovery(error.kind, error.reason, query);
    if missing && let Some((message, retry)) = missing_ref(provider, query, context).await {
        let message = message.unwrap_or_else(|| error.message.to_string());
        error.message = match retry {
            Some(retry) => {
                let named = retry.message(&message);
                // A commit read's fix is a ref the refs listing names: the
                // default branch's head answers another question. A
                // comparison keeps its retry, which still compares two refs.
                if query.get("operation").and_then(Value::as_str) == Some("compare") {
                    let (name, lead) = retry.lead();
                    next.get_or_insert_with(|| json!({}))[name] = lead;
                }
                named
            }
            None => message,
        }
        .into_boxed_str();
    }
    if let Some((from, to)) = &renamed {
        error.message = format!(
            "{} {}",
            error.message,
            renamed_warning(&from.0, &from.1, to)
        )
        .into_boxed_str();
    }
    let (message, hint) = octocode_tools_shared::gh_shared::history::history_failure(&error, false);
    let mut failure = GhFailure::new(error, message);
    if let Some(hint) = hint {
        failure = failure.hint(hint);
    }
    failure.calls = next.map(Box::new);
    failure
}

/// A history item read at a ref that is not there: a commit's `ref`, or
/// the comparison sides GitHub's one not-found answer leaves unnamed
/// (probed, so the message names each missing side whatever the retry).
/// The retry, named against the default branch, sets each missing ref to
/// it; a comparison it would turn into the default branch against itself
/// has none. `Some(message)` replaces the provider's message.
async fn missing_ref(
    provider: &GitHubProvider,
    query: &Value,
    context: &RequestContext,
) -> Option<(Option<String>, Option<DefaultBranchRetry>)> {
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
        "commit" => Some((None, Some(retry(&["ref"]).await?))),
        "compare" => {
            let (owner, repo) = (text("owner"), text("repo"));
            let sides = [("baseRef", text("baseRef")), ("headRef", text("headRef"))];
            let missing = unresolved_refs(provider, owner, repo, &sides, context).await;
            if missing.is_empty() {
                return None;
            }
            // The retry must still compare two refs: a side that resolved
            // and already is the default branch leaves none.
            let retry = retry(&missing).await.filter(|retry| {
                !sides.iter().all(|(field, reference)| {
                    missing.contains(field) || *reference == retry.branch()
                })
            });
            let named = missing
                .iter()
                .map(|field| format!("\"{}\" ({field})", text(field)))
                .collect::<Vec<_>>()
                .join(" and ");
            let message = format!(
                "Cannot compare {}...{} in {owner}/{repo}: {named} not found as a branch, tag, or SHA",
                text("baseRef"),
                text("headRef")
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
        return Some(
            json!({"viewRefs": octocode_tools_shared::gh_shared::ref_recovery(owner, repo)}),
        );
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
    let read =
        Continuation::related_query(wire_query::<GhGetHistoryItemQuery>(Value::Object(next))?)
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
    let listing = Continuation::related_query(wire_query::<GhSearchHistoryQuery>(
        json!({"operation": search, "owner": owner, "repo": repo,
            "limit": octocode_tools_shared::gh_shared::RECOVERY_LIMIT}),
    )?)
    .build();
    Some(Value::Object(Map::from_iter([(name.to_owned(), listing)])))
}

#[cfg(test)]
mod missing_repository_tests;
#[cfg(test)]
mod tests;
