//! `next.*` builders: exact follow-up `ghGetHistoryItem` queries derived from
//! the public query and the page objects of a shaped response.
use super::util::{content_flag, merge};
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation};
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// The caller's query as a replayable public query for `operation`.
pub(super) fn base_public_query(q: &HistoryItemRequest, operation: ItemOperation) -> Value {
    let mut v = serde_json::to_value(&q.query).unwrap_or_default();
    remove_nulls(&mut v);
    if let Some(m) = v.as_object_mut() {
        m.insert(
            "operation".into(),
            json!(match operation {
                ItemOperation::PullRequest => "pullRequest",
                ItemOperation::Issue => "issue",
                ItemOperation::Commit => "commit",
                ItemOperation::Compare => "compare",
            }),
        );
        m.remove("goal");
        m.remove("reasoning");
        m.remove("debug");
        match operation {
            ItemOperation::PullRequest => {
                m.insert(
                    "pageSize".into(),
                    json!(q.page_size().unwrap_or(DEFAULT_PAGE_SIZE)),
                );
                m.insert(
                    "minify".into(),
                    json!(
                        q.minify()
                            .map_or_else(|| "standard".to_owned(), |m| m.to_string())
                    ),
                );
                if let Some(Value::Object(content)) = m.get_mut("content")
                    && content.get("patches").is_some()
                {
                    content.insert("changedFiles".into(), json!(true));
                }
            }
            ItemOperation::Compare => {
                m.insert("page".into(), json!(q.page().unwrap_or(1)));
                m.insert("filePage".into(), json!(q.file_page().unwrap_or(1)));
                m.insert(
                    "pageSize".into(),
                    json!(q.page_size().unwrap_or(DEFAULT_PAGE_SIZE)),
                );
            }
            _ => {}
        }
    }
    v
}

fn continuation(q: Value) -> Value {
    json!({"tool":"ghGetHistoryItem","query":q,"confidence":"exact"})
}

/// Per-row menu of first-page fetches for content the call did not request.
pub(super) fn pr_next_menu(
    query: &HistoryItemRequest,
    content: Option<&Map<String, Value>>,
    patch_mode: &str,
    first_path: Option<&str>,
) -> Value {
    // Start from the base public query so contract-required fields (pageSize,
    // minify) are present, then drop the current content selection and every
    // per-surface cursor: each menu entry is a fresh first-page fetch.
    let mut target = base_public_query(query, ItemOperation::PullRequest);
    if let Some(object) = target.as_object_mut() {
        for key in [
            "content",
            "charOffset",
            "charLength",
            "commentBodyOffset",
            "commentPage",
            "commitPage",
            "reviewPage",
            "filePage",
            "page",
            "matchString",
        ] {
            object.remove(key);
        }
    }
    let mut next = Map::new();
    let call = |content: Value| continuation(merge(target.clone(), json!({"content":content})));
    if !content_flag(content, "body") {
        next.insert("getBody".into(), call(json!({"body":true})));
    }
    if !content_flag(content, "changedFiles") && patch_mode == "none" {
        next.insert("getChangedFiles".into(), call(json!({"changedFiles":true})));
    }
    if patch_mode == "none" {
        if let Some(path) = first_path {
            next.insert(
                "getSelectedPatches".into(),
                call(json!({"patches":{"mode":"selected","files":[path]}})),
            );
        }
        next.insert(
            "getAllPatches".into(),
            call(json!({"patches":{"mode":"all"}})),
        );
    }
    if content.and_then(|v| v.get("comments")).is_none() {
        next.insert(
            "getComments".into(),
            call(json!({"comments":{"discussion":true,"reviewInline":true}})),
        );
    }
    if !content_flag(content, "reviews") {
        next.insert("getReviews".into(), call(json!({"reviews":true})));
    }
    if content.and_then(|v| v.get("commits")).is_none() {
        next.insert("getCommits".into(), call(json!({"commits":{}})));
    }
    Value::Object(next)
}

