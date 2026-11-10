//! The last commit that touched a repository path at one commit: the one
//! source of that fact for every tool (ghGetFileContent's receipts).
//!
//! The path travels as a GraphQL variable, never inside the document text, so
//! quotes, backslashes, and non-ASCII names need no escaping.
use super::{
    CommitListRequest, GitHubTransport, GraphQlPage, ProviderError, ProviderErrorKind,
    RequestContext,
};
use futures_util::future::join_all;
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};
use tokio::sync::watch;

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
    Ok(last_commit_of(history))
}

/// The date and author of the first node of a `history` answer.
fn last_commit_of(history: Option<&Value>) -> LastCommit {
    let node = history.and_then(|history| history.pointer("/nodes/0"));
    let text = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_owned);
    (
        text(node.and_then(|node| node.get("committedDate"))),
        text(node.and_then(|node| {
            node.pointer("/author/user/login")
                .or_else(|| node.pointer("/author/name"))
        })),
    )
}

/// The most paths one aliased request asks about.
const WARM_CHUNK: usize = 100;

/// One file a batch will ask the last commit of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastCommitWant {
    pub owner: String,
    pub repo: String,
    /// The full commit SHA the read is pinned to.
    pub sha: String,
    pub path: String,
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Stage {
    Collecting,
    RefsKnown,
    Settled,
}

/// What the warm-up learned about one path.
#[derive(Clone)]
enum Warmed {
    Known(LastCommit),
    /// Asked and not answered: the path asks REST itself, never GraphQL again.
    Unanswered,
    /// The whole request was refused for the credential: REST with the same
    /// credential would be refused too, so the path fails as its own GraphQL
    /// request would have.
    Failed(ProviderError),
}

impl Warmed {
    /// What a GraphQL request that failed means for the paths it asked about.
    fn of_failure(error: ProviderError) -> Self {
        if error.kind == ProviderErrorKind::Authentication {
            Self::Failed(error)
        } else {
            Self::Unanswered
        }
    }
}

type RefKey = (String, String, String);

/// A ref's resolution and the spelling of the repository it was asked under.
type RefAnswer = ((String, String), Result<String, ProviderError>);

/// The paths of one repository commit, keyed by (owner, repo, commit).
type CommitGroup<'a> = (RefKey, Vec<&'a LastCommitWant>);
type PathKey = (String, String, String, String);

fn lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// What one batch warmed before its rows ran: the commits its refs name and
/// its paths' last commits, held by the batch and read by its rows' requests.
/// A reader waits for the warm-up stage it needs, so a row's own work (its
/// file read) overlaps the warm-up instead of following it; a warm-up that
/// ends without answering leaves the reader to ask for itself.
pub struct LastCommitMemo {
    stage: watch::Sender<Stage>,
    refs: Mutex<HashMap<RefKey, RefAnswer>>,
    commits: Mutex<HashMap<PathKey, Warmed>>,
    /// The request that resolved the refs: what it observed of each
    /// repository is what a row that reads its answer would have observed.
    observed: Mutex<Option<RequestContext>>,
}

impl Default for LastCommitMemo {
    fn default() -> Self {
        Self::new()
    }
}

