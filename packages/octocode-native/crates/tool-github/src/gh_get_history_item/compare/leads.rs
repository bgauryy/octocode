//! A capped comparison's leads: the scoped paths' own history and the
//! range's commits that reach files past GitHub's cap.
use super::pages::ListedCommits;
use crate::gh_get_history_item::HistoryItemRequest;
use crate::gh_get_history_item::file_page::{Lead, first_page, literal_prefix};
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::util::str_at;
use crate::gh_get_history_item::window::{COMPARE_FILE_LIMIT, page_starting_at};
use octocode_contract_types::{GhGetHistoryItemQuery, GhSearchHistoryQuery};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::paging::push_warnings;
use octocode_runtime_base::response::result::Continuation;
use octocode_runtime_base::response::result::wire_query;
use serde_json::{Value, json};

/// The leads of a comparison whose file list stops at GitHub's compare cap
/// (the trees could not extend it): the scoped paths may look unchanged when
/// they are not. Each scope's own history lists the range's commits that
/// touched it: `ghSearchHistory` at the pinned head, from the range's start
/// (its merge base: the base commit's own date is the first it can list,
/// never an older commit). A file read also warns and leads to the range's
/// commits, whose touched files cover the net change (and more: a file
/// changed and then reverted is touched). Below the cap the file list is
/// whole: no lead.
pub(super) fn capped_file_leads(
    query: &HistoryItemRequest,
    out: &mut Value,
    raw: &Value,
    file_limit: bool,
    wants_files: bool,
    commits: Option<&ListedCommits>,
) -> Vec<Lead> {
    let mut leads = Vec::new();
    if !file_limit {
        return leads;
    }
    leads.extend(scope_histories(query, out, raw));
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

/// `narrowScope`: the history of each `include` pattern's literal prefix (a
/// glob's directory, or the exact path), one query per prefix that no other
/// prefix already covers, in `include` order, in one call. `None` without a literal prefix, or
/// when the prefixes need more queries than one call takes: the range's
/// commits ([`capped_files_reach`]) reach every file then.
fn scope_histories(query: &HistoryItemRequest, out: &Value, raw: &Value) -> Option<Lead> {
    let mut prefixes = Vec::new();
    for prefix in gh_get_history_item_query::include_patterns(query)
        .into_iter()
        .filter_map(literal_prefix)
    {
        if !prefixes.contains(&prefix) {
            prefixes.push(prefix);
        }
    }
    let covered = |path: &String| {
        prefixes
            .iter()
            .any(|dir| dir != path && dir.ends_with('/') && path.starts_with(dir.as_str()))
    };
    let paths = prefixes
        .iter()
        .filter(|path| !covered(path))
        .collect::<Vec<_>>();
    if paths.is_empty()
        || paths.len()
            > octocode_runtime_base::response::result::rows_per_call(ToolId::GhSearchHistory)
    {
        return None;
    }
    let since = str_at(raw, "/merge_base_commit/commit/committer/date")
        .or_else(|| str_at(raw, "/base_commit/commit/committer/date"));
    let rows = paths
        .into_iter()
        .map(|path| {
            let mut history = json!({"operation":"commit","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"path":path,"ref":out["headCommitSha"]});
            if let Some(date) = since {
                history["filters"]["committedAt"] = json!({"from":date});
            }
            wire_query::<GhSearchHistoryQuery>(history).and_then(|row| serde_json::to_value(row).ok())
        })
        .collect::<Option<Vec<_>>>()?;
    Some((
        "narrowScope".to_owned(),
        // The history of the scope's paths reaches the changes a capped file
        // list may leave out: the row needs it.
        Continuation::related_input(ToolId::GhSearchHistory, json!({"queries": rows}))
            .required()
            .build(),
    ))
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
) -> Option<Lead> {
    let batch = octocode_runtime_base::response::result::rows_per_call(ToolId::GhGetHistoryItem);
    let Some(listed) = commits else {
        return commit_list_lead(query, out, batch, 1);
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
            "readCommitFiles".to_owned(),
            Continuation::continued_input(ToolId::GhGetHistoryItem, json!({"queries": rows}))
                .build(),
        ));
    }
    let (page, size) = page_starting_at(listed.seen?, batch);
    commit_list_lead(query, out, size, page)
}

fn commit_list_lead(
    query: &HistoryItemRequest,
    out: &Value,
    size: usize,
    page: usize,
) -> Option<Lead> {
    let mut list = json!({"operation":"compare","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),"baseRef":out["baseCommitSha"],"headRef":out["headCommitSha"],"sections":["commits"],"pageSize":size});
    if page > 1 {
        list["page"] = json!(page);
    }
    let list = wire_query::<GhGetHistoryItemQuery>(list)?;
    Some((
        "listCommits".to_owned(),
        Continuation::continued_query(list).build(),
    ))
}
