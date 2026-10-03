//! `next.*` builders: exact follow-up `ghGetHistoryItem` queries derived from
//! the public query and the page objects of a shaped response.
use super::util::{content_flag, merge};
use super::{HistoryItemRequest, ItemOperation, default_page_size};
use crate::tools::id::ToolId;
use crate::tools::result::remove_nulls;
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// The caller's query as a replayable public query for `operation`.
pub(super) fn base_public_query(q: &HistoryItemRequest, operation: ItemOperation) -> Value {
    let mut v = serde_json::to_value(&q.query).unwrap_or_default();
    remove_nulls(&mut v);
    if let Some(m) = v.as_object_mut() {
        if let Some(content) = q.content_value() {
            m.insert("content".into(), content);
        }
        m.insert(
            "operation".into(),
            json!(match operation {
                ItemOperation::PullRequest => "pullRequest",
                ItemOperation::Issue => "issue",
                ItemOperation::Commit => "commit",
                ItemOperation::Compare => "compare",
            }),
        );
        m.remove("debug");
        match operation {
            ItemOperation::PullRequest => {
                // pageSize has no contract default: each surface sizes its own
                // page, so an omitted pageSize stays omitted on replay.
                m.insert(
                    "minify".into(),
                    json!(q.minify().unwrap_or_else(|| "standard".to_owned())),
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
                    json!(q.page_size().unwrap_or_else(default_page_size)),
                );
            }
            _ => {}
        }
    }
    v
}

fn continuation(q: Value) -> Value {
    json!({"tool":ToolId::GhGetHistoryItem.as_str(),"query":q,"confidence":"exact"})
}

/// A first-page menu read in the flat spelling.
fn menu_read(mut q: Value) -> Value {
    flatten_selectors(&mut q);
    continuation(q)
}

/// Rewrite a pull-request query's `content` into the flat `include`/`files`
/// spelling when it says the same thing (no ranges, bot or per-commit-file
/// options, and exact selected paths only). Both spellings validate.
pub(super) fn flatten_selectors(query: &mut Value) {
    let Some(fields) = query.as_object_mut() else {
        return;
    };
    if fields.get("operation").and_then(Value::as_str) != Some("pullRequest")
        || fields.contains_key("fileFilter")
    {
        return;
    }
    let Some(Value::Object(content)) = fields.get("content").cloned() else {
        return;
    };
    let mut include = Vec::new();
    let mut files = None;
    for (key, value) in &content {
        match (key.as_str(), value) {
            ("body", Value::Bool(true)) => include.push("body"),
            ("changedFiles", Value::Bool(true)) => include.push("files"),
            ("reviews", Value::Bool(true)) => include.push("reviews"),
            ("body" | "changedFiles" | "reviews", Value::Bool(false)) => {}
            ("comments", comments)
                if comments == &json!({"discussion":true,"reviewInline":true}) =>
            {
                include.push("comments");
            }
            ("commits", commits) if commits == &json!({}) => include.push("commits"),
            ("patches", patches) => match patches.get("mode").and_then(Value::as_str) {
                Some("all") if patches.as_object().is_some_and(|p| p.len() == 1) => {
                    include.push("patches");
                }
                Some("selected")
                    if patches.get("ranges").is_none()
                        && patches.get("files").and_then(Value::as_array).is_some_and(
                            |paths| {
                                !paths.is_empty()
                                    && paths.iter().all(|path| {
                                        path.as_str().is_some_and(|path| {
                                            !path.contains(['*', '?', '[', '{'])
                                        })
                                    })
                            },
                        ) =>
                {
                    include.push("patches");
                    files = patches.get("files").cloned();
                }
                _ => return,
            },
            _ => return,
        }
    }
    fields.remove("content");
    if !include.is_empty() {
        fields.insert("include".into(), json!(include));
    }
    if let Some(files) = files {
        fields.insert("files".into(), files);
    }
}

/// A diff at most this many changed lines reads in one all-patches call, so a
/// separate file-list-only fetch would only repeat its file list.
const SMALL_DIFF_LINES: u64 = 100;
/// Unfiltered inventories up to this many files keep `getAllPatches` beside
/// the selected-patch pick: every patch is still a bounded read.
pub(super) const INVENTORY_ALL_PATCHES_FILES: u64 = 30;

