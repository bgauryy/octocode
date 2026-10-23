//! `operation: "commit"`: one commit's header and its changed-file page.
use super::file_page::{
    DiffSides, FilePage, Lead, empty_scope_warning, file_page_note, lead_first, patch_needle,
    read_commit_lead, shape_file_page,
};
use super::filter::{FileFilter, FileScope, FilteredPage};
use super::inventory::file_page_size;
use super::patch_hop::{attach_diff_continuations, commit_pull_request_lead, commit_query};
use super::util::{str_at, string};
use super::window::{
    COMMIT_FILE_BATCH, MAX_COMMIT_FILE_BATCHES, WindowSpec, WindowState, commit_file_items,
    load_window_with,
};
use super::{HistoryItemRequest, ItemOperation, validation};
use crate::response::paging::push_warnings;
use crate::response::result::{ToolData, remove_nulls};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::dates::utc_date;
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use serde_json::{Value, json};

pub(super) async fn commit(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let transport = &provider.transport;
    let reference =
        gh_get_history_item_query::reference(query).ok_or_else(|| validation("ref is required"))?;
    let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
    let path = scope.as_ref();
    let needle = patch_needle(query);
    let filter = FileFilter {
        needle: needle.as_deref(),
        scope: path,
    };
    // File batches are derived from page×pageSize; a scope or a literal
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
    // that moves between them cannot mix two commits; the pin is the
    // memoized single-flight resolution every GitHub read of the ref shares.
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
                false,
                context,
            )
            .await?
    };
    let mut loaded = load_window_with(
        transport,
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
    // The commit API ends at its file boundary without an authoritative
    // file total or a next link. At that boundary the observed count is a
    // lower bound, including for an include-filtered read.
    if loaded.state.exhausted
        && loaded.state.skipped.saturating_add(loaded.items.len())
            >= MAX_COMMIT_FILE_BATCHES * COMMIT_FILE_BATCH
    {
        loaded.state.capped = true;
    }
    let state = loaded.state;
    let raw = &loaded.first;
    let unmatched = path
        .is_some()
        .then(|| empty_scope_warning(&loaded.items, path, query, state.exhausted && !state.capped))
        .flatten();
    let listed = state.skipped + loaded.items.len();
    let commit = CommitFacts::of(raw);
    let mut out = commit_header(&commit, reference, query.include_diff(), listed, state);
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
    let remaining = file_page_note(&out);
    if scope_removed_all {
        out["status"] = json!("empty");
    }
    let leads = commit_leads(query, &commit, page);
    lead_first(&mut out, leads);
    remove_nulls(&mut out);
    Ok(ToolData {
        remaining,
        ..ToolData::from(out)
    })
}

/// The facts of one commit a read names it by.
struct CommitFacts<'a> {
    raw: &'a Value,
    sha: String,
    message: &'a str,
    headline: &'a str,
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
            headline: message.lines().next().unwrap_or(message),
            parent: raw
                .pointer("/parents/0/sha")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }
}

/// The commit row before its files. A patch read is the diff's evidence: it
/// names the commit by its headline once; the summary read
/// (`hints.readCommit`) holds the whole message, whose first line the
/// headline would repeat.
fn commit_header(
    commit: &CommitFacts<'_>,
    reference: &str,
    patches: bool,
    listed: usize,
    state: WindowState,
) -> Value {
    let raw = commit.raw;
    let author = identity(raw, "author");
    let committer = committer(raw, &author);
    let mut out = json!({
        "ref":reference,"commitSha":commit.sha,
        "message":(!patches).then_some(commit.message),
        "messageHeadline":patches.then_some(commit.headline),
        "author":author,"committer":committer,
        "parents":raw.get("parents").and_then(Value::as_array).into_iter().flatten().filter_map(|v|str_at(v,"/sha").map(str::to_owned)).collect::<Vec<_>>(),
        // Top-level counts always describe the whole commit; a path scope's
        // own count is `contentPagination.files.totalItems`.
        "additions":raw.pointer("/stats/additions"),"deletions":raw.pointer("/stats/deletions"),
        "changedFilesCount":listed,
        // A scan stopped at the batch cap never saw the remaining files; a
        // complete count states no scope.
        "changedFilesCountScope":if state.capped {Some("partial")} else if state.exhausted {None} else {Some("loaded")}
    });
    // A caller-supplied SHA is not restated as the ref it already names.
    if reference.eq_ignore_ascii_case(&commit.sha)
        && let Some(fields) = out.as_object_mut()
    {
        fields.remove("ref");
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
        && gh_get_history_item_query::match_string(query).is_none()
        && files.page["countScope"] != "partial"
        && files.page["outOfRange"] != true
        && gh_get_history_item_query::page(query).unwrap_or(1) == 1
        && files.page["hasMore"] != true;
    if whole && let Some(fields) = out.as_object_mut() {
        fields.remove("changedFilesCount");
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

/// A commit read's leads in rank order: the unfiltered read when a
/// `matchString` hit nothing, the rest of the message, the pull request the
/// commit names, then both sides of the diff; the response keeps the first
/// ones its lead cap allows.
fn commit_leads(query: &HistoryItemRequest, commit: &CommitFacts<'_>, page: FilePage) -> Vec<Lead> {
    let mut leads = Vec::from_iter(page.without_match);
    let pull_request = commit_pull_request_lead(commit.message, &commit.sha, query);
    if query.include_diff() && commit.message.trim_end() != commit.headline {
        leads.push(("readCommit", read_commit_lead(query, &commit.sha)));
    }
    // A pull request the commit names is the route to its discussion, ahead
    // of the diff's side reads; a search by SHA is only a guess, so it
    // yields to them.
    let named = pull_request.0 == "readPullRequest";
    if named {
        leads.push(pull_request.clone());
    }
    leads.extend(page.side_reads);
    if !named {
        leads.push(pull_request);
    }
    leads
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
mod tests {
    use super::*;

    /// A committer that repeats the author whole is not restated; one that
    /// differs in any field (a rebased date, another email) is evidence.
    #[test]
    fn a_committer_is_shown_only_when_it_differs_from_the_author() {
        let raw = |committer: Value| {
            json!({"commit":{
                "author":{"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"},
                "committer":committer}})
        };
        let same = raw(json!({"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"}));
        assert_eq!(committer(&same, &identity(&same, "author")), None);
        let other_email =
            raw(json!({"name":"Ada","email":"ada@y.dev","date":"2024-01-01T00:00:00Z"}));
        let kept = committer(&other_email, &identity(&other_email, "author")).expect("kept");
        assert_eq!(kept["email"], "ada@y.dev");
        let rebased = raw(json!({"name":"Ada","email":"ada@x.dev","date":"2024-02-01T00:00:00Z"}));
        let kept = committer(&rebased, &identity(&rebased, "author")).expect("kept");
        assert_eq!(kept["date"], "2024-02-01T00:00:00Z");
    }
}
