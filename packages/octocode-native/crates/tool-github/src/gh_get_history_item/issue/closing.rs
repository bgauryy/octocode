//! What closed an issue: the pull requests GitHub links as closing it, the
//! commit whose push closed it, each close and reopen, and the fix read
//! they lead to.
use crate::gh_get_history_item::HistoryItemRequest;
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::util::str_at;
use octocode_contract_types::{GhGetHistoryItemQuery, GhSearchHistoryQuery};
use octocode_github::{GitHubTransport, GraphQlError, RequestContext};
use octocode_runtime_base::response::result::wire_query;
use octocode_runtime_base::response::result::{Continuation, remove_nulls};
use octocode_tools_shared::gh_shared::dates::utc_value;
use octocode_tools_shared::gh_shared::ensure_calls;
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

/// The closing read of one page of the issue's closing references, with
/// `$events` selected beside them.
macro_rules! closing_document {
    ($events:literal) => {
        concat!(
            "query($owner:String!,$repo:String!,$number:Int!,$first:Int!,$after:String){ repository(owner:$owner,name:$repo){ issue(number:$number){ ",
            $events,
            "closedByPullRequestsReferences(first:$first,after:$after,includeClosedPrs:true){ totalCount pageInfo{ hasNextPage endCursor } nodes{ number state mergedAt additions deletions changedFiles repository{ nameWithOwner } } } } } }"
        )
    };
}

/// One page of the closing-reference lookup: `first` is GitHub's page
/// maximum, `after` the previous page's end cursor (null on the first).
pub(in crate::gh_get_history_item) const CLOSING_REFERENCES_DOCUMENT: &str = closing_document!("");

/// The first page, in the same request as the issue's latest 100 close and
/// reopen events (`last:100`, oldest first): when, by whom, why, and the
/// commit or pull request that closed it.
pub(in crate::gh_get_history_item) const CLOSING_FIRST_DOCUMENT: &str = closing_document!(
    "timelineItems(itemTypes:[CLOSED_EVENT,REOPENED_EVENT],last:100){ totalCount nodes{ __typename ... on ClosedEvent{ createdAt stateReason actor{ login } closer{ __typename ... on Commit{ oid repository{ nameWithOwner } } ... on PullRequest{ number repository{ nameWithOwner } } } } ... on ReopenedEvent{ createdAt actor{ login } } } } "
);

/// One linked pull request: its public `closedBy` row and whether its whole
/// diff fits one patch read.
pub(super) struct ClosingPr {
    pub(super) row: Value,
    small: bool,
    /// The repository GitHub names the pull request in (`owner/repo`); a
    /// fix in another repository is named there on its row and its read.
    repository: Option<(String, String)>,
}

/// The commit whose push made the issue's last close: its `closedBy` row,
/// named in its own repository when that is not the issue's.
struct CommitCloser {
    row: Value,
}

/// What the closing read could not show: GitHub withheld it from the token
/// (`errors[]` beside the data, or a nulled node).
#[derive(Default)]
struct Withheld {
    /// Linked pull requests nulled in the answer.
    pull_requests: usize,
    /// An error named the closing references: some closer may be missing
    /// even where no node was nulled.
    references: bool,
    /// An error named the close and reopen events, or nulled one.
    events: bool,
}

/// What one closing read returned: the linked pull requests (merged first),
/// how many the first window lists, GitHub's total when more exist than
/// that, the commit closer, each close and reopen, and what was withheld.
pub(super) struct ClosingReferences {
    pub(super) prs: Vec<ClosingPr>,
    pub(super) listed: usize,
    /// GitHub linked pull requests, and none of them closed the issue, nor
    /// did a known commit: the fix is not known from the links.
    pub(super) unsettled: bool,
    pub(super) bounded_total: Option<u64>,
    commit: Option<CommitCloser>,
    /// Close and reopen transitions, oldest first.
    events: Vec<Value>,
    /// GitHub's count of the transitions, when more exist than were read.
    events_total: Option<u64>,
    withheld: Withheld,
}