/// A first-page fetch of `query`'s PR: the base public query without its
/// content selection, filters, or per-surface cursors.
fn fresh_pr_query(query: &HistoryItemRequest) -> Value {
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
    target
}

/// Whether a pull request's whole diff fits one patch read: at most
/// `SMALL_DIFF_LINES` changed lines over at most
/// [`INVENTORY_ALL_PATCHES_FILES`] files (an unknown file count passes). An
/// unknown line count is never small.
pub(super) fn is_small_pr(
    additions: Option<u64>,
    deletions: Option<u64>,
    changed_files: Option<u64>,
) -> bool {
    matches!(
        (additions, deletions),
        (Some(additions), Some(deletions)) if additions + deletions <= SMALL_DIFF_LINES
    ) && changed_files.is_none_or(|files| files <= INVENTORY_ALL_PATCHES_FILES)
}

/// Per-row menu of first-page fetches for content the call did not request:
/// at most three entries (`getChangedFiles`, `reviewPatches`,
/// `getDiscussion`; an inventory read may add `getAllPatches`). A merged
/// row already shows `mergeCommitSha`, so no merge-commit read is offered.
///
/// `raw` is the provider PR object; `review` is the inventory's
/// [`super::files::review_selection`] (empty before files were read). An
/// entry is emitted only when it can return something the row does not
/// already hold: a non-empty body (the summary row carries none) rides
/// `getChangedFiles`, else the patch read or `getBody`; a small diff reads every patch (`reviewPatches` mode all)
/// instead of a separate file list; `getDiscussion` reads comments and
/// reviews together, without comments when the provider counts none.
pub(super) fn pr_next_menu(
    query: &HistoryItemRequest,
    content: Option<&Map<String, Value>>,
    patch_mode: &str,
    review: &[String],
    raw: &Value,
) -> Value {
    let count = |key: &str| raw.get(key).and_then(Value::as_u64);
    let has_body = raw
        .get("body")
        .and_then(Value::as_str)
        .is_some_and(|body| !body.trim().is_empty());
    let want_body = !content_flag(content, "body") && has_body;
    let changed_files = count("changed_files");
    let has_files = changed_files != Some(0);
    let small_pr = is_small_pr(count("additions"), count("deletions"), changed_files);
    let no_comments = count("comments") == Some(0) && count("review_comments") == Some(0);
    // Each menu entry is a fresh first-page fetch: the base public query
    // without the current content selection or any per-surface cursor.
    let target = fresh_pr_query(query);
    let mut next = Map::new();
    let call = |content: Value| menu_read(merge(target.clone(), json!({"content":content})));
    let with_body = |mut content: Value| {
        if want_body {
            content["body"] = json!(true);
        }
        content
    };
    let files_read = content_flag(content, "changedFiles") || patch_mode != "none";
    let mut body_offered = false;
    if !files_read && has_files && !small_pr {
        next.insert(
            "getChangedFiles".into(),
            call(with_body(json!({"changedFiles":true}))),
        );
        body_offered = true;
    }
    if patch_mode == "none" && has_files {
        let covers_all = changed_files.is_some_and(|files| review.len() as u64 >= files);
        if !review.is_empty() && !covers_all {
            // The query is exact; which files answer the question is a
            // ranking guess (source files by churn, packed to one budget).
            let mut selected = call(json!({"patches":{"mode":"selected","files":review}}));
            selected["confidence"] = json!("high");
            next.insert("reviewPatches".into(), selected);
            if changed_files.is_none_or(|files| files <= INVENTORY_ALL_PATCHES_FILES) {
                next.insert(
                    "getAllPatches".into(),
                    call(json!({"patches":{"mode":"all"}})),
                );
            }
        } else if small_pr || covers_all {
            let patches = json!({"patches":{"mode":"all"}});
            let patches = if body_offered {
                patches
            } else {
                with_body(patches)
            };
            body_offered = true;
            next.insert("reviewPatches".into(), call(patches));
        }
    }
    if want_body && !body_offered && !files_read {
        next.insert("getBody".into(), call(json!({"body":true})));
    }
    let mut discussion = Map::new();
    if content.and_then(|v| v.get("comments")).is_none() && !no_comments {
        discussion.insert(
            "comments".into(),
            json!({"discussion":true,"reviewInline":true}),
        );
    }
    if !content_flag(content, "reviews") {
        discussion.insert("reviews".into(), json!(true));
    }
    if !discussion.is_empty() {
        next.insert("getDiscussion".into(), call(Value::Object(discussion)));
    }
    Value::Object(next)
}

