//! The pull-request read menu: the public query a continuation starts from,
//! and its optional section menu.
use super::patch::hunks::hit_hunks;
use super::patch::locate::hunk_windows;
use super::patch_hop::includable;
use super::util::merge;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation, Sections};
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

/// Leads one menu offers at most; other surfaces stay reachable through
/// `include`.
pub(super) const MENU_CAP: usize = 2;

/// Per-row menu of first-page fetches for content the call did not request:
/// at most [`MENU_CAP`] entries, in priority order (files or patches, then
/// the body, then the discussion). A merged row already shows
/// `mergeCommitSha`, so no merge-commit read is offered.
///
/// `raw` is the provider PR object; `review` is the inventory's
/// [`super::inventory::review_selection`] (empty before files were read). An
/// entry is emitted only when it can return something the row does not
/// already hold: a non-empty body (the summary row carries none) rides
/// `readFiles`, else the patch read or `readBody`; a small diff reads every patch (`readPatches`)
/// instead of a separate file list; `readDiscussion` reads comments and
/// reviews together, without comments when the provider counts none.
pub(super) fn pr_next_menu(query: &HistoryItemRequest, review: &[String], raw: &Value) -> Value {
    let read = query.sections();
    let patches = read.has(Section::Patches);
    let count = |key: &str| raw.get(key).and_then(Value::as_u64);
    // A body the read did not fetch (a GraphQL section read) may hold
    // text: only a fetched empty one rules its read out.
    let has_body = raw
        .get("body")
        .is_none_or(|body| body.as_str().is_some_and(|body| !body.trim().is_empty()));
    let want_body = !read.has(Section::Body) && has_body;
    let changed_files = count("changed_files");
    let has_files = changed_files != Some(0);
    let small_pr = is_small_pr(count("additions"), count("deletions"), changed_files);
    let no_comments = count("comments") == Some(0) && count("review_comments") == Some(0);
    // Each menu entry is a fresh first-page fetch: the base public query
    // without the current content selection or any per-surface cursor.
    let target = fresh_pr_query(query);
    let mut next = Map::new();
    let call = |sections: Sections| {
        let mut query = target.clone();
        sections.set_on(&mut query);
        restart_query(ToolId::GhGetHistoryItem, &query)
    };
    let with_body = |sections: Sections| {
        if want_body {
            sections.with(Section::Body)
        } else {
            sections
        }
    };
    let files_read = read.lists_files();
    let mut body_offered = false;
    if !files_read && has_files && !small_pr {
        next.insert(
            "readFiles".into(),
            call(with_body(Sections::of([Section::Files]))),
        );
        body_offered = true;
    }
    if !patches && has_files {
        let review = includable(&target, review);
        let covers_all = changed_files.is_some_and(|files| review.len() as u64 >= files);
        if !review.is_empty() && !covers_all {
            // The query is exact; which files answer the question is a
            // ranking guess (source files by churn, packed to one budget).
            let selected = restart_query(
                ToolId::GhGetHistoryItem,
                &merge(
                    target.clone(),
                    json!({"include": review, "sections": ["patches"]}),
                ),
            );
            next.insert("readSelectedPatches".into(), selected);
            if changed_files.is_none_or(|files| files <= INVENTORY_ALL_PATCHES_FILES) {
                next.insert("readPatches".into(), call(Sections::of([Section::Patches])));
            }
        } else if small_pr || covers_all {
            let patches = Sections::of([Section::Patches]);
            let patches = if body_offered {
                patches
            } else {
                with_body(patches)
            };
            body_offered = true;
            next.insert("readPatches".into(), call(patches));
        }
    }
    if want_body && !body_offered && !files_read {
        next.insert("readBody".into(), call(Sections::of([Section::Body])));
    }
    let mut discussion = Sections::default();
    if !read.has(Section::Comments) && !read.has(Section::ReviewComments) && !no_comments {
        discussion = Sections::of([Section::Comments, Section::ReviewComments]);
    }
    if !read.has(Section::Reviews) {
        discussion = discussion.with(Section::Reviews);
    }
    if !discussion.is_empty() {
        next.insert("readDiscussion".into(), call(discussion));
    }
    Value::Object(next.into_iter().take(MENU_CAP).collect())
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

/// `readAtCommit` and `readParent` (one call reading both when the patch was
/// delivered, named for its first side): the first file GitHub sent without a
/// patch (its change is in no response, so both sides are read whole), else
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
) -> Vec<(&'static str, Value)> {
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
    let Some(file) = missing_patch.or_else(|| {
        files
            .iter()
            .filter(|file| selected(file) && file.get("patch").and_then(Value::as_str).is_some())
            .rev()
            .max_by_key(|file| rank(file))
    }) else {
        return Vec::new();
    };
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
        if missing_patch.is_none() {
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
    // Missing source is the rest of the evidence: each side its own
    // continuation. Optional source is one related call reading both
    // sides, so the one lead a complete row keeps names both commits.
    if missing_patch.is_some() {
        return rows
            .into_iter()
            .map(|(name, row)| {
                (
                    name,
                    Continuation::continued(ToolId::GhGetFileContent, row).build(),
                )
            })
            .collect();
    }
    let Some((name, _)) = rows.first() else {
        return Vec::new();
    };
    let name = *name;
    let rows = rows.into_iter().map(|(_, row)| row).collect::<Vec<_>>();
    vec![(
        name,
        Continuation::related_input(ToolId::GhGetFileContent, json!({"queries": rows})).build(),
    )]
}

/// The note a `matchString` read states when its patches are cut to the
/// hunks that hold it and `side_reads` offers the source of those hunks
/// (`related`): it promises that call, so the response stage keeps it. `None`
/// without a literal or a related read (a missing patch's reads are required
/// continuations, never optional).
pub(super) fn hunk_read_warning(
    needle: bool,
    side_reads: &[(&'static str, Value)],
) -> Option<String> {
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
