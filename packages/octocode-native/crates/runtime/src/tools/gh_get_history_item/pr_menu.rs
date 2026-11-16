//! The pull-request read menu: the public query a continuation starts from,
//! and its optional section menu.
use super::patch::hunks::hit_hunks;
use super::patch::locate::hunk_windows;
use super::patch_hop::includable;
use super::util::merge;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation};
use crate::id::ToolId;
use crate::response::result::wire_query;
use crate::response::result::{Continuation, remove_nulls, restart_query};
use crate::tools::gh_get_history_item::sections::Section;
use octocode_contract_types::{GhGetFileContentQuery, GhGetHistoryItemQuery};
use octocode_engine::text::test_paths::is_test_path;
use serde_json::{Map, Value, json};

/// The caller's query as a working query for `operation`: the public
/// fields, with the sections the read resolved (`patches` alone already
/// lists their files: it is not restated as `files`); builders replace
/// `sections` with [`Sections::set_on`].
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
    for key in [
        "matchString",
        "contextLines",
        "regexEngine",
        "caseMode",
        "wholeWord",
    ] {
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
            "caseMode",
            "wholeWord",
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

/// Added plus removed lines of a provider file.
pub(super) fn churn(file: &Value) -> u64 {
    ["additions", "deletions"]
        .iter()
        .filter_map(|key| file.get(*key).and_then(Value::as_u64))
        .sum()
}

/// The two refs a change's numbered gutters read at: `new_ref` for ` `/`+`
/// lines (the commit, the PR head, the comparison head), `old_ref` for `-`
/// lines (the first parent, the PR base, the comparison merge base).
pub(super) struct ChangeSides<'a> {
    pub(super) new_ref: Option<&'a str>,
    pub(super) old_ref: Option<&'a str>,
    /// The read's `matchString`: the pick is the file with the most hits and
    /// its windows read the hit hunks only.
    pub(super) needle: Option<&'a super::matcher::Matcher>,
}

/// The first selected file GitHub sent without a patch whose text is
/// readable (`tooLarge` or `omitted`; not binary, not a pure rename). Only
/// REST entries (with a blob `sha`) say anything about patches.
pub(super) fn first_unpatched(
    files: &[Value],
    selected: impl Fn(&Value) -> bool,
) -> Option<&Value> {
    files.iter().find(|file| {
        selected(file)
            && file.get("patch").is_none()
            && file.get("sha").is_some()
            && matches!(
                super::inventory::missing_patch_reason(file),
                Some("tooLarge" | "omitted")
            )
    })
}

/// `readAtCommit` and `readParent` (related calls reading both sides when
/// the patch was delivered, the first named for its first side): the first
/// file GitHub sent without a patch (its change is in no response, so both
/// sides are read whole), else with a `matchString` every file whose hunks
/// hold it (most hits first, as many calls as the queries limit needs), else
/// the change's most-changed code file (a test, doc or other file only when
/// no code file has a patch), read over its hunks' windows at each side: the
/// new-side windows at the new side, the old-side windows at the old side
/// (its pre-rename path). A side the diff has no lines on (an added file's
/// old side, a removed file's new side, a pure addition's or removal's
/// other side) has no read, so a delivered patch never leads to a whole-file
/// read. Only files the read selected are candidates: `selected` names the
/// files the page shows. Missing patches require source recovery; delivered
/// patches offer their source as optional related work.
pub(super) fn change_reads(
    owner: &str,
    repo: &str,
    files: &[Value],
    sides: &ChangeSides<'_>,
    selected: impl Fn(&Value) -> bool,
) -> Vec<(String, Value)> {
    use crate::content::{FileType, classify_file_type};
    let hits = |file: &Value| {
        sides.needle.map_or(0, |needle| {
            let patch = file.get("patch").and_then(Value::as_str).unwrap_or("");
            hit_hunks(patch, needle).1
        })
    };
    let rank = |file: &Value| {
        let path = file.get("filename").and_then(Value::as_str).unwrap_or("");
        let code = classify_file_type(path) == Some(FileType::Code) && !is_test_path(path);
        (hits(file), code, churn(file))
    };
    let missing_patch = first_unpatched(files, &selected);
    let patched =
        |file: &&Value| selected(file) && file.get("patch").and_then(Value::as_str).is_some();
    // Source recovery reads the one file with no patch. Optional source
    // reads cover every file a `matchString` hit (its hunks at each side),
    // most hits first; with no literal, the one most-changed code file.
    let picks: Vec<&Value> = match missing_patch {
        Some(file) => vec![file],
        None if sides.needle.is_some() && files.iter().filter(patched).any(|f| hits(f) > 0) => {
            let mut hit_files: Vec<&Value> = files
                .iter()
                .filter(patched)
                .filter(|file| hits(file) > 0)
                .collect();
            hit_files.sort_by_key(|file| std::cmp::Reverse(rank(file)));
            hit_files
        }
        None => files
            .iter()
            .filter(patched)
            .rev()
            .max_by_key(|file| rank(file))
            .into_iter()
            .collect(),
    };
    let mut rows = Vec::new();
    for file in picks {
        rows.extend(file_side_rows(
            owner,
            repo,
            file,
            sides,
            missing_patch.is_some(),
        ));
    }
    // Missing source is the rest of the evidence: each side its own
    // continuation. Optional source is related calls reading both sides,
    // so the one lead a complete row keeps names both commits; rows beyond
    // one call's queries limit ride further calls.
    if missing_patch.is_some() {
        return rows
            .into_iter()
            .map(|(name, row)| {
                (
                    name.to_owned(),
                    Continuation::continued(ToolId::GhGetFileContent, row).build(),
                )
            })
            .collect();
    }
    let Some((name, _)) = rows.first() else {
        return Vec::new();
    };
    let name = *name;
    let per_call = crate::response::result::rows_per_call(ToolId::GhGetFileContent).max(1);
    let rows = rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>();
    rows.chunks(per_call)
        .enumerate()
        .map(|(index, chunk)| {
            (
                crate::id::numbered(name, index),
                Continuation::related_input(ToolId::GhGetFileContent, json!({"queries": chunk}))
                    .build(),
            )
        })
        .collect()
}

