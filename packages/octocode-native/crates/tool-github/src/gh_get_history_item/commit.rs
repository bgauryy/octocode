//! `operation: "commit"`: one commit's header and its changed-file page.
use super::file_page::{
    DiffSides, FilePage, Lead, empty_scope_warning, lead_first, patch_needle, shape_file_page,
};
use super::filter::{FileFilter, FileScope, FilteredPage};
use super::inventory::file_page_size;
use super::patch_hop::{attach_diff_continuations, commit_pull_request_lead, commit_query};
use super::util::{str_at, string};
use super::window::{
    COMMIT_FILE_BATCH, MAX_COMMIT_FILE_BATCHES, WindowSpec, commit_file_items, load_window_with,
};
use super::{HistoryItemRequest, ItemOperation, validation};
use crate::gh_get_history_item::Source;
use crate::gh_get_history_item::gh_get_history_item_query;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use octocode_runtime_base::response::paging::push_warnings;
use octocode_runtime_base::response::result::{ToolData, remove_nulls};
use octocode_tools_shared::gh_shared::dates::utc_date;
use serde_json::{Value, json};

pub(super) async fn commit(
    provider: &GitHubProvider,
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let reference =
        gh_get_history_item_query::reference(query).ok_or_else(|| validation("ref is required"))?;
    let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
    let path = scope.as_ref();
    let needle = patch_needle(query);
    let filter = FileFilter {
        needle: needle.as_deref(),
        scope: path,
    };
    // File batches derive from saved page×pageSize state; a scope or a literal
    // hides files, so a filtered read scans from the first batch.
    let spec = WindowSpec {
        batch_size: COMMIT_FILE_BATCH,
        max_batches: MAX_COMMIT_FILE_BATCHES,
        page: gh_get_history_item_query::page(query).unwrap_or(1),
        page_size: file_page_size(query, query.include_diff()),
        filtered: !filter.is_trivial(),
        provider_total: None,
        prefetched: None,
    };
    // A movable ref is pinned before a read of several batches, so a branch
    // that moves between them cannot mix two commits.
    // A page one batch holds reads the commit by its name: the answer names
    // the commit, which pins every continuation.
    let pinned = if octocode_github::is_full_sha(reference) || !spec.spans_batches() {
        reference.to_owned()
    } else {
        provider
            .resolve_reference(
                gh_get_history_item_query::owner(query),
                gh_get_history_item_query::repo(query),
                Some(reference),
                context,
            )
            .await?
    };
    let mut loaded = load_window_with(
        source,
        &[
            "repos",
            gh_get_history_item_query::owner(query),
            gh_get_history_item_query::repo(query),
            "commits",
            &pinned,
        ],
        spec,
        |file| filter.matches(file),
        commit_file_items,
        context,
    )
    .await?;
    loaded.mark_commit_file_limit();
    let state = loaded.state;
    let raw = &loaded.first;
    let unmatched = path
        .is_some()
        .then(|| empty_scope_warning(&loaded.items, path, query, state.exhausted && !state.capped))
        .flatten();
    let listed = state.skipped + loaded.items.len();
    let commit = CommitFacts::of(raw);
    let summary = query.sections().is_empty();
    let mut out = commit_header(&commit, reference, summary, listed);
    // A filter that left no file is an empty answer, not a bare `files: []`:
    // the row says so and its leads show.
    let scope_removed_all = unmatched.is_some();
    push_warnings(&mut out, unmatched);
    let files = filter.page(loaded.items, state, query, query.include_diff());
    let page = commit_file_page(&mut out, files, query, path.is_none(), &commit);
    attach_diff_continuations(
        &mut out,
        query,
        ItemOperation::Commit,
        Some(&commit.sha),
        page.cursors,
    );
    if scope_removed_all {
        out["status"] = json!("empty");
    }
    let leads = commit_leads(query, &commit, page);
    // Which pull request merged the commit is the next fact a reader needs:
    // when the message names none, the warning says so and requires its
    // search, which a successful row otherwise leaves out of related[].
    if leads.iter().any(|(name, _)| name == FIND_PULL_REQUEST) {
        push_warnings(&mut out, [NO_PULL_REQUEST_NAMED.to_owned()]);
    }
    lead_first(&mut out, leads);
    remove_nulls(&mut out);
    Ok(ToolData::from(out))
}

