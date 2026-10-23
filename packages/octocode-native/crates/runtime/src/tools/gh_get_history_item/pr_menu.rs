//! The pull-request read menu: the public query a continuation starts from,
//! the section menu, and the merge-commit read.
use super::patch::locate::{distinctive_added_lines, parent_windows};
use super::util::merge;
use super::{DEFAULT_PAGE_SIZE, HistoryItemRequest, ItemOperation, Section, Sections};
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, remove_nulls};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
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
        m.remove("extras");
        // A continuation restates no default: page 1 and the contract's
        // page size are what an omitted field reads. Only an issue declares
        // a pageSize default; a pull request, commit or comparison sizes
        // each section itself, so its pageSize stays as the caller set it.
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
pub(super) fn continuation(q: Value) -> Value {
    Continuation::new(ToolId::GhGetHistoryItem, q)
        .confidence(Confidence::Exact)
        .build()
}

/// The recovery of a `matchString` that hit no patch line (a pull
/// request's or a commit's): the same read without the literal and the
/// `contextLines` that only shape its hits.
pub(super) fn without_match_string(mut q: Value) -> Value {
    for key in ["matchString", "contextLines"] {
        remove_key(&mut q, key);
    }
    continuation(q)
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
        for key in ["sections", "offset", "length", "page", "matchString"] {
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
        continuation(query)
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
        let covers_all = changed_files.is_some_and(|files| review.len() as u64 >= files);
        if !review.is_empty() && !covers_all {
            // The query is exact; which files answer the question is a
            // ranking guess (source files by churn, packed to one budget).
            let mut selected = continuation(merge(
                target.clone(),
                json!({"include": review, "sections": ["patches"]}),
            ));
            selected["confidence"] = json!("high");
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

/// `next.readAtMerge`: a merged pull request whose patches were read offers
/// its most-changed code file (not a test) at the merge commit, searched for
/// one distinctive added line per hunk (at most
/// [`super::patch::locate::MERGE_MATCH_LINES`]), so
/// the fix is checked in the code that shipped. The diff's line numbers
/// belong to `headSha`; the target branch may have moved the lines by the
/// merge, so the read locates them by text, never by number. `files` is
/// every loaded provider file (`filename`, `additions`, `deletions`,
/// `patch`), not only the patch window shown. `None` for unmerged pull
/// requests, for a `matchString` read (a targeted answer already), and when
/// no such file adds a line with something to find. Only files the read selected
/// (`include`, `status`, `minChanges`) are candidates: `selected`.
pub(super) fn read_at_merge(
    query: &HistoryItemRequest,
    raw: &Value,
    files: &[Value],
    selected: impl Fn(&Value) -> bool,
) -> Option<Value> {
    use crate::content::{FileType, classify_file_type, is_test_path};
    if gh_get_history_item_query::match_string(query).is_some() {
        return None;
    }
    raw.get("merged_at").filter(|merged| !merged.is_null())?;
    let sha = raw
        .get("merge_commit_sha")
        .and_then(Value::as_str)
        .filter(|sha| !sha.is_empty())?;
    let (path, lines) = files
        .iter()
        .filter(|file| selected(file))
        .filter_map(|file| {
            let path = file.get("filename")?.as_str()?;
            let patch = file.get("patch")?.as_str()?;
            (classify_file_type(path) == Some(FileType::Code) && !is_test_path(path))
                .then_some(())?;
            let lines = distinctive_added_lines(patch);
            (!lines.is_empty()).then(|| (churn(file), path, lines))
        })
        // The most-changed file; the first of equals.
        .rev()
        .max_by_key(|(churn, _, _)| *churn)
        .map(|(_, path, lines)| (path, lines))?;
    Some(
        Continuation::new(
            ToolId::GhGetFileContent,
            json!({
                "owner": gh_get_history_item_query::owner(query),
                "repo": gh_get_history_item_query::repo(query),
                "ref": sha,
                "path": path,
                "matchString": lines,
            }),
        )
        .confidence(Confidence::High)
        .build(),
    )
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
    /// `high` when `old_ref` is the diff's exact old side; `medium` for a
    /// PR base, which may have moved past the diff's merge base.
    pub(super) old_confidence: Confidence,
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

/// `readAtCommit` and `readParent`: the first file GitHub sent without a
/// patch (its change is in no response, so both sides are read whole), else
/// the change's most-changed code file (a test, doc or other file only when
/// no code file has a patch), read whole at the new side and, over its
/// hunks' old-side windows, at the old side (its pre-rename path). An added
/// file has no parent read, a removed one no new-side read. Only files the
/// read selected are candidates.
pub(super) fn change_reads(
    owner: &str,
    repo: &str,
    files: &[Value],
    sides: &ChangeSides<'_>,
    selected: impl Fn(&Value) -> bool,
) -> Vec<(&'static str, Value)> {
    use crate::content::{FileType, classify_file_type, is_test_path};
    let rank = |file: &Value| {
        let path = file.get("filename").and_then(Value::as_str).unwrap_or("");
        let code = classify_file_type(path) == Some(FileType::Code) && !is_test_path(path);
        (code, churn(file))
    };
    let Some(file) = first_unpatched(files, &selected).or_else(|| {
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
    let mut reads = Vec::new();
    if let Some(new_ref) = sides.new_ref.filter(|_| status != "removed") {
        reads.push((
            "readAtCommit",
            Continuation::new(
                ToolId::GhGetFileContent,
                json!({"owner": owner, "repo": repo, "path": path, "ref": new_ref}),
            )
            .confidence(Confidence::High)
            .build(),
        ));
    }
    if let Some(old_ref) = sides.old_ref.filter(|_| status != "added") {
        let old_path = file
            .get("previous_filename")
            .and_then(Value::as_str)
            .unwrap_or(path);
        let mut row = json!({"owner": owner, "repo": repo, "path": old_path, "ref": old_ref});
        let ranges = parent_windows(patch);
        if !ranges.is_empty() {
            row["ranges"] = json!(ranges);
        }
        reads.push((
            "readParent",
            Continuation::new(ToolId::GhGetFileContent, row)
                .confidence(sides.old_confidence)
                .build(),
        ));
    }
    reads
}

pub(super) fn remove_key(value: &mut Value, key: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file GitHub sent without a readable patch (tooLarge, omitted)
    /// is read at both sides, whole, ahead of the most-changed patched
    /// file; a binary file or a GraphQL node (no blob sha) never is.
    #[test]
    fn change_reads_prefer_the_first_unpatched_text_file() {
        let sides = ChangeSides {
            new_ref: Some("head"),
            old_ref: Some("base"),
            old_confidence: Confidence::Medium,
        };
        let file = |name: &str, additions: u64, patch: Option<&str>| {
            let mut file = json!({"filename":name,"status":"modified","additions":additions,
                "deletions":0,"sha":"1111111111111111111111111111111111111111"});
            if let Some(patch) = patch {
                file["patch"] = json!(patch);
            }
            file
        };
        let files = [
            file("src/big.rs", 900, Some("@@ -1,1 +1,2 @@\n x\n+y")),
            file("assets/logo.png", 0, None),
            file("src/huge.rs", 70_000, None),
        ];
        let reads = change_reads("o", "r", &files, &sides, |_| true);
        let paths = reads
            .iter()
            .map(|(name, read)| (*name, read["query"]["queries"][0].clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                (
                    "readAtCommit",
                    json!({"owner":"o","repo":"r","path":"src/huge.rs","ref":"head"})
                ),
                (
                    "readParent",
                    json!({"owner":"o","repo":"r","path":"src/huge.rs","ref":"base"})
                ),
            ]
        );
        // Without an unpatched text file the top patched file is read.
        let reads = change_reads("o", "r", &files[..2], &sides, |_| true);
        assert_eq!(reads[0].1["query"]["queries"][0]["path"], "src/big.rs");
        let mut graphql = files[2].clone();
        graphql.as_object_mut().map(|file| file.remove("sha"));
        assert!(first_unpatched(&[graphql], |_| true).is_none());
    }
}
