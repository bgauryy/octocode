//! Patch hops: the whole-patch re-read, selected-patch continuations, and
//! commit/comparison file-page cursors.
use super::pr_menu::*;
use super::{HistoryItemRequest, ItemOperation};
use crate::tools::id::ToolId;
use crate::tools::result::Continuation;
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// The lossless re-read of every patch row a `matchString` view narrowed
/// (hit lines, clipped long lines; rows marked `fullPatchChars`, kept on
/// rows only with `debug`): `next.readFullPatches` reads the selected files
/// whole and pages through the patch window; more files than one selection
/// holds continue in `readFullPatches2`, `readFullPatches3`, ….
pub(super) fn attach_full_patch_continuation(out: &mut Value, q: &HistoryItemRequest) {
    let narrowed = take_reshaped_paths(out.pointer_mut("/pullRequests/0/files"), q.debug());
    if narrowed.is_empty() {
        return;
    }
    let mut nq = base_public_query(q, ItemOperation::PullRequest);
    for key in ["offset", "length", "filePage"] {
        remove_key(&mut nq, key);
    }
    if !out.get("next").is_some_and(Value::is_object) {
        out["next"] = json!({});
    }
    remove_key(&mut nq, "matchString");
    remove_key(&mut nq, "contextLines");
    let name = "readFullPatches";
    // Patch hunks are never minified: the whole patch needs no `minify`.
    remove_key(&mut nq, "minify");
    for (index, files) in narrowed.chunks(SELECTED_PATCH_FILES).enumerate() {
        let mut read = nq.clone();
        read["content"] = json!({"patches":{"mode":"selected","files":files}});
        let key = if index == 0 {
            name.to_owned()
        } else {
            format!("{name}{}", index + 1)
        };
        out["next"][key] = menu_read(read);
    }
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

/// Paths one `content.patches.files` selection holds (the contract's
/// `maxItems`).
pub(super) const SELECTED_PATCH_FILES: usize = 100;

/// The patch continuation of a pull-request page: the same query (file
/// page, filters, selection) at the page-stream cursor, narrowed to the
/// patch surface.
pub(super) fn patch_continuation(mut nq: Value, entry: &Value, q: &HistoryItemRequest) -> Value {
    let files_requested = q
        .content_value()
        .as_ref()
        .and_then(|value| value.get("files"))
        .and_then(Value::as_bool)
        == Some(true);
    if let Some(content) = nq.get_mut("content").and_then(Value::as_object_mut) {
        if !files_requested {
            content.remove("files");
        }
        content.retain(|key, _| key == "patches" || key == "files");
    }
    if let Some(cursor) = entry.get("nextOffset") {
        nq["offset"] = cursor.clone();
    }
    clamp_hop_length(&mut nq, q);
    menu_read(nq)
}

/// A hop carries the caller's `length` as the window it got, so a length
/// clamped to the response page is said once, on the call that asked.
pub(super) fn clamp_hop_length(nq: &mut Value, q: &HistoryItemRequest) {
    if let Some(length) = q.char_length() {
        nq["length"] = json!(super::patch::patch_window(Some(length), q.auto_page_chars));
    }
}

pub(super) fn selected_patch_paths(query: &HistoryItemRequest) -> Option<Vec<String>> {
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

pub(super) fn retain_unresolved_patch_selection(query: &mut Value, unresolved: &[String]) {
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

/// The page state a commit or comparison read leaves to continue, as the
/// shaping code computed it (never re-read from the shaped output).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DiffCursors {
    /// The next comparison commit page.
    pub(super) commit_page: Option<u64>,
    /// The next changed-file page.
    pub(super) file_page: Option<u64>,
    /// The page-stream cursor of an unfinished patch window.
    pub(super) patch_offset: Option<usize>,
    /// The page listed changed files.
    pub(super) files_listed: bool,
}

impl DiffCursors {
    /// The cursors of a file page object (`nextPage`, `nextPatchOffset`).
    pub(super) fn of_file_page(files_page: &Value, files_listed: bool) -> Self {
        Self {
            commit_page: None,
            file_page: files_page.get("nextPage").and_then(Value::as_u64),
            patch_offset: files_page
                .get("nextPatchOffset")
                .and_then(Value::as_u64)
                .and_then(|offset| usize::try_from(offset).ok()),
            files_listed,
        }
    }
}

/// The commit read a commit file page continues: the generated
/// `ghGetHistoryItem` commit query at `reference`, with the caller's
/// page, window and scope (a nested PR commit reads its patches from the
/// first file page).
pub(super) fn commit_query(q: &HistoryItemRequest, reference: &str, nested: bool) -> Value {
    use crate::contracts::tool_types::{
        GhGetHistoryItemQuery, GhGetHistoryItemQueryRef, GhGetHistoryItemQuerySectionsItem, HcPath,
        HiInclude, HiIncludeItem, HiOwner, HiRepo,
    };
    let positive = |value: usize| {
        u64::try_from(value)
            .ok()
            .and_then(std::num::NonZeroU64::new)
    };
    let typed = || -> Option<GhGetHistoryItemQuery> {
        Some(GhGetHistoryItemQuery::Commit {
            base: None,
            debug: false,
            file_page: q
                .file_page()
                .or((!nested).then_some(1))
                .and_then(positive)
                .map(Into::into),
            include: (!nested && !q.file_scope.is_empty())
                .then(|| {
                    q.file_scope
                        .iter()
                        .map(|path| HiIncludeItem::try_from(path.as_str()).ok())
                        .collect::<Option<Vec<_>>>()
                        .map(HiInclude)
                })
                .flatten(),
            length: q.char_length().and_then(positive).map(Into::into),
            main_goal: None,
            offset: q
                .char_offset()
                .and_then(|offset| u64::try_from(offset).ok())
                .map(Into::into),
            owner: HiOwner::try_from(q.owner()).ok()?,
            page_size: q.page_size().and_then(positive).map(Into::into),
            path: q.path().map(|path| HcPath(path.to_owned())),
            reasoning: None,
            ref_: GhGetHistoryItemQueryRef::try_from(reference).ok()?,
            repo: HiRepo::try_from(q.repo()).ok()?,
            sections: (nested || q.include_diff())
                .then_some([GhGetHistoryItemQuerySectionsItem::Patches]),
        })
    };
    let mut value = typed()
        .and_then(|query| serde_json::to_value(query).ok())
        .unwrap_or_default();
    remove_key(&mut value, "debug");
    value
}

/// `next.*` for a commit or comparison file page (also nested per PR commit
/// when `with_why`), from the cursors the page computed.
pub(super) fn attach_diff_continuations(
    out: &mut Value,
    q: &HistoryItemRequest,
    operation: ItemOperation,
    resolved_ref: Option<&str>,
    with_why: bool,
    cursors: DiffCursors,
) {
    let mut next = Map::new();
    let base = match (operation, resolved_ref.or(q.reference())) {
        (ItemOperation::Commit, Some(reference)) => commit_query(q, reference, with_why),
        _ => base_public_query(q, operation),
    };
    let make = |query: Value, why: &str| {
        if with_why {
            Continuation::new(ToolId::GhGetHistoryItem, query)
                .why(why)
                .confidence("exact")
                .build()
        } else {
            continuation(query)
        }
    };
    if let Some(page) = cursors.commit_page {
        let mut nq = base.clone();
        nq["page"] = json!(page);
        // A commit page carries commits only; its file and patch cursors
        // would be inert.
        for key in ["filePage", "offset", "length"] {
            remove_key(&mut nq, key);
        }
        next.insert(
            "nextPage".into(),
            make(
                nq,
                "Continue the comparison commit list. Changed files are returned on page 1.",
            ),
        );
    }
    // A file page with unread patches continues them first: its window
    // that finishes the patches offers the next file page.
    if let Some(page) = cursors.file_page.filter(|_| cursors.patch_offset.is_none()) {
        let mut nq = base.clone();
        nq["filePage"] = json!(page);
        remove_key(&mut nq, "offset");
        next.insert(
            "nextFilePage".into(),
            make(
                nq,
                "Continue the changed-file list from the beginning of each new patch.",
            ),
        );
    }
    // Files were listed without patches: offer the same page with diffs.
    if base.get("sections").is_none() && cursors.files_listed {
        let mut nq = base.clone();
        nq["sections"] = json!(["patches"]);
        remove_key(&mut nq, "offset");
        next.insert(
            "readPatches".into(),
            make(nq, "Read the patches for this page of changed files."),
        );
    }
    // The page-stream cursor, not a file's own `patchPagination` offset.
    if let Some(offset) = cursors.patch_offset {
        let mut nq = base;
        nq["offset"] = json!(offset);
        clamp_hop_length(&mut nq, q);
        next.insert(
            "continuePatch".into(),
            make(nq, "Continue the current patch window."),
        );
    }
    if matches!(operation, ItemOperation::Commit)
        && !with_why
        && let Some((name, lead)) = commit_pull_request_lead(out, q)
    {
        next.insert(name.into(), lead);
    }
    if !next.is_empty() {
        out["next"] = Value::Object(next);
    }
}

/// Commit → pull request: a squash-merge headline names its PR
/// (`… (#8506)`); otherwise issue search matches PRs by commit SHA.
pub(super) fn commit_pull_request_lead(
    out: &Value,
    q: &HistoryItemRequest,
) -> Option<(&'static str, Value)> {
    if let Some(number) = out
        .get("messageHeadline")
        .and_then(Value::as_str)
        .and_then(headline_pull_request)
    {
        return Some((
            "readPullRequest",
            Continuation::new(
                ToolId::GhGetHistoryItem,
                json!({"operation":"pullRequest","owner":q.owner(),"repo":q.repo(),"number":number}),
            )
            .confidence("high")
            .build(),
        ));
    }
    let sha = out.get("sha").and_then(Value::as_str)?;
    Some((
        "findPullRequest",
        Continuation::new(
            ToolId::GhSearchHistory,
            json!({"operation":"pullRequest","owner":q.owner(),"repo":q.repo(),"keywords":[sha]}),
        )
        .confidence("high")
        .build(),
    ))
}

/// The pull request a squash-merge headline ends with: `subject (#123)`.
pub(super) fn headline_pull_request(headline: &str) -> Option<u64> {
    let digits = headline.trim_end().strip_suffix(')')?.rsplit_once("(#")?.1;
    digits.parse().ok().filter(|number| *number > 0)
}