/// A pull-request `contentPagination` axis: its `next.*` name, the page field
/// holding the cursor, and the query key the cursor continues.
fn pr_axis(axis: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match axis {
        "body" => ("continueBody", "nextCharOffset", "charOffset"),
        "reviewBody" => ("continueReviewBody", "nextCharOffset", "charOffset"),
        "patches" => ("continuePatch", "nextCharOffset", "charOffset"),
        "commentBody" => ("continueCommentBody", "nextCharOffset", "commentBodyOffset"),
        "changedFiles" => ("nextChangedFilesPage", "nextPage", "filePage"),
        "comments" => ("nextCommentsPage", "nextPage", "commentPage"),
        "reviews" => ("nextReviewsPage", "nextPage", "reviewPage"),
        "commits" => ("nextCommitsPage", "nextPage", "commitPage"),
        _ => return None,
    })
}

pub(super) fn promote_pr_continuations(out: &mut Value, q: &HistoryItemRequest) {
    let unresolved_selected_paths = selected_patch_paths(q).map(|requested| {
        let returned = out
            .pointer("/pullRequests/0/changedFiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|file| file.get("path").and_then(Value::as_str))
            .collect::<HashSet<_>>();
        requested
            .into_iter()
            .filter(|path| !returned.contains(path.as_str()))
            .collect::<Vec<_>>()
    });
    let Some(pages) = out
        .pointer_mut("/pullRequests/0/contentPagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let mut next = Map::new();
    let mut partial = false;
    for (axis, entry) in pages.iter_mut() {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        if axis == "changedFiles"
            && unresolved_selected_paths
                .as_ref()
                .is_some_and(Vec::is_empty)
        {
            if let Some(entry) = entry.as_object_mut() {
                entry.insert("hasMore".into(), json!(false));
                entry.remove("nextPage");
            }
            continue;
        }
        partial = true;
        let Some((name, field, key)) = pr_axis(axis) else {
            continue;
        };
        let Some(cursor) = entry.get(field).cloned() else {
            continue;
        };
        let mut nq = base_public_query(q, ItemOperation::PullRequest);
        nq[key] = cursor;
        if axis == "changedFiles"
            && let Some(unresolved) = unresolved_selected_paths.as_deref()
        {
            retain_unresolved_patch_selection(&mut nq, unresolved);
            nq["filePage"] = json!(1);
        }
        // charOffset is a single field shared by the body, review-body and
        // patch windows. A char-window continuation therefore narrows content
        // to the surface it continues, and page continuations restart every
        // char window, so one surface's offset never skews another's.
        match axis.as_str() {
            "body" => nq["content"] = json!({"body":true}),
            "reviewBody" => nq["content"] = json!({"reviews":true}),
            "patches" => narrow_patch_continuation(&mut nq, entry, q),
            _ => {
                remove_key(&mut nq, "charOffset");
                // A collection page continues only its own surface; the other
                // surfaces were already delivered on this call.
                let keep: &[&str] = match axis.as_str() {
                    "changedFiles" => &["changedFiles", "patches"],
                    "comments" => &["comments"],
                    "reviews" => &["reviews"],
                    "commits" => &["commits"],
                    _ => &[],
                };
                if !keep.is_empty()
                    && let Some(content) = nq.get_mut("content").and_then(Value::as_object_mut)
                {
                    content.retain(|key, _| keep.contains(&key.as_str()));
                }
            }
        }
        if axis == "comments" {
            remove_key(&mut nq, "commentBodyOffset");
        }
        next.insert(name.into(), continuation(nq));
    }
    if partial {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["contentPagination"]);
        if !next.is_empty() {
            out["next"] = Value::Object(next);
        }
    }
}

/// Narrow a patch char-window continuation to the patch surface and to the
/// files whose window has more; completed files are not re-emitted.
fn narrow_patch_continuation(nq: &mut Value, entry: &mut Value, q: &HistoryItemRequest) {
    let changed_files_requested = q
        .content_value()
        .as_ref()
        .and_then(|value| value.get("changedFiles"))
        .and_then(Value::as_bool)
        == Some(true);
    if let Some(content) = nq.get_mut("content").and_then(Value::as_object_mut) {
        if !changed_files_requested {
            content.remove("changedFiles");
        }
        content.retain(|key, _| key == "patches" || key == "changedFiles");
    }
    if let Some(unfinished) = entry
        .get("files")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|files| !files.is_empty())
    {
        if nq.pointer("/content/patches/mode").and_then(Value::as_str) == Some("selected") {
            retain_unresolved_patch_selection(nq, &unfinished);
        } else if let Some(patches) = nq.pointer_mut("/content/patches") {
            *patches = json!({"mode":"selected","files":unfinished});
        }
        nq["filePage"] = json!(1);
    }
    // The continuation carries the list; keep only a count.
    if let Some(entry) = entry.as_object_mut()
        && let Some(Value::Array(files)) = entry.remove("files")
    {
        entry.insert("unfinishedFiles".into(), json!(files.len()));
    }
}

