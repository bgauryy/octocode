//! The last commit that touched a repository path at one commit: the one
//! source of that fact for every tool (ghGetFileContent's receipts).
//!
//! The path travels as a GraphQL variable, never inside the document text, so
//! quotes, backslashes, and non-ASCII names need no escaping.
use super::{
    CommitListRequest, GitHubTransport, GraphQlPage, ProviderError, ProviderErrorKind,
    RequestContext,
};
use serde_json::{Value, json};

/// The date and the author of a path's last commit.
type LastCommit = (Option<String>, Option<String>);

/// Variables `owner`, `name`, `oid` and `path`.
const LAST_COMMIT_DOCUMENT: &str = "query($owner:String!,$name:String!,$oid:GitObjectID!,$path:String!){repository(owner:$owner,name:$name){object(oid:$oid){...on Commit{history(first:1,path:$path){nodes{committedDate author{name user{login}}}}}}}}";

/// The path's last commit in one response; an answer without the commit
/// object is an error, and so are GraphQL errors that left its history
/// unanswered.
fn parse_last_commit(page: &GraphQlPage) -> Result<LastCommit, ProviderError> {
    let commit = page
        .data
        .as_ref()
        .and_then(|data| data.pointer("/repository/object"))
        .filter(|object| object.is_object());
    let Some(commit) = commit else {
        let message = page.errors.first().map_or_else(
            || "GitHub GraphQL returned no commit for the listed ref".to_owned(),
            |error| format!("GitHub GraphQL: {}", error.message),
        );
        return Err(ProviderError::new(ProviderErrorKind::NotFound, message));
    };
    let history = commit.get("history");
    if !history.is_some_and(Value::is_object)
        && let Some(error) = page.errors.first()
    {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            format!("GitHub GraphQL: {}", error.message),
        ));
    }
    let node = history.and_then(|history| history.pointer("/nodes/0"));
    let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    Ok((
        text(node.and_then(|node| node.get("committedDate"))),
        text(node.and_then(|node| {
            node.pointer("/author/user/login")
                .or_else(|| node.pointer("/author/name"))
        })),
    ))
}

impl GitHubTransport {
    /// The `(date, author)` of the last commit at or before `commit` (a full
    /// SHA) that touched `path`; the author is the GitHub login when GitHub
    /// links one, else the git author name. `(None, None)` when the path has
    /// no history there. When GraphQL cannot be sent (no token, throttled), the
    /// REST commit list answers in one request, so a file's receipt needs no
    /// GraphQL.
    pub async fn path_last_commit(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        context: &RequestContext,
    ) -> Result<LastCommit, ProviderError> {
        if self.graphql_ready(context) {
            // Bound to a commit by its `oid`: the answer cannot change.
            let page = self
                .execute_pinned_graphql(
                    LAST_COMMIT_DOCUMENT,
                    json!({"owner": owner, "name": repo, "oid": commit, "path": path}),
                    context,
                )
                .await?;
            return parse_last_commit(&page);
        }
        let page = self
            .list_commits(
                &CommitListRequest {
                    owner: owner.to_owned(),
                    repo: repo.to_owned(),
                    branch: Some(commit.to_owned()),
                    path: Some(path.to_owned()),
                    author: None,
                    since: None,
                    until: None,
                    page: 1,
                    per_page: 1,
                },
                context,
            )
            .await?;
        let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
        Ok(page.items.first().map_or((None, None), |last| {
            (
                text(
                    last.pointer("/commit/committer/date")
                        .or_else(|| last.pointer("/commit/author/date")),
                ),
                text(
                    last.pointer("/author/login")
                        .or_else(|| last.pointer("/commit/author/name")),
                ),
            )
        }))
    }

    /// GraphQL can be sent now: a credential is configured (GitHub
    /// GraphQL rejects anonymous calls), and the bucket is not blocked.
    fn graphql_ready(&self, context: &RequestContext) -> bool {
        context.resolved_credential().is_some() && self.graphql_available(context)
    }
}

#[cfg(test)]
mod tests;