/// Longest added-line prefix `next.readAtMerge` anchors its read on.
const MERGE_ANCHOR_CHARS: usize = 60;

/// `next.readAtMerge`: a merged pull request whose patch rows were read
/// offers its first changed code file (not a test) at the merge commit, as a
/// block read anchored on that file's first added line, so the fix is
/// checked in the code that shipped. `None` for unmerged pull requests, for
/// a `matchString` read (a targeted answer already), and for windows without
/// such a file or an added line to anchor on.
pub(super) fn read_at_merge(
    query: &HistoryItemRequest,
    raw: &Value,
    files: &Value,
) -> Option<Value> {
    use crate::content::{FileType, classify_file_type, is_test_path};
    if query.match_string().is_some() {
        return None;
    }
    raw.get("merged_at").filter(|merged| !merged.is_null())?;
    let sha = raw
        .get("merge_commit_sha")
        .and_then(Value::as_str)
        .filter(|sha| !sha.is_empty())?;
    let patched = files
        .as_array()?
        .iter()
        .filter_map(|file| {
            let path = file.get("path")?.as_str()?;
            Some((path, first_added_line(file.get("patch")?.as_str()?)?))
        })
        .collect::<Vec<_>>();
    let (path, anchor) = patched.iter().find(|(path, _)| {
        classify_file_type(path) == Some(FileType::Code) && !is_test_path(path)
    })?;
    Some(json!({
        "tool": ToolId::GhGetFileContent.as_str(),
        "confidence": "high",
        "query": {
            "owner": query.owner(),
            "repo": query.repo(),
            "branch": sha,
            "path": path,
            "matchString": anchor,
            "block": true,
        },
    }))
}