fn remove_key(value: &mut Value, key: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}

fn selected_patch_paths(query: &HistoryItemRequest) -> Option<Vec<String>> {
    let content = query.content_value()?;
    let patches = content.get("patches")?;
    if patches.get("mode").and_then(Value::as_str) != Some("selected") {
        return None;
    }
    let files = patches
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    let ranges = patches
        .get("ranges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|range| range.get("file").and_then(Value::as_str));
    let mut paths = Vec::<String>::new();
    for path in files.chain(ranges) {
        if !paths.iter().any(|existing| existing == path) {
            paths.push(path.to_owned());
        }
    }
    Some(paths)
}

fn retain_unresolved_patch_selection(query: &mut Value, unresolved: &[String]) {
    let unresolved = unresolved
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let Some(patches) = query
        .pointer_mut("/content/patches")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if let Some(files) = patches.get_mut("files").and_then(Value::as_array_mut) {
        files.retain(|file| file.as_str().is_some_and(|path| unresolved.contains(path)));
    }
    if let Some(ranges) = patches.get_mut("ranges").and_then(Value::as_array_mut) {
        ranges.retain(|range| {
            range
                .get("file")
                .and_then(Value::as_str)
                .is_some_and(|path| unresolved.contains(path))
        });
    }
}

pub(super) fn promote_issue_continuations(out: &mut Value, q: &HistoryItemRequest) {
    let Some(pages) = out
        .pointer("/issues/0/contentPagination")
        .and_then(Value::as_object)
    else {
        return;
    };
    let comments = || json!({"comments":q.content_value().and_then(|v|v.get("comments").cloned()).unwrap_or(json!({"discussion":true}))});
    let mut next = Map::new();
    for (axis, entry) in pages {
        if entry.get("hasMore").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let (name, key, field, content) = match axis.as_str() {
            "body" => (
                "continueBody",
                "charOffset",
                "nextCharOffset",
                json!({"body":true}),
            ),
            "commentBody" => (
                "continueCommentBody",
                "charOffset",
                "nextCharOffset",
                comments(),
            ),
            "comments" => (
                "nextCommentsPage",
                "commentPage",
                "nextCommentPage",
                comments(),
            ),
            _ => continue,
        };
        let Some(cursor) = entry.get(field) else {
            continue;
        };
        let mut nq = base_public_query(q, ItemOperation::Issue);
        nq[key] = cursor.clone();
        nq["content"] = content;
        if axis == "comments" {
            nq["charOffset"] = json!(0);
        }
        next.insert(name.into(), continuation(nq));
    }
    if !next.is_empty() {
        out["isPartial"] = json!(true);
        out["partialReasons"] = json!(["contentPagination"]);
        out["next"] = Value::Object(next);
    }
}

/// `next.*` for a commit or comparison file page (also nested per PR commit
/// when `with_why`).
pub(super) fn attach_diff_continuations(
    out: &mut Value,
    q: &HistoryItemRequest,
    operation: ItemOperation,
    resolved_ref: Option<&str>,
    with_why: bool,
) {
    let mut next = Map::new();
    let mut base = if matches!(operation, ItemOperation::Commit) {
        json!({"operation":"commit","owner":q.owner(),"repo":q.repo(),"ref":resolved_ref.or(q.reference()),"includeDiff":with_why || q.include_diff(),"path":q.path(),"filePage":q.file_page().or((!with_why).then_some(1)),"pageSize":q.page_size(),"charOffset":q.char_offset(),"charLength":q.char_length()})
    } else {
        base_public_query(q, operation)
    };
    remove_nulls(&mut base);
    let make = |query: Value, why: &str| {
        if with_why {
            json!({"tool":"ghGetHistoryItem","query":query,"why":why,"confidence":"exact"})
        } else {
            continuation(query)
        }
    };
    // Only emit a next-page continuation when there is a real next page. The
    // pagination object carries `nextPage: null` when there is none; without the
    // null guard that null would clobber the base query's required `page`,
    // producing an outputContractViolation.
    if let Some(page) = out
        .pointer("/pagination/nextPage")
        .cloned()
        .filter(|value| !value.is_null())
    {
        let mut nq = base.clone();
        nq["page"] = page;
        next.insert(
            "nextPage".into(),
            make(
                nq,
                "Continue the comparison commit list. Changed files are returned on page 1.",
            ),
        );
    }
    if let Some(page) = out
        .pointer("/filesPagination/nextFilePage")
        .and_then(Value::as_u64)
        .map(Value::from)
    {
        let mut nq = base.clone();
        nq["filePage"] = page;
        remove_key(&mut nq, "charOffset");
        next.insert(
            "nextFilePage".into(),
            make(
                nq,
                "Continue the changed-file list from the beginning of each new patch.",
            ),
        );
    }
    // Files were listed without patches: offer the same page with diffs.
    if base.get("includeDiff").and_then(Value::as_bool) != Some(true)
        && out
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|files| !files.is_empty())
    {
        let mut nq = base.clone();
        nq["includeDiff"] = json!(true);
        remove_key(&mut nq, "charOffset");
        next.insert(
            "includeDiff".into(),
            make(nq, "Read the patches for this page of changed files."),
        );
    }
    if let Some(offset) = out
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|v| v.pointer("/patchPagination/nextCharOffset"))
        .cloned()
    {
        let mut nq = base;
        nq["charOffset"] = offset;
        next.insert(
            "continuePatch".into(),
            make(nq, "Continue the current patch window."),
        );
    }
    if !next.is_empty() {
        out["next"] = Value::Object(next);
    }
}

