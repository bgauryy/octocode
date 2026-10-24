//! A comparison's scope and capped-file leads: the scoped path's own
//! history and the range's commits that reach files past GitHub's cap.
use super::pages::ListedCommits;
use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Continuation;
use crate::tools::gh_get_history_item::HistoryItemRequest;
use crate::tools::gh_get_history_item::file_page::{Lead, first_page, literal_prefix};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::util::str_at;
use crate::tools::gh_get_history_item::window::{COMPARE_FILE_LIMIT, page_starting_at};
use serde_json::{Value, json};

/// A scoped path's own history lists the commits of the range that touched
/// it: `ghSearchHistory` at the pinned head, from the range's start (its
/// merge base: the base commit's own date is the first it can list, never an
/// older commit). When GitHub's compare file cap stands (the trees could
/// not extend it) the scoped path may look unchanged when it is not; a file
/// read warns and leads to the range's commits, whose touched files cover
/// the net change (and more: a file changed and then reverted is touched).
pub(super) fn capped_file_leads(
    query: &HistoryItemRequest,
    out: &mut Value,
    raw: &Value,
    file_limit: bool,
    wants_files: bool,
    commits: Option<&ListedCommits>,
) -> Vec<Lead> {
    let mut leads = Vec::new();
    // The scope's literal prefix: a glob's directory, or the exact path.
    if let Some(path) = gh_get_history_item_query::include_patterns(query)
        .into_iter()
        .find_map(literal_prefix)
    {
        let mut history = json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"pathPrefix":path,"ref":out["headSha"]});
        if let Some(date) = str_at(raw, "/merge_base_commit/commit/committer/date")
            .or_else(|| str_at(raw, "/base_commit/commit/committer/date"))
        {
            history["filters"]["committedAt"] = json!({"from":date});
        }
        leads.push((
            "narrowScope",
            Continuation::related(ToolId::GhSearchHistory, history).build(),
        ));
    }
    if !file_limit {
        return leads;
    }
    if wants_files && first_page(query) {
        // The cap is stated once, on the comparison's first page, with what
        // it may hide.
        let include = gh_get_history_item_query::include_patterns(query);
        let hidden = if include.is_empty() {
            "other changed files".to_owned()
        } else {
            format!("changes to {}", include.join(", "))
        };
        push_warnings(
            out,
            [format!(
                "GitHub lists at most {COMPARE_FILE_LIMIT} changed files, so {hidden} may be missing; the range's commits reach them, as files each commit touched (a superset of the net change)."
            )],
        );
    }
    if wants_files {
        leads.extend(capped_files_reach(query, out, commits));
    }
    leads
}

/// The lead that reaches the changed files past GitHub's compare cap through
/// the range's commits: this page's commits as one batch of commit reads when
/// the page lists no more commits than a batch takes, else the commits the
/// page does not show, in batch-sized pages (each carries that batch read;
/// [`page_starting_at`] aligns them). A wider page that ends the range has
/// no unseen commit to list: its rows' `commitSha` are the reads.
fn capped_files_reach(
    query: &HistoryItemRequest,
    out: &Value,
    commits: Option<&ListedCommits>,
) -> Option<(&'static str, Value)> {
    let batch = crate::response::result::rows_per_call(ToolId::GhGetHistoryItem);
    let Some(listed) = commits else {
        return Some(commit_list_lead(query, out, batch, 1));
    };
    if !listed.shas.is_empty() && listed.shas.len() <= batch {
        let rows = listed
            .shas
            .iter()
            .map(|sha| {
                json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"ref":sha})
            })
            .collect::<Vec<_>>();
        return Some((
            "readCommitFiles",
            Continuation::continued_input(ToolId::GhGetHistoryItem, json!({"queries": rows}))
                .build(),
        ));
    }
    let (page, size) = page_starting_at(listed.seen?, batch);
    Some(commit_list_lead(query, out, size, page))
}

fn commit_list_lead(
    query: &HistoryItemRequest,
    out: &Value,
    size: usize,
    page: usize,
) -> (&'static str, Value) {
    let mut list = json!({"operation":"compare","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"baseRef":out["baseSha"],"headRef":out["headSha"],"sections":["commits"],"pageSize":size});
    if page > 1 {
        list["page"] = json!(page);
    }
    (
        "listCommits",
        Continuation::continued(ToolId::GhGetHistoryItem, list).build(),
    )
}