impl ClosingReferences {
    /// The linked pull requests `prs` alone: no commit closer, no events,
    /// nothing withheld.
    fn linked(prs: Vec<ClosingPr>) -> Self {
        Self {
            listed: prs.len(),
            prs,
            unsettled: false,
            bounded_total: None,
            commit: None,
            events: Vec::new(),
            events_total: None,
            withheld: Withheld::default(),
        }
    }

    /// Only the closers: pull requests whose merge closed the issue, or
    /// (while it is open) would close it: merged ones, and an open issue's
    /// open ones; and the commit of the last close, while the issue is
    /// closed. GitHub lists every pull request that declares a fix, so one
    /// closed without merging, or one still open on an issue closed another
    /// way, is a link, not a closer.
    pub(super) fn closers(mut self, issue_closed: bool) -> Self {
        let linked = !self.prs.is_empty();
        self.prs.retain(|pr| match pr.row["state"].as_str() {
            Some("merged") => true,
            Some("open") => !issue_closed,
            _ => pr.row.get("mergedAt").is_some(),
        });
        if !issue_closed {
            self.commit = None;
        }
        self.listed = self.prs.len();
        self.unsettled = linked && self.prs.is_empty() && self.commit.is_none();
        self
    }

    /// The closing pull requests a fix lead may follow: `None` when the
    /// links settle nothing ([`Self::unsettled`]).
    pub(super) fn settled(&self) -> Option<&[ClosingPr]> {
        (!self.unsettled).then_some(self.prs.as_slice())
    }

    /// The `closedBy` rows: the closing pull requests, then the commit
    /// whose push closed the issue.
    pub(super) fn rows(&self) -> Vec<Value> {
        self.prs
            .iter()
            .map(|pr| pr.row.clone())
            .chain(self.commit.as_ref().map(|commit| commit.row.clone()))
            .collect()
    }

    /// Each close and reopen, oldest first, when the issue changed state
    /// more than once: one close is stated by `closedAt`, `stateReason` and
    /// `closedBy`.
    pub(super) fn state_events(&self) -> Option<Value> {
        (self.events.len() > 1).then(|| Value::Array(self.events.clone()))
    }

    /// The warnings for what GitHub withheld from the token; empty when the
    /// closing record is whole. A withheld closer leads to the related
    /// search ([`attach_fix_leads`]).
    pub(super) fn withheld_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        let withheld = &self.withheld;
        match withheld.pull_requests {
            0 if withheld.references => warnings.push(
                "GitHub withheld part of this issue's linked pull requests from this token (access or SAML): closedBy may omit a closer; the related search lists pull requests that name this issue.".to_owned(),
            ),
            0 => {}
            1 => warnings.push(
                "GitHub withheld 1 linked pull request from this token (access or SAML): closedBy omits it; the related search lists pull requests that name this issue.".to_owned(),
            ),
            count => warnings.push(format!(
                "GitHub withheld {count} linked pull requests from this token (access or SAML): closedBy omits them; the related search lists pull requests that name this issue."
            )),
        }
        if withheld.events {
            warnings.push(
                "GitHub withheld some of this issue's close and reopen events from this token: stateEvents and a closing commit may be missing.".to_owned(),
            );
        }
        warnings
    }

    /// The bound of a listed `stateEvents`: GitHub holds more transitions
    /// than the read lists (the latest ones).
    pub(super) fn events_bound(&self) -> Option<String> {
        let total = self.events_total?;
        (self.events.len() > 1).then(|| {
            format!(
                "stateEvents lists the latest {} of {total} close and reopen events.",
                self.events.len()
            )
        })
    }
}