#[cfg(test)]
mod tests {
    use super::super::GhGetHistoryItemQuery;
    use super::*;

    #[test]
    fn selected_patch_continuation_stops_after_every_requested_path_is_returned() {
        let query: HistoryItemRequest = serde_json::from_value::<GhGetHistoryItemQuery>(json!({
            "operation":"pullRequest","reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"selected","files":["src/lib.rs"]}}
        }))
        .expect("selected patch query")
        .into();
        let mut output = json!({
            "type":"pullRequests",
            "pullRequests":[{
                "changedFiles":[{"path":"src/lib.rs","patch":"diff"}],
                "contentPagination":{"changedFiles":{
                    "hasMore":true,
                    "nextPage":1,
                    "nextCollectionPages":{"changedFiles":2}
                }}
            }]
        });

        promote_pr_continuations(&mut output, &query);

        assert_eq!(
            output["pullRequests"][0]["contentPagination"]["changedFiles"]["hasMore"], false,
            "{output}"
        );
        assert!(
            output.pointer("/next/nextChangedFilesPage").is_none(),
            "{output}"
        );
    }

    #[test]
    fn selected_patch_continuation_carries_only_unresolved_paths() {
        let query: HistoryItemRequest = serde_json::from_value::<GhGetHistoryItemQuery>(json!({
            "operation":"pullRequest","reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"selected","files":["src/a.rs","src/b.rs"]}}
        }))
        .expect("selected patch query")
        .into();
        let mut output = json!({
            "type":"pullRequests",
            "pullRequests":[{
                "changedFiles":[{"path":"src/a.rs","patch":"diff"}],
                "contentPagination":{"changedFiles":{
                    "hasMore":true,
                    "nextPage":1,
                    "nextCollectionPages":{"changedFiles":2}
                }}
            }]
        });

        promote_pr_continuations(&mut output, &query);

        let next_query = &output["next"]["nextChangedFilesPage"]["query"];
        assert_eq!(
            next_query["content"]["patches"]["files"],
            json!(["src/b.rs"]),
            "{output}"
        );
        assert_eq!(next_query["filePage"], 1, "{output}");
    }

