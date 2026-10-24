//! Optional whole-patch reads and required commit/comparison page cursors.
use super::pr_menu::*;
use super::{HistoryItemRequest, ItemOperation, Sections};
use crate::id::ToolId;
use crate::response::result::Continuation;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::sections::Section;
use crate::tools::gh_shared::commit::commit_pull_request;
use crate::tools::gh_shared::ensure_calls;
use serde_json::{Map, Value, json};

/// Offer unfiltered patches as related work when a matching view selected
/// only hit runs (`fullPatchChars`). Batch selected files within the contract limit.
pub(super) fn attach_full_patch_continuation(out: &mut Value, q: &HistoryItemRequest) {
    let narrowed = reshaped_paths(out.pointer("/pullRequests/0/files"));
    if narrowed.is_empty() {
        return;
    }
    let mut nq = base_public_query(q, ItemOperation::PullRequest);
    for key in ["offset", "page"] {
        remove_key(&mut nq, key);
    }
    ensure_calls(out);
    for key in [
        "matchString",
        "contextLines",
        "regexEngine",
        "caseMode",
        "wholeWord",
    ] {
        remove_key(&mut nq, key);
    }
    // Each read names its files in the public `include` beside `patches`.
    for (index, files) in narrowed.chunks(selected_patch_files()).enumerate() {
        let read = super::util::merge(
            nq.clone(),
            json!({"include": files, "sections": ["patches"]}),
        );
        let lead = crate::response::result::restart_query(ToolId::GhGetHistoryItem, &read);
        out["calls"][crate::id::numbered("readFullPatches", index)] = lead;
    }
}

/// Paths of the rows marked `fullPatchChars` (a view that is not the raw
/// patch). The marker stays on rows only with `debug`.
fn reshaped_paths(rows: Option<&Value>) -> Vec<String> {
    let mut paths = Vec::new();
    for file in rows.and_then(Value::as_array).into_iter().flatten() {
        let Some(fields) = file.as_object() else {
            continue;
        };
        let marked = fields.contains_key("fullPatchChars");
        if marked && let Some(path) = fields.get("path").and_then(Value::as_str) {
            paths.push(path.to_owned());
        }
    }
    paths
}

/// Paths one selected-patch read names: the contract's pull-request
/// `include` `maxItems`.
#[expect(
    clippy::panic,
    reason = "a missing bound is a contract defect caught by the contract tests, never a request-time failure"
)]
pub(super) fn selected_patch_files() -> usize {
    crate::contracts::query_schema_number(
        ToolId::GhGetHistoryItem,
        Some("pullRequest"),
        "include",
        "maxItems",
    )
    .and_then(|maximum| usize::try_from(maximum).ok())
    .unwrap_or_else(|| panic!("contract declares no maxItems for ghGetHistoryItem include"))
}

/// The patch continuation of a pull-request page: the same query (file
/// page, filters, selection) at the page-stream cursor, narrowed to the
/// patch surface.
pub(super) fn patch_continuation(mut nq: Value, entry: &Value, q: &HistoryItemRequest) -> Value {
    let read = q.sections();
    let patch_read = Sections::of([Section::Patches]);
    if read.has(Section::Files) {
        patch_read.with(Section::Files)
    } else {
        patch_read
    }
    .set_on(&mut nq);
    if let Some(cursor) = entry.get("nextOffset") {
        nq["offset"] = cursor.clone();
    }

    continuation(nq)
}