/// The `readAtCommit`/`readParent` rows of one file: its hunks' windows at
/// each side (`whole`: a file with no patch, both sides read whole).
fn file_side_rows(
    owner: &str,
    repo: &str,
    file: &Value,
    sides: &ChangeSides<'_>,
    whole: bool,
) -> Vec<(&'static str, Value)> {
    let path = file.get("filename").and_then(Value::as_str).unwrap_or("");
    let status = file.get("status").and_then(Value::as_str).unwrap_or("");
    let patch = file.get("patch").and_then(Value::as_str).unwrap_or("");
    // A literal reads the hunks that hold it, not the file's other changes.
    let hit_patch = sides
        .needle
        .map(|needle| hit_hunks(patch, needle).0)
        .filter(|hit| !hit.is_empty());
    let patch = hit_patch.as_deref().unwrap_or(patch);
    // A side read row the contract accepts.
    let source_read = |row: Value| wire_query::<GhGetFileContentQuery>(row.clone()).map(|_| row);
    // A file with no patch has no hunks to bound a read: both sides read whole.
    let side_read = |path: &str, reference: &str, mark: char| {
        let mut row = json!({"owner": owner, "repo": repo, "path": path, "ref": reference});
        if !whole {
            let ranges = hunk_windows(patch, mark);
            if ranges.is_empty() {
                return None;
            }
            row["ranges"] = json!(
                ranges
                    .iter()
                    .map(|(start, end)| json!({"startLine":start,"endLine":end}))
                    .collect::<Vec<_>>()
            );
        }
        source_read(row)
    };
    let mut rows = Vec::new();
    if let Some(new_ref) = sides.new_ref.filter(|_| status != "removed")
        && let Some(read) = side_read(path, new_ref, '+')
    {
        rows.push(("readAtCommit", read));
    }
    if let Some(old_ref) = sides.old_ref.filter(|_| status != "added") {
        let old_path = file
            .get("previous_filename")
            .and_then(Value::as_str)
            .unwrap_or(path);
        if let Some(read) = side_read(old_path, old_ref, '-') {
            rows.push(("readParent", read));
        }
    }
    rows
}

/// The note a `matchString` read states when its patches are cut to the
/// hunks that hold it and `side_reads` offers the source of those hunks
/// (`related`): it promises that call, so the response stage keeps it. `None`
/// without a literal or a related read (a missing patch's reads are required
/// continuations, never optional).
pub(super) fn hunk_read_warning(needle: bool, side_reads: &[(String, Value)]) -> Option<String> {
    (needle && side_reads.iter().any(|(_, call)| call["kind"] == "related")).then(|| {
        "patches show only the hunks that hold matchString; related reads the source of those hunks at each side."
            .to_owned()
    })
}

pub(super) fn remove_key(value: &mut Value, key: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}

#[cfg(test)]
mod tests;
