//! Continuations: the read of a likely row, the next page, and the
//! recovery from an empty search.
use super::rows::{is_completed, is_merged, item_number, item_repo, read_target};
use super::{GhSearchHistoryQuery, HistorySearch};
use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};
use crate::tools::gh_search_history::gh_search_history_query;
use crate::tools::leads::broaden::{Broaden, broaden};
use serde_json::{Value, json};

/// The default PR read: body and the patch-free file inventory.
pub(super) fn pr_read_query(owner: &str, repo: &str, number: u64) -> Value {
    json!({"operation":"pullRequest","owner":owner,"repo":repo,"number":number,
        "sections":["body","files"]})
}

/// The issue body and discussion.
pub(super) fn issue_read_query(owner: &str, repo: &str, number: u64) -> Value {
    json!({"operation":"issue","owner":owner,"repo":repo,"number":number,
        "sections":["body","comments"]})
}

/// Rows in GitHub's relevance order: a keyword search without an explicit
/// sort. Any other order (a sort, a REST listing) says nothing about fit.
fn best_match_order(q: &GhSearchHistoryQuery) -> bool {
    !gh_search_history_query::keywords(q).is_empty()
        && gh_search_history_query::sort(q).is_none_or(|sort| sort == "best-match")
}

/// The PR read: the best match (row 0) of a relevance-ordered search, else
/// a merged row as the likely fix (the rows name every other readable
/// number; the read takes any).
pub(super) fn read_pull_request(q: &GhSearchHistoryQuery, items: &[Value]) -> Option<Value> {
    let prefer: fn(&Value) -> bool = if best_match_order(q) {
        |_| true
    } else {
        is_merged
    };
    let target = read_target(items, prefer)?;
    let (owner, repo) = gh_search_history_query::owner(q)
        .zip(gh_search_history_query::repo(q))
        .map(|(owner, repo)| (owner.to_owned(), repo.to_owned()))
        .or_else(|| item_repo(&items[target]))?;
    let number = item_number(&items[target])?;
    let read = Continuation::related(
        ToolId::GhGetHistoryItem,
        pr_read_query(&owner, &repo, number),
    );
    Some(read.build())
}

/// The PR a commit row's `(#N)` headline names: the first such row's read,
/// scoped to the listed path (any other row's `(#N)` runs the same read).
pub(super) fn read_commit_pull_request(q: &GhSearchHistoryQuery, rows: &[Value]) -> Option<Value> {
    let (owner, repo) = gh_search_history_query::owner(q).zip(gh_search_history_query::repo(q))?;
    let number = rows.iter().find_map(|row| {
        row.get("messageHeadline")
            .and_then(Value::as_str)
            .and_then(crate::tools::gh_shared::commit::headline_pull_request)
    })?;
    let mut read = pr_read_query(owner, repo, number);
    if let Some(path) = gh_search_history_query::path(q) {
        read["include"] = json!([path]);
    }
    Some(Continuation::related(ToolId::GhGetHistoryItem, read).build())
}

/// The PR holding a commit no headline names: issue search matches PRs
/// by commit SHA (the first row's; any other row's SHA runs the same search).
pub(super) fn find_commit_pull_request(q: &GhSearchHistoryQuery, sha: &str) -> Option<Value> {
    let (owner, repo) = gh_search_history_query::owner(q).zip(gh_search_history_query::repo(q))?;
    Some(
        Continuation::related(
            ToolId::GhSearchHistory,
            json!({"operation":"pullRequest","owner":owner,"repo":repo,"keywords":[sha]}),
        )
        .build(),
    )
}

/// The issue read: a completed issue first.
pub(super) fn read_issue(q: &GhSearchHistoryQuery, items: &[Value]) -> Option<Value> {
    let target = read_target(items, is_completed)?;
    let (owner, repo) = gh_search_history_query::owner(q).zip(gh_search_history_query::repo(q))?;
    let number = item_number(&items[target])?;
    Some(
        Continuation::related(
            ToolId::GhGetHistoryItem,
            issue_read_query(owner, repo, number),
        )
        .build(),
    )
}

/// The first commit's patches, scoped to the listed path in the read's
/// published `include`; the read takes any row's sha as `ref`.
pub(super) fn read_commit(q: &GhSearchHistoryQuery, sha: &str) -> Option<Value> {
    let (owner, repo) = gh_search_history_query::owner(q).zip(gh_search_history_query::repo(q))?;
    let mut read = json!({
        "operation":"commit","owner":owner,"repo":repo,"ref":sha,"sections":["patches"]
    });
    if let Some(path) = gh_search_history_query::path(q) {
        read["include"] = json!([path]);
    }
    Some(Continuation::related(ToolId::GhGetHistoryItem, read).build())
}