/// A file-page continuation carries the page size its grid was cut on when
/// the read sized it from the response budget
/// ([`super::inventory::pinned_file_page_size`]), so it continues the same
/// file list in any batch.
pub(super) fn pin_file_page_size(nq: &mut Value, q: &HistoryItemRequest, patches: bool) {
    if let Some(size) = super::inventory::pinned_file_page_size(q, patches) {
        nq["pageSize"] = json!(size);
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
    /// The cursors of a file page: its `nextPage`, read out of the block
    /// (a cursor rides the `next` continuation, never the block), and the
    /// patch-stream cursor of its unfinished patch window. The cursor is
    /// internal state for `next.continuePatch`, never an output field.
    pub(super) fn of_file_page(
        files_page: &mut Value,
        patch_offset: Option<usize>,
        files_listed: bool,
    ) -> Self {
        Self {
            commit_page: None,
            file_page: files_page
                .as_object_mut()
                .and_then(|page| page.remove("nextPage"))
                .and_then(|page| page.as_u64()),
            patch_offset,
            files_listed,
        }
    }
}

/// The commit read a commit file page continues: the generated
/// `ghGetHistoryItem` commit query at `reference`, with the caller's
/// page, window and scope.
pub(super) fn commit_query(q: &HistoryItemRequest, reference: &str) -> Value {
    let mut row = q.public.clone();
    if let Some(fields) = row.as_object_mut() {
        for key in ["number", "headSha", "baseRef", "headRef"] {
            fields.remove(key);
        }
    }
    row["operation"] = serde_json::json!("commit");
    row["ref"] = serde_json::json!(reference);
    if q.include_diff() {
        row["sections"] = serde_json::json!(["patches"]);
    } else if let Some(fields) = row.as_object_mut() {
        fields.remove("sections");
    }
    row
}

/// Required pages from computed cursors, plus optional patch expansion for
/// a commit or comparison inventory.
pub(super) fn attach_diff_continuations(
    out: &mut Value,
    q: &HistoryItemRequest,
    operation: ItemOperation,
    resolved_ref: Option<&str>,
    cursors: DiffCursors,
) {
    let mut next = Map::new();
    let base = match (
        operation,
        resolved_ref.or(gh_get_history_item_query::reference(q)),
    ) {
        (ItemOperation::Commit, Some(reference)) => commit_query(q, reference),
        _ => base_public_query(q, operation),
    };
    if let Some(page) = cursors.commit_page {
        let mut nq = base.clone();
        // The first commit page is the page a read without `page` lists.
        if page > 1 {
            nq["page"] = json!(page);
        } else {
            remove_key(&mut nq, "page");
        }
        // A commit page carries commits only; patch cursors would be inert.
        nq["sections"] = json!(["commits"]);
        for key in [
            "offset",
            "matchString",
            "contextLines",
            "regexEngine",
            "caseMode",
            "wholeWord",
        ] {
            remove_key(&mut nq, key);
        }
        next.insert("nextCommitPage".into(), continuation(nq));
    }
    // A file page with unread patches continues them first: its window
    // that finishes the patches offers the next file page.
    if let Some(page) = cursors.file_page.filter(|_| cursors.patch_offset.is_none()) {
        let mut nq = base.clone();
        nq["page"] = json!(page);
        remove_key(&mut nq, "offset");
        files_only(&mut nq, operation);
        pin_file_page_size(&mut nq, q, q.include_diff());
        next.insert("nextFilePage".into(), continuation(nq));
    }
    // Patches expand a file inventory into a fresh section read.
    if !q.include_diff() && base.get("sections").is_none() && cursors.files_listed {
        let mut nq = base.clone();
        nq["sections"] = json!(["patches"]);
        remove_key(&mut nq, "offset");
        next.insert(
            "readPatches".into(),
            crate::response::result::restart_query(ToolId::GhGetHistoryItem, &nq),
        );
    }
    // The page-stream cursor, not a file's own `patchPagination` offset.
    if let Some(offset) = cursors.patch_offset {
        let mut nq = base;
        nq["offset"] = json!(offset);
        files_only(&mut nq, operation);

        next.insert("continuePatch".into(), continuation(nq));
    }
    // The page's own continuations join the reads the page already offers
    // (an unpatched file's search), never replace them.
    if !next.is_empty() {
        crate::tools::gh_shared::ensure_calls(out);
        if let Some(pages) = out["calls"].as_object_mut() {
            pages.extend(next);
        }
    }
}

/// A comparison file or patch continuation reads files alone: the commit
/// list was delivered with the page it continues.
pub(super) fn files_only(query: &mut Value, operation: ItemOperation) {
    if !matches!(operation, ItemOperation::Compare) {
        return;
    }
    let sections = query
        .get("sections")
        .and_then(Value::as_array)
        .map(|sections| {
            sections
                .iter()
                .filter(|section| section.as_str() != Some("commits"))
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|sections| !sections.is_empty())
        // A `matchString` with no section named reads patches.
        .unwrap_or_else(|| {
            let section = if query.get("matchString").is_some() {
                "patches"
            } else {
                "files"
            };
            vec![json!(section)]
        });
    query["sections"] = Value::Array(sections);
}

/// Commit → pull request: the PR the message names (a squash headline's
/// `(#8506)`, a merge commit, or a `PR-URL:` trailer); otherwise issue search
/// matches PRs by commit SHA. `message` is the whole commit message, which a
/// patch read does not carry.
pub(super) fn commit_pull_request_lead(
    message: &str,
    sha: &str,
    q: &HistoryItemRequest,
) -> (&'static str, Value) {
    if let Some(number) = commit_pull_request(
        message,
        gh_get_history_item_query::owner(q),
        gh_get_history_item_query::repo(q),
    ) {
        return (
            "readPullRequest",
            Continuation::related(
                ToolId::GhGetHistoryItem,
                json!({"operation":"pullRequest","owner":gh_get_history_item_query::owner(q),"repo":gh_get_history_item_query::repo(q),"number":number}),
            )
            .build(),
        );
    }
    (
        "findPullRequest",
        Continuation::related(
            ToolId::GhSearchHistory,
            json!({"operation":"pullRequest","owner":gh_get_history_item_query::owner(q),"repo":gh_get_history_item_query::repo(q),"keywords":[sha]}),
        )
        .build(),
    )
}