impl LastCommitMemo {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stage: watch::channel(Stage::Collecting).0,
            refs: Mutex::default(),
            commits: Mutex::default(),
            observed: Mutex::default(),
        }
    }

    fn reference_key(owner: &str, repo: &str, reference: Option<&str>) -> RefKey {
        (
            lower(owner),
            lower(repo),
            reference.unwrap_or("HEAD").to_owned(),
        )
    }

    /// What resolving `reference` of `owner/repo` answered: a request that
    /// failed fails each row that reads it, with the same error.
    pub fn record_ref(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
        resolved: Result<String, ProviderError>,
    ) {
        self.refs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                Self::reference_key(owner, repo, reference),
                ((owner.to_owned(), repo.to_owned()), resolved),
            );
    }

    /// The request that resolved the batch's refs.
    pub fn observed_by(&self, request: &RequestContext) {
        *self.observed.lock().unwrap_or_else(PoisonError::into_inner) = Some(request.clone());
    }

    /// Adds the warm-up's client-side waits so far to `row`, which waited
    /// on the warm-up: its admission was the row's wait too. The time the
    /// warm-up's own sends took is provider latency, not recorded.
    pub(crate) fn adopt_waits(&self, row: &RequestContext) {
        if let Some(request) = self
            .observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            row.waits.adopt(&request.waits);
        }
    }

    /// Gives `row` what the refs' request observed of the repositories.
    pub(crate) fn adopt_observed(&self, row: &RequestContext) {
        if let Some(request) = self
            .observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            row.adopt_repository_facts(request);
        }
    }

    /// Every ref of the batch has been tried: readers of refs may go on.
    pub fn refs_known(&self) {
        self.stage.send_if_modified(|stage| {
            let advance = *stage < Stage::RefsKnown;
            if advance {
                *stage = Stage::RefsKnown;
            }
            advance
        });
    }

    /// The warm-up is over, whatever it learned: readers of commits may go on.
    pub fn settle(&self) {
        self.stage.send_replace(Stage::Settled);
    }

    async fn reached(&self, stage: Stage) {
        // A closed channel cannot happen while `self` lives; either way the
        // reader then reads what exists.
        let _ = self.stage.subscribe().wait_for(|now| *now >= stage).await;
    }

    /// What the warm-up resolved `reference` to, once the refs are known;
    /// `None` when it did not try.
    pub async fn resolved_ref(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
    ) -> Option<Result<String, ProviderError>> {
        self.reached(Stage::RefsKnown).await;
        self.refs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&Self::reference_key(owner, repo, reference))
            // An error names the repository as it was asked: only that
            // spelling reads it; another resolves for itself.
            .filter(|((asked_owner, asked_repo), resolved)| {
                resolved.is_ok() || (asked_owner == owner && asked_repo == repo)
            })
            .map(|(_, resolved)| resolved.clone())
    }

    fn path_key(want: &LastCommitWant) -> PathKey {
        (
            lower(&want.owner),
            lower(&want.repo),
            want.sha.to_ascii_lowercase(),
            want.path.clone(),
        )
    }

    fn record(&self, want: &LastCommitWant, warmed: Warmed) {
        self.commits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(Self::path_key(want), warmed);
    }

    async fn warmed(&self, want: &LastCommitWant) -> Option<Warmed> {
        self.reached(Stage::Settled).await;
        self.commits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&Self::path_key(want))
            .cloned()
    }
}

/// Settles a memo when dropped: a warm-up that ends, fails or is aborted
/// never leaves a row waiting.
pub struct SettleOnDrop(pub std::sync::Arc<LastCommitMemo>);

impl Drop for SettleOnDrop {
    fn drop(&mut self) {
        self.0.refs_known();
        self.0.settle();
    }
}

/// One aliased document for `count` paths of one commit: variables `owner`,
/// `name`, `oid` and `p<i>`; the answer's alias `p<i>` is path `i`'s history.
fn warm_document(count: usize) -> String {
    let variables = (0..count).fold(String::new(), |mut text, index| {
        text.push_str(&format!(",$p{index}:String!"));
        text
    });
    let histories = (0..count).fold(String::new(), |mut text, index| {
        text.push_str(&format!(
            "p{index}:history(first:1,path:$p{index}){{nodes{{committedDate author{{name user{{login}}}}}}}}"
        ));
        text
    });
    format!(
        "query($owner:String!,$name:String!,$oid:GitObjectID!{variables}){{repository(owner:$owner,name:$name){{object(oid:$oid){{...on Commit{{{histories}}}}}}}}}"
    )
}