/// The pull requests GitHub links as closing the issue, merged first:
/// `{number, state, mergedAt?}`, with the commit of its last close and its
/// close and reopen events, from one GraphQL request (and one more per
/// further page of links); [`ClosingReferences::closers`] keeps the ones
/// that closed it. `None` when GraphQL is unavailable or failed (the caller
/// falls back to search); no rows when none are linked.
pub(super) async fn closing_pull_requests(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    let (owner, repo) = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
    );
    let answer = closing_answer(transport, query, context).await?;
    let references_errored = answer.errored("closedByPullRequestsReferences");
    let events_errored = answer.errored("timelineItems");
    let ClosingAnswer {
        references,
        timeline,
        ..
    } = answer;
    // A connection missing with no error naming it is no answer: as if
    // GraphQL had failed.
    let references = match references {
        Some(references) => references,
        None if references_errored => json!({}),
        None => return None,
    };
    let more = references
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(true);
    let nodes = references
        .get("nodes")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let mut prs = map_closing_pull_requests(nodes);
    name_other_repositories(&mut prs, owner, repo);
    let listed = u64::try_from(prs.len()).unwrap_or(u64::MAX);
    let timeline = timeline.as_ref();
    let events = timeline
        .and_then(|timeline| timeline.get("nodes"))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let mut closing = ClosingReferences::linked(prs);
    closing.bounded_total = more.then(|| {
        references
            .get("totalCount")
            .and_then(Value::as_u64)
            .map_or(listed, |total| total.max(listed))
    });
    closing.commit = events
        .last()
        .and_then(|last| commit_closer(last, owner, repo));
    closing.events = events.iter().filter_map(state_event).collect();
    closing.events_total = timeline
        .and_then(|timeline| timeline.get("totalCount"))
        .and_then(Value::as_u64)
        .filter(|&total| total > u64::try_from(events.len()).unwrap_or(u64::MAX));
    closing.withheld = Withheld {
        pull_requests: nodes.iter().filter(|node| node.is_null()).count(),
        references: references_errored,
        events: events_errored || events.iter().any(Value::is_null),
    };
    Some(closing)
}

/// The closing read as GitHub answered it: the closing references (every
/// page, read on from its cursor), the first page's close and reopen
/// events, and the issue fields its `errors[]` name.
struct ClosingAnswer {
    references: Option<Value>,
    timeline: Option<Value>,
    errored: Vec<String>,
}

impl ClosingAnswer {
    fn errored(&self, field: &str) -> bool {
        self.errored.iter().any(|name| name == field)
    }
}

