//! Selected-patch reads and required commit/comparison page cursors.
use super::pr_menu::*;
use super::{HistoryItemRequest, ItemOperation, Sections};
use crate::gh_get_history_item::gh_get_history_item_query;
use crate::gh_get_history_item::sections::Section;
use octocode_contract_types::GhGetHistoryItemQuery;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use serde_json::{Value, json};

/// The `include` globs that name exactly each of `paths` in a
/// selected-patch read of `target` ([`crate::include_globs::exact`]): the
/// ones its query contract admits (a file GitHub reported without a usable
/// name, such as an empty one, is left out). A lead never carries a call its
/// own tool would refuse.
pub(super) fn includable(target: &Value, paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| !path.is_empty())
        .map(|path| crate::include_globs::exact(path))
        .filter(|glob| {
            let probe = super::util::merge(
                target.clone(),
                json!({"include": [glob], "sections": ["patches"]}),
            );
            serde_json::from_value::<GhGetHistoryItemQuery>(probe).is_ok()
        })
        .collect()
}

/// The patch continuation of a pull-request page: the same query (file
/// page, filters, selection) at the page-stream cursor, narrowed to the
/// patch surface.
pub(super) fn patch_continuation(
    mut nq: Value,
    entry: &Value,
    q: &HistoryItemRequest,
) -> Option<Continuation> {
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
}

impl DiffCursors {
    /// The cursors of a file page: its `nextPage`, read out of the block
    /// (a cursor rides the `next` continuation, never the block), and the
    /// patch-stream cursor of its unfinished patch window. The cursor is
    /// internal state for `next.patches`, never an output field.
    pub(super) fn of_file_page(files_page: &mut Value, patch_offset: Option<usize>) -> Self {
        Self {
            commit_page: None,
            file_page: files_page
                .as_object_mut()
                .and_then(|page| page.remove("nextPage"))
                .and_then(|page| page.as_u64()),
            patch_offset,
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
    // A diff read continues as a patches read; any other read keeps the
    // sections it named, so a `files` walk stays a `files` read. A summary
    // names none: its `next.files` opens the list ([`files_only`]).
    if q.include_diff() {
        row["sections"] = serde_json::json!(["patches"]);
    }
    row
}

/// Required pages from computed cursors of a commit or comparison read.
pub(super) fn attach_diff_continuations(
    out: &mut Value,
    q: &HistoryItemRequest,
    operation: ItemOperation,
    resolved_ref: Option<&str>,
    cursors: DiffCursors,
) {
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
        super::sections::drop_idle_file_filters(&mut nq);
        for key in ["offset", "matchString", "contextLines", "regexEngine"] {
            remove_key(&mut nq, key);
        }
        if let Some(call) = continuation(nq) {
            octocode_runtime_base::response::collection::insert_walk(
                &mut out["next"],
                ToolId::GhGetHistoryItem,
                Some("commits"),
                call,
            );
        }
    }
    // A file page with unread patches continues them first: its window
    // that finishes the patches offers the next file page.
    if let Some(page) = cursors.file_page.filter(|_| cursors.patch_offset.is_none()) {
        let mut nq = base.clone();
        // The first file page is the page a read without `page` lists.
        if page > 1 {
            nq["page"] = json!(page);
        } else {
            remove_key(&mut nq, "page");
        }
        remove_key(&mut nq, "offset");
        files_only(&mut nq, operation);
        pin_file_page_size(&mut nq, q, q.include_diff());
        if let Some(call) = continuation(nq) {
            octocode_runtime_base::response::collection::insert_walk(
                &mut out["next"],
                ToolId::GhGetHistoryItem,
                Some("files"),
                call,
            );
        }
    }
    // The page-stream cursor, not a file's own `patchPagination` offset.
    if let Some(offset) = cursors.patch_offset {
        let mut nq = base;
        nq["offset"] = json!(offset);
        files_only(&mut nq, operation);

        if let Some(call) = continuation(nq) {
            octocode_runtime_base::response::collection::insert_walk(
                &mut out["next"],
                ToolId::GhGetHistoryItem,
                Some("patches"),
                call,
            );
        }
    }
}

/// A comparison file or patch continuation reads files alone: the commit
/// list was delivered with the page it continues.
pub(super) fn files_only(query: &mut Value, operation: ItemOperation) {
    // A commit read that named no section is its summary or a filtered file
    // answer: the page that lists its files is a `files` read (a
    // `matchString` reads patches), so the list opens instead of the
    // summary answering again.
    if matches!(operation, ItemOperation::Commit) && query.get("sections").is_none() {
        let section = if query.get("matchString").is_some() {
            "patches"
        } else {
            "files"
        };
        query["sections"] = json!([section]);
        return;
    }
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
