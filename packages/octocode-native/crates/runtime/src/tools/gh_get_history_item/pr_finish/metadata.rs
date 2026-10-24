//! The pull-request metadata row, and the slim header of a section read.
use crate::response::result::remove_nulls;
use crate::tools::gh_get_history_item::HistoryItemRequest;
use crate::tools::gh_get_history_item::util::{nonzero, str_at, string};
use crate::tools::gh_shared::dates::{utc_date, utc_value};
use serde_json::{Value, json};

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
    let mut preview = flat.chars().take(BODY_PREVIEW_CHARS).collect::<String>();
    preview.push('…');
    Some(preview)
}

pub(in crate::tools::gh_get_history_item) fn pr_metadata(
    raw: &Value,
    query: &HistoryItemRequest,
) -> Value {
    let merged = raw.get("merged_at").is_some_and(|v| !v.is_null());
    let open = !merged && str_at(raw, "/state").unwrap_or("open") == "open";
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
    // The body is opt-in (`include:["body"]`, offered by the menu): the
    // summary names the change; a merged PR's close time is its merge time,
    // and only an open PR's last update says whether it is still moving.
    let mut row = json!({
        "number": raw["number"],
        "title": string(raw.get("title")),
        "state": if merged {"merged"} else {str_at(raw,"/state").unwrap_or("open")},
        "draft": raw.get("draft").and_then(Value::as_bool).filter(|v|*v),
        "author": str_at(raw,"/user/login").unwrap_or(""),
        "labels": (!labels.is_empty()).then_some(labels),
        // GraphQL reads the first 20 labels; say when more exist.
        "labelsTruncated": raw.get("labels_truncated").and_then(Value::as_bool).filter(|v| *v),
        "baseBranch": str_at(raw,"/base/ref").filter(|v|!v.is_empty()),
        "headBranch": str_at(raw,"/head/ref").filter(|v|!v.is_empty()),
        "headSha": str_at(raw,"/head/sha").filter(|v|!v.is_empty()),
        // The target-branch tip GitHub recorded for the PR: not the diff's
        // old side (that is the merge base, which `readParent` reads).
        "baseSha": str_at(raw,"/base/sha").filter(|v|!v.is_empty()),
        "createdAt": utc_date(str_at(raw, "/created_at")),
        "updatedAt": open.then(|| utc_date(str_at(raw, "/updated_at"))).filter(|v| !v.is_empty()),
        "closedAt": (!merged).then(|| utc_value(raw.get("closed_at"))),
        "mergedAt": utc_value(raw.get("merged_at")),
        // Only a merged PR has a real merge commit (open PRs expose GitHub's
        // test-merge SHA, which is not on the base branch).
        "mergeCommitSha": merged.then(|| str_at(raw,"/merge_commit_sha")).flatten().filter(|v|!v.is_empty()),
        "commentsCount": nonzero(raw.get("comments")),
        "commitsCount": nonzero(raw.get("commits")),
        // Review threads (GraphQL only): REST counts review comments, a
        // different unit, so a REST read leaves this out.
        "reviewThreadsCount": nonzero(raw.get("review_threads")),
        "changedFilesCount": nonzero(raw.get("changed_files")),
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
    {
        if let Some(updated) = raw.get("updated_at").filter(|v| v.is_string()) {
            row["updatedAt"] = utc_value(Some(updated));
        }
        if let Some(closed) = raw.get("closed_at").filter(|v| !v.is_null()) {
            row["closedAt"] = utc_value(Some(closed));
        }
    }
    remove_nulls(&mut row);
    row
}