    #[test]
    fn selected_patch_continuation_filters_resolved_range_selectors() {
        let query: HistoryItemRequest = serde_json::from_value::<GhGetHistoryItemQuery>(json!({
            "operation":"pullRequest","reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"selected","ranges":[
                {"file":"src/a.rs","additions":[1]},
                {"file":"src/b.rs","deletions":[2]}
            ]}}
        }))
        .expect("selected patch range query")
        .into();
        let mut output = json!({
            "type":"pullRequests",
            "pullRequests":[{
                "changedFiles":[{"path":"src/a.rs","patch":"diff"}],
                "contentPagination":{"changedFiles":{
                    "hasMore":true,
                    "nextPage":1,
                    "nextCollectionPages":{"changedFiles":2}
                }}
            }]
        });

        promote_pr_continuations(&mut output, &query);

        let ranges =
            &output["next"]["nextChangedFilesPage"]["query"]["content"]["patches"]["ranges"];
        assert_eq!(
            ranges,
            &json!([{"file":"src/b.rs","deletions":[2]}]),
            "{output}"
        );
    }

    #[test]
    fn pr_next_menu_carries_required_defaults_and_drops_cursors() {
        let query: HistoryItemRequest = serde_json::from_value::<GhGetHistoryItemQuery>(json!({
            "operation":"pullRequest","reasoning":"test","owner":"o","repo":"r","number":5,
            "content":{"body":true},"charOffset":100,"commentPage":2,
            "reasoning":"r"
        }))
        .expect("query")
        .into();
        let content_value = query.content_value();
        let content = content_value.as_ref().and_then(Value::as_object);
        let menu = pr_next_menu(&query, content, "none", Some("src/a.rs"));
        let reviews = &menu["getReviews"]["query"];
        assert_eq!(reviews["pageSize"], 30);
        assert_eq!(reviews["minify"], "standard");
        assert_eq!(reviews["content"], json!({"reviews":true}));
        for key in ["charOffset", "commentPage", "reasoning"] {
            assert!(reviews.get(key).is_none(), "{key} leaked: {reviews}");
        }
        assert!(menu.get("getBody").is_none());
    }

    #[test]
    fn char_offset_continuations_narrow_content_to_their_own_surface() {
        // charOffset is one shared field: continuing the body must not skew
        // review bodies or patches by the body offset (and vice versa).
        let query: HistoryItemRequest = serde_json::from_value::<GhGetHistoryItemQuery>(json!({
            "operation":"pullRequest","reasoning":"test","owner":"o","repo":"r","number":5,
            "content":{"body":true,"reviews":true,"patches":{"mode":"all"},"comments":{"discussion":true}},
            "charOffset":0
        }))
        .expect("query")
            .into();
        let mut out = json!({"type":"pullRequests","pullRequests":[{"contentPagination":{
            "body":{"hasMore":true,"nextCharOffset":12000},
            "reviewBody":{"hasMore":true,"nextCharOffset":300},
            "patches":{"hasMore":true,"nextCharOffset":900},
            "comments":{"hasMore":true,"nextPage":2}
        }}]});
        promote_pr_continuations(&mut out, &query);
        let next = &out["next"];
        assert_eq!(
            next["continueBody"]["query"]["content"],
            json!({"body":true})
        );
        assert_eq!(next["continueBody"]["query"]["charOffset"], 12000);
        assert_eq!(
            next["continueReviewBody"]["query"]["content"],
            json!({"reviews":true})
        );
        assert_eq!(
            next["continuePatch"]["query"]["content"],
            json!({"patches":{"mode":"all"}})
        );
        let comments = &next["nextCommentsPage"]["query"];
        assert!(comments.get("charOffset").is_none(), "{comments}");
        assert_eq!(comments["commentPage"], 2);
    }
}
