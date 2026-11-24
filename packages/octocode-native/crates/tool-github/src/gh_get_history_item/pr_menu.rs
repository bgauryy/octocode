//! The pull-request read menu: the public query a continuation starts from,
//! and its optional section menu.
use super::patch_hop::includable;
use super::util::merge;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation};
use crate::gh_get_history_item::sections::Section;
use octocode_contract_types::{GhGetFileContentQuery, GhGetHistoryItemQuery};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::wire_query;
use octocode_runtime_base::response::result::{Continuation, remove_nulls, restart_query};
use serde_json::{Map, Value, json};

/// The caller's query as a working query for `operation`: the public
/// fields, with the sections the read resolved (`patches` alone already
/// lists their files: it is not restated as `files`); builders replace
/// `sections` with [`Sections::set_on`](super::sections::Sections::set_on).
pub(super) fn base_public_query(q: &HistoryItemRequest, operation: ItemOperation) -> Value {
    let mut v = q.public.clone();
    remove_nulls(&mut v);
    if matches!(operation, ItemOperation::PullRequest | ItemOperation::Issue) {
        q.sections().set_on(&mut v);
    }
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
        // Omit private progress equal to the runtime defaults. Other saved
        // page sizes retain the grid chosen by the preceding read.
        if m.get("page").and_then(Value::as_u64) == Some(1) {
            m.remove("page");
        }
        if operation == ItemOperation::Issue
            && m.get("pageSize").and_then(Value::as_u64) == u64::try_from(DEFAULT_PAGE_SIZE).ok()
        {
            m.remove("pageSize");
        }
    }
    v
}

/// An executable `ghGetHistoryItem` continuation.
pub(super) fn continuation(q: Value) -> Option<Continuation> {
    Some(Continuation::continued_query(wire_query::<
        GhGetHistoryItemQuery,
    >(q)?))
}

/// The recovery of a `matchString` that hit no patch line (a pull
/// request's or a commit's): the same read without the literal and the
/// `contextLines` that only shape its hits.
pub(super) fn without_match_string(mut q: Value) -> Value {
    for key in ["matchString", "contextLines", "regexEngine"] {
        remove_key(&mut q, key);
    }
    restart_query(ToolId::GhGetHistoryItem, &q)
}

/// A diff at most this many changed lines reads in one all-patches call, so a
/// separate file-list-only fetch would only repeat its file list.
pub(super) const SMALL_DIFF_LINES: u64 = 100;

/// Unfiltered inventories up to this many files keep `readPatches` beside
/// the selected-patch pick: every patch is still a bounded read.
pub(super) const INVENTORY_ALL_PATCHES_FILES: u64 = 30;