/// The issue's closing references and events within the request's
/// deadline. A later page that fails keeps the pages before it, with
/// `hasNextPage` still true (the bound is disclosed). `None` when GraphQL
/// is unavailable or its first page failed.
async fn closing_answer(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingAnswer> {
    if !transport.graphql_available(context) {
        return None;
    }
    let mut variables = json!({
        "owner": gh_get_history_item_query::owner(query),
        "repo": gh_get_history_item_query::repo(query),
        "number": gh_get_history_item_query::number(query)?,
        "first": octocode_github::MAX_PER_PAGE,
        "after": null});
    let (mut issue, mut errored) =
        page_of(transport, CLOSING_FIRST_DOCUMENT, &variables, context).await?;
    let timeline = take_object(&mut issue, "timelineItems");
    let mut references = take_object(&mut issue, "closedByPullRequestsReferences");
    if let Some(held) = references.as_mut() {
        while held.pointer("/pageInfo/hasNextPage") == Some(&Value::Bool(true)) {
            let Some(cursor) = held.pointer("/pageInfo/endCursor").cloned() else {
                break;
            };
            variables["after"] = cursor;
            let Some((mut page, more_errored)) =
                page_of(transport, CLOSING_REFERENCES_DOCUMENT, &variables, context).await
            else {
                break;
            };
            errored.extend(more_errored);
            let Some(mut next) = take_object(&mut page, "closedByPullRequestsReferences") else {
                break;
            };
            let nodes = next.get_mut("nodes").map(Value::take);
            if let (Some(list), Some(Value::Array(nodes))) =
                (held.get_mut("nodes").and_then(Value::as_array_mut), nodes)
            {
                list.extend(nodes);
            }
            held["pageInfo"] = next["pageInfo"].take();
        }
    }
    Some(ClosingAnswer {
        references,
        timeline,
        errored,
    })
}

/// `issue[key]` taken out, when it is an object.
fn take_object(issue: &mut Value, key: &str) -> Option<Value> {
    issue.get_mut(key).map(Value::take).filter(Value::is_object)
}

/// One page of the closing read: the issue GitHub answered, and the issue
/// fields its `errors[]` name. `None` when the read failed or answered no
/// issue.
async fn page_of(
    transport: &GitHubTransport,
    document: &str,
    variables: &Value,
    context: &RequestContext,
) -> Option<(Value, Vec<String>)> {
    let page = transport
        .execute_graphql(document, variables.clone(), context)
        .await
        .ok()?;
    let errored = page
        .errors
        .iter()
        .filter_map(issue_field)
        .map(str::to_owned)
        .collect();
    let issue = page
        .data?
        .pointer_mut("/repository/issue")
        .map(Value::take)
        .filter(Value::is_object)?;
    Some((issue, errored))
}

/// The issue field an error nulled (part of): the response key under
/// `repository.issue`, `None` for an error outside the issue.
fn issue_field(error: &GraphQlError) -> Option<&str> {
    match error.path.as_slice() {
        [
            Value::String(repository),
            Value::String(issue),
            Value::String(field),
            ..,
        ] if repository == "repository" && issue == "issue" => Some(field),
        _ => None,
    }
}

/// One close or reopen event as a `stateEvents` row: `event`, `at`, `by`,
/// a close's `stateReason` (GitHub's enum, lowercased as REST spells it)
/// and its closer (`commitSha` or `pullRequest`). `None` for a node that
/// is neither (a nulled one).
fn state_event(node: &Value) -> Option<Value> {
    let event = match str_at(node, "/__typename")? {
        "ClosedEvent" => "closed",
        "ReopenedEvent" => "reopened",
        _ => return None,
    };
    let closer = |kind: &str| (str_at(node, "/closer/__typename") == Some(kind)).then_some(());
    let mut row = json!({
        "event": event,
        "at": utc_value(node.get("createdAt")),
        "by": str_at(node, "/actor/login"),
        "stateReason": str_at(node, "/stateReason").map(str::to_ascii_lowercase),
        "commitSha": closer("Commit").and_then(|()| str_at(node, "/closer/oid")),
        "pullRequest": closer("PullRequest")
            .and_then(|()| node.pointer("/closer/number"))
            .and_then(Value::as_u64)
            .filter(|&number| number >= 1),
    });
    remove_nulls(&mut row);
    Some(row)
}

/// The commit whose push made the close `event`, as a `closedBy` row
/// (`{commitSha}`, with `owner`/`repo` when it lives in another repository
/// than the issue's `owner`/`repo`); `None` when another kind of closer, or
/// none, closed it.
fn commit_closer(event: &Value, owner: &str, repo: &str) -> Option<CommitCloser> {
    if str_at(event, "/__typename") != Some("ClosedEvent")
        || str_at(event, "/closer/__typename") != Some("Commit")
    {
        return None;
    }
    let sha = str_at(event, "/closer/oid").filter(|sha| !sha.is_empty())?;
    let mut row = json!({"commitSha": sha});
    if let Some((other_owner, other_repo)) = str_at(event, "/closer/repository/nameWithOwner")
        .and_then(|name| name.split_once('/'))
        .filter(|(other_owner, other_repo)| {
            !(other_owner.eq_ignore_ascii_case(owner) && other_repo.eq_ignore_ascii_case(repo))
        })
    {
        row["owner"] = json!(other_owner);
        row["repo"] = json!(other_repo);
    }
    Some(CommitCloser { row })
}

/// The linked pull requests as `closedBy` rows, merged first. A node whose
/// number is not a valid one (GitHub numbers start at 1), or one GitHub
/// nulled, names no pull request: it yields no row and no lead.
fn map_closing_pull_requests(nodes: &[Value]) -> Vec<ClosingPr> {
    let mut prs = nodes
        .iter()
        .filter_map(|node| {
            let number = node
                .get("number")
                .and_then(Value::as_u64)
                .filter(|&n| n >= 1)?;
            let mut row = json!({
                "number": number,
                "state": str_at(node, "/state").unwrap_or("closed").to_ascii_lowercase(),
                "mergedAt": node.get("mergedAt").filter(|v| !v.is_null())});
            remove_nulls(&mut row);
            let count = |key: &str| node.get(key).and_then(Value::as_u64);
            let small = crate::gh_get_history_item::pr_menu::is_small_pr(
                count("additions"),
                count("deletions"),
                count("changedFiles"),
            );
            let repository = str_at(node, "/repository/nameWithOwner")
                .and_then(|name| name.split_once('/'))
                .map(|(owner, repo)| (owner.to_owned(), repo.to_owned()));
            Some(ClosingPr {
                row,
                small,
                repository,
            })
        })
        .collect::<Vec<_>>();
    // Stable: merged first, otherwise GitHub's order.
    prs.sort_by_key(|pr| pr.row.get("mergedAt").is_none());
    prs
}

/// A closing pull request in another repository than the issue's is named
/// by its `owner`/`repo` on its row: a bare number would read as the
/// issue's repository. One in the issue's repository (any case) keeps the
/// row's number alone, and its read the issue's repository.
fn name_other_repositories(prs: &mut [ClosingPr], owner: &str, repo: &str) {
    for pr in prs {
        let same = pr
            .repository
            .as_ref()
            .is_none_or(|(other_owner, other_repo)| {
                other_owner.eq_ignore_ascii_case(owner) && other_repo.eq_ignore_ascii_case(repo)
            });
        if same {
            pr.repository = None;
        } else if let Some((other_owner, other_repo)) = &pr.repository
            && let Some(row) = pr.row.as_object_mut()
        {
            row.insert("owner".into(), json!(other_owner));
            row.insert("repo".into(), json!(other_repo));
        }
    }
}

/// The fix leads of a closed or linked issue, ahead of its other leads: a
/// related read opens the first closing (merged-first) pull request; with
/// no closing information (`None`: GraphQL unavailable, or links that
/// closed nothing) a closed issue offers the keyword search, as does a
/// read whose closers GitHub withheld in part (the search finds the ones
/// the token can read). A closing commit is its own lead: `closedBy`
/// states its `commitSha`.
pub(super) fn attach_fix_leads(
    out: &mut Value,
    query: &HistoryItemRequest,
    closing: Option<&ClosingReferences>,
    closed: bool,
) {
    let settled = closing.and_then(ClosingReferences::settled);
    let withheld = closing
        .is_some_and(|closing| closing.withheld.pull_requests > 0 || closing.withheld.references);
    let mut leads = Vec::new();
    if let Some(pr) = settled.and_then(<[ClosingPr]>::first) {
        leads.extend(read_pull_request(query, pr));
    }
    if (closed && settled.is_none()) || withheld {
        // A withheld closer is one `closedBy` omits: the row needs the
        // search that lists it.
        leads.extend(find_pull_request(query).map(|(name, call)| {
            if withheld {
                (
                    name,
                    octocode_runtime_base::response::result::required_call(call),
                )
            } else {
                (name, call)
            }
        }));
    }
    if leads.is_empty() {
        return;
    }
    ensure_calls(out);
    // Related fix evidence precedes optional discussion exploration.
    if let Some(calls) = out["calls"].as_object_mut() {
        for (at, (name, call)) in leads.into_iter().enumerate() {
            calls.shift_insert(at, name.to_owned(), call);
        }
    }
}

/// The read of a closing pull request. A small fix reads whole in one
/// call: its patches are the evidence (`closedBy` already links it to the
/// issue). A larger one names its files, beside its description, so the
/// review can pick patches.
fn read_pull_request(query: &HistoryItemRequest, pr: &ClosingPr) -> Option<(&'static str, Value)> {
    let (owner, repo) = pr.repository.as_ref().map_or_else(
        || {
            (
                gh_get_history_item_query::owner(query),
                gh_get_history_item_query::repo(query),
            )
        },
        |(owner, repo)| (owner.as_str(), repo.as_str()),
    );
    let read = wire_query::<GhGetHistoryItemQuery>(json!({
        "operation":"pullRequest","owner":owner,"repo":repo,
        "number":pr.row["number"],
        "sections": if pr.small { json!(["patches"]) } else { json!(["body", "files"]) }}))?;
    Some(("readPullRequest", Continuation::related_query(read).build()))
}

/// The keyword search for pull requests that name the issue's number.
fn find_pull_request(query: &HistoryItemRequest) -> Option<(&'static str, Value)> {
    let search = wire_query::<GhSearchHistoryQuery>(json!({
        "operation":"pullRequest",
        "owner":gh_get_history_item_query::owner(query),
        "repo":gh_get_history_item_query::repo(query),
        "keywords":[gh_get_history_item_query::number(query).unwrap_or_default().to_string()]}))?;
    Some((
        "findPullRequest",
        Continuation::related_query(search).build(),
    ))
}