/// The facts of one commit a read names it by.
struct CommitFacts<'a> {
    raw: &'a Value,
    sha: String,
    message: &'a str,
    /// The first parent: the old side of the commit's diff.
    parent: Option<String>,
}

impl<'a> CommitFacts<'a> {
    fn of(raw: &'a Value) -> Self {
        let message = str_at(raw, "/commit/message").unwrap_or("");
        Self {
            raw,
            sha: string(raw.get("sha")),
            message,
            parent: raw
                .pointer("/parents/0/sha")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }
}

/// The commit row before its files. A summary read (no `sections`) is the
/// commit whole: its message, who wrote and committed it, its parents and its
/// line counts. A section read answers with its files or patches: the row
/// names the commit by its SHA and whole message (issue and pull request
/// references live past the first line) and dates it (the author's login,
/// else name, and date). Every read keeps the upstream parent SHAs: they
/// identify the old source of a patch, including every parent of a merge.
/// The committer and line counts are the summary read's.
fn commit_header(commit: &CommitFacts<'_>, reference: &str, summary: bool, listed: usize) -> Value {
    let raw = commit.raw;
    let author = identity(raw, "author");
    let committer = committer(raw, &author);
    let parents = raw.get("parents").and_then(Value::as_array).map(|parents| {
        parents
            .iter()
            .filter_map(|parent| str_at(parent, "/sha").map(str::to_owned))
            .collect::<Vec<_>>()
    });
    let mut out = json!({
        "ref":reference,"commitSha":commit.sha,
        "message":commit.message,
        "author":if summary { person(&author, AUTHORED_AT) } else { dated(&author) },
        "committer":committer.filter(|_| summary).map(|committer| person(&committer, COMMITTED_AT)),
        "parentCommitShas":parents,
        // Top-level counts always describe the whole commit; a path scope's
        // own count is `sectionPagination.files.total`, and the file
        // page's `countScope` says when this count is a floor. The counts
        // keep one order on every read, as a pull request's do.
        "changedFileCount":listed,
        "additions":raw.pointer("/stats/additions").filter(|_| summary),
        "deletions":raw.pointer("/stats/deletions").filter(|_| summary),
    });
    // A caller-supplied SHA is not restated as the ref it already names.
    if reference.eq_ignore_ascii_case(&commit.sha)
        && let Some(fields) = out.as_object_mut()
    {
        fields.remove("ref");
    }
    out
}

/// Who a section read's header names: the login, else the git name, and
/// when it was authored. The email and the name beside a login are the
/// summary read's.
fn dated(author: &Value) -> Value {
    let who = if author.get("login").is_some() {
        "login"
    } else {
        "name"
    };
    let mut out = json!({ who: author.get(who), AUTHORED_AT: author.get(DATE) });
    remove_nulls(&mut out);
    out
}

/// The key a person's date is read under, and the ones a row states it as:
/// the author's `authoredAt`, the committer's `committedAt`.
const DATE: &str = "date";
const AUTHORED_AT: &str = "authoredAt";
const COMMITTED_AT: &str = "committedAt";

/// A person as the row states it: its date under `dated`.
fn person(identity: &Value, dated: &str) -> Value {
    let mut out = identity.clone();
    if let Some(fields) = out.as_object_mut()
        && let Some(date) = fields.remove(DATE)
    {
        fields.insert(dated.to_owned(), date);
    }
    out
}

/// One page of a commit's changed files on `out`: its window of the
/// commit's file batches, then the shared file-page shaping.
fn commit_file_page(
    out: &mut Value,
    files: FilteredPage,
    query: &HistoryItemRequest,
    unscoped: bool,
    commit: &CommitFacts<'_>,
) -> FilePage {
    // A first page that lists every changed file (no scope, no grep) needs
    // no separate count.
    let whole = unscoped
        && gh_get_history_item_query::match_strings(query).is_none()
        && files.page["countScope"] != "partial"
        && files.page["outOfRange"] != true
        && gh_get_history_item_query::page(query).unwrap_or(1) == 1
        && files.page["hasMore"] != true;
    if whole && let Some(fields) = out.as_object_mut() {
        fields.remove("changedFileCount");
    }
    shape_file_page(
        out,
        files,
        query,
        DiffSides {
            new: Some(commit.sha.as_str()),
            old: commit.parent.as_deref(),
        },
        || commit_query(query, &commit.sha),
    )
}

/// The lead that searches for the pull request of a commit whose message
/// names none.
const FIND_PULL_REQUEST: &str = "findPullRequest";

/// The note beside [`FIND_PULL_REQUEST`].
pub(in crate::gh_get_history_item) const NO_PULL_REQUEST_NAMED: &str = "This commit's message names no pull request: the related[] ghSearchHistory search by its SHA finds the one that holds it.";

/// A commit read's leads in rank order: the unfiltered read when a
/// `matchString` hit nothing, the pull request that holds the commit (the
/// route to its discussion, ahead of the diff's side reads: the one its
/// message names, else GitHub's pull-request search by its SHA), then both
/// sides of the diff; the response keeps the first ones its lead cap allows.
fn commit_leads(query: &HistoryItemRequest, commit: &CommitFacts<'_>, page: FilePage) -> Vec<Lead> {
    let mut leads = Vec::from_iter(page.without_match);
    let pull_request = match commit_pull_request_lead(commit.message, query) {
        Some(read) => Some(("readPullRequest", read)),
        None => find_pull_request(query, &commit.sha).map(|find| (FIND_PULL_REQUEST, find)),
    };
    leads.extend(pull_request.map(|(name, lead)| (name.to_owned(), lead)));
    leads.extend(page.side_reads);
    leads
}

/// GitHub's pull-request search for `sha` in the read's repository: the
/// pull request holding a commit whose message names none (ghSearchHistory
/// leads its commit rows the same way).
fn find_pull_request(query: &HistoryItemRequest, sha: &str) -> Option<Value> {
    if sha.is_empty() {
        return None;
    }
    let search = octocode_runtime_base::response::result::wire_query::<
        octocode_contract_types::GhSearchHistoryQuery,
    >(json!({
        "operation": "pullRequest",
        "owner": gh_get_history_item_query::owner(query),
        "repo": gh_get_history_item_query::repo(query),
        "keywords": [sha],
    }))?;
    Some(octocode_runtime_base::response::result::Continuation::related_query(search).build())
}

/// The committer, unless GitHub's web-flow committed for the author (a
/// merge or edit in the web UI: that identity says nothing of the change)
/// or it repeats the author whole. A committer that differs in any field
/// (date or email after a rebase, cherry-pick or web merge) is evidence and
/// stays.
fn committer(raw: &Value, author: &Value) -> Option<Value> {
    if str_at(raw, "/committer/login") == Some(WEB_FLOW_LOGIN) {
        return None;
    }
    Some(identity(raw, "committer")).filter(|committer| committer != author)
}

fn identity(raw: &Value, kind: &str) -> Value {
    let p = format!("/commit/{kind}");
    let login = format!("/{kind}/login");
    let mut out = json!({"name":str_at(raw,&format!("{p}/name")).unwrap_or("unknown"),"email":str_at(raw,&format!("{p}/email")).unwrap_or(""),"login":str_at(raw,&login),"date":str_at(raw,&format!("{p}/date")).map(|date| utc_date(Some(date)))});
    remove_nulls(&mut out);
    out
}

/// The login GitHub commits under for changes made in its web UI.
const WEB_FLOW_LOGIN: &str = "web-flow";

#[cfg(test)]
mod tests;