/// A first-page fetch of `query`'s PR: the base public query without its
/// sections, literal, or per-surface cursors.
pub(super) fn fresh_pr_query(query: &HistoryItemRequest) -> Value {
    let mut target = base_public_query(query, ItemOperation::PullRequest);
    if let Some(object) = target.as_object_mut() {
        for key in [
            "sections",
            "offset",
            "page",
            "pageSize",
            "headSha",
            "matchString",
            "contextLines",
            "regexEngine",
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

/// Per-row menu of first-page fetches for content the call did not request.
/// The static views (files, patches, body, discussion) are one `sections`
/// value away and are named once in the tool description; the menu holds
/// only the ranked pick, `readSelectedPatches`: which files to read is a
/// guess the row's inventory made, not a spelling in the schema.
///
/// `raw` is the provider PR object; `review` is the inventory's
/// [`super::inventory::review_selection`] (empty before files were read).
/// The entry is emitted only when the pick is a proper subset of the changed
/// files and the call did not already read patches.
pub(super) fn pr_next_menu(query: &HistoryItemRequest, review: &[String], raw: &Value) -> Value {
    let changed_files = raw.get("changed_files").and_then(Value::as_u64);
    if query.sections().has(Section::Patches) || changed_files == Some(0) {
        return Value::Object(Map::new());
    }
    // Each entry is a fresh first-page fetch: the base public query without
    // the current content selection or any per-surface cursor.
    let target = fresh_pr_query(query);
    let review = includable(&target, review);
    let covers_all = changed_files.is_some_and(|files| review.len() as u64 >= files);
    let mut next = Map::new();
    if !review.is_empty() && !covers_all {
        next.insert(
            "readSelectedPatches".into(),
            restart_query(
                ToolId::GhGetHistoryItem,
                &merge(target, json!({"include": review, "sections": ["patches"]})),
            ),
        );
    }
    Value::Object(next)
}

/// The two refs a change's source reads at: `new_ref` for the new side (the
/// commit, the PR head, the comparison head), `old_ref` for the old side
/// (the first parent, the PR base, the comparison merge base).
pub(super) struct ChangeSides<'a> {
    pub(super) new_ref: Option<&'a str>,
    pub(super) old_ref: Option<&'a str>,
}

/// The selected files GitHub sent without a patch whose text is readable
/// (`tooLarge` or `omitted`; not binary, not a pure rename). Only REST
/// entries (with a blob `sha`) say anything about patches.
pub(super) fn unpatched(files: &[Value], selected: impl Fn(&Value) -> bool) -> Vec<&Value> {
    files
        .iter()
        .filter(|file| selected(file) && super::inventory::source_readable_without_patch(file))
        .collect()
}

/// `readAtCommit` and `readParent`: the source of every selected file GitHub
/// sent without a patch (its change is in no response), read whole at each
/// side the change has: the new side at `new_ref` (none for a removed file),
/// the old side at `old_ref` under its pre-rename path (none for an added
/// file). Each side's reads ride required continuations within one call's
/// queries limit. A delivered patch is the evidence itself and leads to no
/// source read. Only files the read selected are candidates: `selected`
/// names the files the page shows.
pub(super) fn change_reads(
    owner: &str,
    repo: &str,
    files: &[Value],
    sides: &ChangeSides<'_>,
    selected: impl Fn(&Value) -> bool,
) -> Vec<(String, Value)> {
    let rows = unpatched(files, &selected)
        .into_iter()
        .flat_map(|file| file_side_rows(owner, repo, file, sides))
        .collect::<Vec<_>>();
    let per_call =
        octocode_runtime_base::response::result::rows_per_call(ToolId::GhGetFileContent).max(1);
    let mut calls = Vec::new();
    for kind in ["readAtCommit", "readParent"] {
        let side = rows
            .iter()
            .filter(|(name, _)| *name == kind)
            .map(|(_, row)| row.clone())
            .collect::<Vec<_>>();
        for (index, chunk) in side.chunks(per_call).enumerate() {
            calls.push((
                octocode_runtime_base::id::numbered(kind, index),
                Continuation::continued_input(ToolId::GhGetFileContent, json!({"queries": chunk}))
                    .build(),
            ));
        }
    }
    calls
}

/// The `readAtCommit`/`readParent` rows of one patchless file, each a whole
/// read the contract accepts.
fn file_side_rows(
    owner: &str,
    repo: &str,
    file: &Value,
    sides: &ChangeSides<'_>,
) -> Vec<(&'static str, Value)> {
    let path = file.get("filename").and_then(Value::as_str).unwrap_or("");
    let status = file.get("status").and_then(Value::as_str).unwrap_or("");
    let side_read = |path: &str, reference: &str| {
        let row = json!({"owner": owner, "repo": repo, "path": path, "ref": reference});
        wire_query::<GhGetFileContentQuery>(row.clone()).map(|_| row)
    };
    let mut rows = Vec::new();
    if let Some(new_ref) = sides.new_ref.filter(|_| status != "removed")
        && let Some(read) = side_read(path, new_ref)
    {
        rows.push(("readAtCommit", read));
    }
    if let Some(old_ref) = sides.old_ref.filter(|_| status != "added") {
        let old_path = file
            .get("previous_filename")
            .and_then(Value::as_str)
            .unwrap_or(path);
        if let Some(read) = side_read(old_path, old_ref) {
            rows.push(("readParent", read));
        }
    }
    rows
}

pub(super) fn remove_key(value: &mut Value, key: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}

#[cfg(test)]
mod tests;
