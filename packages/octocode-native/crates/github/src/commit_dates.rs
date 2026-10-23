//! The last commit that touched each repository path at one commit: the
//! one source of that fact for every tool (ghStructure's dated listings,
//! ghGetFileContent's receipts).
//!
//! One GraphQL request answers up to [`MAX_PATHS_PER_REQUEST`] paths: each
//! path is an aliased `history(first:1, path:)` on the commit object. Paths
//! travel as GraphQL variables, never inside the document text, so quotes,
//! backslashes, and non-ASCII names need no escaping.
use super::{
    CommitListRequest, GitHubTransport, GraphQlPage, ProviderError, ProviderErrorKind,
    RequestContext,
};
use serde_json::{Map, Value, json};

/// Aliased `history` connections per GraphQL request.
pub const MAX_PATHS_PER_REQUEST: usize = 100;

/// Dates answered for one commit's paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathDates {
    /// The commit's own `committedDate` (RFC 3339, UTC).
    pub commit_date: Option<String>,
    /// Per requested path, in input order: the `committedDate` of the last
    /// commit at or before the commit that touched it. `None` when the path
    /// has no history there or its request failed.
    pub dates: Vec<Option<String>>,
    /// Per requested path, in input order: that commit's author, the GitHub
    /// login when GitHub links one, else the git author name.
    pub authors: Vec<Option<String>>,
    /// The first failure. Requests after it are not sent, so every path of
    /// the failed and later requests stays `None`.
    pub error: Option<ProviderError>,
}

/// The GraphQL document for `count` paths: variables `owner`, `name`, `oid`,
/// and `p0..p{count-1}`; the alias of path `i` is `p{i}`.
pub(crate) fn path_dates_document(count: usize) -> String {
    let mut variables = String::from("$owner:String!,$name:String!,$oid:GitObjectID!");
    let mut selections = String::from("committedDate");
    for index in 0..count {
        variables.push_str(&format!(",$p{index}:String!"));
        selections.push_str(&format!(
            " p{index}:history(first:1,path:$p{index}){{nodes{{committedDate author{{name user{{login}}}}}}}}"
        ));
    }
    format!(
        "query({variables}){{repository(owner:$owner,name:$name){{object(oid:$oid){{...on Commit{{{selections}}}}}}}}}"
    )
}

/// The variables binding [`path_dates_document`] to one request.
pub(crate) fn path_dates_variables(owner: &str, repo: &str, commit: &str, paths: &[&str]) -> Value {
    let mut variables = Map::new();
    variables.insert("owner".into(), json!(owner));
    variables.insert("name".into(), json!(repo));
    variables.insert("oid".into(), json!(commit));
    for (index, path) in paths.iter().enumerate() {
        variables.insert(format!("p{index}"), json!(path));
    }
    Value::Object(variables)
}

/// The commit date and the `count` path commits (date, author) of one
/// response; an answer without the commit object is an error, and so are
/// GraphQL errors that left any requested alias unanswered.
fn parse_path_dates(page: &GraphQlPage, count: usize) -> Result<ParsedDates, ProviderError> {
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
    let date = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    let mut answered = true;
    let commits = (0..count)
        .map(|index| {
            let history = commit.get(format!("p{index}"));
            answered &= history.is_some_and(Value::is_object);
            let node = history.and_then(|history| history.pointer("/nodes/0"));
            (
                date(node.and_then(|node| node.get("committedDate"))),
                date(node.and_then(|node| {
                    node.pointer("/author/user/login")
                        .or_else(|| node.pointer("/author/name"))
                })),
            )
        })
        .collect();
    if !answered && let Some(error) = page.errors.first() {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            format!("GitHub GraphQL: {}", error.message),
        ));
    }
    Ok((date(commit.get("committedDate")), commits))
}

/// A commit's date and the `(date, author)` of each path's last commit.
type ParsedDates = (Option<String>, Vec<(Option<String>, Option<String>)>);

impl GitHubTransport {
    /// The last-commit date of each of `paths` at `commit` (a full SHA) and
    /// the commit's own date, in one GraphQL request per
    /// [`MAX_PATHS_PER_REQUEST`] paths, sent in order under the shared
    /// executor. A disabled, throttled, or tokenless GraphQL sends nothing.
    pub async fn path_commit_dates(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        paths: &[&str],
        context: &RequestContext,
    ) -> PathDates {
        let mut result = PathDates {
            commit_date: None,
            dates: vec![None; paths.len()],
            authors: vec![None; paths.len()],
            error: None,
        };
        if paths.is_empty() {
            return result;
        }
        if let Err(error) = self.graphql_ready(context) {
            result.error = Some(error);
            return result;
        }
        for (chunk, slice) in paths.chunks(MAX_PATHS_PER_REQUEST).enumerate() {
            let answer = self
                .execute_graphql(
                    &path_dates_document(slice.len()),
                    path_dates_variables(owner, repo, commit, slice),
                    context,
                )
                .await
                .and_then(|page| parse_path_dates(&page, slice.len()));
            match answer {
                Ok((commit_date, commits)) => {
                    result.commit_date = result.commit_date.or(commit_date);
                    let start = chunk * MAX_PATHS_PER_REQUEST;
                    for (offset, (date, author)) in commits.into_iter().enumerate() {
                        result.dates[start + offset] = date;
                        result.authors[start + offset] = author;
                    }
                }
                Err(error) => {
                    result.error = Some(error);
                    break;
                }
            }
        }
        result
    }

    /// The `(date, author)` of the last commit at or before `commit` (a full
    /// SHA) that touched `path`, as [`Self::path_commit_dates`] answers it;
    /// `(None, None)` when the path has no history there. When GraphQL
    /// cannot be sent (no token, disabled, throttled), the REST commit list
    /// answers the one path in one request, so a single file's receipt
    /// needs no GraphQL.
    pub async fn path_last_commit(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        context: &RequestContext,
    ) -> Result<(Option<String>, Option<String>), ProviderError> {
        if self.graphql_ready(context).is_ok() {
            let mut answer = self
                .path_commit_dates(owner, repo, commit, &[path], context)
                .await;
            if let Some(error) = answer.error {
                return Err(error);
            }
            return Ok((answer.dates.swap_remove(0), answer.authors.swap_remove(0)));
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

    /// GraphQL can be sent now: enabled, a credential is configured (GitHub
    /// GraphQL rejects anonymous calls), and the bucket is not blocked.
    fn graphql_ready(&self, context: &RequestContext) -> Result<(), ProviderError> {
        if !self.graphql_enabled {
            return Err(ProviderError::new(
                ProviderErrorKind::Configuration,
                "GitHub GraphQL is disabled",
            ));
        }
        if context.resolved_credential().is_none() {
            return Err(ProviderError::new(
                ProviderErrorKind::Authentication,
                "GitHub GraphQL needs a token",
            ));
        }
        if !self.graphql_available(context) {
            return Err(ProviderError::new(
                ProviderErrorKind::RateLimited,
                "GitHub GraphQL rate limit is exhausted",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
