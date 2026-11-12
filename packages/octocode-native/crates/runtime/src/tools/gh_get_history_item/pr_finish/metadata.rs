//! The pull-request metadata row of a summary read, and the slim header of a
//! section read.
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::HistoryItemRequest;
use crate::tools::gh_get_history_item::util::{clean_char_cut, nonzero, str_at, string};
use crate::tools::gh_shared::dates::{utc_date, utc_value};
use serde_json::{Value, json};

/// What a section read's row keeps of the header: the pull request's
/// identity and state, and the commits its files, patches and parents are
/// read at.
const SECTION_HEADER: &[&str] = &[
    "number",
    "state",
    "headCommitSha",
    "baseCommitSha",
    "mergeCommitSha",
];

/// Characters a PR summary's `bodyPreview` shows.
const BODY_PREVIEW_CHARS: usize = 300;

/// The opening of a PR body, whitespace runs collapsed; a cut preview ends
/// with `…` (`readBody` reads it whole). `None` for an empty body.
fn body_preview(body: &str) -> Option<String> {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    if flat.chars().count() <= BODY_PREVIEW_CHARS {
        return Some(flat);
    }
    // Never inside a redaction marker: it is whole or left out.
    let end = clean_char_cut(&flat, 0, BODY_PREVIEW_CHARS);
    let mut preview = flat.chars().take(end).collect::<String>();
    preview.push('…');
    Some(preview)
}

pub(in crate::tools::gh_get_history_item) fn pr_metadata(
    raw: &Value,
    query: &HistoryItemRequest,
) -> Value {
    let merged = raw.get("merged_at").is_some_and(|v| !v.is_null());
    let labels = raw
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| str_at(v, "/name").map(str::to_owned))
        })
        .collect::<Vec<_>>();
    // The body is opt-in (`sections:["body"]`, offered by the menu): the
    // summary names the change. A merged PR's close time is its merge time:
    // `closedAt` is stated beside `mergedAt` only when the two differ.
    let closed = raw.get("closed_at").filter(|closed| !closed.is_null());
    let closed_apart = closed.filter(|closed| Some(*closed) != raw.get("merged_at"));
    let mut row = json!({
        "number": raw["number"],
        "title": string(raw.get("title")),
        "state": if merged {"merged"} else {str_at(raw,"/state").unwrap_or("open")},
        "draft": raw.get("draft").and_then(Value::as_bool).filter(|v|*v),
        "author": str_at(raw,"/user/login").unwrap_or(""),
        "labels": (!labels.is_empty()).then_some(labels),
        // GraphQL reads every label page; a page it could not reach is said.
        "labelsClipped": raw.get("labels_truncated").and_then(Value::as_bool).filter(|v| *v),
        "baseBranch": str_at(raw,"/base/ref").filter(|v|!v.is_empty()),
        "headBranch": str_at(raw,"/head/ref").filter(|v|!v.is_empty()),
        "headCommitSha": str_at(raw,"/head/sha").filter(|v|!v.is_empty()),
        // The target-branch tip GitHub recorded for the PR: not the diff's
        // old side (that is the merge base, which `readParent` reads).
        "baseCommitSha": str_at(raw,"/base/sha").filter(|v|!v.is_empty()),
        "createdAt": utc_date(str_at(raw, "/created_at")),
        "updatedAt": utc_value(raw.get("updated_at").filter(|updated| updated.is_string())),
        "closedAt": utc_value(closed_apart),
        "mergedAt": utc_value(raw.get("merged_at")),
        // Only a merged PR has a real merge commit (open PRs expose GitHub's
        // test-merge SHA, which is not on the base branch).
        "mergeCommitSha": merged.then(|| str_at(raw,"/merge_commit_sha")).flatten().filter(|v|!v.is_empty()),
        "commentsCount": nonzero(raw.get("comments")),
        "commitCount": nonzero(raw.get("commits")),
        // Review threads (GraphQL only): REST counts review comments, a
        // different unit, so a REST read leaves this out.
        "reviewThreadsCount": nonzero(raw.get("review_threads")),
        "changedFileCount": nonzero(raw.get("changed_files")),
        "additions": nonzero(raw.get("additions")),
        "deletions": nonzero(raw.get("deletions")),
    });
    // A summary previews what the PR says; the menu's body read (`readFiles`,
    // `readPatches` or `readBody`) holds the whole body.
    if query.sections().is_empty()
        && let Some(preview) = str_at(raw, "/body").and_then(body_preview)
    {
        row["bodyPreview"] = json!(preview);
    }
    remove_nulls(&mut row);
    // A section read answers with its sections: the row names the pull
    // request (`number`, `state`) and the commits a next read is made at
    // (`headCommitSha`, which its pages are pinned to, `baseCommitSha` and a merged
    // one's `mergeCommitSha`), and leaves the rest of the header to the
    // summary read (no `sections`). A later page continues a row whose first
    // page said as much: it keeps what the contract requires.
    let later = crate::tools::gh_get_history_item::gh_get_history_item_query::later_page(query);
    if let Some(fields) = row.as_object_mut() {
        if later {
            fields.retain(|key, _| matches!(key.as_str(), "number" | "state" | "headCommitSha"));
        } else if !query.sections().is_empty() {
            // A file list states how many files the pull request changes:
            // a filter or a page shows fewer.
            let lists_files = query.sections().lists_files();
            fields.retain(|key, _| {
                SECTION_HEADER.contains(&key.as_str()) || (lists_files && key == "changedFileCount")
            });
        }
    }
    row
}