/// The first added patch line with something to match on (not a lone
/// brace or a redaction placeholder), trimmed and cut to
/// [`MERGE_ANCHOR_CHARS`].
fn first_added_line(patch: &str) -> Option<String> {
    patch
        .lines()
        .filter(|line| !line.starts_with("+++"))
        .filter_map(|line| line.strip_prefix('+'))
        .map(str::trim)
        .find(|text| {
            text.chars().filter(|c| c.is_alphanumeric()).count() >= 3 && !text.contains("[REDACTED")
        })
        .map(|text| {
            text.chars()
                .take(MERGE_ANCHOR_CHARS)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
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

/// The lossless re-read of every patch row whose view is not the raw patch
/// (rows marked `fullPatchChars`, kept on rows only with `debug`):
/// `next.readFullPatches` for a `matchString` view (hit lines, clipped long
/// lines), `next.readUntrimmed` for a minified view (context replaced by
/// `...`). Both read the selected files with `minify:"none"` and page through
/// the patch window; more files than one selection holds continue in
/// `readFullPatches2`, `readFullPatches3`, …. Many narrowed files without an
/// explicit `matchContext` also get `next.widenContext`.
pub(super) fn attach_full_patch_continuation(out: &mut Value, q: &HistoryItemRequest) {
    let narrowed = take_reshaped_paths(out.pointer_mut("/pullRequests/0/changedFiles"), q.debug());
    if narrowed.is_empty() {
        return;
    }
    let mut nq = base_public_query(q, ItemOperation::PullRequest);
    for key in ["charOffset", "charLength", "filePage"] {
        remove_key(&mut nq, key);
    }
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    let name = if q.match_string().is_some() {
        if narrowed.len() > WIDEN_CONTEXT_FILES && q.match_context().is_none() {
            let mut widen = nq.clone();
            widen["matchContext"] = json!(WIDEN_MATCH_CONTEXT);
            out["next"]["widenContext"] = continuation(widen);
        }
        remove_key(&mut nq, "matchString");
        remove_key(&mut nq, "matchContext");
        "readFullPatches"
    } else {
        "readUntrimmed"
    };
    nq["minify"] = json!("none");
    for (index, files) in narrowed.chunks(SELECTED_PATCH_FILES).enumerate() {
        let mut read = nq.clone();
        read["content"] = json!({"patches":{"mode":"selected","files":files}});
        let key = if index == 0 {
            name.to_owned()
        } else {
            format!("{name}{}", index + 1)
        };
        out["next"][key] = continuation(read);
    }
}

/// `next.readRawBody`: the PR text surfaces whose minified view dropped text
/// (the row says `bodyView:"minified"`), re-read with `minify:"none"` from
/// their first character, on the same comment and review pages.
pub(super) fn attach_raw_body_read(out: &mut Value, q: &HistoryItemRequest, surfaces: &[&str]) {
    if surfaces.is_empty() {
        return;
    }
    let mut nq = base_public_query(q, ItemOperation::PullRequest);
    for key in [
        "charOffset",
        "commentBodyOffset",
        "filePage",
        "commitPage",
        "fileFilter",
        "files",
    ] {
        remove_key(&mut nq, key);
    }
    let requested = q.content_value().unwrap_or_default();
    let mut content = Map::new();
    for surface in surfaces {
        let value = match *surface {
            "comments" => requested
                .get("comments")
                .cloned()
                .unwrap_or(json!({"discussion":true})),
            _ => json!(true),
        };
        content.insert((*surface).to_owned(), value);
    }
    nq["content"] = Value::Object(content);
    nq["minify"] = json!("none");
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    out["next"]["readRawBody"] = menu_read(nq);
}

/// Paths of the rows marked `fullPatchChars` (a view that is not the raw
/// patch). The marker stays on rows only with `debug`.
pub(super) fn take_reshaped_paths(rows: Option<&mut Value>, debug: bool) -> Vec<String> {
    let mut paths = Vec::new();
    for file in rows.and_then(Value::as_array_mut).into_iter().flatten() {
        let Some(fields) = file.as_object_mut() else {
            continue;
        };
        let marked = if debug {
            fields.contains_key("fullPatchChars")
        } else {
            fields.remove("fullPatchChars").is_some()
        };
        if marked && let Some(path) = fields.get("path").and_then(Value::as_str) {
            paths.push(path.to_owned());
        }
    }
    paths
}

/// Narrowed files above which `next.widenContext` (a [`WIDEN_MATCH_CONTEXT`]
/// line view) is offered beside the whole-patch re-read.
const WIDEN_CONTEXT_FILES: usize = 5;
const WIDEN_MATCH_CONTEXT: u64 = 3;
/// Paths one `content.patches.files` selection holds (the contract's
/// `maxItems`).
pub(super) const SELECTED_PATCH_FILES: usize = 100;

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
        // The narrowed selection restarts at the first file page (omitted).
        remove_key(nq, "filePage");
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
    if !q.file_scope.is_empty() && !with_why {
        base["files"] = json!(q.file_scope);
    }
    remove_nulls(&mut base);
    let make = |query: Value, why: &str| {
        if with_why {
            json!({"tool":ToolId::GhGetHistoryItem.as_str(),"query":query,"why":why,"confidence":"exact"})
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
        // A commit page carries commits only; its file cursor would be inert.
        remove_key(&mut nq, "filePage");
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
    // The page-stream cursor, not a file's own `patchPagination` offset.
    if let Some(offset) = out.pointer("/filesPagination/nextPatchCharOffset").cloned() {
        let mut nq = base;
        nq["charOffset"] = offset;
        next.insert(
            "continuePatch".into(),
            make(nq, "Continue the current patch window."),
        );
    }
    // Commit → pull request: a squash-merge headline names its PR
    // (`… (#8506)`); otherwise issue search matches PRs by commit SHA.
    if matches!(operation, ItemOperation::Commit) && !with_why {
        if let Some(number) = out
            .get("messageHeadline")
            .and_then(Value::as_str)
            .and_then(headline_pull_request)
        {
            next.insert(
                "readPullRequest".into(),
                json!({"tool":ToolId::GhGetHistoryItem.as_str(),"confidence":"high","query":{
                    "operation":"pullRequest","owner":q.owner(),"repo":q.repo(),"number":number
                }}),
            );
        } else if let Some(sha) = out.get("sha").and_then(Value::as_str) {
            next.insert(
                "findPullRequest".into(),
                json!({"tool":ToolId::GhSearchHistory.as_str(),"confidence":"high","query":{
                    "operation":"pullRequest","owner":q.owner(),"repo":q.repo(),"keywords":[sha]
                }}),
            );
        }
    }
    if !next.is_empty() {
        out["next"] = Value::Object(next);
    }
}

/// The pull request a squash-merge headline ends with: `subject (#123)`.
fn headline_pull_request(headline: &str) -> Option<u64> {
    let digits = headline.trim_end().strip_suffix(')')?.rsplit_once("(#")?.1;
    digits.parse().ok().filter(|number| *number > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squash_headline_names_its_pull_request() {
        assert_eq!(
            headline_pull_request("io: use `spawn_mandatory_blocking` (#8506)"),
            Some(8506)
        );
        assert_eq!(headline_pull_request("Merge branch main"), None);
        assert_eq!(headline_pull_request("fix (#abc)"), None);
        assert_eq!(headline_pull_request("revert (#12) partially"), None);
    }

    /// D2: a commit's `continuePatch` copies the page-stream cursor named on
    /// `filesPagination`, never a file's own per-file `nextCharOffset`.
    #[test]
    fn commit_continue_patch_copies_the_page_stream_cursor() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"commit","goal":"test","reasoning":"test","owner":"a","repo":"b",
            "ref":"abc","includeDiff":true,"charLength":10
        }))
        .expect("commit query");
        let mut out = json!({
            "files":[{"filename":"a.rs","patch":"aaa"},{"filename":"b.rs","patch":"bbbbbbb",
                "patchPagination":{"charOffset":0,"charLength":7,"totalChars":20,"hasMore":true,"nextCharOffset":7}}],
            "filesPagination":{"currentPage":1,"hasMore":false,"nextPatchCharOffset":10}
        });
        attach_diff_continuations(&mut out, &query, ItemOperation::Commit, Some("abc"), false);
        let next = &out["next"]["continuePatch"]["query"];
        assert_eq!(next["charOffset"], 10, "{out}");
        assert_eq!(next["charLength"], 10);
    }

    #[test]
    fn selected_patch_continuation_stops_after_every_requested_path_is_returned() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"selected","files":["src/lib.rs"]}}
        }))
        .expect("selected patch query");
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
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"selected","files":["src/a.rs","src/b.rs"]}}
        }))
        .expect("selected patch query");
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
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"selected","ranges":[
                {"file":"src/a.rs","additions":[1]},
                {"file":"src/b.rs","deletions":[2]}
            ]}}
        }))
        .expect("selected patch range query");
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
    fn match_string_views_offer_the_whole_patches_they_narrowed() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"all"}},"matchString":"needle","charOffset":10,"filePage":2
        }))
        .expect("match query");
        let mut out = json!({"type":"pullRequests","pullRequests":[{"changedFiles":[
            {"path":"src/a.rs","patch":"@@ -1,1 +1,1 @@\n+needle","fullPatchChars":900},
            {"path":"src/b.rs","patch":"+needle"}
        ]}]});
        attach_full_patch_continuation(&mut out, &query);
        let next = &out["next"]["readFullPatches"]["query"];
        assert_eq!(
            next["content"],
            json!({"patches":{"mode":"selected","files":["src/a.rs"]}}),
            "{out}"
        );
        for key in ["matchString", "charOffset", "filePage"] {
            assert!(next.get(key).is_none(), "{key} kept: {next}");
        }
        // The re-read is raw: a minified view would trim the context again.
        assert_eq!(next["minify"], "none", "{next}");
        // An explicit matchContext still clips long lines: the whole patch
        // stays reachable.
        let explicit: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"patches":{"mode":"all"}},"matchString":"needle","matchContext":0
        }))
        .expect("match query");
        let mut out = json!({"type":"pullRequests","pullRequests":[{"changedFiles":[
            {"path":"src/a.rs","patch":"+needle","fullPatchChars":900}
        ]}]});
        attach_full_patch_continuation(&mut out, &explicit);
        let next = &out["next"]["readFullPatches"]["query"];
        assert_eq!(
            next["content"],
            json!({"patches":{"mode":"selected","files":["src/a.rs"]}}),
            "{out}"
        );
        assert!(next.get("matchContext").is_none(), "{next}");
        assert!(out["next"].get("widenContext").is_none(), "{out}");
    }

    /// A minified PR patch view (context replaced by `...`) offers
    /// `next.readUntrimmed`: the trimmed files only, raw.
    #[test]
    fn minified_patch_views_offer_the_untrimmed_patches() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "include":["patches"],"charOffset":40
        }))
        .expect("pr query");
        let mut out = json!({"type":"pullRequests","pullRequests":[{"changedFiles":[
            {"path":"src/a.rs","patch":"@@ -1,40 +1,40 @@\n a\n...\n-x\n+y","fullPatchChars":900},
            {"path":"src/b.rs","patch":"+short"}
        ]}]});
        attach_full_patch_continuation(&mut out, &query);
        let next = &out["next"]["readUntrimmed"]["query"];
        assert_eq!(next["minify"], "none", "{out}");
        assert_eq!(
            next["content"],
            json!({"patches":{"mode":"selected","files":["src/a.rs"]}}),
            "{out}"
        );
        assert!(next.get("charOffset").is_none(), "{next}");
        assert!(out["next"].get("readFullPatches").is_none(), "{out}");
        assert!(!out.to_string().contains("fullPatchChars"), "{out}");
    }

    /// `next.readRawBody` re-reads only the minified surfaces, raw, from
    /// their first character, on the same comment page.
    #[test]
    fn raw_body_read_targets_the_minified_surfaces_on_the_same_page() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "content":{"body":true,"comments":{"discussion":true},"patches":{"mode":"all"}},
            "commentPage":2,"commentBodyOffset":30,"charOffset":10
        }))
        .expect("pr query");
        let mut out = json!({"type":"pullRequests","pullRequests":[{"number":1}]});
        attach_raw_body_read(&mut out, &query, &[]);
        assert!(out.get("next").is_none(), "{out}");
        attach_raw_body_read(&mut out, &query, &["body", "comments"]);
        let next = &out["next"]["readRawBody"]["query"];
        assert_eq!(next["minify"], "none", "{out}");
        assert_eq!(next["commentPage"], 2, "{out}");
        for key in ["charOffset", "commentBodyOffset"] {
            assert!(next.get(key).is_none(), "{key} kept: {next}");
        }
        assert_eq!(
            next["content"],
            json!({"body":true,"comments":{"discussion":true}}),
            "{next}"
        );
    }

    /// More reshaped files than one selection holds continue in numbered
    /// reads that reach each file exactly once.
    #[test]
    fn reshaped_files_past_one_selection_continue_in_numbered_reads() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "include":["patches"]
        }))
        .expect("pr query");
        let rows = (0..SELECTED_PATCH_FILES + 3)
            .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"...","fullPatchChars":90}))
            .collect::<Vec<_>>();
        let mut out = json!({"type":"pullRequests","pullRequests":[{"changedFiles":rows}]});
        attach_full_patch_continuation(&mut out, &query);
        let files = |name: &str| {
            out["next"][name]["query"]["content"]["patches"]["files"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(files("readUntrimmed").len(), SELECTED_PATCH_FILES, "{out}");
        assert_eq!(
            files("readUntrimmed2"),
            (SELECTED_PATCH_FILES..SELECTED_PATCH_FILES + 3)
                .map(|i| json!(format!("src/{i}.rs")))
                .collect::<Vec<_>>()
        );
        assert!(out["next"].get("readUntrimmed3").is_none(), "{out}");
    }

    /// Many narrowed files get `widenContext` (matchContext 3) beside the
    /// whole-patch re-read; the marker never reaches default rows.
    #[test]
    fn many_narrowed_files_widen_context_beside_full_patches() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
            "matchString":"miri"
        }))
        .expect("match query");
        let rows = (0..6)
            .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"+miri","fullPatchChars":90}))
            .collect::<Vec<_>>();
        let mut out = json!({"type":"pullRequests","pullRequests":[{"changedFiles":rows}]});
        attach_full_patch_continuation(&mut out, &query);
        let widen = &out["next"]["widenContext"]["query"];
        assert_eq!(widen["matchContext"], 3, "{out}");
        assert_eq!(widen["matchString"], "miri", "{out}");
        assert_eq!(
            out["next"]["readFullPatches"]["query"]["content"]["patches"]["files"]
                .as_array()
                .map(Vec::len),
            Some(6),
            "{out}"
        );
        assert!(!out.to_string().contains("fullPatchChars"), "{out}");
    }

    /// The row already shows a merged PR's `mergeCommitSha`, so the menu
    /// offers no separate merge-commit read beside it.
    #[test]
    fn merged_pull_requests_offer_no_merge_commit_read_beside_the_shown_sha() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"o","repo":"r","number":5
        }))
        .expect("pr query");
        for raw in [
            json!({"merged_at":"2026-09-26T15:24:18Z","merge_commit_sha":"facc6fc"}),
            json!({"merged_at":"2026-09-26T15:24:18Z","merge_commit_sha":"facc6fc",
                "changed_files":3,"additions":63,"deletions":4}),
            json!({"merged_at":null,"merge_commit_sha":"deadbee"}),
        ] {
            let menu = pr_next_menu(&query, None, "none", &[], &raw);
            assert!(menu.get("getMergeCommit").is_none(), "{menu}");
        }
    }

    /// A merged PR whose patches were read offers the first changed source
    /// file at the merge commit, anchored on its first added line.
    #[test]
    fn merged_patch_reads_offer_the_changed_source_at_the_merge_commit() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal":"g","reasoning":"r","owner":"cli","repo":"cli","number":14429
        }))
        .expect("pr query");
        let merged = json!({"merged_at":"2026-09-11T15:55:40Z","merge_commit_sha":"8fcd6a6"});
        let files = json!([
            {"path":"acceptance/testdata/pr/merge.txtar","stat":"M +6 -2","patch":"@@ -33,2 +33,2 @@\n-# Merge\n+# Merge and delete"},
            {"path":"pkg/cmd/pr/merge/merge.go","stat":"M +2 -2","patch":"@@ -589,7 +589,7 @@ func New() {\n \t\tdeleteBranch: opts.DeleteBranch,\n-\t\tcrossRepoPR: pr.HeadRepositoryOwner.Login != baseRepo.RepoOwner(),\n+\t\t}\n+\t\tcrossRepoPR:        pr.IsCrossRepository,"},
            {"path":"pkg/cmd/pr/merge/merge_test.go","stat":"M +55 -0","patch":"@@ -1 +1 @@\n+func TestX() {}"}
        ]);
        let read = read_at_merge(&query, &merged, &files).expect("offer");
        assert_eq!(read["tool"], "ghGetFileContent");
        assert_eq!(
            read["query"],
            json!({"owner":"cli","repo":"cli","branch":"8fcd6a6",
                "path":"pkg/cmd/pr/merge/merge.go",
                "matchString":"crossRepoPR:        pr.IsCrossRepository,","block":true})
        );
        // Open PRs, inventories without patches, and patches with nothing
        // added offer nothing.
        let open = json!({"merged_at":null,"merge_commit_sha":"deadbee"});
        assert!(read_at_merge(&query, &open, &files).is_none());
        let inventory = json!(["M +2 -2 pkg/cmd/pr/merge/merge.go"]);
        assert!(read_at_merge(&query, &merged, &inventory).is_none());
        let removed = json!([{"path":"a.go","stat":"M +0 -1","patch":"@@ -1 +0,0 @@\n-x := 1"}]);
        assert!(read_at_merge(&query, &merged, &removed).is_none());
        // Docs and tests are not the shipped fix: no code file, no offer.
        let no_code = json!([
            {"path":"acceptance/README.md","stat":"M +1 -0","patch":"@@ -1 +1 @@\n+Run the acceptance suite"},
            {"path":"pkg/cmd/pr/merge/merge_test.go","stat":"M +1 -0","patch":"@@ -1 +1 @@\n+func TestX() {}"}
        ]);
        assert!(read_at_merge(&query, &merged, &no_code).is_none());
        // The anchor is a short prefix of the added line.
        let long = format!(
            "+\t{}",
            "fields := []string{\"id\", \"number\", \"state\", \"title\", \"lastCommit\", \"headRefName\"}"
        );
        let files =
            json!([{"path":"a.go","stat":"M +1 -0","patch":format!("@@ -1 +1 @@\n{long}")}]);
        let anchor = read_at_merge(&query, &merged, &files).expect("offer")["query"]["matchString"]
            .as_str()
            .map(str::to_owned)
            .expect("anchor");
        assert!(anchor.chars().count() <= 60, "{anchor}");
        assert!(long.contains(&anchor), "{anchor}");
    }

    #[test]
    fn pr_next_menu_carries_required_defaults_and_drops_cursors() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
            "content":{"body":true},"charOffset":100,"commentPage":2,
            "reasoning":"r"
        }))
        .expect("query");
        let content_value = query.content_value();
        let content = content_value.as_ref().and_then(Value::as_object);
        let menu = pr_next_menu(&query, content, "none", &["src/a.rs".into()], &json!({}));
        let reviews = &menu["getDiscussion"]["query"];
        // pageSize has no contract default; an omitted one stays omitted.
        assert!(reviews.get("pageSize").is_none(), "{reviews}");
        assert_eq!(reviews["minify"], "standard");
        // Menu reads use the flat spelling.
        assert_eq!(reviews["include"], json!(["comments", "reviews"]));
        assert!(reviews.get("content").is_none(), "{reviews}");
        for key in ["charOffset", "commentPage"] {
            assert!(reviews.get(key).is_none(), "{key} leaked: {reviews}");
        }
        assert_eq!(reviews["goal"], "test");
        assert_eq!(reviews["reasoning"], "r");
        assert!(menu.get("getBody").is_none());
    }

    #[test]
    fn pr_next_menu_omits_entries_the_row_already_answers() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("query");
        let names = |menu: &Value| {
            menu.as_object()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        // Empty body, one small file, comments present: every patch reads in
        // one call, so no separate file list.
        let small = json!({"body":"","changed_files":1,"additions":6,"deletions":1,"comments":3});
        let menu = pr_next_menu(&query, None, "none", &[], &small);
        assert_eq!(names(&menu), ["reviewPatches", "getDiscussion"]);
        assert_eq!(
            menu["reviewPatches"]["query"]["include"],
            json!(["patches"])
        );
        // A body rides the file list of a large diff.
        let large = json!({"body":"x","changed_files":40,
            "additions":900,"deletions":50,"comments":0,"review_comments":0});
        let menu = pr_next_menu(&query, None, "none", &[], &large);
        assert_eq!(names(&menu), ["getChangedFiles", "getDiscussion"]);
        assert_eq!(
            menu["getChangedFiles"]["query"]["include"],
            json!(["files", "body"])
        );
        // Provably no comments: the discussion read asks for reviews only.
        assert_eq!(
            menu["getDiscussion"]["query"]["include"],
            json!(["reviews"])
        );
        // No changed files and a body: the body read stands alone.
        let empty = json!({"body":"y","changed_files":0,
            "comments":0,"review_comments":0});
        assert_eq!(
            names(&pr_next_menu(&query, None, "none", &[], &empty)),
            ["getBody", "getDiscussion"]
        );
    }

    /// A summary menu holds at most three entries; an inventory read
    /// turns its review pick into `reviewPatches` over several files.
    #[test]
    fn summary_menu_is_three_entries_and_inventory_reviews_many_files() {
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("query");
        let merged = json!({"body":"x".repeat(900),"changed_files":656,"additions":282_700,
            "deletions":284_842,"comments":5,"merged_at":"2026-01-01T00:00:00Z","merge_commit_sha":"abc"});
        let menu = pr_next_menu(&query, None, "none", &[], &merged);
        let keys = menu
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(keys, ["getChangedFiles", "getDiscussion"], "{menu}");
        assert!(!menu.to_string().contains("<literal"), "{menu}");
        let review = vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()];
        let inventory = json!({"changed_files":30,"additions":900,"deletions":10});
        let content = json!({"changedFiles":true});
        let menu = pr_next_menu(&query, content.as_object(), "none", &review, &inventory);
        assert_eq!(
            menu["reviewPatches"]["query"]["include"],
            json!(["patches"])
        );
        assert_eq!(
            menu["reviewPatches"]["query"]["files"],
            json!(["src/a.rs", "src/b.rs"])
        );
        assert_eq!(menu["reviewPatches"]["confidence"], "high");
        assert!(menu.get("getAllPatches").is_some(), "{menu}");
        // A pick covering every changed file is the every-patch read.
        let two = json!({"body":"","changed_files":2,"additions":900,"deletions":10});
        let menu = pr_next_menu(&query, content.as_object(), "none", &review, &two);
        assert_eq!(
            menu["reviewPatches"]["query"]["include"],
            json!(["patches"]),
            "{menu}"
        );
        assert!(
            menu["reviewPatches"]["query"].get("files").is_none(),
            "{menu}"
        );
    }

    #[test]
    fn char_offset_continuations_narrow_content_to_their_own_surface() {
        // charOffset is one shared field: continuing the body must not skew
        // review bodies or patches by the body offset (and vice versa).
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
            "content":{"body":true,"reviews":true,"patches":{"mode":"all"},"comments":{"discussion":true}},
            "charOffset":0
        }))
        .expect("query");
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