/// A PR search whose keywords are one bare issue number (`13786`, `#13786`)
/// is an issue → fix-PR hop: the issue read lists the PRs that closed it.
pub(super) fn issue_links_read(q: &GhSearchHistoryQuery) -> Option<Value> {
    let (owner, repo) = gh_search_history_query::owner(q).zip(gh_search_history_query::repo(q))?;
    let [keyword] = gh_search_history_query::keywords(q)[..] else {
        return None;
    };
    let number = keyword
        .trim()
        .trim_start_matches('#')
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)?;
    Some(
        Continuation::related(
            ToolId::GhGetHistoryItem,
            json!({"operation":"issue","owner":owner,"repo":repo,"number":number}),
        )
        .build(),
    )
}

/// The same query as a wire row without `page`, the base of every
/// same-tool continuation.
fn same_query(q: &GhSearchHistoryQuery) -> Value {
    query_row(q, &["page"])
}

/// The search again without its person qualifiers: the way on when GitHub
/// cannot find a login they name (`None` when the query names none, or
/// when nothing would scope the search without them: no owner, repository,
/// keyword or other qualifier).
pub(super) fn without_users(q: &GhSearchHistoryQuery) -> Option<Value> {
    let mut row = same_query(q);
    let filters = row.get_mut("filters")?.as_object_mut()?;
    let mut removed = false;
    for key in [
        "author",
        "committer",
        "assignee",
        "mentions",
        "commenter",
        "reviewedBy",
        "reviewRequested",
    ] {
        removed |= filters.remove(key).is_some();
    }
    if !removed {
        return None;
    }
    if filters.is_empty() {
        row.as_object_mut()?.remove("filters");
    }
    Some(Continuation::related(ToolId::GhSearchHistory, row).build())
}

/// Result page `page` of the same query.
pub(super) fn next_page(q: &GhSearchHistoryQuery, page: usize) -> Value {
    let mut next = same_query(q);
    next["page"] = json!(page);
    Continuation::continued(ToolId::GhSearchHistory, next).build()
}

/// An empty search, broadened ([`broaden`]): drop `filters` and `pathPrefix`
/// as separate groups, then drop keywords one at a time. A
/// variant keeps at least one keyword: without any it lists the scope, not
/// the topic searched.
pub(super) fn broaden_search(q: &GhSearchHistoryQuery) -> Option<Value> {
    let mut lead = broaden(
        ToolId::GhSearchHistory,
        &query_row(q, &[]),
        &Broaden {
            strip: &[],
            groups: &[&["filters"], &["pathPrefix"]],
            split_lone_keyword: false,
        },
        crate::response::result::rows_per_call(ToolId::GhSearchHistory),
    )?;
    if !gh_search_history_query::keywords(q).is_empty() {
        let rows = lead.pointer_mut("/input/queries")?.as_array_mut()?;
        rows.retain(|row| row.get("keywords").is_some());
        if rows.is_empty() {
            return None;
        }
    }
    Some(lead)
}

/// A capped search's narrowing (`narrowScope`), one call. Rows in a date
/// order continue past GitHub's 1,000-result window from this page's last
/// row: creation order narrows `filters.createdAt`, and update order narrows
/// `filters.updatedAt`. Newest first sets an upper bound; oldest first sets
/// a lower bound. A row at that instant may show again; none is skipped. An
/// order with no date to resume from (best match, comments, reactions), or
/// an update order whose existing filter owns the window, is re-sorted
/// newest first, whose pages end in that continuation. `None` when a
/// `filters.createdAt` owns a creation-ordered window or no row is dated.
pub(super) fn narrow_capped(q: &HistorySearch, items: &[Value]) -> Option<Value> {
    let (sort, order) = super::query::search_order(q);
    let newest_first = order.as_deref() != Some("asc");
    let last = |pointers: &[&str]| {
        items.iter().rev().find_map(|item| {
            pointers
                .iter()
                .find_map(|pointer| item.pointer(pointer))
                .and_then(Value::as_str)
                .map(|date| crate::tools::gh_shared::dates::utc_date(Some(date)))
        })
    };
    let mut row = same_query(q);
    match sort.as_deref() {
        Some("committer-date") => {
            let date = last(&["/commit/committer/date", "/commit/author/date"])?;
            row["filters"]["committedAt"]["to"] = json!(date);
        }
        Some("created" | "updated") => {
            let (field, pointer) = if sort.as_deref() == Some("created") {
                ("createdAt", "/created_at")
            } else {
                ("updatedAt", "/updated_at")
            };
            let date = last(&[pointer])?;
            let bound = if newest_first { "to" } else { "from" };
            let selection = &mut row["filters"][field];
            let mut range = selection
                .pointer("/include/0")
                .cloned()
                .unwrap_or_else(|| json!({}));
            range[bound] = json!(date);
            selection["include"] = json!([range]);
        }
        _ => {
            row["sort"] = json!({"by":"createdAt","order":"desc"});
        }
    }
    crate::contracts::validate(
        ToolId::GhSearchHistory.as_str(),
        json!({ "queries": [row] }),
    )
    .ok()?;
    Some(Continuation::related(ToolId::GhSearchHistory, row).build())
}
