//! A comparison's commit page and changed-file page.
use crate::response::paging::push_warnings;
use crate::tools::gh_get_history_item::axis::Axis;
use crate::tools::gh_get_history_item::file_page::{
    DiffSides, FilePage, empty_scope_warning, patch_needle, read_commit_lead, shape_file_page,
};
use crate::tools::gh_get_history_item::filter::{FileFilter, FileScope};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::patch_hop::files_only;
use crate::tools::gh_get_history_item::pr_menu::{base_public_query, remove_key};
use crate::tools::gh_get_history_item::util::{array, str_at};
use crate::tools::gh_get_history_item::window::WindowState;
use crate::tools::gh_get_history_item::{HistoryItemRequest, ItemOperation, validation};
use crate::tools::gh_shared::commit;
use octocode_github::ProviderError;
use serde_json::{Value, json};

/// The comparison's commit page a read asked for.
pub(super) struct CommitPage {
    pub(super) page: usize,
    pub(super) per: usize,
    /// GitHub's count of the range's commits (`total_commits`).
    pub(super) total: usize,
    pub(super) more: bool,
}

/// What a listed commit page leaves to the row's leads.
pub(super) struct ListedCommits {
    /// The page's commit SHAs, in order.
    pub(super) shas: Vec<String>,
    /// How many of the range's commits this page and the pages before it
    /// show, when more follow; `None` when it shows the last one.
    pub(super) seen: Option<usize>,
    /// The whole-message read of the first commit whose message has more
    /// than its headline.
    pub(super) read: Option<Value>,
}

/// The commit page: each commit by its SHA and headline (any row's
/// `commitSha` reads its whole message). A scoped comparison lists every
/// commit of the range, not only the scope's: the `narrowScope` lead lists
/// the scope's own commits.
pub(super) fn attach_commit_page(
    out: &mut Value,
    raw: &Value,
    query: &HistoryItemRequest,
    at: CommitPage,
) -> ListedCommits {
    let commits = array(raw.get("commits").cloned().unwrap_or(json!([])));
    let shas = commits
        .iter()
        .filter_map(|v| str_at(v, "/sha"))
        .map(str::to_owned)
        .collect();
    let read = commits
        .iter()
        .find(|v| {
            let message = str_at(v, "/commit/message").unwrap_or("").trim_end();
            message.lines().nth(1).is_some()
        })
        .and_then(|v| str_at(v, "/sha"))
        .map(|sha| read_commit_lead(query, sha));
    out["commits"] = json!(
        commits
            .into_iter()
            .map(|v| commit::row(&v))
            .collect::<Vec<_>>()
    );
    // Every commit page states where it stands, the last one too. GitHub
    // counts the range (`total_commits`): a count-cut page states that
    // total unless the provider's links say otherwise.
    let counted = at.total > 0 && (at.page.saturating_mul(at.per) < at.total) == at.more;
    let facts = if counted {
        crate::response::pages::PageFacts::counted(at.page, at.per, at.total)
    } else {
        crate::response::pages::PageFacts::open(at.page, Some(at.per), at.more)
    };
    // A commit page past the last is the shared out-of-range outcome (the
    // response stage states `page N of M` and the restart), unless the same
    // page still lists files. The commit page continuation
    // (`nextCommitPage`) is the cursor.
    let files_listed = out
        .get("files")
        .and_then(Value::as_array)
        .is_some_and(|files| !files.is_empty());
    let past_end = !files_listed && facts.past_end().is_some();
    out["contentPagination"][Axis::Commits.key()] = facts.out_of_range(past_end).to_value();
    ListedCommits {
        shas,
        seen: at.more.then(|| at.page.saturating_mul(at.per)),
        read,
    }
}

/// The comparison's changed-file page (paths and line stats; patches with
/// the patches section or a `matchString`): GitHub lists the files whole
/// with any commit page, so the page is a window of that list, then the
/// shared file-page shaping. `query` names the comparison by its pinned
/// commits.
pub(super) fn attach_file_page(
    out: &mut Value,
    files: Option<Value>,
    merge_base: Option<&str>,
    query: &HistoryItemRequest,
    file_limit: bool,
) -> Result<FilePage, ProviderError> {
    let all_files = array(files.unwrap_or(json!([])));
    let all_count = all_files.len();
    let scope = FileScope::from_query(query).map_err(|message| validation(&message))?;
    let first_window =
        gh_get_history_item_query::char_offset(query).is_none_or(|offset| offset == 0);
    let unmatched = scope
        .as_ref()
        .filter(|_| first_window)
        .and_then(|scope| empty_scope_warning(&all_files, Some(scope), query, !file_limit));
    let include_diff = query.include_diff();
    let needle = patch_needle(query);
    let mut files = FileFilter {
        needle: needle.as_deref(),
        scope: scope.as_ref(),
    }
    .page(all_files, WindowState::COMPLETE, query, include_diff);
    // The whole comparison's count, when the listed files are not all of
    // it (a scope or a later page). At GitHub's cap the count is only a
    // floor: the file page's `countScope` says so and no count is claimed.
    if !include_diff && !file_limit && files.files.len() < all_count {
        out["changedFilesCount"] = json!(all_count);
    }
    // The page's own count fact: at the cap its total is a floor.
    if file_limit {
        files.page["countScope"] = json!("partial");
    }
    let head = out["headSha"].as_str().map(str::to_owned);
    let page = shape_file_page(
        out,
        files,
        query,
        DiffSides {
            new: head.as_deref(),
            // The old side of a three-dot comparison is the merge base.
            old: merge_base,
        },
        || {
            let mut same = base_public_query(query, ItemOperation::Compare);
            remove_key(&mut same, "offset");
            files_only(&mut same, ItemOperation::Compare);
            same
        },
    );
    if let Some(warning) = unmatched {
        push_warnings(out, [warning]);
        // A scope that left none of a complete file list is an empty answer,
        // as it is on a commit; a capped list may hide the scope's files.
        if !file_limit {
            out["status"] = json!("empty");
        }
    }
    Ok(page)
}
