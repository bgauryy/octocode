//! Continuations: the read of a likely row, the next page, and the
//! recovery from an empty search.
use super::rows::{is_completed, is_merged, item_number, item_repo, read_target};
use super::{GhSearchHistoryQuery, HistoryOperation};
use crate::tools::id::ToolId;
use crate::tools::result::{Continuation, remove_null_fields};
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

/// The PR read: a merged row is the likely fix (the rows name every other
/// readable number; the read takes any).
pub(super) fn read_pull_request(q: &GhSearchHistoryQuery, items: &[Value]) -> Option<Value> {
    let target = read_target(items, is_merged)?;
    let (owner, repo) = q
        .owner()
        .zip(q.repo())
        .map(|(owner, repo)| (owner.to_owned(), repo.to_owned()))
        .or_else(|| item_repo(&items[target]))?;
    let number = item_number(&items[target])?;
    let merged = items.iter().any(is_merged);
    Some(
        Continuation::new(
            ToolId::GhGetHistoryItem,
            pr_read_query(&owner, &repo, number),
        )
        .confidence(if merged { "medium" } else { "low" })
        .build(),
    )
}

/// The issue read: a completed issue first.
pub(super) fn read_issue(q: &GhSearchHistoryQuery, items: &[Value]) -> Option<Value> {
    let target = read_target(items, is_completed)?;
    let (owner, repo) = q.owner().zip(q.repo())?;
    let number = item_number(&items[target])?;
    Some(
        Continuation::new(
            ToolId::GhGetHistoryItem,
            issue_read_query(owner, repo, number),
        )
        .confidence("low")
        .build(),
    )
}

/// The first commit's patches, scoped to the listed path in the read's
/// published `include`; the read takes any row's sha as `ref`.
pub(super) fn read_commit(q: &GhSearchHistoryQuery, sha: &str) -> Option<Value> {
    let (owner, repo) = q.owner().zip(q.repo())?;
    let mut read = json!({
        "operation":"commit","owner":owner,"repo":repo,"ref":sha,"sections":["patches"]
    });
    if let Some(path) = q.path() {
        read["include"] = json!([path]);
    }
    Some(
        Continuation::new(ToolId::GhGetHistoryItem, read)
            .confidence("low")
            .build(),
    )
}

/// A PR search whose keywords are one bare issue number (`13786`, `#13786`)
/// is an issue → fix-PR hop: the issue read lists the PRs that closed it.
pub(super) fn issue_links_read(q: &GhSearchHistoryQuery) -> Option<Value> {
    let (owner, repo) = q.owner().zip(q.repo())?;
    let [keyword] = q.keywords()[..] else {
        return None;
    };
    let number = keyword
        .trim()
        .trim_start_matches('#')
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)?;
    Some(
        Continuation::new(
            ToolId::GhGetHistoryItem,
            json!({"operation":"issue","owner":owner,"repo":repo,"number":number}),
        )
        .confidence("high")
        .build(),
    )
}

/// The same query as a wire row without `page`, the base of every
/// same-tool continuation.
fn same_query(q: &GhSearchHistoryQuery) -> Value {
    let mut row = serde_json::to_value(q).unwrap_or_default();
    remove_null_fields(&mut row);
    if let Some(row) = row.as_object_mut() {
        row.remove("page");
    }
    row
}

/// The next result page, at the page size this one used.
pub(super) fn next_page(q: &GhSearchHistoryQuery, page: usize, page_size: usize) -> Value {
    let mut next = same_query(q);
    next["page"] = json!(page + 1);
    next["pageSize"] = json!(page_size);
    Continuation::new(ToolId::GhSearchHistory, next)
        .confidence("exact")
        .build()
}

/// An empty search, broadened: drop the keywords of a repository-scoped
/// search, else the `qualifiers`, else a commit listing's date window.
pub(super) fn broaden_search(q: &GhSearchHistoryQuery) -> Option<Value> {
    let mut row = same_query(q);
    let object = row.as_object_mut()?;
    let scoped = q.owner().is_some() && q.repo().is_some();
    let dropped = if scoped && !q.keywords().is_empty() {
        vec!["keywords"]
    } else if q.qualifiers().is_some() {
        vec!["qualifiers"]
    } else if q.operation() == HistoryOperation::Commit
        && (q.since().is_some() || q.until().is_some())
    {
        vec!["since", "until"]
    } else {
        return None;
    };
    for key in dropped {
        object.remove(key);
    }
    Some(
        Continuation::new(ToolId::GhSearchHistory, row)
            .confidence("low")
            .build(),
    )
}