/// Each path's answer in one aliased response. A path is known only when its
/// alias carries a history and no error names it; an error that names no
/// alias leaves every path unanswered.
fn parse_warm(page: &GraphQlPage, count: usize) -> Vec<Warmed> {
    let object = page
        .data
        .as_ref()
        .and_then(|data| data.pointer("/repository/object"))
        .filter(|object| object.is_object());
    let anonymous_error = page.errors.iter().any(|error| {
        !error
            .path
            .iter()
            .any(|step| step.as_str().is_some_and(|s| s.starts_with('p')))
    });
    (0..count)
        .map(|index| {
            let alias = format!("p{index}");
            let named = page.errors.iter().any(|error| {
                error
                    .path
                    .iter()
                    .any(|step| step.as_str() == Some(alias.as_str()))
            });
            match object.and_then(|object| object.get(&alias)) {
                Some(history) if history.is_object() && !named && !anonymous_error => {
                    Warmed::Known(last_commit_of(Some(history)))
                }
                _ => Warmed::Unanswered,
            }
        })
        .collect()
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
        let want = LastCommitWant {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            sha: commit.to_owned(),
            path: path.to_owned(),
        };
        let warmed = match context.last_commits() {
            Some(memo) => {
                let warmed = memo.warmed(&want).await;
                memo.adopt_waits(context);
                warmed
            }
            None => None,
        };
        match warmed {
            Some(Warmed::Known(last)) => return Ok(last),
            Some(Warmed::Failed(error)) => return Err(error),
            // The warm-up asked GraphQL and was not answered: REST answers.
            Some(Warmed::Unanswered) => {}
            None if self.graphql_ready(context) => {
                return self
                    .graphql_last_commit(owner, repo, commit, path, context)
                    .await;
            }
            None => {}
        }
        self.rest_last_commit(owner, repo, commit, path, context)
            .await
    }

    /// The path's last commit by its own GraphQL request.
    async fn graphql_last_commit(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        context: &RequestContext,
    ) -> Result<LastCommit, ProviderError> {
        // Bound to a commit by its `oid`: the answer cannot change.
        let page = self
            .execute_pinned_graphql(
                LAST_COMMIT_DOCUMENT,
                json!({"owner": owner, "name": repo, "oid": commit, "path": path}),
                context,
            )
            .await?;
        parse_last_commit(&page)
    }

    /// The path's last commit by the REST commit list, one request.
    async fn rest_last_commit(
        &self,
        owner: &str,
        repo: &str,
        commit: &str,
        path: &str,
        context: &RequestContext,
    ) -> Result<LastCommit, ProviderError> {
        // One commit is wanted: the list's `Link` is never followed. The list
        // is bound to a commit by its `sha`, so its answer cannot change.
        let spec = self
            .commit_list_spec(
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
                None,
            )?
            .reading_first_page_only()
            .pinned_to_commit();
        let page = self.read_commit_list(spec, context).await?;
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

    /// Asks the last commit of every path in `wants` with one aliased GraphQL
    /// request per (repository, commit), at most [`WARM_CHUNK`] paths each,
    /// and records the answers in `memo`. A path whose request failed, or
    /// whose alias is missing or named by an error, is recorded unanswered:
    /// it asks REST itself. Nothing is asked when GraphQL cannot be sent.
    pub async fn warm_last_commits(
        &self,
        wants: &[LastCommitWant],
        memo: &LastCommitMemo,
        context: &RequestContext,
    ) {
        if !self.graphql_ready(context) {
            return;
        }
        let mut groups: Vec<CommitGroup<'_>> = Vec::new();
        for want in wants {
            let key = (
                lower(&want.owner),
                lower(&want.repo),
                want.sha.to_ascii_lowercase(),
            );
            match groups.iter_mut().find(|(known, _)| *known == key) {
                Some((_, members)) => {
                    if !members.iter().any(|member| member.path == want.path) {
                        members.push(want);
                    }
                }
                None => groups.push((key, vec![want])),
            }
        }
        let requests = groups
            .iter()
            .flat_map(|(_, members)| members.chunks(WARM_CHUNK))
            .map(|chunk| async move {
                let first = chunk[0];
                if let [only] = chunk {
                    // The request a lone read always sent: its kept and
                    // recorded answers still serve it.
                    let answered = self
                        .execute_pinned_graphql(
                            LAST_COMMIT_DOCUMENT,
                            json!({"owner": only.owner, "name": only.repo, "oid": only.sha, "path": only.path}),
                            context,
                        )
                        .await
                        .and_then(|page| parse_last_commit(&page));
                    memo.record(
                        only,
                        match answered {
                            Ok(last) => Warmed::Known(last),
                            Err(error) => Warmed::of_failure(error),
                        },
                    );
                    return;
                }
                let mut variables = Map::new();
                variables.insert("owner".into(), json!(first.owner));
                variables.insert("name".into(), json!(first.repo));
                variables.insert("oid".into(), json!(first.sha));
                for (index, want) in chunk.iter().enumerate() {
                    variables.insert(format!("p{index}"), json!(want.path));
                }
                let answered = self
                    .execute_pinned_graphql(
                        &warm_document(chunk.len()),
                        Value::Object(variables),
                        context,
                    )
                    .await;
                let outcomes = match answered {
                    Ok(page) => parse_warm(&page, chunk.len()),
                    Err(error) => vec![Warmed::of_failure(error); chunk.len()],
                };
                for (want, outcome) in chunk.iter().zip(outcomes) {
                    memo.record(want, outcome);
                }
            });
        join_all(requests).await;
    }

    /// GraphQL can be sent now: a credential is configured (GitHub
    /// GraphQL rejects anonymous calls), and the bucket is not blocked.
    fn graphql_ready(&self, context: &RequestContext) -> bool {
        context.resolved_credential().is_some() && self.graphql_available(context)
    }
}

#[cfg(test)]
mod tests;
